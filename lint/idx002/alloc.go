package idx002

import (
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"slices"

	"golang.org/x/tools/go/analysis"

	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
)

// allocScan tracks, inside one function, which locals hold an integer
// decoded from xdr, and which expressions the function compares.
type allocScan struct {
	info *types.Info
	// decoded maps a local to the source expressions it was derived from
	decoded map[types.Object][]string
	checks  []check
}

// check is one operand (or a variable or selector inside one) of an ordered
// comparison: where the comparison is, and the blocks around it, so that
// only a comparison that can run before a make counts for it.
type check struct {
	key    string
	pos    token.Pos
	blocks []ast.Node
}

// identKey names a variable by its declaration, so that two variables of one
// name in different scopes are told apart.
func identKey(name string, obj types.Object) string {
	if obj == nil {
		return name
	}

	return fmt.Sprintf("%s@%d", name, obj.Pos())
}

func checkAllocations(pass *analysis.Pass, rep *suppress.Reporter) {
	for _, f := range pass.Files {
		if sdk.IsTestFile(pass, f) {
			continue
		}

		for _, d := range f.Decls {
			fd, ok := d.(*ast.FuncDecl)
			if !ok || fd.Body == nil {
				continue
			}

			s := &allocScan{info: pass.TypesInfo, decoded: map[types.Object][]string{}}
			s.propagate(fd.Body)
			s.collectChecks(fd.Body)
			s.report(rep, fd)
		}
	}
}

// xdrInt reports whether e has a named integer type declared in the xdr
// package (xdr.Uint32, xdr.Int64 and so on) and is not a constant.
func (s *allocScan) xdrInt(e ast.Expr) bool {
	tv, ok := s.info.Types[e]
	if !ok || tv.Value != nil || tv.IsType() {
		return false
	}

	pkg, _, named := sdk.NamedIn(tv.Type)
	if !named || pkg != sdk.XDR {
		return false
	}

	_, isPtr := types.Unalias(tv.Type).(*types.Pointer)

	return !isPtr && sdk.IsInteger(tv.Type)
}

// sources returns the decoded-integer sources an expression is built from,
// as printed expressions. Calls other than conversions stop the flow: a
// function may well bound its input, and guessing otherwise would be noise.
func (s *allocScan) sources(e ast.Expr) []string {
	switch x := ast.Unparen(e).(type) {
	case *ast.Ident:
		if obj := s.info.Uses[x]; obj != nil {
			if src, ok := s.decoded[obj]; ok {
				return append([]string{identKey(x.Name, obj)}, src...)
			}
		}
	case *ast.BinaryExpr:
		switch x.Op {
		case token.ADD, token.SUB, token.MUL, token.SHL, token.OR:
			return append(s.sources(x.X), s.sources(x.Y)...)
		}
	case *ast.CallExpr:
		if sdk.IsConversion(s.info, x) && len(x.Args) == 1 {
			return s.sources(x.Args[0])
		}
	case *ast.StarExpr:
		return s.sources(x.X)
	}

	if s.xdrInt(e) {
		return []string{types.ExprString(e)}
	}

	return nil
}

func (s *allocScan) propagate(body *ast.BlockStmt) {
	// a few passes reach assignments that appear before their sources in
	// the text, such as inside loops
	for range 4 {
		changed := false

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

				obj := s.info.ObjectOf(id)
				if obj == nil {
					continue
				}

				if _, done := s.decoded[obj]; done {
					continue
				}

				if src := s.sources(as.Rhs[i]); len(src) > 0 {
					s.decoded[obj] = src
					changed = true
				}
			}

			return true
		})

		if !changed {
			return
		}
	}
}

// collectChecks records every expression that is an operand of an ordered
// comparison, plus every variable inside one, with the blocks the
// comparison sits in.
func (s *allocScan) collectChecks(body *ast.BlockStmt) {
	var stack []ast.Node

	ast.Inspect(body, func(x ast.Node) bool {
		if x == nil {
			stack = stack[:len(stack)-1]

			return true
		}

		stack = append(stack, x)

		be, ok := x.(*ast.BinaryExpr)
		if !ok {
			return true
		}

		switch be.Op {
		case token.LSS, token.LEQ, token.GTR, token.GEQ:
		default:
			return true
		}

		var blocks []ast.Node

		for _, n := range stack {
			switch n := n.(type) {
			case *ast.BlockStmt:
				blocks = append(blocks, n)
			case *ast.CaseClause:
				blocks = clause(blocks, n, n.Colon, be.Pos())
			case *ast.CommClause:
				blocks = clause(blocks, n, n.Colon, be.Pos())
			}
		}

		add := func(key string) {
			s.checks = append(s.checks, check{key: key, pos: be.Pos(), blocks: blocks})
		}

		for _, side := range []ast.Expr{be.X, be.Y} {
			if _, isIdent := ast.Unparen(side).(*ast.Ident); !isIdent {
				add(types.ExprString(side))
			}

			ast.Inspect(side, func(y ast.Node) bool {
				switch y := y.(type) {
				case *ast.Ident:
					add(identKey(y.Name, s.info.ObjectOf(y)))
				case *ast.SelectorExpr:
					add(types.ExprString(y))
				case *ast.CallExpr:
					add(types.ExprString(y))
				}

				return true
			})
		}

		return true
	})
}

func (s *allocScan) report(rep *suppress.Reporter, fd *ast.FuncDecl) {
	ast.Inspect(fd.Body, func(x ast.Node) bool {
		call, ok := x.(*ast.CallExpr)
		if !ok || !sdk.IsBuiltin(s.info, call, "make") || len(call.Args) < 2 {
			return true
		}

		for _, size := range call.Args[1:] {
			if c, ok := ast.Unparen(size).(*ast.CallExpr); ok && sdk.IsBuiltin(s.info, c, "min") {
				continue
			}

			src := s.sources(size)
			if len(src) == 0 || s.anyChecked(src, call) {
				continue
			}

			rep.Report(size.Pos(), fd, fmt.Sprintf(
				"make sized by %s, an integer decoded from xdr (%s), with no bound check; compare it against a limit before allocating",
				types.ExprString(size), src[len(src)-1]))
		}

		return true
	})
}

// clause adds a case clause to the blocks of a comparison in its body. a
// comparison in the case expression runs before the switch goes on, like an
// if condition, so neither the clause nor the switch body (the block just
// before it) holds it in.
func clause(blocks []ast.Node, n ast.Node, colon, at token.Pos) []ast.Node {
	if at > colon {
		return append(blocks, n)
	}

	return blocks[:len(blocks)-1]
}

// anyChecked reports whether a comparison of one of src can run before the
// make: it comes earlier in the source and every block it sits in also holds
// the make, so it is in a statement before the make or in the head of one
// around it (an if condition), not in a branch the make is not part of.
func (s *allocScan) anyChecked(src []string, mk *ast.CallExpr) bool {
	for _, c := range s.checks {
		if c.pos >= mk.Pos() || !holdsAll(c.blocks, mk) {
			continue
		}

		if slices.Contains(src, c.key) {
			return true
		}
	}

	return false
}

func holdsAll(blocks []ast.Node, mk *ast.CallExpr) bool {
	for _, b := range blocks {
		if mk.Pos() < b.Pos() || mk.End() > b.End() {
			return false
		}
	}

	return true
}
