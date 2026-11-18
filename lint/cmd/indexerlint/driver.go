package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"golang.org/x/tools/go/analysis/checker"
	"golang.org/x/tools/go/packages"

	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
)

type options struct {
	format   string
	baseline string
	patterns []string
}

const usageText = `usage: indexerlint [-format text|json] [-baseline file] packages...

Checks Go indexer code built on go-stellar-sdk for three ingestion mistakes:
  IDX001  an unvalidated transaction field is used as a key, stored, or converted lossily
  IDX002  a recursive xdr structure or a decoded length is used without a depth or size cap
  IDX003  participants are derived from the envelope instead of meta changes or events

Run it from the root of the module to check, for example indexerlint ./...
Under go vet: go vet -vettool=$(command -v indexerlint) ./...
Exit status: 0 no findings, 1 findings, 2 usage, load or analysis error.
Suppressions and the baseline file are described in docs/linter.md.

Flags:
`

func parseFlags(args []string, stderr io.Writer) (options, error) {
	fs := flag.NewFlagSet("indexerlint", flag.ContinueOnError)
	fs.SetOutput(stderr)
	fs.Usage = func() {
		_, _ = fmt.Fprint(stderr, usageText)
		fs.PrintDefaults()
	}

	var o options

	fs.StringVar(&o.format, "format", "text", "output format, text or json")
	fs.StringVar(&o.baseline, "baseline", os.Getenv(suppress.BaselineEnv),
		"baseline file of accepted, issue-linked findings (default $"+suppress.BaselineEnv+")")

	if err := fs.Parse(args); err != nil {
		return o, err
	}

	o.patterns = fs.Args()

	if o.format != "text" && o.format != "json" {
		_, _ = fmt.Fprintf(stderr, "indexerlint: -format must be text or json, got %q\n", o.format)

		return o, errors.New("bad format")
	}

	if len(o.patterns) == 0 {
		_, _ = fmt.Fprintln(stderr, "indexerlint: no packages given; pass package patterns such as ./...")
		fs.Usage()

		return o, errors.New("no packages")
	}

	return o, nil
}

// report is what the standalone driver prints.
type report struct {
	Findings      []suppress.Record `json:"findings"`
	Suppressed    []suppress.Record `json:"suppressed"`
	Baselined     []suppress.Record `json:"baselined"`
	UnusedEntries []suppress.Entry  `json:"unused_baseline_entries"`
	Counts        counts            `json:"counts"`
}

type counts struct {
	Findings   int `json:"findings"`
	Suppressed int `json:"suppressed"`
	Baselined  int `json:"baselined"`
}

func analyze(o options) (*report, error) {
	var baseline *suppress.Baseline

	if o.baseline != "" {
		abs, err := filepath.Abs(o.baseline)
		if err != nil {
			return nil, err
		}

		// the analyzers read the baseline from the environment, the same
		// way they do under go vet
		if err := os.Setenv(suppress.BaselineEnv, abs); err != nil {
			return nil, err
		}

		if baseline, err = suppress.LoadBaseline(abs); err != nil {
			return nil, err
		}
	}

	cfg := &packages.Config{Mode: packages.LoadAllSyntax}

	pkgs, err := packages.Load(cfg, o.patterns...)
	if err != nil {
		return nil, fmt.Errorf("loading packages: %w", err)
	}

	if len(pkgs) == 0 {
		return nil, fmt.Errorf("no packages match %s", strings.Join(o.patterns, " "))
	}

	var loadErrs []error

	packages.Visit(pkgs, nil, func(p *packages.Package) {
		for _, e := range p.Errors {
			loadErrs = append(loadErrs, e)
		}
	})

	if len(loadErrs) > 0 {
		return nil, fmt.Errorf("packages did not load cleanly: %w", errors.Join(loadErrs...))
	}

	g, err := checker.Analyze(analyzers(), pkgs, nil)
	if err != nil {
		return nil, err
	}

	rep := &report{}

	for act := range g.All() {
		if !act.IsRoot {
			continue
		}

		if act.Err != nil {
			return nil, fmt.Errorf("%s on %s: %w", act.Analyzer.Name, act.Package.PkgPath, act.Err)
		}

		if act.Analyzer == suppress.Analyzer {
			rep.addDirectiveFindings(act)

			continue
		}

		res, ok := act.Result.(*suppress.Result)
		if !ok {
			return nil, fmt.Errorf("%s returned %T", act.Analyzer.Name, act.Result)
		}

		rep.Findings = append(rep.Findings, res.Reported...)
		rep.Suppressed = append(rep.Suppressed, res.Suppressed...)
		rep.Baselined = append(rep.Baselined, res.Baselined...)
	}

	rep.finish(baseline)

	return rep, nil
}

// addDirectiveFindings turns malformed directives into findings, so a broken
// suppression fails the run instead of silently suppressing nothing.
func (r *report) addDirectiveFindings(act *checker.Action) {
	for _, d := range act.Diagnostics {
		p := act.Package.Fset.Position(d.Pos)
		r.Findings = append(r.Findings, suppress.Record{
			Rule:    "directive",
			Package: act.Package.PkgPath,
			File:    p.Filename,
			Line:    p.Line,
			Column:  p.Column,
			Message: d.Message,
		})
	}
}

func (r *report) finish(baseline *suppress.Baseline) {
	cwd, err := os.Getwd()

	for _, list := range [][]suppress.Record{r.Findings, r.Suppressed, r.Baselined} {
		for i := range list {
			if err == nil {
				if rel, relErr := filepath.Rel(cwd, list[i].File); relErr == nil && !strings.HasPrefix(rel, "..") {
					list[i].File = rel
				}
			}
		}

		slices.SortFunc(list, compareRecords)
	}

	if baseline != nil {
		for _, e := range baseline.Entries {
			used := slices.ContainsFunc(r.Baselined, func(rec suppress.Record) bool {
				return rec.Rule == e.Rule && rec.Package == e.Package && rec.Function == e.Function &&
					(e.Message == "" || e.Message == rec.Message)
			})
			if !used {
				r.UnusedEntries = append(r.UnusedEntries, e)
			}
		}
	}

	if r.Findings == nil {
		r.Findings = []suppress.Record{}
	}

	if r.Suppressed == nil {
		r.Suppressed = []suppress.Record{}
	}

	if r.Baselined == nil {
		r.Baselined = []suppress.Record{}
	}

	if r.UnusedEntries == nil {
		r.UnusedEntries = []suppress.Entry{}
	}

	r.Counts = counts{Findings: len(r.Findings), Suppressed: len(r.Suppressed), Baselined: len(r.Baselined)}
}

func compareRecords(a, b suppress.Record) int {
	if c := strings.Compare(a.File, b.File); c != 0 {
		return c
	}

	if a.Line != b.Line {
		return a.Line - b.Line
	}

	if a.Column != b.Column {
		return a.Column - b.Column
	}

	return strings.Compare(a.Rule, b.Rule)
}

func (r *report) write(w io.Writer, format string) error {
	if format == "json" {
		enc := json.NewEncoder(w)
		enc.SetIndent("", "  ")

		return enc.Encode(r)
	}

	var b strings.Builder

	for _, f := range r.Findings {
		fmt.Fprintf(&b, "%s:%d:%d: %s: %s", f.File, f.Line, f.Column, f.Rule, f.Message)

		if f.Function != "" {
			fmt.Fprintf(&b, " [%s]", f.Function)
		}

		b.WriteByte('\n')
	}

	for _, e := range r.UnusedEntries {
		fmt.Fprintf(&b, "baseline entry matched nothing: %s %s %s (%s)\n", e.Rule, e.Package, e.Function, e.Issue)
	}

	fmt.Fprintf(&b, "indexerlint: %d findings, %d suppressed, %d baselined\n",
		r.Counts.Findings, r.Counts.Suppressed, r.Counts.Baselined)

	_, err := io.WriteString(w, b.String())

	return err
}
