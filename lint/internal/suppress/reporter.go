package suppress

import (
	"go/ast"
	"go/token"
	"os"
	"reflect"

	"golang.org/x/tools/go/analysis"

	"github.com/ErenTopaal/shapekit/lint/internal/sdk"
)

// Record is one finding with enough context to print it, match it against
// a baseline and count it.
type Record struct {
	Rule     string `json:"rule"`
	Package  string `json:"package"`
	Function string `json:"function"`
	File     string `json:"file"`
	Line     int    `json:"line"`
	Column   int    `json:"column"`
	Message  string `json:"message"`
	// Reason is the text of the suppressing directive.
	Reason string `json:"reason,omitempty"`
	// Issue is the link of the matching baseline entry.
	Issue string `json:"issue,omitempty"`
}

// Result is what every rule analyzer returns. Reported findings also go to
// pass.Report, so go vet shows them; the result carries the suppressed and
// baselined ones too, which the standalone driver counts.
type Result struct {
	Reported   []Record
	Suppressed []Record
	Baselined  []Record
}

// ResultType is the analysis.Analyzer ResultType of every rule analyzer.
var ResultType = reflect.TypeFor[*Result]()

// Reporter applies directives and the baseline before reporting.
type Reporter struct {
	pass     *analysis.Pass
	rule     string
	baseline *Baseline
	lines    map[*token.File]map[int][]Directive
	res      *Result
}

// NewReporter returns a reporter for one rule in one pass. It fails when
// the baseline named by BaselineEnv cannot be read or is invalid, so a
// broken baseline is never mistaken for an empty one.
func NewReporter(pass *analysis.Pass, rule string) (*Reporter, error) {
	b, err := LoadBaseline(os.Getenv(BaselineEnv))
	if err != nil {
		return nil, err
	}

	r := &Reporter{
		pass:     pass,
		rule:     rule,
		baseline: b,
		lines:    map[*token.File]map[int][]Directive{},
		res:      &Result{},
	}

	for _, f := range pass.Files {
		r.lines[pass.Fset.File(f.Pos())] = fileDirectives(pass, f)
	}

	return r, nil
}

// Report reports msg at pos unless a directive suppresses it or the
// baseline accepts it. fn is the enclosing function declaration, used for
// function-level directives and baseline matching; it may be nil for a
// finding outside any function.
func (r *Reporter) Report(pos token.Pos, fn *ast.FuncDecl, msg string) {
	p := r.pass.Fset.Position(pos)
	rec := Record{
		Rule:    r.rule,
		Package: r.pass.Pkg.Path(),
		File:    p.Filename,
		Line:    p.Line,
		Column:  p.Column,
		Message: msg,
	}

	if fn != nil {
		rec.Function = sdk.FuncName(fn)
	}

	if d, ok := r.directive(pos, fn); ok {
		rec.Reason = d.Reason
		r.res.Suppressed = append(r.res.Suppressed, rec)

		return
	}

	if e, ok := r.baseline.Match(rec); ok {
		rec.Issue = e.Issue
		r.res.Baselined = append(r.res.Baselined, rec)

		return
	}

	r.res.Reported = append(r.res.Reported, rec)
	r.pass.Report(analysis.Diagnostic{
		Pos:      pos,
		Category: r.rule,
		Message:  r.rule + ": " + msg,
	})
}

// directive finds a directive for this rule on the finding's line, on the
// line above it, or in the doc comment of the enclosing function.
func (r *Reporter) directive(pos token.Pos, fn *ast.FuncDecl) (Directive, bool) {
	tf := r.pass.Fset.File(pos)
	if tf != nil {
		line := tf.Line(pos)
		for _, l := range []int{line, line - 1} {
			for _, d := range r.lines[tf][l] {
				if d.Covers(r.rule) {
					return d, true
				}
			}
		}
	}

	return docDirective(fn, r.rule)
}

// Result returns everything reported so far.
func (r *Reporter) Result() *Result {
	return r.res
}
