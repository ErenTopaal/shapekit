package wire

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

func TestParseRequest(t *testing.T) {
	r, err := parseRequest([]byte(`{"id":"1","cmd":"start","chain":"c","network":"testnet","network_passphrase":"p","protocol":28,"delivery":{"mode":"rpc","rpc_url":"http://g:1/","archive_url":"http://g:1/archive/","lake":{"url":"u","path":"/l","ledgers_per_batch":1,"batches_per_partition":64000}},"ledgers":{"bootstrap":63,"last":72,"checkpoint_frequency":64},"limits":{"memory_bytes":4096},"docker_network":"n","workdir":"/w","later":1}`))
	if err != nil {
		t.Fatal(err)
	}
	if r.Start == nil || r.Start.Ledgers.Bootstrap != 63 || r.Start.Delivery.Lake.BatchesPerPartition != 64000 || r.Start.DockerNetwork != "n" {
		t.Fatalf("start: %+v", r.Start)
	}
	for _, bad := range []string{`{"cmd":"status"}`, `{"id":"1","cmd":"advance"}`, `{"id":"1","cmd":"export","from":5,"to":4}`, `{"id":"1","cmd":"nope"}`, `not json`} {
		if _, err := parseRequest([]byte(bad)); err == nil {
			t.Errorf("%s parsed", bad)
		}
	}
}

func TestChangeFlattens(t *testing.T) {
	b, err := json.Marshal(Change{Kind: "signer-added", Fields: map[string]any{"account": "G", "weight": 1}, Source: &RowRef{Table: "state_changes"}})
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	if err := json.Unmarshal(b, &m); err != nil {
		t.Fatal(err)
	}
	if m["kind"] != "signer-added" || m["account"] != "G" || m["weight"] != float64(1) || m["source"] == nil {
		t.Fatalf("%s", b)
	}
	if _, ok := m["raw"]; ok {
		t.Fatalf("empty raw was written: %s", b)
	}
}

type fakeHandler struct{}

func (fakeHandler) Describe(int) (DescribeResult, error)   { return DescribeResult{}, nil }
func (fakeHandler) Start(StartParams) (StartResult, error) { return StartResult{}, nil }
func (fakeHandler) Advance(h uint32) (AdvanceResult, error) {
	return AdvanceResult{Head: h}, nil
}
func (fakeHandler) Status() (StatusResult, error) { return StatusResult{}, nil }
func (fakeHandler) Export(uint32, uint32, func(any) error) (ExportResult, error) {
	return ExportResult{}, nil
}
func (fakeHandler) Stop() (StopResult, error) { return StopResult{}, nil }
func (fakeHandler) Close() error              { return nil }

type fakeDumper struct{ fakeHandler }

func (fakeDumper) Dump(_ []string, table func(DumpTable) error, row func(DumpRow) error) (DumpResult, error) {
	if err := table(DumpTable{Name: "t", Columns: []DumpColumn{{Name: "a", Type: "bytea"}, {Name: "b", Type: "text"}}}); err != nil {
		return DumpResult{}, err
	}
	v := "00ff"
	if err := row(DumpRow{Table: "t", Cells: []*string{&v, nil}}); err != nil {
		return DumpResult{}, err
	}
	return DumpResult{Tables: 1, Rows: 1}, nil
}

func serveLines(t *testing.T, h Handler, in string) []map[string]any {
	t.Helper()
	var out strings.Builder
	if err := Serve(strings.NewReader(in), &out, h); err != nil {
		t.Fatal(err)
	}
	var lines []map[string]any
	for l := range strings.SplitSeq(strings.TrimSpace(out.String()), "\n") {
		var m map[string]any
		if err := json.Unmarshal([]byte(l), &m); err != nil {
			t.Fatalf("%q: %v", l, err)
		}
		lines = append(lines, m)
	}
	return lines
}

func TestDumpStreamsTablesAndRows(t *testing.T) {
	lines := serveLines(t, fakeDumper{}, `{"id":"1","cmd":"dump"}`+"\n"+`{"id":"2","cmd":"stop"}`+"\n")
	if len(lines) != 4 {
		t.Fatalf("%v", lines)
	}
	if lines[0]["status"] != "table" || lines[1]["status"] != "row" || lines[2]["status"] != "ok" {
		t.Fatalf("%v", lines)
	}
	cells := lines[1]["row"].(map[string]any)["cells"].([]any)
	if cells[0] != "00ff" || cells[1] != nil {
		t.Fatalf("cells %v", cells)
	}
}

func TestDumpWithoutDumperIsUnsupported(t *testing.T) {
	lines := serveLines(t, fakeHandler{}, `{"id":"1","cmd":"dump","tables":["t"]}`+"\n"+`{"id":"2","cmd":"stop"}`+"\n")
	if lines[0]["status"] != "error" || lines[0]["error"].(map[string]any)["class"] != "unsupported" {
		t.Fatalf("%v", lines)
	}
}

type closeCounter struct {
	fakeHandler
	closed *int
}

func (c closeCounter) Close() error { *c.closed++; return nil }

type failingWriter struct{}

func (failingWriter) Write([]byte) (int, error) { return 0, errors.New("the harness is gone") }

// a target must not outlive a harness that can no longer be answered or read
func TestServeTearsDownWhenItCannotGoOn(t *testing.T) {
	closed := 0
	err := Serve(strings.NewReader(`{"id":"1","cmd":"status"}`+"\n"), failingWriter{}, closeCounter{closed: &closed})
	if err == nil || closed != 1 {
		t.Fatalf("write failure: err %v, closed %d times", err, closed)
	}
	closed = 0
	long := `{"id":"1","cmd":"status","pad":"` + strings.Repeat("x", 17<<20) + `"}` + "\n"
	err = Serve(strings.NewReader(long), &strings.Builder{}, closeCounter{closed: &closed})
	if err == nil || closed != 1 {
		t.Fatalf("read failure: err %v, closed %d times", err, closed)
	}
}
