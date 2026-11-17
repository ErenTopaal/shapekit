package stellarrpc

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// client calls stellar-rpc's JSON-RPC endpoint.
type client struct {
	url  string
	http *http.Client
}

func newClient(url string) *client {
	return &client{url: url, http: &http.Client{Timeout: 60 * time.Second}}
}

// rpcError is an error object stellar-rpc answered with.
type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func (e *rpcError) Error() string { return fmt.Sprintf("json-rpc error %d: %s", e.Code, e.Message) }

// call sends one request and decodes its result into out, keeping numbers as
// json.Number when out is a map or an interface.
func (c *client) call(ctx context.Context, method string, params, out any) error {
	req := map[string]any{"jsonrpc": "2.0", "id": 1, "method": method}
	if params != nil {
		req["params"] = params
	}
	body, err := json.Marshal(req)
	if err != nil {
		return err
	}
	hr, err := http.NewRequestWithContext(ctx, http.MethodPost, c.url, bytes.NewReader(body))
	if err != nil {
		return err
	}
	hr.Header.Set("Content-Type", "application/json")
	resp, err := c.http.Do(hr)
	if err != nil {
		return err
	}
	defer func() { _ = resp.Body.Close() }()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("%s: reading the answer: %w", method, err)
	}
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("%s: %s %s", method, resp.Status, strings.TrimSpace(string(raw)))
	}
	var env struct {
		Result json.RawMessage `json:"result"`
		Error  *rpcError       `json:"error"`
	}
	if err := json.Unmarshal(raw, &env); err != nil {
		return fmt.Errorf("%s: decoding the answer: %w", method, err)
	}
	if env.Error != nil {
		return fmt.Errorf("%s: %w", method, env.Error)
	}
	dec := json.NewDecoder(bytes.NewReader(env.Result))
	dec.UseNumber()
	if err := dec.Decode(out); err != nil {
		return fmt.Errorf("%s: decoding the result: %w", method, err)
	}
	return nil
}

type versionInfo struct {
	Version         string `json:"version"`
	CommitHash      string `json:"commitHash"`
	ProtocolVersion uint32 `json:"protocolVersion"`
}

type latestLedger struct {
	Sequence uint32 `json:"sequence"`
}

// served is one item of a page exactly as stellar-rpc answered it, and the
// members the adapter reads from it.
type served struct {
	raw map[string]any
}

func (s served) str(k string) (string, error) {
	v, ok := s.raw[k].(string)
	if !ok {
		return "", fmt.Errorf("member %q is %T, not a string", k, s.raw[k])
	}
	return v, nil
}

func (s served) uint(k string) (uint32, error) {
	var text string
	switch v := s.raw[k].(type) {
	case json.Number:
		text = v.String()
	case string:
		text = v
	default:
		return 0, fmt.Errorf("member %q is %T, not a number", k, s.raw[k])
	}
	n, err := strconv.ParseUint(text, 10, 32)
	if err != nil {
		return 0, fmt.Errorf("member %q: %w", k, err)
	}
	return uint32(n), nil
}

func (s served) strings(k string) ([]string, error) {
	v, ok := s.raw[k]
	if !ok || v == nil {
		return nil, nil
	}
	list, ok := v.([]any)
	if !ok {
		return nil, fmt.Errorf("member %q is %T, not an array", k, v)
	}
	out := make([]string, len(list))
	for i, x := range list {
		if out[i], ok = x.(string); !ok {
			return nil, fmt.Errorf("member %q item %d is %T, not a string", k, i, x)
		}
	}
	return out, nil
}

type page struct {
	Transactions []map[string]any `json:"transactions"`
	Events       []map[string]any `json:"events"`
	Ledgers      []map[string]any `json:"ledgers"`
	Cursor       string           `json:"cursor"`
}

// Page sizes: each method's maximum in stellar-rpc's default configuration
// (cmd/stellar-rpc/internal/rpcv1/config/options.go: max-transactions-limit
// 200, max-events-limit 10000, max-ledgers-limit 200).
const (
	txPage     = 200
	eventPage  = 10000
	ledgerPage = 200
)

// transactions pages getTransactions from ledger from and returns every
// transaction of ledgers from..to in the order served.
func (c *client) transactions(ctx context.Context, from, to uint32, format string) ([]served, error) {
	var out []served
	params := map[string]any{"startLedger": from, "pagination": map[string]any{"limit": txPage}}
	for {
		if format != "" {
			params["xdrFormat"] = format
		}
		var p page
		if err := c.call(ctx, "getTransactions", params, &p); err != nil {
			return nil, err
		}
		past := false
		for _, t := range p.Transactions {
			s := served{t}
			l, err := s.uint("ledger")
			if err != nil {
				return nil, fmt.Errorf("getTransactions: %w", err)
			}
			if l > to {
				past = true
				break
			}
			out = append(out, s)
		}
		if past || len(p.Transactions) < txPage {
			return out, nil
		}
		params = map[string]any{"pagination": map[string]any{"cursor": p.Cursor, "limit": txPage}}
	}
}

// ledgers pages getLedgers over ledgers from..to.
func (c *client) ledgers(ctx context.Context, from, to uint32, format string) ([]served, error) {
	var out []served
	params := map[string]any{"startLedger": from, "pagination": map[string]any{"limit": ledgerPage}}
	for {
		if format != "" {
			params["xdrFormat"] = format
		}
		var p page
		if err := c.call(ctx, "getLedgers", params, &p); err != nil {
			return nil, err
		}
		for _, l := range p.Ledgers {
			s := served{l}
			seq, err := s.uint("sequence")
			if err != nil {
				return nil, fmt.Errorf("getLedgers: %w", err)
			}
			if seq > to {
				return out, nil
			}
			out = append(out, s)
			if seq == to {
				return out, nil
			}
		}
		if len(p.Ledgers) == 0 {
			return out, nil
		}
		params = map[string]any{"pagination": map[string]any{"cursor": p.Cursor, "limit": ledgerPage}}
	}
}

// events pages getEvents over ledgers from..to with no filter, so every
// stored event is returned. endLedger is exclusive in stellar-rpc
// (cmd/stellar-rpc/internal/methods/get_events.go).
func (c *client) events(ctx context.Context, from, to uint32, format string) ([]served, error) {
	var out []served
	params := map[string]any{"startLedger": from, "endLedger": to + 1, "filters": []any{},
		"pagination": map[string]any{"limit": eventPage}}
	for {
		if format != "" {
			params["xdrFormat"] = format
		}
		var p page
		if err := c.call(ctx, "getEvents", params, &p); err != nil {
			return nil, err
		}
		for _, e := range p.Events {
			out = append(out, served{e})
		}
		// a page shorter than the limit ends the window; its cursor then names
		// the window's last ledger
		if len(p.Events) < eventPage {
			return out, nil
		}
		params = map[string]any{"endLedger": to + 1, "filters": []any{},
			"pagination": map[string]any{"cursor": p.Cursor, "limit": eventPage}}
	}
}
