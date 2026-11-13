package idx001

import (
	"go/ast"
	"go/token"
	"go/types"
	"reflect"
	"regexp"
	"slices"

	"github.com/ErenTopaal/shapekit/lint/internal/fieldtable"
	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
)

// label says which row a value carries and which variables it passed
// through, so a check on any of them clears it.
type label struct {
	row   fieldtable.Field
	chain []types.Object
}

func (l label) ok() bool { return l.row.ID != "" }

func (l label) mux() bool { return muxRows[l.row.ID] }

// muxRows are the rows that list MuxedAccount.GetId.
var muxRows = func() map[string]bool {
	out := map[string]bool{}

	for _, f := range fieldtable.Fields {
		if slices.Contains(f.Accessors, fieldtable.Accessor{Type: "MuxedAccount", Member: "GetId"}) {
			out[f.ID] = true
		}
	}

	return out
}()

// paramRow labels values derived from the parameter being followed when
// working out which parameters reach storage.
var paramRow = fieldtable.Field{ID: "parameter"}

var (
	sqlMethods = map[string]bool{
		"Exec": true, "ExecContext": true, "Query": true, "QueryContext": true,
		"QueryRow": true, "QueryRowContext": true, "CopyFrom": true, "Queue": true,
	}
	setMethods = map[string]bool{"Add": true, "Store": true, "Put": true, "Insert": true, "Union": true}
	newSetName = regexp.MustCompile(`^New\w*Set`)
	utf8Checks = map[[2]string]bool{
		{"unicode/utf8", "Valid"}:       true,
		{"unicode/utf8", "ValidString"}: true,
		{"strings", "ToValidUTF8"}:      true,
	}
)

type flow struct {
	c       *checker
	info    *types.Info
	fd      *ast.FuncDecl
	param   types.Object // set when following a parameter
	vars    map[types.Object]label
	params  map[types.Object][]types.Object
	checked map[types.Object]bool
	changed bool
}

func (c *checker) newFlow(fd *ast.FuncDecl, param types.Object) *flow {
	f := &flow{
		c:       c,
		info:    c.pass.TypesInfo,
		fd:      fd,
		param:   param,
		vars:    map[types.Object]label{},
		params:  map[types.Object][]types.Object{},
		checked: map[types.Object]bool{},
	}

	if param != nil {
		f.vars[param] = label{row: paramRow, chain: []types.Object{param}}
	}

	f.findClosures()

	return f
}

func (f *flow) findClosures() {
	ast.Inspect(f.fd.Body, func(x ast.Node) bool {
		as, ok := x.(*ast.AssignStmt)
		if !ok || len(as.Lhs) != len(as.Rhs) {
			return true
		}

		for i, r := range as.Rhs {
			lit, ok := ast.Unparen(r).(*ast.FuncLit)
			if !ok {
				continue
			}

			if id, ok := as.Lhs[i].(*ast.Ident); ok {
				if obj := f.info.ObjectOf(id); obj != nil {
					f.params[obj] = paramObjects(f.info, lit.Type)
				}
			}
		}

		return true
	})
}

func (f *flow) solve() {
	for range 8 {
		f.changed = false
		f.round()

		if !f.changed {
			break
		}
	}

	f.findChecks()
}

func (f *flow) round() {
	ast.Inspect(f.fd.Body, func(x ast.Node) bool {
		switch x := x.(type) {
		case *ast.AssignStmt:
			f.assign(x.Lhs, x.Rhs)
		case *ast.ValueSpec:
			lhs := make([]ast.Expr, len(x.Names))
			for i, id := range x.Names {
				lhs[i] = id
			}

			f.assign(lhs, x.Values)
		case *ast.RangeStmt:
			if l := f.expr(x.X); l.ok() {
				if x.Key != nil {
					f.mark(x.Key, l)
				}

				if x.Value != nil {
					f.mark(x.Value, l)
				}
			}
		case *ast.CallExpr:
			f.call(x)
		}

		return true
	})
}

func (f *flow) assign(lhs, rhs []ast.Expr) {
	switch {
	case len(lhs) == len(rhs):
		for i, l := range lhs {
			f.mark(l, f.expr(rhs[i]))
		}
	case len(rhs) == 1:
		l := f.expr(rhs[0])
		for _, e := range lhs {
			if t := f.info.TypeOf(e); t == nil || !types.Identical(t, types.Universe.Lookup("error").Type()) {
				f.mark(e, l)
			}
		}
	}
}

func (f *flow) mark(e ast.Expr, l label) {
	if !l.ok() {
		return
	}

	var obj types.Object
	if id, ok := ast.Unparen(e).(*ast.Ident); ok {
		obj = f.info.ObjectOf(id)
	} else {
		obj = sdk.Root(f.info, e)
	}

	f.markObj(obj, l)
}

func (f *flow) markObj(obj types.Object, l label) {
	if !l.ok() || obj == nil || obj.Name() == "_" {
		return
	}

	if _, done := f.vars[obj]; done {
		return
	}

	chain := append(slices.Clip(l.chain), obj)
	f.vars[obj] = label{row: l.row, chain: chain}
	f.changed = true
}

func (f *flow) call(call *ast.CallExpr) {
	if params, ok := f.params[sdk.Callee(f.info, call)]; ok {
		for i, arg := range call.Args {
			if i < len(params) {
				f.markObj(params[i], f.expr(arg))
			}
		}
	}

	sel, ok := ast.Unparen(call.Fun).(*ast.SelectorExpr)
	if !ok {
		return
	}

	if s, ok := f.info.Selections[sel]; !ok || s.Kind() != types.MethodVal {
		return
	}

	// set.Add(v) makes set carry v
	for _, arg := range call.Args {
		if l := f.expr(arg); l.ok() {
			f.mark(sel.X, l)

			return
		}
	}
}

// accessor returns the xdr accessor a selector names, if any.
func (f *flow) accessor(sel *ast.SelectorExpr) (fieldtable.Accessor, bool) {
	if _, ok := f.info.Selections[sel]; !ok {
		return fieldtable.Accessor{}, false
	}

	pkg, typ, ok := sdk.NamedIn(f.info.TypeOf(sel.X))
	if !ok || pkg != sdk.XDR {
		return fieldtable.Accessor{}, false
	}

	return fieldtable.Accessor{Type: typ, Member: sel.Sel.Name}, true
}

func (f *flow) source(sel *ast.SelectorExpr) label {
	if f.param != nil {
		return label{}
	}

	a, ok := f.accessor(sel)
	if !ok {
		return label{}
	}

	if row, ok := sourceRow[a]; ok {
		return label{row: row}
	}

	return label{}
}

// expr returns the label e carries, or an empty label.
func (f *flow) expr(e ast.Expr) label {
	switch x := ast.Unparen(e).(type) {
	case *ast.Ident:
		return f.vars[f.info.ObjectOf(x)]
	case *ast.SelectorExpr:
		if l := f.source(x); l.ok() {
			// keep what the value was read from, so a check on the
			// account, as in acct.MustMed25519().Id after a look at
			// acct.Type, clears the field read from it
			if inner := f.expr(x.X); inner.ok() {
				l.chain = inner.chain
			} else if r := sdk.Root(f.info, x.X); r != nil {
				l.chain = []types.Object{r}
			}

			return l
		}

		if s, ok := f.info.Selections[x]; ok && s.Kind() == types.FieldVal {
			return f.expr(x.X)
		}
	case *ast.CallExpr:
		return f.callExpr(x)
	case *ast.CompositeLit:
		for _, el := range x.Elts {
			if kv, ok := el.(*ast.KeyValueExpr); ok {
				el = kv.Value
			}

			if l := f.expr(el); l.ok() {
				return l
			}
		}
	case *ast.IndexExpr:
		return f.expr(x.X)
	case *ast.SliceExpr:
		return f.expr(x.X)
	case *ast.StarExpr:
		return f.expr(x.X)
	case *ast.UnaryExpr:
		return f.expr(x.X)
	case *ast.TypeAssertExpr:
		return f.expr(x.X)
	case *ast.BinaryExpr:
		if sdk.IsComparison(x.Op) || x.Op == token.LAND || x.Op == token.LOR {
			return label{}
		}

		if l := f.expr(x.X); l.ok() {
			return l
		}

		return f.expr(x.Y)
	}

	return label{}
}

func (f *flow) callExpr(call *ast.CallExpr) label {
	if sdk.IsConversion(f.info, call) {
		if len(call.Args) == 1 {
			return f.expr(call.Args[0])
		}

		return label{}
	}

	if id, ok := ast.Unparen(call.Fun).(*ast.Ident); ok {
		if b, ok := f.info.Uses[id].(*types.Builtin); ok {
			if b.Name() == "append" {
				for _, arg := range call.Args {
					if l := f.expr(arg); l.ok() {
						return l
					}
				}
			}

			return label{}
		}
	}

	if sel, ok := ast.Unparen(call.Fun).(*ast.SelectorExpr); ok {
		if l := f.source(sel); l.ok() {
			// the receiver is part of the chain, so a check on it, such as
			// a switch on a muxed account's Type, clears the result
			if r := sdk.Root(f.info, sel.X); r != nil {
				l.chain = []types.Object{r}
			}

			return l
		}

		if s, ok := f.info.Selections[sel]; ok && s.Kind() == types.MethodVal {
			recv := f.expr(sel.X)

			if a, isXDR := f.accessor(sel); isXDR && unmuxers[a] && recv.mux() {
				return label{}
			}

			if recv.ok() {
				return recv
			}
		}
	}

	for _, arg := range call.Args {
		if l := f.expr(arg); l.ok() {
			return l
		}
	}

	return label{}
}

// findChecks records the variables a recognized check looks at.
func (f *flow) findChecks() {
	readsMeta := false
	if obj, ok := f.info.Defs[f.fd.Name].(*types.Func); ok {
		readsMeta = f.c.meta.ReadsMeta(obj)
	}

	markChecked := func(e ast.Expr, want func(label) bool) {
		ast.Inspect(e, func(x ast.Node) bool {
			if ex, ok := x.(ast.Expr); ok {
				if l := f.expr(ex); l.ok() && want(l) {
					for _, o := range l.chain {
						f.checked[o] = true
					}

					if r := sdk.Root(f.info, ex); r != nil {
						f.checked[r] = true
					}
				}
			}

			return true
		})
	}

	all := func(label) bool { return true }
	declared := func(l label) bool { return !l.mux() }
	guards := f.muxGuards()

	ast.Inspect(f.fd.Body, func(x ast.Node) bool {
		switch x := x.(type) {
		case *ast.CallExpr:
			if fn, ok := sdk.Callee(f.info, x).(*types.Func); ok && fn.Pkg() != nil &&
				utf8Checks[[2]string{fn.Pkg().Path(), fn.Name()}] {
				for _, a := range x.Args {
					markChecked(a, all)
				}
			}
		case *ast.BinaryExpr:
			if x.Op != token.EQL && x.Op != token.NEQ {
				return true
			}

			for _, side := range []ast.Expr{x.X, x.Y} {
				f.muxTypeCheck(side)

				if readsMeta {
					markChecked(side, declared)
				}
			}
		case *ast.SwitchStmt:
			if x.Tag != nil {
				f.muxTypeCheck(x.Tag)
			}
		case *ast.IfStmt:
			f.guardCheck(x.Cond, guards)
		case *ast.IndexExpr:
			// a lookup in a map built from meta, such as created[contractID]
			if _, isMap := f.info.TypeOf(x.X).Underlying().(*types.Map); isMap && readsMeta {
				markChecked(x.Index, declared)
			}
		}

		return true
	})
}

// muxTypeCheck treats a look at a muxed account's Type as handling the mux
// id: code that branches on the key type has seen that there may be one.
func (f *flow) muxTypeCheck(e ast.Expr) {
	sel, ok := ast.Unparen(e).(*ast.SelectorExpr)
	if !ok || sel.Sel.Name != "Type" || !sdk.Is(f.info.TypeOf(sel.X), sdk.XDR, "MuxedAccount") {
		return
	}

	if r := sdk.Root(f.info, sel.X); r != nil {
		f.checked[r] = true
	}
}

// muxGuards maps the ok of GetMed25519 and the error of GetId to what they
// guard: the id and the account it came from. GetId returns 0 with an error
// for an account that is not muxed, so storing it without looking at the
// error invents a mux id of 0.
func (f *flow) muxGuards() map[types.Object][]types.Object {
	out := map[types.Object][]types.Object{}

	ast.Inspect(f.fd.Body, func(x ast.Node) bool {
		as, ok := x.(*ast.AssignStmt)
		if !ok || len(as.Lhs) != 2 || len(as.Rhs) != 1 {
			return true
		}

		call, ok := ast.Unparen(as.Rhs[0]).(*ast.CallExpr)
		if !ok {
			return true
		}

		sel, ok := ast.Unparen(call.Fun).(*ast.SelectorExpr)
		if !ok {
			return true
		}

		if a, ok := f.accessor(sel); !ok || a.Type != "MuxedAccount" || (a.Member != "GetMed25519" && a.Member != "GetId") {
			return true
		}

		g, ok := as.Lhs[1].(*ast.Ident)
		if !ok {
			return true
		}

		guard := f.info.ObjectOf(g)
		if guard == nil || guard.Name() == "_" {
			return true
		}

		var guarded []types.Object

		if id, ok := as.Lhs[0].(*ast.Ident); ok {
			if o := f.info.ObjectOf(id); o != nil {
				guarded = append(guarded, o)
			}
		}

		if r := sdk.Root(f.info, sel.X); r != nil {
			guarded = append(guarded, r)
		}

		out[guard] = guarded

		return true
	})

	return out
}

// guardCheck marks the values a guard protects when an if condition looks
// at the guard, as in `if ok` or `if err != nil`.
func (f *flow) guardCheck(cond ast.Expr, guards map[types.Object][]types.Object) {
	if len(guards) == 0 {
		return
	}

	ast.Inspect(cond, func(x ast.Node) bool {
		if id, ok := x.(*ast.Ident); ok {
			for _, o := range guards[f.info.ObjectOf(id)] {
				f.checked[o] = true
			}
		}

		return true
	})
}

func (f *flow) isChecked(l label, e ast.Expr) bool {
	for _, o := range l.chain {
		if f.checked[o] {
			return true
		}
	}

	if r := sdk.Root(f.info, e); r != nil && f.checked[r] {
		return true
	}

	return false
}

type sinkKind int

const (
	sinkKey sinkKind = iota
	sinkStore
	sinkConvert
)

type sinkHit struct {
	pos  token.Pos
	row  fieldtable.Field
	kind sinkKind
	what string
	verb string
}

// sinks lists where labeled values reach a key, storage or a lossy
// conversion, in source order.
func (f *flow) sinks() []sinkHit {
	var out []sinkHit

	hit := func(e ast.Expr, kind sinkKind, verb string) {
		l := f.expr(e)
		if !l.ok() || f.isChecked(l, e) {
			return
		}

		if kind != sinkConvert && !keyable(l, f.info.TypeOf(e)) {
			return
		}

		out = append(out, sinkHit{pos: e.Pos(), row: l.row, kind: kind, what: types.ExprString(e), verb: verb})
	}

	ast.Inspect(f.fd.Body, func(x ast.Node) bool {
		switch x := x.(type) {
		case *ast.AssignStmt:
			for i, lhs := range x.Lhs {
				switch l := lhs.(type) {
				case *ast.IndexExpr:
					if _, isMap := f.info.TypeOf(l.X).Underlying().(*types.Map); isMap {
						hit(l.Index, sinkKey, "is used as a map key")
					}
				case *ast.SelectorExpr:
					if i < len(x.Rhs) && f.dbField(l) {
						hit(x.Rhs[i], sinkStore, "is stored in db field "+l.Sel.Name)
					}
				}
			}
		case *ast.IncDecStmt:
			// m[k]++ stores under k just like m[k] = v
			if ix, ok := ast.Unparen(x.X).(*ast.IndexExpr); ok {
				if _, isMap := f.info.TypeOf(ix.X).Underlying().(*types.Map); isMap {
					hit(ix.Index, sinkKey, "is used as a map key")
				}
			}
		case *ast.CompositeLit:
			f.compositeSinks(x, hit)
		case *ast.CallExpr:
			f.callSinks(x, hit)
		}

		return true
	})

	slices.SortFunc(out, func(a, b sinkHit) int { return int(a.pos - b.pos) })

	return out
}

func (f *flow) reachesStorage() bool {
	for _, s := range f.sinks() {
		if s.storage() {
			return true
		}
	}

	return false
}

func (s sinkHit) storage() bool { return s.kind == sinkStore }

func (f *flow) compositeSinks(lit *ast.CompositeLit, hit func(ast.Expr, sinkKind, string)) {
	st, ok := sdk.Deref(f.info.TypeOf(lit)).Underlying().(*types.Struct)
	if !ok {
		return
	}

	for _, el := range lit.Elts {
		kv, ok := el.(*ast.KeyValueExpr)
		if !ok {
			continue
		}

		k, ok := kv.Key.(*ast.Ident)
		if !ok {
			continue
		}

		for i := range st.NumFields() {
			if st.Field(i).Name() == k.Name && dbTag(st.Tag(i)) {
				hit(kv.Value, sinkStore, "is stored in db field "+k.Name)
			}
		}
	}
}

func (f *flow) callSinks(call *ast.CallExpr, hit func(ast.Expr, sinkKind, string)) {
	if sdk.IsConversion(f.info, call) && len(call.Args) == 1 {
		to := f.info.TypeOf(call.Fun)
		from := f.info.TypeOf(call.Args[0])

		if b, ok := to.Underlying().(*types.Basic); ok && b.Info()&types.IsString != 0 && storable(from) && !isStringType(from) {
			hit(call.Args[0], sinkConvert, "is converted to a string")
		}

		if signedFromUnsigned64(to, from) {
			hit(call.Args[0], sinkConvert, "is converted to a signed integer")
		}

		return
	}

	callee := sdk.Callee(f.info, call)

	if fn, ok := callee.(*types.Func); ok {
		if newSetName.MatchString(fn.Name()) {
			for _, a := range call.Args {
				hit(a, sinkKey, "is used as a set key")
			}
		}

		for i, a := range call.Args {
			if f.c.paramPersists(fn, i) {
				hit(a, sinkStore, "is stored through "+fn.Name())
			}
		}
	}

	sel, ok := ast.Unparen(call.Fun).(*ast.SelectorExpr)
	if !ok {
		return
	}

	if s, ok := f.info.Selections[sel]; !ok || s.Kind() != types.MethodVal {
		return
	}

	name := sel.Sel.Name

	if sqlMethods[name] {
		for _, a := range call.Args {
			hit(a, sinkStore, "is passed to SQL")
		}
	}

	if setMethods[name] && collection(f.info.TypeOf(sel.X)) {
		for _, a := range call.Args {
			hit(a, sinkKey, "is used as a set key")
		}
	}
}

func isStringType(t types.Type) bool {
	b, ok := t.Underlying().(*types.Basic)

	return ok && b.Info()&types.IsString != 0
}

func signedFromUnsigned64(to, from types.Type) bool {
	tb, ok := to.Underlying().(*types.Basic)
	if !ok || tb.Info()&types.IsInteger == 0 || tb.Info()&types.IsUnsigned != 0 {
		return false
	}

	fb, ok := from.Underlying().(*types.Basic)

	return ok && fb.Kind() == types.Uint64
}

func collection(t types.Type) bool {
	if t == nil {
		return false
	}

	t = sdk.Deref(t)

	if n, ok := t.(*types.Named); ok && n.TypeArgs().Len() > 0 {
		return true
	}

	switch t.Underlying().(type) {
	case *types.Map, *types.Slice:
		return true
	default:
		return false
	}
}

// dbField reports whether a selector names a struct field with a db tag.
func (f *flow) dbField(sel *ast.SelectorExpr) bool {
	s, ok := f.info.Selections[sel]
	if !ok || s.Kind() != types.FieldVal || len(s.Index()) != 1 {
		return false
	}

	st, ok := sdk.Deref(s.Recv()).Underlying().(*types.Struct)
	if !ok {
		return false
	}

	return dbTag(st.Tag(s.Index()[0]))
}

func dbTag(tag string) bool {
	v, ok := reflect.StructTag(tag).Lookup("db")

	return ok && v != "-" && v != ""
}
