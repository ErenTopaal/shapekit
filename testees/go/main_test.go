package main

import (
	"bytes"
	"encoding/json"
	"runtime/debug"
	"slices"
	"strings"
	"testing"
)

func TestServe(t *testing.T) {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		t.Fatal("test binary has no build info")
	}

	in := strings.Join([]string{
		`{"op":"hello"}`,
		`{"op":"decode","xdr":"AAAA"}`,
		`{}`,
		`not json`,
		`{"op":"hello","extra":1}`,
	}, "\n") + "\n"

	var out bytes.Buffer
	if err := serve(strings.NewReader(in), &out); err != nil {
		t.Fatalf("serve: %v", err)
	}

	version, err := json.Marshal(info.Main.Version)
	if err != nil {
		t.Fatal(err)
	}

	hello := `{"sdk":"go-stellar-sdk","version":` + string(version) + `,"ops":["hello"]}`
	want := []string{
		hello,
		`{"error":"unsupported op"}`,
		`{"error":"unsupported op"}`,
		`{"error":"invalid request: invalid character 'o' in literal null (expecting 'u')"}`,
		hello,
	}

	got := strings.Split(strings.TrimSuffix(out.String(), "\n"), "\n")
	if !slices.Equal(got, want) {
		t.Errorf("responses:\n%s\nwant:\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
}

func TestServeLongLine(t *testing.T) {
	// a request bigger than bufio's default token size must still be read
	line := `{"op":"decode","xdr":"` + strings.Repeat("A", 1<<20) + `"}` + "\n"

	var out bytes.Buffer
	if err := serve(strings.NewReader(line), &out); err != nil {
		t.Fatalf("serve: %v", err)
	}

	if got := strings.TrimSpace(out.String()); got != `{"error":"unsupported op"}` {
		t.Errorf("got %s", got)
	}
}

func TestServeLineTooLong(t *testing.T) {
	line := strings.Repeat("A", maxLineBytes+1)

	var out bytes.Buffer
	if err := serve(strings.NewReader(line), &out); err == nil {
		t.Fatal("serve accepted a line over the limit")
	}
}
