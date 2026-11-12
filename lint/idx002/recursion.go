package idx002

import (
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"slices"
	"strings"

	"golang.org/x/tools/go/analysis"

	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
)

// recursiveTypes are the xdr types whose values can nest without a limit
// other than the decoder's depth limit.
var recursiveTypes = map[string]bool{
	"ScVal":                       true,
	"ScVec":                       true,
	"ScMap":                       true,
	"ScMapEntry":                  true,
	"SorobanAuthorizedInvocation": true,
	"ClaimPredicate":              true,
	"SorobanDelegateSignature":    true,
}

// fnode is one function or closure in the package call graph.
type fnode struct {
	obj    types.Object // *types.Func for a declaration, *types.Var for a closure
	name   string
	pos    token.Pos
	decl   *ast.FuncDecl // enclosing declaration, for directives and baselines
	types  []*ast.FuncType
	recv   *ast.FieldList
	bodies []*ast.BlockStmt
	calls  []callSite
}

type callSite struct {
	callee types.Object
	call   *ast.CallExpr
}

type graph struct {
	info  *types.Info
	pkg   *types.Package
	nodes map[types.Object]*fnode
	order []*fnode
	bound map[*ast.FuncLit]bool
}

func checkRecursion(pass *analysis.Pass, rep *suppress.Reporter) {
	g := &graph{info: pass.TypesInfo, pkg: pass.Pkg, nodes: map[types.Object]*fnode{}, bound: map[*ast.FuncLit]bool{}}

	for _, f := range pass.Files {
		if sdk.IsTestFile(pass, f) {
			continue
		}

		for _, d := range f.Decls {
			fd, ok := d.(*ast.FuncDecl)
			if !ok || fd.Body == nil {
				continue
			}

			obj := pass.TypesInfo.Defs[fd.Name]
			if obj == nil {
				continue
			}

			n := g.node(obj, sdk.FuncName(fd), fd.Name.Pos(), fd)
			n.types = append(n.types, fd.Type)
			n.recv = fd.Recv
			g.collect(n, fd, fd.Body)
		}
	}

	for _, scc := range g.sccs() {
		if !g.recursive(scc) {
			continue
		}

		typeName, at := g.recursiveParam(scc)
		if at == nil {
			continue
		}

		if g.bounded(scc) {
			continue
		}

		rep.Report(at.pos, at.decl, recursionMessage(scc, at, typeName))
	}
}

func recursionMessage(scc []*fnode, at *fnode, typeName string) string {
	if len(scc) == 1 {
		return fmt.Sprintf("%s recurses over xdr.%s with no depth bound; carry a depth counter and stop at a fixed limit", at.name, typeName)
	}

	names := make([]string, 0, len(scc))
	for _, n := range scc {
		names = append(names, n.name)
	}

	slices.Sort(names)

	return fmt.Sprintf("%s recurse over xdr.%s with no depth bound; carry a depth counter and stop at a fixed limit", strings.Join(names, ", "), typeName)
}

func (g *graph) node(obj types.Object, name string, pos token.Pos, decl *ast.FuncDecl) *fnode {
	if n, ok := g.nodes[obj]; ok {
		return n
	}

	n := &fnode{obj: obj, name: name, pos: pos, decl: decl}
	g.nodes[obj] = n
	g.order = append(g.order, n)

	return n
}

// collect walks body as part of n. A function literal bound to a variable
// becomes a node of its own, because only a named closure can call itself;
// any other literal stays part of n.
func (g *graph) collect(n *fnode, decl *ast.FuncDecl, body *ast.BlockStmt) {
	n.bodies = append(n.bodies, body)

	ast.Inspect(body, func(x ast.Node) bool {
		switch x := x.(type) {
		case *ast.AssignStmt:
			g.closureAssign(decl, x.Lhs, x.Rhs)
		case *ast.ValueSpec:
			lhs := make([]ast.Expr, len(x.Names))
			for i, id := range x.Names {
				lhs[i] = id
			}

			g.closureAssign(decl, lhs, x.Values)
		case *ast.FuncLit:
			if g.boundLit(x) {
				return false
			}
		case *ast.CallExpr:
			if callee := g.callee(x); callee != nil {
				n.calls = append(n.calls, callSite{callee: callee, call: x})
			}
		}

		return true
	})
}

// closureAssign turns `f = func(...) {...}` into a closure node named f.
func (g *graph) closureAssign(decl *ast.FuncDecl, lhs, rhs []ast.Expr) {
	if len(lhs) != len(rhs) {
		return
	}

	for i, r := range rhs {
		lit, ok := ast.Unparen(r).(*ast.FuncLit)
		if !ok {
			continue
		}

		id, ok := lhs[i].(*ast.Ident)
		if !ok {
			continue
		}

		v, ok := g.info.ObjectOf(id).(*types.Var)
		if !ok {
			continue
		}

		name := id.Name + " (closure in " + sdk.FuncName(decl) + ")"
		c := g.node(v, name, id.Pos(), decl)
		c.types = append(c.types, lit.Type)
		g.collect(c, decl, lit.Body)
		g.bound[lit] = true
	}
}

func (g *graph) boundLit(lit *ast.FuncLit) bool {
	return g.bound[lit]
}

func (g *graph) callee(call *ast.CallExpr) types.Object {
	switch obj := sdk.Callee(g.info, call).(type) {
	case *types.Func:
		obj = obj.Origin()
		if obj.Pkg() != g.pkg {
			return nil
		}

		return obj
	case *types.Var:
		if obj.Pkg() != g.pkg || obj.IsField() {
			return nil
		}

		if _, ok := obj.Type().Underlying().(*types.Signature); !ok {
			return nil
		}

		return obj
	default:
		return nil
	}
}

// sccs returns the strongly connected components of the call graph in a
// stable order (Tarjan's algorithm).
func (g *graph) sccs() [][]*fnode {
	index := map[*fnode]int{}
	low := map[*fnode]int{}
	onStack := map[*fnode]bool{}

	var (
		stack []*fnode
		out   [][]*fnode
		next  int
	)

	var visit func(n *fnode)
	visit = func(n *fnode) {
		index[n] = next
		low[n] = next
		next++

		stack = append(stack, n)
		onStack[n] = true

		for _, c := range n.calls {
			m, ok := g.nodes[c.callee]
			if !ok {
				continue
			}

			if _, seen := index[m]; !seen {
				visit(m)
				low[n] = min(low[n], low[m])
			} else if onStack[m] {
				low[n] = min(low[n], index[m])
			}
		}

		if low[n] != index[n] {
			return
		}

		var scc []*fnode

		for {
			m := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			onStack[m] = false

			scc = append(scc, m)
			if m == n {
				break
			}
		}

		out = append(out, scc)
	}

	for _, n := range g.order {
		if _, seen := index[n]; !seen {
			visit(n)
		}
	}

	return out
}

func (g *graph) recursive(scc []*fnode) bool {
	if len(scc) > 1 {
		return true
	}

	n := scc[0]
	for _, c := range n.calls {
		if c.callee == n.obj {
			return true
		}
	}

	return false
}

// recursiveParam returns the recursive xdr type taken by a function of the
// component, and the first such function by position.
func (g *graph) recursiveParam(scc []*fnode) (string, *fnode) {
	var (
		best     *fnode
		bestType string
	)

	for _, n := range scc {
		lists := []*ast.FieldList{n.recv}
		for _, ft := range n.types {
			lists = append(lists, ft.Params)
		}

		for _, fl := range lists {
			if fl == nil {
				continue
			}

			for _, field := range fl.List {
				name, ok := recursiveXDR(g.info.TypeOf(field.Type))
				if !ok {
					continue
				}

				if best == nil || n.pos < best.pos {
					best, bestType = n, name
				}
			}
		}
	}

	return bestType, best
}

// recursiveXDR reports whether t is, or holds through pointers, slices,
// arrays or maps, one of the recursive xdr types.
func recursiveXDR(t types.Type) (string, bool) {
	for range 8 {
		if t == nil {
			return "", false
		}

		t = types.Unalias(t)

		if pkg, name, ok := sdk.NamedIn(t); ok && pkg == sdk.XDR && recursiveTypes[name] {
			return name, true
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
			return "", false
		}
	}

	return "", false
}

// bounded reports whether some integer that survives from one call to the
// next is both compared and stepped inside the component: a depth parameter
// passed as depth+1 and checked against a limit, or a captured or field
// counter incremented and checked. Locals declared inside the component,
// such as loop indexes, do not count. Which way the comparison goes is not
// checked; a comparison that never stops anything is a bug the rule does
// not try to find.
func (g *graph) bounded(scc []*fnode) bool {
	members := map[types.Object]bool{}
	params := map[types.Object]bool{}

	for _, n := range scc {
		members[n.obj] = true

		lists := []*ast.FieldList{n.recv}
		for _, ft := range n.types {
			lists = append(lists, ft.Params)
		}

		for _, fl := range lists {
			if fl == nil {
				continue
			}

			for _, field := range fl.List {
				for _, id := range field.Names {
					if obj := g.info.Defs[id]; obj != nil {
						params[obj] = true
					}
				}
			}
		}
	}

	compared := map[types.Object]bool{}
	stepped := map[types.Object]bool{}

	for _, n := range scc {
		for _, body := range n.bodies {
			g.scanBounds(body, members, compared, stepped)
		}
	}

	for obj := range compared {
		if stepped[obj] && (params[obj] || !declaredWithin(obj, scc)) {
			return true
		}
	}

	return false
}

// declaredWithin reports whether obj is a local of one of the component's
// bodies, which is reset on every call and so cannot bound the recursion.
func declaredWithin(obj types.Object, scc []*fnode) bool {
	if v, ok := obj.(*types.Var); ok && v.IsField() {
		return false
	}

	for _, n := range scc {
		for _, body := range n.bodies {
			if body.Pos() <= obj.Pos() && obj.Pos() < body.End() {
				return true
			}
		}
	}

	return false
}

func (g *graph) scanBounds(body *ast.BlockStmt, members, compared, stepped map[types.Object]bool) {
	aliases := g.arithAliases(body)

	ast.Inspect(body, func(x ast.Node) bool {
		switch x := x.(type) {
		case *ast.FuncLit:
			// a closure node is scanned as its own body
			return !g.boundLit(x)
		case *ast.BinaryExpr:
			if sdk.IsComparison(x.Op) {
				g.intVars(x.X, compared)
				g.intVars(x.Y, compared)
			}
		case *ast.IncDecStmt:
			g.intVars(x.X, stepped)
		case *ast.AssignStmt:
			g.assignSteps(x, stepped)
		case *ast.CallExpr:
			if c := g.callee(x); c != nil && members[c] {
				for _, a := range x.Args {
					g.arithVars(a, stepped)
					g.aliasVars(a, aliases, stepped)
				}
			}
		}

		return true
	})
}

// aliasVars marks the variables behind a local such as next in
// `next := depth + 1; walk(v, next)`.
func (g *graph) aliasVars(e ast.Expr, aliases map[types.Object]map[types.Object]bool, into map[types.Object]bool) {
	id, ok := ast.Unparen(e).(*ast.Ident)
	if !ok {
		return
	}

	for obj := range aliases[g.info.Uses[id]] {
		into[obj] = true
	}
}

func (g *graph) assignSteps(as *ast.AssignStmt, stepped map[types.Object]bool) {
	switch as.Tok {
	case token.ADD_ASSIGN, token.SUB_ASSIGN:
		for _, l := range as.Lhs {
			g.intVars(l, stepped)
		}
	case token.ASSIGN, token.DEFINE:
		if len(as.Lhs) != len(as.Rhs) {
			return
		}

		for i, l := range as.Lhs {
			lhs := map[types.Object]bool{}
			g.intVars(l, lhs)

			rhs := map[types.Object]bool{}
			g.arithVars(as.Rhs[i], rhs)

			for obj := range lhs {
				if rhs[obj] {
					stepped[obj] = true
				}
			}
		}
	}
}

// arithAliases maps each local assigned an addition or subtraction to the
// integer variables in it.
func (g *graph) arithAliases(body *ast.BlockStmt) map[types.Object]map[types.Object]bool {
	aliases := map[types.Object]map[types.Object]bool{}

	ast.Inspect(body, func(x ast.Node) bool {
		as, ok := x.(*ast.AssignStmt)
		if !ok || len(as.Lhs) != len(as.Rhs) {
			return true
		}

		for i, l := range as.Lhs {
			id, ok := l.(*ast.Ident)
			if !ok {
				continue
			}

			vars := map[types.Object]bool{}
			g.arithVars(as.Rhs[i], vars)

			if obj := g.info.ObjectOf(id); obj != nil && len(vars) > 0 {
				aliases[obj] = vars
			}
		}

		return true
	})

	return aliases
}

// intVars collects the integer variables and fields named in e.
func (g *graph) intVars(e ast.Expr, into map[types.Object]bool) {
	ast.Inspect(e, func(x ast.Node) bool {
		switch x := x.(type) {
		case *ast.FuncLit:
			return false
		case *ast.Ident:
			if v, ok := g.info.Uses[x].(*types.Var); ok && sdk.IsInteger(v.Type()) {
				into[v] = true
			}
		}

		return true
	})
}

// arithVars collects the integer variables that appear under an addition
// or subtraction in e, as in depth+1 or remaining-1.
func (g *graph) arithVars(e ast.Expr, into map[types.Object]bool) {
	ast.Inspect(e, func(x ast.Node) bool {
		switch x := x.(type) {
		case *ast.FuncLit:
			return false
		case *ast.BinaryExpr:
			if x.Op == token.ADD || x.Op == token.SUB {
				g.intVars(x.X, into)
				g.intVars(x.Y, into)
			}
		}

		return true
	})
}
