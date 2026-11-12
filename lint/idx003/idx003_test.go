package idx003_test

import (
	"testing"

	"golang.org/x/tools/go/analysis/analysistest"

	"github.com/ErenTopaal/shapekit/lint/idx003"
	"github.com/ErenTopaal/shapekit/lint/internal/suppress"
	"github.com/ErenTopaal/shapekit/lint/internal/testmod"
)

func TestIDX003(t *testing.T) {
	dir := analysistest.TestData()
	testmod.Download(t, dir)

	results := analysistest.Run(t, dir, idx003.Analyzer, "./src/...")

	suppressed := map[string]bool{}

	for _, r := range results {
		res, ok := r.Action.Result.(*suppress.Result)
		if !ok {
			t.Fatalf("%s: result is %T, want *suppress.Result", r.Action.Package.PkgPath, r.Action.Result)
		}

		for _, s := range res.Suppressed {
			if s.Reason == "" {
				t.Errorf("suppression without a reason: %+v", s)
			}

			suppressed[s.Function] = true
		}
	}

	for _, fn := range []string{"feeAccounts", "sourceOnly"} {
		if !suppressed[fn] {
			t.Errorf("%s was not counted as suppressed (got %v)", fn, suppressed)
		}
	}

	if len(suppressed) != 2 {
		t.Errorf("got suppressions in %v, want only feeAccounts and sourceOnly", suppressed)
	}
}
