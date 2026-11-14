// Package idx001 implements indexerlint rule IDX001, unvalidated field
// trusted.
//
// The fields come from registry/fields.toml through the generated package
// fieldtable: every row stellar-core does not validate ("no") plus the
// partial rows listed in trustedPartial. A read of one of their go-stellar-sdk
// accessors, or of a carrier such as MuxedAccount.Address, is followed
// through the function. It is reported when it reaches:
//
//   - a key: a map store, a set Add/Store/Put/Insert/Union, a New...Set call;
//   - storage: a struct field with a db tag, a SQL Exec/Query/CopyFrom/Queue
//     call, or a function parameter that itself reaches storage (followed
//     across packages through facts);
//   - a lossy conversion: bytes to string, or an unsigned 64-bit value to a
//     signed integer;
//
// unless the function checks it first: utf8.Valid, utf8.ValidString or
// strings.ToValidUTF8 for any field, a comparison on the muxed account's
// Type for mux ids, and, in a function whose call tree reads transaction
// meta, a comparison or map lookup for declared auth and footprint data.
// MuxedAccount.ToAccountId drops the mux id. See docs/linter.md for what
// the rule cannot see.
package idx001

import (
	"errors"
	"fmt"
	"go/ast"
	"go/types"

	"golang.org/x/tools/go/analysis"

	"github.com/ErenTopaal/shapekit/lint/internal/metaread"
	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
	"github.com/ErenTopaal/shapekit/lint/rules"
)

// ID is the rule ID this analyzer reports under.
const ID = "IDX001"

// Analyzer is the IDX001 analyzer.
var Analyzer = &analysis.Analyzer{
	Name:       ID,
	Doc:        doc(),
	URL:        "https://github.com/ErenTopaal/shapekit/blob/main/docs/linter.md#idx001-unvalidated-field-trusted",
	Run:        run,
	ResultType: suppress.ResultType,
	FactTypes:  []analysis.Fact{new(persistsFact)},
	Requires:   []*analysis.Analyzer{metaread.Analyzer},
}

func doc() string {
	r, err := rules.Lookup(ID)
	if err != nil {
		panic(err)
	}

	return r.Name + ": " + r.Description
}

// persistsFact marks an exported function whose parameters, by index, reach
// storage.
type persistsFact struct {
	Params []bool
}

func (*persistsFact) AFact() {}

func (f *persistsFact) String() string { return fmt.Sprintf("persists%v", f.Params) }

type checker struct {
	pass     *analysis.Pass
	meta     *metaread.Result
	decls    []*ast.FuncDecl
	persists map[*types.Func][]bool
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

	c := &checker{pass: pass, meta: meta, persists: map[*types.Func][]bool{}}

	for _, f := range pass.Files {
		if sdk.IsTestFile(pass, f) {
			continue
		}

		for _, d := range f.Decls {
			if fd, ok := d.(*ast.FuncDecl); ok && fd.Body != nil {
				c.decls = append(c.decls, fd)
			}
		}
	}

	c.solvePersists()

	for _, fd := range c.decls {
		obj, _ := pass.TypesInfo.Defs[fd.Name].(*types.Func)
		if obj != nil && obj.Exported() && hasTrue(c.persists[obj]) {
			pass.ExportObjectFact(obj, &persistsFact{Params: c.persists[obj]})
		}
	}

	for _, fd := range c.decls {
		fl := c.newFlow(fd, nil)
		fl.solve()

		seen := map[string]bool{}

		for _, s := range fl.sinks() {
			if seen[s.row.ID] {
				continue
			}

			seen[s.row.ID] = true

			rep.Report(s.pos, fd, fmt.Sprintf("%s %s without a check, but it carries %s, which stellar-core validates: %s; %s",
				s.what, s.verb, s.row.ID, s.row.ValidatedByCore, s.row.Handling))
		}
	}

	return rep.Result(), nil
}

func hasTrue(bs []bool) bool {
	for _, b := range bs {
		if b {
			return true
		}
	}

	return false
}

// solvePersists works out which parameters of each function reach storage,
// repeating until callers of persisting functions settle.
func (c *checker) solvePersists() {
	for range len(c.decls) + 1 {
		changed := false

		for _, fd := range c.decls {
			obj, _ := c.pass.TypesInfo.Defs[fd.Name].(*types.Func)
			if obj == nil {
				continue
			}

			params := paramObjects(c.pass.TypesInfo, fd.Type)
			cur := c.persists[obj]

			if cur == nil {
				cur = make([]bool, len(params))
			}

			for i, p := range params {
				if cur[i] || p == nil || (!storable(p.Type()) && !isInteger(p.Type())) {
					continue
				}

				fl := c.newFlow(fd, p)
				fl.solve()

				if fl.reachesStorage() {
					cur[i] = true
					changed = true
				}
			}

			c.persists[obj] = cur
		}

		if !changed {
			return
		}
	}
}

// paramPersists reports whether argument i of a call to fn reaches storage.
func (c *checker) paramPersists(fn *types.Func, i int) bool {
	fn = fn.Origin()

	var params []bool

	if fn.Pkg() == c.pass.Pkg {
		params = c.persists[fn]
	} else {
		var f persistsFact
		if c.pass.ImportObjectFact(fn, &f) {
			params = f.Params
		}
	}

	return i < len(params) && params[i]
}

func paramObjects(info *types.Info, ft *ast.FuncType) []types.Object {
	var out []types.Object

	for _, field := range ft.Params.List {
		if len(field.Names) == 0 {
			out = append(out, nil)

			continue
		}

		for _, n := range field.Names {
			out = append(out, info.Defs[n])
		}
	}

	return out
}

// storable reports whether a value of type t is worth following into
// storage: strings, byte slices and arrays, and named struct types such as
// nullable column wrappers. Numbers go through keyable.
func storable(t types.Type) bool {
	if t == nil {
		return false
	}

	switch u := sdk.Deref(t).Underlying().(type) {
	case *types.Basic:
		return u.Info()&types.IsString != 0
	case *types.Slice:
		return isByte(u.Elem())
	case *types.Array:
		return isByte(u.Elem())
	case *types.Struct:
		_, named := sdk.Deref(t).(*types.Named)

		return named
	default:
		return false
	}
}

// keyable reports whether a value labeled l of type t counts at a key or
// storage sink. Integers count only for mux ids: a mux id is any 64-bit
// value the submitter picked, so keying or storing it without first
// looking at the account type treats a number with no account behind it as
// an identity. Other integers, such as a memo id, are only checked at
// conversions. Integer parameters are followed so a mux id stored through
// a helper is seen.
func keyable(l label, t types.Type) bool {
	if storable(t) {
		return true
	}

	return isInteger(t) && (l.mux() || l.row.ID == paramRow.ID)
}

func isInteger(t types.Type) bool {
	if t == nil {
		return false
	}

	b, ok := t.Underlying().(*types.Basic)

	return ok && b.Info()&types.IsInteger != 0
}

func isByte(t types.Type) bool {
	b, ok := t.Underlying().(*types.Basic)

	return ok && b.Kind() == types.Byte
}
