package idx002

import (
	"go/ast"
	"go/constant"
	"go/types"
	"math"
	"slices"

	"golang.org/x/tools/go/analysis"

	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
)

const unlimitedMessage = "xdr decoding with the depth limit switched off; keep the default (go-xdr DecodeDefaultMaxDepth) or a smaller limit"

// checkDecoding reports decoding whose depth limit has been switched off.
// go-xdr's DecodeOptions and the sdk's Unmarshal, SafeUnmarshal and
// UnmarshalBinary helpers all default to a depth of 1500 and cap slice
// preallocation at 256 elements, so the defaults are not reported; only
// code that asks for unlimited depth is.
func checkDecoding(pass *analysis.Pass, rep *suppress.Reporter) {
	info := pass.TypesInfo

	for _, f := range pass.Files {
		if sdk.IsTestFile(pass, f) {
			continue
		}

		var stack []ast.Node

		ast.Inspect(f, func(x ast.Node) bool {
			if x == nil {
				stack = stack[:len(stack)-1]

				return true
			}

			stack = append(stack, x)
			fd := enclosingFunc(stack)

			switch x := x.(type) {
			case *ast.Ident:
				if c, ok := info.Uses[x].(*types.Const); ok && c.Pkg() != nil &&
					c.Pkg().Path() == sdk.GoXDR && c.Name() == "DecodeUnlimitedDepth" {
					rep.Report(x.Pos(), fd, unlimitedMessage)
				}
			case *ast.CompositeLit:
				if isDecodeOptions(info.TypeOf(x)) {
					for _, el := range x.Elts {
						kv, ok := el.(*ast.KeyValueExpr)
						if !ok {
							continue
						}

						if k, ok := kv.Key.(*ast.Ident); ok && k.Name == "MaxDepth" && unlimitedLiteral(info, kv.Value) {
							rep.Report(kv.Value.Pos(), fd, unlimitedMessage)
						}
					}
				}
			case *ast.AssignStmt:
				for i, l := range x.Lhs {
					sel, ok := l.(*ast.SelectorExpr)
					if !ok || sel.Sel.Name != "MaxDepth" || !isDecodeOptions(info.TypeOf(sel.X)) || i >= len(x.Rhs) {
						continue
					}

					if unlimitedLiteral(info, x.Rhs[i]) {
						rep.Report(x.Rhs[i].Pos(), fd, unlimitedMessage)
					}
				}
			case *ast.CallExpr:
				if depth := depthArg(info, x); depth != nil && unlimitedLiteral(info, depth) {
					rep.Report(depth.Pos(), fd, unlimitedMessage)
				}
			}

			return true
		})
	}
}

func enclosingFunc(stack []ast.Node) *ast.FuncDecl {
	for _, n := range slices.Backward(stack) {
		if fd, ok := n.(*ast.FuncDecl); ok {
			return fd
		}
	}

	return nil
}

func isDecodeOptions(t types.Type) bool {
	return sdk.Is(t, sdk.GoXDR, "DecodeOptions")
}

// unlimitedLiteral reports whether e is a constant equal to the largest
// uint written out directly, such as math.MaxUint. A reference to
// DecodeUnlimitedDepth is reported where it is named, so it is skipped
// here to avoid reporting the same thing twice.
func unlimitedLiteral(info *types.Info, e ast.Expr) bool {
	if refersToUnlimited(info, e) {
		return false
	}

	tv, ok := info.Types[e]
	if !ok || tv.Value == nil || tv.Value.Kind() != constant.Int {
		return false
	}

	v, exact := constant.Uint64Val(tv.Value)

	return exact && v == math.MaxUint64
}

func refersToUnlimited(info *types.Info, e ast.Expr) bool {
	found := false

	ast.Inspect(e, func(x ast.Node) bool {
		if id, ok := x.(*ast.Ident); ok {
			if c, ok := info.Uses[id].(*types.Const); ok && c.Name() == "DecodeUnlimitedDepth" {
				found = true
			}
		}

		return !found
	})

	return found
}

// depthArg returns the depth argument of a call to an xdr type's
// DecodeFrom(d, maxDepth). The decoder's own depth comes from
// DecodeOptions, which the cases above cover.
func depthArg(info *types.Info, call *ast.CallExpr) ast.Expr {
	sel, ok := ast.Unparen(call.Fun).(*ast.SelectorExpr)
	if !ok || len(call.Args) != 2 {
		return nil
	}

	fn, ok := info.Uses[sel.Sel].(*types.Func)
	if !ok || fn.Pkg() == nil {
		return nil
	}

	if fn.Name() != "DecodeFrom" || fn.Pkg().Path() != sdk.XDR {
		return nil
	}

	return call.Args[1]
}
