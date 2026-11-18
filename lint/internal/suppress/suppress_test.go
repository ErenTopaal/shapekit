package suppress

import (
	"strings"
	"testing"
)

func TestParse(t *testing.T) {
	cases := []struct {
		text    string
		isDir   bool
		wantErr string
		rules   []string
		reason  string
	}{
		{text: "// ordinary comment"},
		{text: "//indexerlint:ignorefoo IDX001 x"},
		{text: "//indexerlint:ignore", isDir: true, wantErr: "needs a rule ID"},
		{text: "//indexerlint:ignore IDX002", isDir: true, wantErr: "needs a reason"},
		{text: "//indexerlint:ignore IDX009 because", isDir: true, wantErr: "unknown indexerlint rule"},
		{text: "//indexerlint:ignore IDX002 decoder caps depth", isDir: true, rules: []string{"IDX002"}, reason: "decoder caps depth"},
		{text: "//indexerlint:ignore IDX001,IDX003 both checked upstream", isDir: true, rules: []string{"IDX001", "IDX003"}, reason: "both checked upstream"},
	}

	for _, c := range cases {
		d, ok, err := Parse(c.text)
		if ok != c.isDir {
			t.Errorf("%q: directive = %v, want %v", c.text, ok, c.isDir)
		}

		if c.wantErr != "" {
			if err == nil || !strings.Contains(err.Error(), c.wantErr) {
				t.Errorf("%q: err = %v, want %q", c.text, err, c.wantErr)
			}

			continue
		}

		if err != nil {
			t.Errorf("%q: unexpected error %v", c.text, err)
		}

		if strings.Join(d.Rules, ",") != strings.Join(c.rules, ",") || d.Reason != c.reason {
			t.Errorf("%q: got %+v", c.text, d)
		}
	}
}

func TestParseBaseline(t *testing.T) {
	good := `{"version":1,"entries":[{"rule":"IDX003","package":"p","function":"f","issue":"https://github.com/stellar/wallet-backend/issues/746"}]}`

	b, err := ParseBaseline([]byte(good))
	if err != nil {
		t.Fatal(err)
	}

	rec := Record{Rule: "IDX003", Package: "p", Function: "f", Message: "anything"}
	if _, ok := b.Match(rec); !ok {
		t.Error("entry without a message should match any message")
	}

	rec.Function = "g"
	if _, ok := b.Match(rec); ok {
		t.Error("matched a different function")
	}

	// a finding outside any function is baselined by its message
	level := `{"version":1,"entries":[{"rule":"IDX002","package":"p","message":"m","issue":"https://x"}]}`

	b, err = ParseBaseline([]byte(level))
	if err != nil {
		t.Fatalf("package-level entry: %v", err)
	}

	if _, ok := b.Match(Record{Rule: "IDX002", Package: "p", Message: "m"}); !ok {
		t.Error("package-level entry did not match its finding")
	}

	for _, r := range []Record{
		{Rule: "IDX002", Package: "p", Message: "other"},
		{Rule: "IDX002", Package: "p", Function: "f", Message: "m"},
	} {
		if _, ok := b.Match(r); ok {
			t.Errorf("package-level entry matched %+v", r)
		}
	}

	bad := map[string]string{
		"no issue":                  `{"version":1,"entries":[{"rule":"IDX003","package":"p","function":"f","issue":""}]}`,
		"http issue":                `{"version":1,"entries":[{"rule":"IDX003","package":"p","function":"f","issue":"http://x"}]}`,
		"unknown rule":              `{"version":1,"entries":[{"rule":"IDX004","package":"p","function":"f","issue":"https://x"}]}`,
		"no function":               `{"version":1,"entries":[{"rule":"IDX003","package":"p","issue":"https://x"}]}`,
		"no package":                `{"version":1,"entries":[{"rule":"IDX003","function":"f","issue":"https://x"}]}`,
		"no package, package level": `{"version":1,"entries":[{"rule":"IDX003","message":"m","issue":"https://x"}]}`,
		"wrong version":             `{"version":2,"entries":[]}`,
		"unknown key":               `{"version":1,"entries":[],"extra":true}`,
	}

	for name, src := range bad {
		if _, err := ParseBaseline([]byte(src)); err == nil {
			t.Errorf("%s: accepted", name)
		}
	}
}
