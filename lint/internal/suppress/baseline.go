package suppress

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"strings"
	"sync"

	"github.com/ErenTopaal/shapekit/lint/rules"
)

// BaselineEnv names the environment variable that points the analyzers at
// a baseline file. An environment variable rather than a flag works the
// same under the standalone driver and under go vet -vettool, which does
// not forward arbitrary flags to every analyzer.
const BaselineEnv = "INDEXERLINT_BASELINE"

// Baseline is a file of accepted findings. Every entry has to link the
// upstream issue that tracks it, so a baseline cannot quietly grow into a
// list of ignored warnings.
type Baseline struct {
	Version int     `json:"version"`
	Entries []Entry `json:"entries"`
}

// Entry matches findings by rule, package and function. Line numbers are
// left out on purpose: they move with every unrelated edit. Message is
// optional; when set it has to match exactly.
type Entry struct {
	Rule     string `json:"rule"`
	Package  string `json:"package"`
	Function string `json:"function"`
	Message  string `json:"message,omitempty"`
	Issue    string `json:"issue"`
	Note     string `json:"note,omitempty"`
}

// ParseBaseline decodes and validates a baseline file.
func ParseBaseline(data []byte) (*Baseline, error) {
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.DisallowUnknownFields()

	var b Baseline
	if err := dec.Decode(&b); err != nil {
		return nil, fmt.Errorf("decoding baseline: %w", err)
	}

	if b.Version != 1 {
		return nil, fmt.Errorf("baseline version %d is not supported, want 1", b.Version)
	}

	var errs []error

	for i, e := range b.Entries {
		if _, err := rules.Lookup(e.Rule); err != nil {
			errs = append(errs, fmt.Errorf("entry %d: %w", i, err))
		}

		if e.Package == "" || e.Function == "" {
			errs = append(errs, fmt.Errorf("entry %d: package and function are required", i))
		}

		if !strings.HasPrefix(e.Issue, "https://") {
			errs = append(errs, fmt.Errorf("entry %d: issue must be an https link to the tracking issue, got %q", i, e.Issue))
		}
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}

	return &b, nil
}

// Match returns the first entry that accepts rec.
func (b *Baseline) Match(rec Record) (Entry, bool) {
	if b == nil {
		return Entry{}, false
	}

	for _, e := range b.Entries {
		if e.Rule == rec.Rule && e.Package == rec.Package && e.Function == rec.Function &&
			(e.Message == "" || e.Message == rec.Message) {
			return e, true
		}
	}

	return Entry{}, false
}

var baselines sync.Map // path -> baselineLoad

type baselineLoad struct {
	once sync.Once
	b    *Baseline
	err  error
}

// LoadBaseline reads and validates the baseline at path once per process.
// Every analyzer of every package asks for it, so it is cached.
func LoadBaseline(path string) (*Baseline, error) {
	if path == "" {
		return nil, nil
	}

	v, _ := baselines.LoadOrStore(path, &baselineLoad{})
	l, _ := v.(*baselineLoad)

	l.once.Do(func() {
		data, err := os.ReadFile(path) //nolint:gosec // the path is the operator's own baseline file
		if err != nil {
			l.err = fmt.Errorf("reading baseline: %w", err)

			return
		}

		l.b, l.err = ParseBaseline(data)
		if l.err != nil {
			l.err = fmt.Errorf("%s: %w", path, l.err)
		}
	})

	return l.b, l.err
}
