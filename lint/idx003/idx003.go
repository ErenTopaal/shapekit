// Package idx003 implements indexerlint rule IDX003, participants derived
// from the envelope.
//
// A function is reported when it puts an address read from the transaction
// envelope into an account set and neither it nor anything it calls reads
// transaction meta. Envelope addresses (source accounts, destinations,
// trustors, auth entry credentials, declared invocations, the footprint)
// are what the submitter wrote; only the meta (ledger entry changes and
// events) says which accounts the ledger actually touched.
//
// What counts as an account set:
//
//   - the return value of a function whose name mentions participants or
//     accounts, when its type is a collection of strings or xdr account
//     types (slices, maps, generic sets);
//   - an append, map store or Add/Union/Insert-style method call on such a
//     collection, inside such a function or on a variable or field whose
//     name mentions participants or accounts;
//   - a struct field named for participants or accounts;
//   - an argument to a With/Set/Add method whose name mentions an account,
//     participant, creator or deployer, as on a state change builder.
//
// "Reads meta" means touching a value of a meta type anywhere in the
// function, its closures, or the functions it calls statically. Callees in
// other packages are followed through analysis facts. See docs/linter.md
// for the blind spots.
package idx003

import (
	"errors"
	"fmt"
	"go/ast"
	"go/types"
	"path"
	"regexp"

	"golang.org/x/tools/go/analysis"

	"github.com/ErenTopaal/shapekit/lint/internal/metaread"
	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
	"github.com/ErenTopaal/shapekit/lint/rules"
)

// ID is the rule ID this analyzer reports under.
const ID = "IDX003"

// Analyzer is the IDX003 analyzer.
var Analyzer = &analysis.Analyzer{
	Name:       ID,
	Doc:        doc(),
	URL:        "https://github.com/ErenTopaal/shapekit/blob/main/docs/linter.md#idx003-participants-from-envelope",
	Run:        run,
	ResultType: suppress.ResultType,
	FactTypes:  []analysis.Fact{new(envelopeFact), new(fillsFact)},
	Requires:   []*analysis.Analyzer{metaread.Analyzer},
}

func doc() string {
	r, err := rules.Lookup(ID)
	if err != nil {
		panic(err)
	}

	return r.Name + ": " + r.Description
}

// envelopeFact marks a function whose result is derived from an envelope
// address, naming the accessor it came from.
type envelopeFact struct {
	Source string
}

func (*envelopeFact) AFact() {}

// fillsFact marks a function that writes envelope addresses into the set
// passed as a parameter, by parameter index, naming the accessor.
type fillsFact struct {
	Params []string
}

func (*fillsFact) AFact() {}

func (f *fillsFact) String() string { return fmt.Sprintf("fills%q", f.Params) }

func (f *envelopeFact) String() string { return "envelope(" + f.Source + ")" }

var (
	accountName = regexp.MustCompile(`(?i)participant|account`)
	builderName = regexp.MustCompile(`^(With|Set|Add)\w*(Account|Participant|Creator|Deployer)`)
	addMethods  = map[string]bool{
		"Add": true, "Append": true, "Insert": true, "Push": true,
		"Put": true, "Store": true, "Union": true,
	}
)

type unit struct {
	decl *ast.FuncDecl
	obj  *types.Func
}

type analysis3 struct {
	pass     *analysis.Pass
	meta     *metaread.Result
	order    []*unit
	envelope map[*types.Func]string
	fills    map[*types.Func][]string
}

func run(pass *analysis.Pass) (any, error) {
	rep, err := suppress.NewReporter(pass, ID)
	if err != nil {
		return nil, err
	}

	if sdk.IsStd(pass.Pkg.Path()) || sdk.IsSDKPackage(pass.Pkg.Path()) {
		return rep.Result(), nil
	}

	meta, ok := pass.ResultOf[metaread.Analyzer].(*metaread.Result)
	if !ok {
		return nil, errors.New("metaread result missing")
	}

	a := &analysis3{
		pass:     pass,
		meta:     meta,
		envelope: map[*types.Func]string{},
		fills:    map[*types.Func][]string{},
	}

	a.collect()
	a.solveEnvelope()
	a.solveFills()
	a.exportFacts()

	for _, u := range a.order {
		if a.meta.ReadsMeta(u.obj) {
			continue
		}

		var sinks []sink

		for _, s := range a.taint(u).sinks() {
			if s.param < 0 {
				sinks = append(sinks, s)
			}
		}

		if len(sinks) == 0 {
			continue
		}

		first := sinks[0]
		rep.Report(first.pos, u.decl, fmt.Sprintf(
			"%s puts an address from %s into an account set but reads no transaction meta (ledger entry changes or events) in its call tree; envelope addresses are what the submitter declared, not what the ledger did",
			sdk.FuncName(u.decl), first.source))
	}

	return rep.Result(), nil
}

func (a *analysis3) collect() {
	for _, f := range a.pass.Files {
		if sdk.IsTestFile(a.pass, f) {
			continue
		}

		for _, d := range f.Decls {
			fd, ok := d.(*ast.FuncDecl)
			if !ok || fd.Body == nil {
				continue
			}

			if obj, ok := a.pass.TypesInfo.Defs[fd.Name].(*types.Func); ok {
				a.order = append(a.order, &unit{decl: fd, obj: obj})
			}
		}
	}
}

// envelopeSource returns the accessor a function's result is derived from,
// or "" when it is not envelope-derived.
func (a *analysis3) envelopeSource(fn *types.Func) string {
	if fn.Pkg() == a.pass.Pkg {
		return a.envelope[fn]
	}

	var f envelopeFact
	if a.pass.ImportObjectFact(fn, &f) {
		return f.Source
	}

	return ""
}

func (a *analysis3) solveEnvelope() {
	// each round can only add functions, so the number of functions bounds
	// the number of rounds
	for range len(a.order) + 1 {
		changed := false

		for _, u := range a.order {
			if a.envelope[u.obj] != "" {
				continue
			}

			if src := a.taint(u).returned; src != "" {
				a.envelope[u.obj] = src
				changed = true
			}
		}

		if !changed {
			return
		}
	}
}

// exportFacts exports summaries for exported functions and methods only:
// nothing in another package can call an unexported one directly.
// solveFills finds helpers that write envelope addresses into a set their
// caller passed in and read no meta themselves.
func (a *analysis3) solveFills() {
	for range len(a.order) + 1 {
		changed := false

		for _, u := range a.order {
			if a.meta.ReadsMeta(u.obj) {
				continue
			}

			for _, s := range a.taint(u).sinks() {
				if s.param < 0 {
					continue
				}

				cur := a.fills[u.obj]
				if cur == nil {
					cur = make([]string, u.obj.Signature().Params().Len())
					a.fills[u.obj] = cur
				}

				if s.param < len(cur) && cur[s.param] == "" {
					cur[s.param] = s.source
					changed = true
				}
			}
		}

		if !changed {
			return
		}
	}
}

func (a *analysis3) fillsOf(fn *types.Func) []string {
	if fn.Pkg() == a.pass.Pkg {
		return a.fills[fn]
	}

	var f fillsFact
	if a.pass.ImportObjectFact(fn, &f) {
		return f.Params
	}

	return nil
}

func (a *analysis3) exportFacts() {
	for _, u := range a.order {
		if !u.obj.Exported() {
			continue
		}

		if fills := a.fills[u.obj]; fills != nil {
			a.pass.ExportObjectFact(u.obj, &fillsFact{Params: fills})
		}

		if src := a.envelope[u.obj]; src != "" {
			a.pass.ExportObjectFact(u.obj, &envelopeFact{Source: src})
		}
	}
}

// accessorName prints an accessor as xdr.PaymentOp.Destination.
func accessorName(pkg, typ, name string) string {
	return path.Base(pkg) + "." + typ + "." + name
}
