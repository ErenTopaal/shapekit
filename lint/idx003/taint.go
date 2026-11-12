package idx003

import (
	"go/ast"
	"go/token"
	"go/types"
	"slices"

	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
)

// taint follows envelope addresses through one function and its closures.
// It is flow-insensitive: a variable that ever holds an envelope-derived
// value is treated as holding one everywhere. Values flow through
// assignments, range loops, field and index reads, conversions, composite
// literals, method calls on a tainted receiver (x.Address()), calls with a
// tainted argument, calls to functions whose result is envelope-derived,
// and into a variable when a method is called on it with a tainted
// argument (set.Add(x)).
type taint struct {
	a    *analysis3
	info *types.Info
	u    *unit

	vars           map[types.Object]string
	closureParams  map[types.Object][]types.Object
	closureReturns map[types.Object]string
	litOwner       map[*ast.FuncLit]types.Object
	returned       string
	changed        bool
}

type sink struct {
	pos    token.Pos
	source string
	// param is the index of the parameter whose collection the value is
	// written into, or -1. A helper that fills a caller's set is judged at
	// the call site, where the caller may well read meta.
	param int
}

func (a *analysis3) taint(u *unit) *taint {
	t := &taint{
		a:              a,
		info:           a.pass.TypesInfo,
		u:              u,
		vars:           map[types.Object]string{},
		closureParams:  map[types.Object][]types.Object{},
		closureReturns: map[types.Object]string{},
		litOwner:       map[*ast.FuncLit]types.Object{},
	}

	t.findClosures()

	// a handful of rounds covers flows that run backwards through the text,
	// such as a closure defined before the variables it is called with
	for range 8 {
		if !t.round() {
			break
		}
	}

	return t
}

func (t *taint) findClosures() {
	ast.Inspect(t.u.decl.Body, func(x ast.Node) bool {
		as, ok := x.(*ast.AssignStmt)
		if !ok || len(as.Lhs) != len(as.Rhs) {
			return true
		}

		for i, r := range as.Rhs {
			lit, ok := ast.Unparen(r).(*ast.FuncLit)
			if !ok {
				continue
			}

			id, ok := as.Lhs[i].(*ast.Ident)
			if !ok {
				continue
			}

			obj := t.info.ObjectOf(id)
			if obj == nil {
				continue
			}

			t.litOwner[lit] = obj

			var params []types.Object

			for _, field := range lit.Type.Params.List {
				for _, name := range field.Names {
					params = append(params, t.info.Defs[name])
				}
			}

			t.closureParams[obj] = params
		}

		return true
	})
}

// round makes one pass over the function and reports whether anything new
// became tainted.
func (t *taint) round() bool {
	t.changed = false

	var lits []*ast.FuncLit

	ast.Inspect(t.u.decl.Body, func(x ast.Node) bool {
		switch x := x.(type) {
		case *ast.FuncLit:
			lits = append(lits, x)
		case *ast.AssignStmt:
			t.assign(x.Lhs, x.Rhs)
		case *ast.ValueSpec:
			lhs := make([]ast.Expr, len(x.Names))
			for i, id := range x.Names {
				lhs[i] = id
			}

			t.assign(lhs, x.Values)
		case *ast.RangeStmt:
			if src := t.expr(x.X); src != "" {
				if x.Key != nil {
					t.mark(x.Key, src)
				}

				if x.Value != nil {
					t.mark(x.Value, src)
				}
			}
		case *ast.CallExpr:
			t.call(x)
		case *ast.ReturnStmt:
			t.returnFrom(x, lits)
		}

		return true
	})

	return t.changed
}

// mark taints the variable an assignment target writes to: x itself, or
// the root variable of x.f, x[i] and *x.
func (t *taint) mark(e ast.Expr, src string) {
	if id, ok := ast.Unparen(e).(*ast.Ident); ok {
		t.markObj(t.info.ObjectOf(id), src)

		return
	}

	t.markObj(sdk.Root(t.info, e), src)
}

func (t *taint) markObj(obj types.Object, src string) {
	if src == "" || obj == nil || obj.Name() == "_" {
		return
	}

	if _, done := t.vars[obj]; !done {
		t.vars[obj] = src
		t.changed = true
	}
}

func (t *taint) assign(lhs, rhs []ast.Expr) {
	switch {
	case len(lhs) == len(rhs):
		for i, l := range lhs {
			t.mark(l, t.expr(rhs[i]))

			if ix, ok := l.(*ast.IndexExpr); ok {
				t.mark(ix, t.expr(ix.Index))
			}
		}
	case len(rhs) == 1:
		// for a, err := f() only the non-error results carry the value
		src := t.expr(rhs[0])
		for _, l := range lhs {
			if !isError(t.info.TypeOf(l)) {
				t.mark(l, src)
			}
		}
	}
}

func (t *taint) call(call *ast.CallExpr) {
	callee := sdk.Callee(t.info, call)

	if params, ok := t.closureParams[callee]; ok {
		for i, arg := range call.Args {
			if i < len(params) {
				t.markObj(params[i], t.expr(arg))
			}
		}
	}

	sel, ok := ast.Unparen(call.Fun).(*ast.SelectorExpr)
	if !ok {
		return
	}

	if s, ok := t.info.Selections[sel]; !ok || s.Kind() != types.MethodVal {
		return
	}

	for _, arg := range call.Args {
		if src := t.expr(arg); src != "" {
			t.mark(sel.X, src)

			return
		}
	}
}

// returnFrom records a tainted return, either from the function itself or
// from a named closure inside it.
func (t *taint) returnFrom(ret *ast.ReturnStmt, lits []*ast.FuncLit) {
	var inner *ast.FuncLit

	for _, lit := range lits {
		if lit.Pos() <= ret.Pos() && ret.End() <= lit.End() && (inner == nil || lit.Pos() > inner.Pos()) {
			inner = lit
		}
	}

	for _, r := range ret.Results {
		src := t.expr(r)
		if src == "" || isError(t.info.TypeOf(r)) {
			continue
		}

		if inner == nil {
			if t.returned == "" {
				t.returned = src
				t.changed = true
			}

			return
		}

		if owner, ok := t.litOwner[inner]; ok && t.closureReturns[owner] == "" {
			t.closureReturns[owner] = src
			t.changed = true
		}

		return
	}
}

// expr returns the envelope accessor e is derived from, or "".
func (t *taint) expr(e ast.Expr) string {
	switch x := ast.Unparen(e).(type) {
	case *ast.Ident:
		if obj := t.info.Uses[x]; obj != nil {
			return t.vars[obj]
		}

		return t.vars[t.info.Defs[x]]
	case *ast.SelectorExpr:
		if src := t.source(x); src != "" {
			return src
		}

		if s, ok := t.info.Selections[x]; ok && s.Kind() == types.FieldVal {
			return t.expr(x.X)
		}
	case *ast.CallExpr:
		return t.callExpr(x)
	case *ast.CompositeLit:
		for _, el := range x.Elts {
			if kv, ok := el.(*ast.KeyValueExpr); ok {
				if src := t.expr(kv.Value); src != "" {
					return src
				}

				if src := t.expr(kv.Key); src != "" {
					return src
				}

				continue
			}

			if src := t.expr(el); src != "" {
				return src
			}
		}
	case *ast.IndexExpr:
		return t.expr(x.X)
	case *ast.SliceExpr:
		return t.expr(x.X)
	case *ast.StarExpr:
		return t.expr(x.X)
	case *ast.UnaryExpr:
		return t.expr(x.X)
	case *ast.TypeAssertExpr:
		return t.expr(x.X)
	case *ast.BinaryExpr:
		if sdk.IsComparison(x.Op) || x.Op == token.LAND || x.Op == token.LOR {
			return ""
		}

		if src := t.expr(x.X); src != "" {
			return src
		}

		return t.expr(x.Y)
	}

	return ""
}

func (t *taint) callExpr(call *ast.CallExpr) string {
	if sdk.IsConversion(t.info, call) {
		if len(call.Args) == 1 {
			return t.expr(call.Args[0])
		}

		return ""
	}

	if id, ok := ast.Unparen(call.Fun).(*ast.Ident); ok {
		if b, ok := t.info.Uses[id].(*types.Builtin); ok {
			if b.Name() != "append" {
				return ""
			}

			for _, arg := range call.Args {
				if src := t.expr(arg); src != "" {
					return src
				}
			}

			return ""
		}
	}

	if sel, ok := ast.Unparen(call.Fun).(*ast.SelectorExpr); ok {
		if src := t.source(sel); src != "" {
			return src
		}

		if s, ok := t.info.Selections[sel]; ok && s.Kind() == types.MethodVal {
			if src := t.expr(sel.X); src != "" {
				return src
			}
		}
	}

	switch callee := sdk.Callee(t.info, call).(type) {
	case *types.Func:
		if src := t.a.envelopeSource(callee.Origin()); src != "" {
			return src
		}
	case *types.Var:
		if src := t.closureReturns[callee]; src != "" {
			return src
		}
	}

	for _, arg := range call.Args {
		if src := t.expr(arg); src != "" {
			return src
		}
	}

	return ""
}

// source reports whether sel is an envelope accessor.
func (t *taint) source(sel *ast.SelectorExpr) string {
	s, ok := t.info.Selections[sel]
	if !ok {
		return ""
	}

	pkg, typ, ok := sdk.NamedIn(t.info.TypeOf(sel.X))
	if !ok {
		return ""
	}

	name := sel.Sel.Name
	if envelopeAccessors[member{pkg, typ, name}] {
		return accessorName(pkg, typ, name)
	}

	if pkg != sdk.XDR {
		return ""
	}

	var yields types.Type

	switch s.Kind() {
	case types.FieldVal:
		yields = s.Type()
	case types.MethodVal:
		if sig, ok := s.Type().(*types.Signature); ok && sig.Results().Len() > 0 {
			yields = sig.Results().At(0).Type()
		}
	default:
		return ""
	}

	if p, n, ok := sdk.NamedIn(yields); ok && p == sdk.XDR && preimageTypes[n] {
		return accessorName(pkg, typ, name)
	}

	return ""
}

// sinks returns where the function puts envelope-derived values into an
// account set, in source order.
func (t *taint) sinks() []sink {
	var out []sink

	params := map[types.Object]int{}

	for i, p := range paramList(t.info, t.u.decl.Type) {
		if p != nil {
			params[p] = i
		}
	}

	add := func(pos token.Pos, src string, into ast.Expr) {
		if src == "" {
			return
		}

		s := sink{pos: pos, source: src, param: -1}

		if into != nil {
			if i, ok := params[sdk.Root(t.info, into)]; ok {
				s.param = i
			}
		}

		out = append(out, s)
	}

	gated := accountName.MatchString(t.u.decl.Name.Name)

	var lits []*ast.FuncLit

	ast.Inspect(t.u.decl.Body, func(x ast.Node) bool {
		switch x := x.(type) {
		case *ast.FuncLit:
			lits = append(lits, x)
		case *ast.ReturnStmt:
			if gated && !insideAny(x, lits) {
				for _, r := range x.Results {
					if isAccountSet(t.info.TypeOf(r)) {
						add(r.Pos(), t.expr(r), nil)
					}
				}
			}
		case *ast.AssignStmt:
			t.assignSinks(x, gated, add)
		case *ast.CallExpr:
			t.callSinks(x, gated, add)
			t.helperSinks(x, add)
		case *ast.CompositeLit:
			// building an xdr value, such as a Transaction to hash, puts
			// accounts into xdr fields named for accounts; that is not
			// an account set
			if pkg, _, ok := sdk.NamedIn(t.info.TypeOf(x)); ok && sdk.IsSDKPackage(pkg) {
				return true
			}

			for _, el := range x.Elts {
				kv, ok := el.(*ast.KeyValueExpr)
				if !ok {
					continue
				}

				if k, ok := kv.Key.(*ast.Ident); ok && accountName.MatchString(k.Name) && t.holdsAccounts(kv.Value) {
					add(kv.Value.Pos(), t.expr(kv.Value), nil)
				}
			}
		}

		return true
	})

	slices.SortFunc(out, func(a, b sink) int { return int(a.pos - b.pos) })

	return out
}

func insideAny(n ast.Node, lits []*ast.FuncLit) bool {
	for _, lit := range lits {
		if lit.Pos() <= n.Pos() && n.End() <= lit.End() {
			return true
		}
	}

	return false
}

func (t *taint) holdsAccounts(e ast.Expr) bool {
	typ := t.info.TypeOf(e)

	return isAccount(typ) || isAccountSet(typ)
}

func (t *taint) assignSinks(as *ast.AssignStmt, gated bool, add func(token.Pos, string, ast.Expr)) {
	for i, l := range as.Lhs {
		if i >= len(as.Rhs) {
			break
		}

		r := as.Rhs[i]

		// xs = append(xs, v...)
		if call, ok := ast.Unparen(r).(*ast.CallExpr); ok && sdk.IsBuiltin(t.info, call, "append") && len(call.Args) > 1 {
			if isAccountSet(t.info.TypeOf(call.Args[0])) && (gated || namedForAccounts(call.Args[0])) {
				for _, v := range call.Args[1:] {
					add(v.Pos(), t.expr(v), call.Args[0])
				}
			}

			continue
		}

		switch l := l.(type) {
		case *ast.IndexExpr:
			// m[k] = v
			if _, isMap := t.info.TypeOf(l.X).Underlying().(*types.Map); isMap &&
				isAccountSet(t.info.TypeOf(l.X)) && (gated || namedForAccounts(l.X)) {
				add(l.Index.Pos(), t.expr(l.Index), l.X)
				add(r.Pos(), t.expr(r), l.X)
			}
		case *ast.SelectorExpr:
			// x.Participants = v, but not tx.SourceAccount = v on an xdr value
			pkg, _, xdrValue := sdk.NamedIn(t.info.TypeOf(l.X))
			if accountName.MatchString(l.Sel.Name) && t.holdsAccounts(r) && (!xdrValue || !sdk.IsSDKPackage(pkg)) {
				add(r.Pos(), t.expr(r), nil)
			}
		}
	}
}

func (t *taint) callSinks(call *ast.CallExpr, gated bool, add func(token.Pos, string, ast.Expr)) {
	sel, ok := ast.Unparen(call.Fun).(*ast.SelectorExpr)
	if !ok {
		return
	}

	name := sel.Sel.Name

	// set.Add(v), participants.Union(other)
	if s, ok := t.info.Selections[sel]; ok && s.Kind() == types.MethodVal && addMethods[name] &&
		isAccountSet(t.info.TypeOf(sel.X)) && (gated || namedForAccounts(sel.X)) {
		for _, arg := range call.Args {
			add(arg.Pos(), t.expr(arg), sel.X)
		}
	}

	// builder.WithAccount(v), builder.WithCreator(v)
	if builderName.MatchString(name) {
		for _, arg := range call.Args {
			if isAccount(t.info.TypeOf(arg)) {
				add(arg.Pos(), t.expr(arg), nil)
			}
		}
	}
}

// helperSinks treats passing a set to a helper that fills it with envelope
// addresses as writing those addresses here.
func (t *taint) helperSinks(call *ast.CallExpr, add func(token.Pos, string, ast.Expr)) {
	fn, ok := sdk.Callee(t.info, call).(*types.Func)
	if !ok {
		return
	}

	fills := t.a.fillsOf(fn.Origin())

	for i, arg := range call.Args {
		if i < len(fills) && fills[i] != "" {
			add(arg.Pos(), fills[i], arg)
		}
	}
}

func paramList(info *types.Info, ft *ast.FuncType) []types.Object {
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

// namedForAccounts reports whether the variable or field an expression
// names mentions participants or accounts.
func namedForAccounts(e ast.Expr) bool {
	switch x := ast.Unparen(e).(type) {
	case *ast.Ident:
		return accountName.MatchString(x.Name)
	case *ast.SelectorExpr:
		return accountName.MatchString(x.Sel.Name)
	case *ast.StarExpr:
		return namedForAccounts(x.X)
	case *ast.IndexExpr:
		return namedForAccounts(x.X)
	default:
		return false
	}
}

var errorType = types.Universe.Lookup("error").Type()

func isError(t types.Type) bool {
	return t != nil && types.Identical(t, errorType)
}
