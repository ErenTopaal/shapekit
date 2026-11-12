package idx002_test

import (
	"testing"

	"golang.org/x/tools/go/analysis/analysistest"

	"github.com/ErenTopaal/shapekit/lint/idx002"
	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
	"github.com/ErenTopaal/shapekit/lint/internal/testmod"
)

func TestIDX002(t *testing.T) {
	dir := analysistest.TestData()
	testmod.Download(t, dir)

	results := analysistest.Run(t, dir, idx002.Analyzer, "./src/...")

	suppressed := 0

	for _, r := range results {
		res, ok := r.Action.Result.(*suppress.Result)
		if !ok {
			t.Fatalf("%s: result is %T, want *suppress.Result", r.Action.Package.PkgPath, r.Action.Result)
		}

		for _, s := range res.Suppressed {
			suppressed++

			if s.Function != "trusted" || s.Reason == "" {
				t.Errorf("unexpected suppression %+v", s)
			}
		}
	}

	if suppressed != 1 {
		t.Errorf("got %d suppressed findings, want 1", suppressed)
	}
}
