// Package sdk holds the go-stellar-sdk names the indexerlint rules match on
// and small helpers for matching them against go/types.
//
// The rules match by import path and type name rather than importing the
// sdk, so the linter does not pin the sdk version of the code it checks.
package sdk

import (
	"go/ast"
	"go/token"
	"go/types"
	"strings"

	"golang.org/x/tools/go/analysis"
)

// Import paths of the packages the rules know about.
const (
	XDR           = "github.com/stellar/go-stellar-sdk/xdr"
	Ingest        = "github.com/stellar/go-stellar-sdk/ingest"
	TokenTransfer = "github.com/stellar/go-stellar-sdk/processors/token_transfer" //nolint:gosec // an import path, not a credential
	GoXDR         = "github.com/stellar/go-xdr/xdr3"
	sdkModule     = "github.com/stellar/go-stellar-sdk"
	goXDRModule   = "github.com/stellar/go-xdr"
)

// Deref strips any number of pointer layers.
func Deref(t types.Type) types.Type {
	for {
		p, ok := types.Unalias(t).(*types.Pointer)
		if !ok {
			return types.Unalias(t)
		}

		t = p.Elem()
	}
}

// NamedIn returns the package path and name of t after pointers are
// stripped, or ok false when t is not a named type.
func NamedIn(t types.Type) (pkg, name string, ok bool) {
	if t == nil {
		return "", "", false
	}

	n, isNamed := Deref(t).(*types.Named)
	if !isNamed || n.Obj().Pkg() == nil {
		return "", "", false
	}

	return n.Obj().Pkg().Path(), n.Obj().Name(), true
}

// Is reports whether t, after pointers are stripped, is pkg.name.
func Is(t types.Type, pkg, name string) bool {
	p, n, ok := NamedIn(t)

	return ok && p == pkg && n == name
}

// IsSDKPackage reports whether path belongs to go-stellar-sdk or go-xdr.
// The rules describe these packages through tables, so they are not
// analyzed themselves.
func IsSDKPackage(path string) bool {
	return within(path, sdkModule) || within(path, goXDRModule)
}

// IsStd reports whether path looks like a standard library package.
func IsStd(path string) bool {
	first, _, _ := strings.Cut(path, "/")

	return !strings.Contains(first, ".")
}

func within(path, module string) bool {
	return path == module || strings.HasPrefix(path, module+"/")
}

// IsTestFile reports whether f is a _test.go file. Test code is not
// ingestion code, so the rules skip it.
func IsTestFile(pass *analysis.Pass, f *ast.File) bool {
	return strings.HasSuffix(pass.Fset.File(f.Pos()).Name(), "_test.go")
}

// FuncName names a function declaration the way baseline entries do:
// "Name" for a function, "(T).Name" or "(*T).Name" for a method.
func FuncName(fd *ast.FuncDecl) string {
	if fd.Recv == nil || len(fd.Recv.List) == 0 {
		return fd.Name.Name
	}

	return "(" + recvString(fd.Recv.List[0].Type) + ")." + fd.Name.Name
}

func recvString(e ast.Expr) string {
	switch t := e.(type) {
	case *ast.StarExpr:
		return "*" + recvString(t.X)
	case *ast.IndexExpr:
		return recvString(t.X)
	case *ast.IndexListExpr:
		return recvString(t.X)
	case *ast.ParenExpr:
		return recvString(t.X)
	case *ast.Ident:
		return t.Name
	default:
		return "?"
	}
}

// Callee returns the function or closure variable a call expression calls,
// or nil when the callee is not a named object (a call through a field of
// function type, an interface value of unknown dynamic type and so on).
func Callee(info *types.Info, call *ast.CallExpr) types.Object {
	switch fun := ast.Unparen(call.Fun).(type) {
	case *ast.Ident:
		return info.Uses[fun]
	case *ast.SelectorExpr:
		return info.Uses[fun.Sel]
	case *ast.IndexExpr:
		return calleeOfGeneric(info, fun.X)
	case *ast.IndexListExpr:
		return calleeOfGeneric(info, fun.X)
	default:
		return nil
	}
}

func calleeOfGeneric(info *types.Info, x ast.Expr) types.Object {
	switch x := ast.Unparen(x).(type) {
	case *ast.Ident:
		return info.Uses[x]
	case *ast.SelectorExpr:
		return info.Uses[x.Sel]
	default:
		return nil
	}
}

// IsBuiltin reports whether call calls the named builtin.
func IsBuiltin(info *types.Info, call *ast.CallExpr, name string) bool {
	id, ok := ast.Unparen(call.Fun).(*ast.Ident)
	if !ok {
		return false
	}

	b, ok := info.Uses[id].(*types.Builtin)

	return ok && b.Name() == name
}

// IsConversion reports whether call is a type conversion.
func IsConversion(info *types.Info, call *ast.CallExpr) bool {
	tv, ok := info.Types[call.Fun]

	return ok && tv.IsType()
}

// IsComparison reports whether op compares two values.
func IsComparison(op token.Token) bool {
	switch op {
	case token.LSS, token.LEQ, token.GTR, token.GEQ, token.EQL, token.NEQ:
		return true
	default:
		return false
	}
}

// IsInteger reports whether t has an integer underlying type.
func IsInteger(t types.Type) bool {
	if t == nil {
		return false
	}

	b, ok := t.Underlying().(*types.Basic)

	return ok && b.Info()&types.IsInteger != 0
}

// Root returns the variable at the root of an expression such as x, x.f,
// x[i].f or *x, or nil when there is none.
func Root(info *types.Info, e ast.Expr) types.Object {
	for {
		switch x := ast.Unparen(e).(type) {
		case *ast.Ident:
			if v, ok := info.ObjectOf(x).(*types.Var); ok {
				return v
			}

			return nil
		case *ast.SelectorExpr:
			if sel, ok := info.Selections[x]; ok && sel.Kind() == types.FieldVal {
				e = x.X

				continue
			}

			return nil
		case *ast.IndexExpr:
			e = x.X
		case *ast.StarExpr:
			e = x.X
		default:
			return nil
		}
	}
}
