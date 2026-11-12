package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/ErenTopaal/shapekit/lint/internal/testmod"
)

// the driver is run against the IDX002 testdata module, which has known
// findings and one suppression.
func runIn(t *testing.T, args ...string) (int, report) {
	t.Helper()

	dir, err := filepath.Abs("../../idx002/testdata")
	if err != nil {
		t.Fatal(err)
	}

	testmod.Download(t, dir)
	t.Setenv("GOWORK", "off")
	t.Setenv("GOFLAGS", "-mod=mod")
	t.Chdir(dir)

	var out, errOut bytes.Buffer

	code := run(append([]string{"-format", "json"}, args...), &out, &errOut)

	var rep report
	if code != 2 {
		if err := json.Unmarshal(out.Bytes(), &rep); err != nil {
			t.Fatalf("decoding output: %v\n%s\n%s", err, out.String(), errOut.String())
		}
	}

	return code, rep
}

func TestFindingsExitOne(t *testing.T) {
	code, rep := runIn(t, "./src/recursion")
	if code != 1 {
		t.Fatalf("exit %d, want 1", code)
	}

	if rep.Counts.Findings != 5 || rep.Counts.Suppressed != 1 {
		t.Errorf("counts = %+v, want 5 findings and 1 suppressed", rep.Counts)
	}
}

func TestDecodeFindings(t *testing.T) {
	code, rep := runIn(t, "./src/decode")
	if code != 1 {
		t.Fatalf("exit %d, want 1 for the seeded decode findings", code)
	}

	if rep.Counts.Findings != 4 {
		t.Errorf("counts = %+v, want 4", rep.Counts)
	}
}

func TestBaseline(t *testing.T) {
	base := filepath.Join(t.TempDir(), "baseline.json")
	entries := `{"version":1,"entries":[
		{"rule":"IDX002","package":"example.com/idx002/src/alloc","function":"fromU32","issue":"https://example.com/issues/1"},
		{"rule":"IDX002","package":"example.com/idx002/src/alloc","function":"fromU64","issue":"https://example.com/issues/2"},
		{"rule":"IDX002","package":"example.com/idx002/src/alloc","function":"direct","issue":"https://example.com/issues/3"},
		{"rule":"IDX002","package":"example.com/idx002/src/alloc","function":"gone","issue":"https://example.com/issues/4"}
	]}`

	if err := os.WriteFile(base, []byte(entries), 0o600); err != nil {
		t.Fatal(err)
	}

	code, rep := runIn(t, "-baseline", base, "./src/alloc")
	if code != 0 {
		t.Fatalf("exit %d, want 0 with every finding baselined: %+v", code, rep)
	}

	if rep.Counts.Baselined != 3 || len(rep.UnusedEntries) != 1 || rep.UnusedEntries[0].Function != "gone" {
		t.Errorf("baselined %d, unused %+v", rep.Counts.Baselined, rep.UnusedEntries)
	}

	for _, b := range rep.Baselined {
		if b.Issue == "" {
			t.Errorf("baselined finding without its issue link: %+v", b)
		}
	}
}

func TestBadBaselineExitTwo(t *testing.T) {
	base := filepath.Join(t.TempDir(), "baseline.json")
	if err := os.WriteFile(base, []byte(`{"version":1,"entries":[{"rule":"IDX002","package":"p","function":"f","issue":""}]}`), 0o600); err != nil {
		t.Fatal(err)
	}

	if code, _ := runIn(t, "-baseline", base, "./src/alloc"); code != 2 {
		t.Fatalf("exit %d, want 2", code)
	}
}

func TestVetMode(t *testing.T) {
	for args, want := range map[string]bool{"-V=full": true, "-flags": true, "x/vet.cfg": true, "./...": false, "-format": false} {
		if got := vetMode([]string{args}); got != want {
			t.Errorf("vetMode(%q) = %v", args, got)
		}
	}
}
