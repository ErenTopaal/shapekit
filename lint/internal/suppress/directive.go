// Package suppress implements the //indexerlint:ignore directive, the
// baseline file and the record of what each rule reported, suppressed or
// matched against the baseline.
package suppress

import (
	"errors"
	"fmt"
	"go/ast"
	"slices"
	"strings"

	"golang.org/x/tools/go/analysis"

	"github.com/ErenTopaal/shapekit/lint/rules"
)

// Prefix starts every directive. Like go directives it has no space after
// the slashes.
const Prefix = "//indexerlint:ignore"

// Directive is one parsed //indexerlint:ignore comment.
type Directive struct {
	Rules  []string
	Reason string
}

var (
	errNoRule   = errors.New("indexerlint:ignore needs a rule ID, for example //indexerlint:ignore IDX003 <reason>")
	errNoReason = errors.New("indexerlint:ignore needs a reason after the rule ID")
)

// Parse parses the text of one comment. ok is false when the comment is not
// a directive at all. A directive with a missing or unknown rule ID or no
// reason returns an error and suppresses nothing.
func Parse(text string) (d Directive, ok bool, err error) {
	rest, found := strings.CutPrefix(text, Prefix)
	if !found {
		return Directive{}, false, nil
	}

	// "//indexerlint:ignorefoo" is some other word, not this directive
	if rest != "" && rest[0] != ' ' && rest[0] != '\t' {
		return Directive{}, false, nil
	}

	fields := strings.Fields(rest)
	if len(fields) == 0 {
		return Directive{}, true, errNoRule
	}

	for id := range strings.SplitSeq(fields[0], ",") {
		if _, lookupErr := rules.Lookup(id); lookupErr != nil {
			return Directive{}, true, fmt.Errorf("indexerlint:ignore: %w", lookupErr)
		}

		d.Rules = append(d.Rules, id)
	}

	d.Reason = strings.Join(fields[1:], " ")
	if d.Reason == "" {
		return Directive{}, true, errNoReason
	}

	return d, true, nil
}

// Covers reports whether the directive names rule.
func (d Directive) Covers(rule string) bool {
	return slices.Contains(d.Rules, rule)
}

// Analyzer reports malformed directives. The rule analyzers ignore a
// malformed directive, so without this one a typo would silently stop
// suppressing and nobody would be told why.
var Analyzer = &analysis.Analyzer{
	Name: "indexerlintignore",
	Doc:  "reports //indexerlint:ignore directives that lack a known rule ID or a reason",
	Run:  runDirectives,
}

func runDirectives(pass *analysis.Pass) (any, error) {
	for _, f := range pass.Files {
		for _, cg := range f.Comments {
			for _, c := range cg.List {
				if _, ok, err := Parse(c.Text); ok && err != nil {
					pass.Reportf(c.Pos(), "%v", err)
				}
			}
		}
	}

	return nil, nil
}

// fileDirectives indexes the valid directives of one file by line.
func fileDirectives(pass *analysis.Pass, f *ast.File) map[int][]Directive {
	byLine := map[int][]Directive{}

	for _, cg := range f.Comments {
		for _, c := range cg.List {
			d, ok, err := Parse(c.Text)
			if !ok || err != nil {
				continue
			}

			line := pass.Fset.Position(c.Pos()).Line
			byLine[line] = append(byLine[line], d)
		}
	}

	return byLine
}

// docDirective returns the first valid directive for rule in a function's
// doc comment.
func docDirective(fd *ast.FuncDecl, rule string) (Directive, bool) {
	if fd == nil || fd.Doc == nil {
		return Directive{}, false
	}

	for _, c := range fd.Doc.List {
		if d, ok, err := Parse(c.Text); ok && err == nil && d.Covers(rule) {
			return d, true
		}
	}

	return Directive{}, false
}
