package stellarrpc

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
)

// every page request names its limit, at or below stellar-rpc's maximum for
// the method (rpcv1/config/options.go: max-transactions-limit 200,
// max-ledgers-limit 200, max-events-limit 10000), so a page never grows to
// what the server would pick on its own
func TestEveryPageRequestSetsALimit(t *testing.T) {
	limits := map[string]float64{"getTransactions": 200, "getLedgers": 200, "getEvents": 10000}
	var mu sync.Mutex
	var seen []map[string]any
	calls := map[string]int{}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string         `json:"method"`
			Params map[string]any `json:"params"`
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Error(err)
			return
		}
		mu.Lock()
		req.Params["method"] = req.Method
		seen = append(seen, req.Params)
		calls[req.Method]++
		n := calls[req.Method]
		mu.Unlock()
		// the first page is full, so the client asks for a second one
		full := n == 1
		var result map[string]any
		switch req.Method {
		case "getTransactions":
			var txs []any
			if full {
				for range txPage {
					txs = append(txs, map[string]any{"ledger": 10, "txHash": "aa"})
				}
			}
			result = map[string]any{"transactions": txs, "cursor": "1"}
		case "getLedgers":
			l := []any{map[string]any{"sequence": 9 + n}}
			result = map[string]any{"ledgers": l, "cursor": "1"}
		case "getEvents":
			var evs []any
			if full {
				for range eventPage {
					evs = append(evs, map[string]any{"id": "x"})
				}
			}
			result = map[string]any{"events": evs, "cursor": "1"}
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer srv.Close()
	c := newClient(srv.URL)
	ctx := context.Background()
	if _, err := c.transactions(ctx, 10, 20, ""); err != nil {
		t.Fatal(err)
	}
	if _, err := c.ledgers(ctx, 10, 11, "json"); err != nil {
		t.Fatal(err)
	}
	if _, err := c.events(ctx, 10, 20, ""); err != nil {
		t.Fatal(err)
	}
	if calls["getTransactions"] != 2 || calls["getLedgers"] != 2 || calls["getEvents"] != 2 {
		t.Fatalf("pages asked: %v", calls)
	}
	for _, p := range seen {
		page, _ := p["pagination"].(map[string]any)
		limit, ok := page["limit"].(float64)
		m := p["method"].(string)
		if !ok || limit <= 0 || limit > limits[m] {
			t.Errorf("%s request without a limit within %v: %v", m, limits[m], p)
		}
	}
}
