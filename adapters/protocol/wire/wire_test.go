package wire

import (
	"encoding/json"
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
