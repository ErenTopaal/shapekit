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
	"crypto/sha256"
	"fmt"
	"io"
	"os"
	"slices"
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
	if slices.Contains(os.Args[1:], "-V=full") {
		if err := printVersion(os.Stdout); err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "indexerlint: %v\n", err)
			os.Exit(2)
		}

		os.Exit(0)
	}

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

// printVersion answers go vet's -V=full the way unitchecker does, except that
// the build ID also covers the INDEXERLINT_BASELINE path and the file's
// contents: go caches vet results by this ID, and an ID of the executable
// alone would replay a result after the baseline was set, changed or unset.
func printVersion(w io.Writer) error {
	progname, err := os.Executable()
	if err != nil {
		return err
	}

	h := sha256.New()

	f, err := os.Open(progname) //nolint:gosec // the running executable
	if err != nil {
		return err
	}

	_, err = io.Copy(h, f)
	_ = f.Close()

	if err != nil {
		return err
	}

	if err := hashBaseline(h, os.Getenv(suppress.BaselineEnv)); err != nil {
		return err
	}

	_, err = fmt.Fprintf(w, "%s version devel comments-go-here buildID=%02x\n", progname, h.Sum(nil))

	return err
}

// hashBaseline adds the baseline in effect to h; nothing when there is none.
// a baseline that cannot be read is an error here as it is in the analysis.
func hashBaseline(h io.Writer, path string) error {
	if path == "" {
		return nil
	}

	data, err := os.ReadFile(path) //nolint:gosec // the operator's baseline file
	if err != nil {
		return fmt.Errorf("reading %s %s: %w", suppress.BaselineEnv, path, err)
	}

	_, _ = fmt.Fprintf(h, "\x00baseline %d %s\x00", len(path), path)
	_, _ = h.Write(data)

	return nil
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
