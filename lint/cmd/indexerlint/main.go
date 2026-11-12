// Command indexerlint runs the indexerlint rules over Go indexer code built
// on go-stellar-sdk.
//
// Standalone:
//
//	indexerlint [-format text|json] [-baseline file] packages...
//
// Under go vet:
//
//	go vet -vettool=$(command -v indexerlint) ./...
//
// The standalone driver prints findings with rule IDs, counts suppressed
// and baselined findings, lists baseline entries that matched nothing, and
// exits 0 when there are no findings, 1 when there are, and 2 on a usage,
// load or analysis error. Under go vet the same analyzers run through the
// unitchecker protocol; suppression directives and the baseline (through
// the INDEXERLINT_BASELINE environment variable) apply there too, but vet
// prints only the findings.
package main

import (
	"fmt"
	"io"
	"os"
	"strings"

	"golang.org/x/tools/go/analysis"
	"golang.org/x/tools/go/analysis/unitchecker"

	"github.com/ErenTopaal/shapekit/lint/idx001"
	"github.com/ErenTopaal/shapekit/lint/idx002"
	"github.com/ErenTopaal/shapekit/lint/idx003"
	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
)

// ruleAnalyzers report findings under a rule ID and return a
// *suppress.Result.
var ruleAnalyzers = []*analysis.Analyzer{idx001.Analyzer, idx002.Analyzer, idx003.Analyzer}

func analyzers() []*analysis.Analyzer {
	return append(append([]*analysis.Analyzer{}, ruleAnalyzers...), suppress.Analyzer)
}

func main() {
	if vetMode(os.Args[1:]) {
		unitchecker.Main(analyzers()...)
	}

	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

// vetMode reports whether go vet is driving the binary: it first asks for
// -V=full and -flags, then passes a single .cfg file per package.
func vetMode(args []string) bool {
	for _, a := range args {
		if a == "-V=full" || a == "-flags" || strings.HasSuffix(a, ".cfg") {
			return true
		}
	}

	return false
}

func run(args []string, stdout, stderr io.Writer) int {
	opts, err := parseFlags(args, stderr)
	if err != nil {
		return 2
	}

	rep, err := analyze(opts)
	if err != nil {
		_, _ = fmt.Fprintf(stderr, "indexerlint: %v\n", err)

		return 2
	}

	if err := rep.write(stdout, opts.format); err != nil {
		_, _ = fmt.Fprintf(stderr, "indexerlint: writing output: %v\n", err)

		return 2
	}

	if len(rep.Findings) > 0 {
		return 1
	}

	return 0
}
