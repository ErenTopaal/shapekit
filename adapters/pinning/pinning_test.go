package pinning

import (
	"context"
	"errors"
	"strings"
	"testing"
)

var table = map[string]uint32{
	"v0.7.3":                               28,
	"v0.8.0":                               30,
	"v0.8.1-0.20261106081502-e31e3b146d00": 30,
}

func TestSDKProtocol(t *testing.T) {
	for _, c := range []struct {
		version string
		want    uint32
		floor   string
	}{
		{"v0.7.3", 28, ""},
		{"v0.8.0", 30, ""},
		{"v0.8.1-0.20261106081502-e31e3b146d00", 30, ""},
		// unlisted: the newest listed release before it
		{"v0.7.4", 28, "v0.7.3"},
		{"v0.8.0-rc.1", 28, "v0.7.3"},
		{"v0.8.1-0.20261101000000-aaaaaaaaaaaa", 30, "v0.8.0"},
		{"v0.8.1", 30, "v0.8.1-0.20261106081502-e31e3b146d00"},
		{"v0.9.1-0.20261113185048-578d9e74c8e4", 30, "v0.8.1-0.20261106081502-e31e3b146d00"},
		{"v1.0.0", 30, "v0.8.1-0.20261106081502-e31e3b146d00"},
	} {
		got, note, err := SDKProtocol(table, c.version, "sdkProtocols in x.go")
		if err != nil {
			t.Fatalf("%s: %v", c.version, err)
		}
		if got != c.want {
			t.Errorf("%s: protocol %d, want %d", c.version, got, c.want)
		}
		if c.floor == "" && note != "" {
			t.Errorf("%s is listed but has a note: %s", c.version, note)
		}
		if c.floor != "" && (!strings.Contains(note, "the protocol of "+c.floor+",") || !strings.Contains(note, "sdkProtocols in x.go")) {
			t.Errorf("%s: note %q does not name %s and the table", c.version, note, c.floor)
		}
	}
	for _, v := range []string{"v0.7.2", "v0.1.0", "0.9.0", "latest", ""} {
		if _, _, err := SDKProtocol(table, v, "sdkProtocols in x.go"); err == nil || !strings.Contains(err.Error(), "sdkProtocols in x.go") {
			t.Errorf("%q: %v", v, err)
		}
	}
}

func TestContexts(t *testing.T) {
	pins := map[string]string{"golang:1.25": "docker.io/library/golang@sha256:" + strings.Repeat("11", 32)}
	resolved := "docker.io/library/debian@sha256:" + strings.Repeat("22", 32)
	byDigest := "ghcr.io/x/y@sha256:" + strings.Repeat("33", 32)
	var asked []string
	resolve := func(_ context.Context, ref string) (string, error) {
		asked = append(asked, ref)
		return resolved, nil
	}
	plan, err := Contexts(context.Background(), pins, "BaseImages in x.go", []string{"golang:1.25", "debian:trixie", byDigest}, resolve)
	if err != nil {
		t.Fatal(err)
	}
	flags, set, notes := plan.Flags, plan.Set, plan.Notes
	if Label(plan.Resolved) != "debian:trixie="+resolved {
		t.Errorf("resolved %v", plan.Resolved)
	}
	if strings.Join(asked, " ") != "debian:trixie" {
		t.Errorf("resolved %q", asked)
	}
	want := []string{
		"--build-context", "golang:1.25=docker-image://" + pins["golang:1.25"],
		"--build-context", "debian:trixie=docker-image://" + resolved,
	}
	if strings.Join(flags, " ") != strings.Join(want, " ") {
		t.Errorf("flags %q", flags)
	}
	if len(notes) != 1 || !strings.Contains(notes[0], "debian:trixie is not in BaseImages in x.go") || !strings.Contains(notes[0], resolved) {
		t.Errorf("notes %q", notes)
	}
	// the set digest does not depend on the order of the bases
	plan2, err := Contexts(context.Background(), pins, "BaseImages in x.go", []string{byDigest, "debian:trixie", "golang:1.25"}, resolve)
	if err != nil || plan2.Set != set || len(set) != 64 {
		t.Errorf("set %q and %q: %v", set, plan2.Set, err)
	}
	failing := func(context.Context, string) (string, error) { return "", errors.New("no registry") }
	if _, err := Contexts(context.Background(), pins, "BaseImages in x.go", []string{"debian:trixie"}, failing); err == nil ||
		!strings.Contains(err.Error(), "no registry") || !strings.Contains(err.Error(), "BaseImages in x.go") {
		t.Errorf("unresolvable: %v", err)
	}
}

func TestRepository(t *testing.T) {
	for ref, want := range map[string]string{
		"golang:1.25.9-bookworm":                   "docker.io/library/golang",
		"ubuntu":                                   "docker.io/library/ubuntu",
		"stellar/stellar-rpc:30.0.1":               "docker.io/stellar/stellar-rpc",
		"ghcr.io/stellar/x:1":                      "ghcr.io/stellar/x",
		"localhost:5000/x":                         "localhost:5000/x",
		"localhost/x:2":                            "localhost/x",
		"golang@sha256:" + strings.Repeat("a", 64): "docker.io/library/golang",
	} {
		if got := Repository(ref); got != want {
			t.Errorf("%s: %s, want %s", ref, got, want)
		}
	}
}

func TestRecordedResolutionsAreReused(t *testing.T) {
	d1 := "docker.io/library/debian@sha256:" + strings.Repeat("44", 32)
	d2 := "ghcr.io/x/y@sha256:" + strings.Repeat("55", 32)
	label := Label(map[string]string{"debian:trixie": d1, "ghcr.io/x/y:2": d2})
	got := Recorded(label)
	if len(got) != 2 || got["debian:trixie"] != d1 || got["ghcr.io/x/y:2"] != d2 {
		t.Fatalf("%q read back as %v", label, got)
	}
	if len(Recorded("<no value>")) != 0 || len(Recorded("")) != 0 || len(Recorded("a=b")) != 0 {
		t.Fatal("an unreadable label was read")
	}
	asked := 0
	r := Cached(got, func(context.Context, string) (string, error) {
		asked++
		return d2, nil
	})
	if d, err := r(context.Background(), "debian:trixie"); err != nil || d != d1 || asked != 0 {
		t.Fatalf("recorded: %s %v, asked %d", d, err, asked)
	}
	if d, err := r(context.Background(), "alpine:3"); err != nil || d != d2 || asked != 1 {
		t.Fatalf("not recorded: %s %v, asked %d", d, err, asked)
	}
}
