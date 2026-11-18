package stellaretl

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

func decode(t *testing.T, s string) map[string]any {
	t.Helper()
	dec := json.NewDecoder(strings.NewReader(s))
	dec.UseNumber()
	var m map[string]any
	if err := dec.Decode(&m); err != nil {
		t.Fatal(err)
	}
	return m
}

func TestEffectsMapToTheVocabulary(t *testing.T) {
	g := "GB4WH2VTILAXT2GUDH4KI2ZA2GGYSLJJUYBH6LP42LXI4WNBRIHHDDVT"
	cases := []struct {
		row     string
		created bool
		kind    string
		field   string
		want    any
	}{
		{`{"type_string":"account_created","address":"` + g + `","details":{"starting_balance":"100.0000000"}}`, false, "account-created", "starting_balance", "1000000000"},
		{`{"type_string":"trustline_created","address":"` + g + `","details":{"asset_type":"credit_alphanum4","asset_code":"USD","asset_issuer":"` + g + `","limit":"922337203685.4775807"}}`, false, "trustline-created", "limit", "9223372036854775807"},
		{`{"type_string":"signer_updated","address":"` + g + `","details":{"public_key":"` + g + `","weight":0}}`, false, "signer-weight", "after", int64(0)},
		{`{"type_string":"data_created","address":"` + g + `","details":{"name":"k","value":"AQI="}}`, false, "data-entry", "value_after_hex", "0102"},
	}
	for _, c := range cases {
		ch, note := effectChange(decode(t, c.row), c.created)
		if ch == nil {
			t.Fatalf("%s: %s", c.row, note)
		}
		if ch.Kind != c.kind || ch.Fields[c.field] != c.want {
			t.Errorf("%s: got %s %v", c.row, ch.Kind, ch.Fields)
		}
	}
	master := `{"type_string":"signer_created","address":"` + g + `","details":{"public_key":"` + g + `","weight":1}}`
	if ch, _ := effectChange(decode(t, master), true); ch != nil {
		t.Errorf("the new account's master key mapped to %v", ch)
	}
}

func TestDumpTableSortsAndKeepsEveryKey(t *testing.T) {
	rows := []map[string]any{decode(t, `{"b":2,"a":"y"}`), decode(t, `{"a":"x","c":{"k":1}}`)}
	head, body, err := dumpTable("t", rows)
	if err != nil {
		t.Fatal(err)
	}
	if len(head.Columns) != 3 || head.Columns[2].Type != "json" {
		t.Fatalf("%+v", head)
	}
	if *body[0].Cells[0] != "x" || body[0].Cells[1] != nil || *body[0].Cells[2] != `{"k":1}` {
		t.Fatalf("%v", *body[0].Cells[0])
	}
}

func TestBatchNames(t *testing.T) {
	if l, ok := batchLast("FFFFFFFD--2.xdr.zst"); !ok || l != 2 {
		t.Fatal(l, ok)
	}
	if l, ok := batchLast("FFFFFFFD--2-5.xdr.zst"); !ok || l != 5 {
		t.Fatal(l, ok)
	}
}

// pins.toml lists the images and the stellar-etl commit this adapter uses.
func TestImagesMatchPins(t *testing.T) {
	pins, err := os.ReadFile(filepath.Join("..", "..", "pins.toml"))
	if err != nil {
		t.Fatal(err)
	}
	digest := regexp.MustCompile(`sha256:[0-9a-f]{64}`)
	for _, ref := range append([]string{GCSImage}, values(BaseImages)...) {
		d := digest.FindString(ref)
		if d == "" || !bytes.Contains(pins, []byte(d)) {
			t.Errorf("%s is not pinned in pins.toml", ref)
		}
	}
}

func values(m map[string]string) []string {
	var out []string
	for _, v := range m {
		out = append(out, v)
	}
	return out
}

func TestAFeeRefundIsNotTheClaimedCharge(t *testing.T) {
	if c, _ := tokenChange(decode(t, `{"event_topic":"fee","amount_raw":"-4839","from":"G"}`)); c != nil {
		t.Fatalf("a refund mapped to %v", c)
	}
	if c, _ := tokenChange(decode(t, `{"event_topic":"fee","amount_raw":"11951","from":"G"}`)); c == nil {
		t.Fatal("the charge did not map")
	}
}

func TestDumpTableOfNoRowsHasAnEmptyColumnList(t *testing.T) {
	head, body, err := dumpTable("t", nil)
	if err != nil {
		t.Fatal(err)
	}
	b, err := json.Marshal(head)
	if err != nil {
		t.Fatal(err)
	}
	if string(b) != `{"name":"t","columns":[]}` || len(body) != 0 {
		t.Fatalf("%s %v", b, body)
	}
}
