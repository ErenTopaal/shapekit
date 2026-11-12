// Package idx002 implements indexerlint rule IDX002, missing depth or size
// cap.
//
// It reports three things:
//
//   - functions that recurse, directly or through other functions and
//     closures of the same package, over xdr.ScVal, xdr.ScVec, xdr.ScMap,
//     xdr.ScMapEntry, xdr.SorobanAuthorizedInvocation,
//     xdr.SorobanDelegateSignature or xdr.ClaimPredicate without an
//     integer that is both compared and stepped along the recursion (a
//     depth parameter or a counter);
//   - make calls whose length or capacity comes from an integer decoded
//     from xdr with no comparison against a bound in the same function;
//   - xdr decoding with the go-xdr depth limit switched off.
//
// go-stellar-sdk decodes with a depth limit of 1500 and bounds slice
// preallocation by default, so only decoding that turns those defaults off
// is reported. See docs/linter.md for what the rule cannot see.
package idx002

import (
	"golang.org/x/tools/go/analysis"

	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
	"github.com/ErenTopaal/shapekit/lint/rules"
)

// ID is the rule ID this analyzer reports under.
const ID = "IDX002"

// Analyzer is the IDX002 analyzer.
var Analyzer = &analysis.Analyzer{
	Name:       ID,
	Doc:        doc(),
	URL:        "https://github.com/ErenTopaal/shapekit/blob/main/docs/linter.md#idx002",
	Run:        run,
	ResultType: suppress.ResultType,
}

func doc() string {
	r, err := rules.Lookup(ID)
	if err != nil {
		panic(err)
	}

	return r.Name + ": " + r.Description
}

func run(pass *analysis.Pass) (any, error) {
	rep, err := suppress.NewReporter(pass, ID)
	if err != nil {
		return nil, err
	}

	if sdk.IsSDKPackage(pass.Pkg.Path()) {
		return rep.Result(), nil
	}

	checkRecursion(pass, rep)
	checkAllocations(pass, rep)
	checkDecoding(pass, rep)

	return rep.Result(), nil
}
