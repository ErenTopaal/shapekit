package stellarrpc

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"sort"
	"strings"
	"testing"

	"github.com/stellar/go-stellar-sdk/xdr"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

func repoRoot(t *testing.T) string {
	t.Helper()
	root, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatal(err)
	}
	return root
}

// readLedgers reads a chain's ledgers.xdr: record-marked LedgerCloseMeta
// values (a 4-byte big-endian length with the last-fragment bit set).
func readLedgers(t *testing.T, path string) []xdr.LedgerCloseMeta {
	t.Helper()
	b, err := os.ReadFile(path) //nolint:gosec // a committed chain file
	if err != nil {
		t.Fatal(err)
	}
	r := bytes.NewReader(b)
	var out []xdr.LedgerCloseMeta
	for {
		var mark uint32
		if err := binary.Read(r, binary.BigEndian, &mark); errors.Is(err, io.EOF) {
			return out
		} else if err != nil {
			t.Fatal(err)
		}
		if mark&0x80000000 == 0 {
			t.Fatalf("%s: record mark %x without the last-fragment bit", path, mark)
		}
		buf := make([]byte, mark&0x7fffffff)
		if _, err := io.ReadFull(r, buf); err != nil {
			t.Fatal(err)
		}
		var lcm xdr.LedgerCloseMeta
		if err := lcm.UnmarshalBinary(buf); err != nil {
			t.Fatalf("%s: %v", path, err)
		}
		out = append(out, lcm)
	}
}

// project keeps the claimed fields of an event change, as canonical JSON.
func project(kind string, fields map[string]any) string {
	var claim []string
	for _, c := range Claims().Changes {
		if c.Kind == kind {
			claim = c.Fields
		}
	}
	m := map[string]any{"kind": kind}
	for _, f := range claim {
		if v, ok := fields[f]; ok && v != nil {
			m[f] = v
		}
	}
	b, err := json.Marshal(m)
	if err != nil {
		panic(err)
	}
	return string(b)
}

type oracleFile struct {
	Ledgers []struct {
		LedgerSeq    uint32 `json:"ledger_seq"`
		Transactions []struct {
			Hash    string           `json:"hash"`
			Changes []map[string]any `json:"changes"`
		} `json:"transactions"`
	} `json:"ledgers"`
}

// The event mapping applied to core's own meta gives the oracle's events for
// every transaction of every committed oracle file whose chain is in the
// corpus, so a difference in a harness run comes from what stellar-rpc
// served, not from the mapping.
func TestEventMappingMatchesTheOracleOnCoreMeta(t *testing.T) {
	root := repoRoot(t)
	files, err := filepath.Glob(filepath.Join(root, "corpus", "entries", "*", "*", "*.oracle.json"))
	if err != nil {
		t.Fatal(err)
	}
	want := map[string][]string{}
	for _, f := range files {
		b, err := os.ReadFile(f) //nolint:gosec // a committed oracle file
		if err != nil {
			t.Fatal(err)
		}
		var o oracleFile
		dec := json.NewDecoder(bytes.NewReader(b))
		dec.UseNumber()
		if err := dec.Decode(&o); err != nil {
			t.Fatalf("%s: %v", f, err)
		}
		net := strings.TrimSuffix(filepath.Base(f), ".oracle.json")
		for _, l := range o.Ledgers {
			for _, tx := range l.Transactions {
				var evs []string
				for _, c := range tx.Changes {
					kind, _ := c["kind"].(string)
					if kind == "token-event" || kind == "contract-event" {
						evs = append(evs, project(kind, c))
					}
				}
				sort.Strings(evs)
				want[net+" "+tx.Hash] = evs
			}
		}
	}
	chains, err := filepath.Glob(filepath.Join(root, "corpus", "chains", "*", "*", "ledgers.xdr"))
	if err != nil {
		t.Fatal(err)
	}
	if len(chains) == 0 {
		t.Fatal("no corpus chains")
	}
	checked, events := 0, 0
	for _, path := range chains {
		net := filepath.Base(filepath.Dir(path))
		for _, lcm := range readLedgers(t, path) {
			txs, err := lcmTransactions(lcm)
			if err != nil {
				t.Fatal(err)
			}
			for _, tx := range txs {
				hash := hex.EncodeToString(tx.hash[:])
				exp, ok := want[net+" "+hash]
				if !ok {
					continue
				}
				evs, err := metaEvents(tx.meta)
				if err != nil {
					t.Fatalf("%s %s: %v", path, hash, err)
				}
				var got []string
				for _, e := range evs {
					c, err := e.change()
					if err != nil {
						t.Fatalf("%s %s: %v", path, hash, err)
					}
					if c != nil {
						got = append(got, project(c.Kind, roundTrip(t, c.Fields)))
					}
				}
				sort.Strings(got)
				if !slices.Equal(got, exp) {
					t.Errorf("%s %s:\n got %v\nwant %v", path, hash, got, exp)
				}
				checked++
				events += len(got)
			}
		}
	}
	if checked == 0 || events == 0 {
		t.Fatalf("checked %d transactions with %d events", checked, events)
	}
	t.Logf("%d transactions, %d events agree with the oracle", checked, events)
}

// roundTrip passes fields through JSON so numbers compare as the oracle
// file's do.
func roundTrip(t *testing.T, m map[string]any) map[string]any {
	t.Helper()
	b, err := json.Marshal(m)
	if err != nil {
		t.Fatal(err)
	}
	dec := json.NewDecoder(bytes.NewReader(b))
	dec.UseNumber()
	var out map[string]any
	if err := dec.Decode(&out); err != nil {
		t.Fatal(err)
	}
	return out
}

func TestIDStage(t *testing.T) {
	cases := []struct {
		tx, op uint32
		stage  string
		index  uint32
	}{
		{0, 0, StageBeforeAllTxs, 0},
		{3, 0, StageOperation, 0},
		{3, 2, StageOperation, 2},
		{3, 4095, StageAfterTx, 0},
		{1048575, 0, StageAfterAllTxs, 0},
	}
	for _, c := range cases {
		s, i := idStage(c.tx, c.op)
		if s != c.stage || i != c.index {
			t.Errorf("tx %d op %d: %s %d", c.tx, c.op, s, i)
		}
	}
}

func TestLossyMatchesRust(t *testing.T) {
	cases := map[string]string{
		"abc":                "abc",
		"a\xffb":             "a\ufffdb",
		"\xe2\x82":           "\ufffd", // a truncated three-byte sequence is one maximal subpart
		"\xe2\x82\xac":       "\u20ac",
		"\xf0\x9f\x98":       "\ufffd",
		"\xed\xa0\x80":       "\ufffd\ufffd\ufffd", // a surrogate: ED A0 is not a valid prefix
		"\xc0\xaf":           "\ufffd\ufffd",
		"x\xf4\x90\x80\x80y": "x\ufffd\ufffd\ufffd\ufffdy",
	}
	for in, want := range cases {
		if got := lossy([]byte(in)); got != want {
			t.Errorf("%q: got %q, want %q", in, got, want)
		}
	}
}

func TestAmountDataShapes(t *testing.T) {
	sym := func(s string) xdr.ScVal {
		v := xdr.ScSymbol(s)
		return xdr.ScVal{Type: xdr.ScValTypeScvSymbol, Sym: &v}
	}
	// the two's complement words of a small value
	i := func(n int64) xdr.ScVal {
		lo := xdr.Uint64(n) //nolint:gosec // the low word of n, sign bits included
		return xdr.ScVal{Type: xdr.ScValTypeScvI128, I128: &xdr.Int128Parts{Hi: xdr.Int64(n >> 63), Lo: lo}}
	}
	u := xdr.Uint64(7)
	mapOf := func(es ...xdr.ScMapEntry) xdr.ScVal {
		m := xdr.ScMap(es)
		p := &m
		return xdr.ScVal{Type: xdr.ScValTypeScvMap, Map: &p}
	}
	if a, m, ok := amountData(i(-5)); !ok || a != "-5" || m != nil {
		t.Fatal(a, m, ok)
	}
	if a, m, ok := amountData(mapOf(xdr.ScMapEntry{Key: sym("amount"), Val: i(9)},
		xdr.ScMapEntry{Key: sym("to_muxed_id"), Val: xdr.ScVal{Type: xdr.ScValTypeScvU64, U64: &u}})); !ok || a != "9" || m["value"] != "7" {
		t.Fatal(a, m, ok)
	}
	if _, _, ok := amountData(mapOf(xdr.ScMapEntry{Key: sym("amount"), Val: i(9)}, xdr.ScMapEntry{Key: sym("other"), Val: i(1)})); ok {
		t.Fatal("a map with another key is not token data")
	}
	if _, _, ok := amountData(mapOf(xdr.ScMapEntry{Key: sym("to_muxed_id"), Val: xdr.ScVal{Type: xdr.ScValTypeScvU64, U64: &u}})); ok {
		t.Fatal("a map without an amount is not token data")
	}
}

func TestBatchNames(t *testing.T) {
	if l, ok := batchLast("FFFFFFFD--2.xdr.zst"); !ok || l != 2 {
		t.Fatal(l, ok)
	}
	if l, ok := batchLast("FFFFFFFD--2-5.xdr.zst"); !ok || l != 5 {
		t.Fatal(l, ok)
	}
	if _, ok := batchLast(".config.json"); ok {
		t.Fatal("a config file is not a batch")
	}
}

func TestIgnoredErrorsAreTheLivePathOnly(t *testing.T) {
	retry := `time="x" level=error msg="could not run ingestion. Retrying" error="error starting prepare range: opening subprocess: trying to start online mode too far (latest checkpoint=63)" pid=1`
	fatal := `time="x" level=fatal msg="could not backfill" error="frontfill failed" pid=1`
	got := newestError(retry + "\n" + fatal + "\n" + retry + "\n")
	if got == nil || *got != fatal {
		t.Fatalf("%v", got)
	}
	if newestError(retry+"\n") != nil {
		t.Fatal("the captive core retry counted as an error")
	}
}

func TestLeavesFlattenEveryScalar(t *testing.T) {
	v := map[string]any{"a": []any{"G1", map[string]any{"b": json.Number("3")}}, "c": map[string]any{}, "d": nil}
	rows := leaves("k", v)
	got := map[string]any{}
	for _, r := range rows {
		got[r["path"].(string)] = r["value"]
	}
	want := map[string]any{"a[0]": "G1", "a[1].b": json.Number("3"), "c": "{}", "d": nil}
	if fmt.Sprint(got) != fmt.Sprint(want) {
		t.Fatalf("%v", got)
	}
}

func TestDumpTablesAreSorted(t *testing.T) {
	if !sort.StringsAreSorted(Tables) {
		t.Fatal(Tables)
	}
	head, body, err := dumpTable("t", []map[string]any{{"b": json.Number("2"), "a": "y"}, {"a": "x", "c": map[string]any{"k": 1}}})
	if err != nil {
		t.Fatal(err)
	}
	if len(head.Columns) != 3 || head.Columns[2].Type != "json" || *body[0].Cells[0] != "x" || body[0].Cells[1] != nil {
		t.Fatalf("%+v %+v", head, body)
	}
}

// pins.toml lists the image and the stellar-rpc release this adapter runs.
func TestImageMatchesPins(t *testing.T) {
	pins, err := os.ReadFile(filepath.Join(repoRoot(t), "pins.toml"))
	if err != nil {
		t.Fatal(err)
	}
	d := regexp.MustCompile(`sha256:[0-9a-f]{64}`).FindString(Image)
	if d == "" || !bytes.Contains(pins, []byte(d)) {
		t.Errorf("%s is not pinned in pins.toml", Image)
	}
	if !bytes.Contains(pins, []byte(Commit)) {
		t.Errorf("commit %s is not pinned in pins.toml", Commit)
	}
}

func TestClaimsAreVocabularyKinds(t *testing.T) {
	for _, c := range Claims().Changes {
		if c.Kind != "token-event" && c.Kind != "contract-event" {
			t.Errorf("claim %s", c.Kind)
		}
	}
	var _ wire.Dumper = (*Adapter)(nil)
}

func TestReleaseDefaultsToThePinAndMustBeWhole(t *testing.T) {
	a, err := New(context.Background(), Config{EventsFrom: FromEvents})
	if err != nil || a.cfg.Release != Pinned {
		t.Fatalf("default release: %v, %+v", err, a)
	}
	if _, err := New(context.Background(), Config{EventsFrom: FromEvents, Release: Release{Image: "stellar/stellar-rpc:30.0.2"}}); err == nil {
		t.Fatal("an image without its version and commit was taken")
	}
}
