// Package metaread works out which functions read transaction meta:
// ledger entry changes or events, the record of what a transaction actually
// did. IDX001 and IDX003 both treat a function whose call tree reads meta
// as one that can check envelope data against it.
//
// A function reads meta when it touches a value of a meta type anywhere in
// its body or closures, or calls, statically, a function that does. Callees
// in other packages are followed through facts.
package metaread

import (
	"go/ast"
	"go/types"
	"reflect"

	"golang.org/x/tools/go/analysis"

	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
)

// Analyzer computes the Result for a package.
var Analyzer = &analysis.Analyzer{
	Name:       "indexerlintmetaread",
	Doc:        "works out which functions read transaction meta, for the indexerlint rules",
	Run:        run,
	ResultType: reflect.TypeFor[*Result](),
	FactTypes:  []analysis.Fact{new(readsFact)},
}

// readsFact marks an exported function that reads meta itself or through a
// callee.
type readsFact struct {
	Reads bool
}

func (*readsFact) AFact() {}

func (*readsFact) String() string { return "readsMeta" }

// Result answers ReadsMeta for the package's own functions and for every
// function of another package the package calls.
type Result struct {
	reads map[*types.Func]bool
}

// ReadsMeta reports whether fn reads meta in its call tree.
func (r *Result) ReadsMeta(fn *types.Func) bool {
	return r.reads[fn.Origin()]
}

type unit struct {
	obj        *types.Func
	directMeta bool
	callees    []*types.Func
}

func run(pass *analysis.Pass) (any, error) {
	res := &Result{reads: map[*types.Func]bool{}}

	if sdk.IsStd(pass.Pkg.Path()) || sdk.IsSDKPackage(pass.Pkg.Path()) {
		return res, nil
	}

	info := pass.TypesInfo

	var units []*unit

	for _, f := range pass.Files {
		for _, d := range f.Decls {
			fd, ok := d.(*ast.FuncDecl)
			if !ok || fd.Body == nil {
				continue
			}

			obj, ok := info.Defs[fd.Name].(*types.Func)
			if !ok {
				continue
			}

			u := &unit{obj: obj}

			ast.Inspect(fd.Body, func(x ast.Node) bool {
				e, ok := x.(ast.Expr)
				if !ok {
					return true
				}

				if !u.directMeta && IsMeta(info.TypeOf(e)) {
					u.directMeta = true
				}

				if call, ok := e.(*ast.CallExpr); ok {
					if fn, ok := sdk.Callee(info, call).(*types.Func); ok {
						fn = fn.Origin()
						u.callees = append(u.callees, fn)

						if fn.Pkg() != nil && fn.Pkg() != pass.Pkg {
							var fact readsFact
							if pass.ImportObjectFact(fn, &fact) && fact.Reads {
								res.reads[fn] = true
							}
						}
					}
				}

				return true
			})

			units = append(units, u)
			res.reads[obj] = u.directMeta
		}
	}

	for changed := true; changed; {
		changed = false

		for _, u := range units {
			if res.reads[u.obj] {
				continue
			}

			for _, c := range u.callees {
				if res.reads[c] {
					res.reads[u.obj] = true
					changed = true

					break
				}
			}
		}
	}

	for _, u := range units {
		if u.obj.Exported() && res.reads[u.obj] {
			pass.ExportObjectFact(u.obj, &readsFact{Reads: true})
		}
	}

	return res, nil
}

// metaTypes are the types of transaction meta: ledger entry changes and
// events. Touching a value of one of these is what "reads meta" means.
var metaTypes = map[[2]string]bool{
	{sdk.XDR, "TransactionMeta"}:          true,
	{sdk.XDR, "TransactionMetaV1"}:        true,
	{sdk.XDR, "TransactionMetaV2"}:        true,
	{sdk.XDR, "TransactionMetaV3"}:        true,
	{sdk.XDR, "TransactionMetaV4"}:        true,
	{sdk.XDR, "OperationMeta"}:            true,
	{sdk.XDR, "OperationMetaV2"}:          true,
	{sdk.XDR, "LedgerEntryChange"}:        true,
	{sdk.XDR, "LedgerEntryChanges"}:       true,
	{sdk.XDR, "SorobanTransactionMeta"}:   true,
	{sdk.XDR, "SorobanTransactionMetaV2"}: true,
	{sdk.XDR, "ContractEvent"}:            true,
	{sdk.XDR, "DiagnosticEvent"}:          true,
	{sdk.XDR, "TransactionEvent"}:         true,
	{sdk.XDR, "TransactionResultMeta"}:    true,
	{sdk.XDR, "TransactionResultMetaV1"}:  true,
	{sdk.Ingest, "Change"}:                true,
	{sdk.Ingest, "TransactionEvents"}:     true,
}

// IsMeta reports whether t is a meta type, possibly behind pointers,
// slices, arrays or maps. Every named type of the token_transfer processor
// counts: its events are derived from meta.
func IsMeta(t types.Type) bool {
	// a call such as tx.GetChanges() has a tuple type
	if tup, ok := t.(*types.Tuple); ok {
		for v := range tup.Variables() {
			if IsMeta(v.Type()) {
				return true
			}
		}

		return false
	}

	for range 8 {
		if t == nil {
			return false
		}

		t = types.Unalias(t)

		if pkg, name, ok := sdk.NamedIn(t); ok {
			if pkg == sdk.TokenTransfer || metaTypes[[2]string{pkg, name}] {
				return true
			}
		}

		switch u := t.(type) {
		case *types.Pointer:
			t = u.Elem()
		case *types.Slice:
			t = u.Elem()
		case *types.Array:
			t = u.Elem()
		case *types.Map:
			t = u.Elem()
		default:
			return false
		}
	}

	return false
}
