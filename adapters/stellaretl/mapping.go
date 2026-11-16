package stellaretl

import (
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"sort"
	"strconv"

	"github.com/stellar/go-stellar-sdk/amount"
	"github.com/stellar/go-stellar-sdk/strkey"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// toidTx clears the operation part of a TOID, leaving the transaction's.
func toidTx(id int64) int64 { return id &^ 0xFFF }

func str(row map[string]any, k string) (string, bool) {
	v, ok := row[k]
	if !ok || v == nil {
		return "", false
	}
	switch x := v.(type) {
	case string:
		return x, true
	case json.Number:
		return x.String(), true
	case bool:
		return strconv.FormatBool(x), true
	}
	return "", false
}

func details(row map[string]any) map[string]any {
	d, _ := row["details"].(map[string]any)
	return d
}

func stroops(s string) (string, error) {
	v, err := amount.ParseInt64(s)
	if err != nil {
		return "", fmt.Errorf("amount %q: %w", s, err)
	}
	return strconv.FormatInt(v, 10), nil
}

// asset names an asset from stellar-etl's asset_type, asset_code and
// asset_issuer details, or a pool-share line from its liquidity_pool_id.
func asset(d map[string]any) (string, error) {
	t, _ := str(d, "asset_type")
	switch t {
	case "native":
		return "native", nil
	case "credit_alphanum4", "credit_alphanum12":
		code, ok1 := str(d, "asset_code")
		issuer, ok2 := str(d, "asset_issuer")
		if !ok1 || !ok2 {
			return "", fmt.Errorf("asset of type %s without code or issuer", t)
		}
		return code + ":" + issuer, nil
	case "liquidity_pool_shares":
		id, ok := str(d, "liquidity_pool_id")
		if !ok {
			return "", fmt.Errorf("pool-share asset without liquidity_pool_id")
		}
		raw, err := hex.DecodeString(id)
		if err != nil {
			return "", fmt.Errorf("liquidity_pool_id %q: %w", id, err)
		}
		l, err := strkey.Encode(strkey.VersionByteLiquidityPool, raw)
		if err != nil {
			return "", fmt.Errorf("liquidity_pool_id %q: %w", id, err)
		}
		return "pool-share:" + l, nil
	}
	return "", fmt.Errorf("asset type %q", t)
}

func unmapped(table string, row map[string]any, note string) wire.Unmapped {
	cat, _ := str(row, "type_string")
	if cat == "" {
		cat, _ = str(row, "event_topic")
	}
	return wire.Unmapped{Table: table, Category: cat, Row: row, Note: note}
}

// effectChange maps one effects row to a vocabulary change, or says why it
// has none. created says whether the same operation created the row's
// account: stellar-etl then lists the new account's master key as a created
// signer, which the vocabulary does not record as an added signer.
func effectChange(row map[string]any, created bool) (*wire.Change, string) {
	typ, _ := str(row, "type_string")
	account, _ := str(row, "address")
	d := details(row)
	src := &wire.RowRef{Table: "effects", Category: typ}
	change := func(kind string, f map[string]any) (*wire.Change, string) {
		f["account"] = account
		return &wire.Change{Kind: kind, Fields: f, Source: src}, ""
	}
	switch typ {
	case "account_created":
		s, _ := str(d, "starting_balance")
		v, err := stroops(s)
		if err != nil {
			return nil, err.Error()
		}
		return change("account-created", map[string]any{"starting_balance": v})
	case "account_removed":
		return change("account-removed", map[string]any{})
	case "signer_created", "signer_removed", "signer_updated":
		signer, _ := str(d, "public_key")
		if signer == account {
			// the master key: its weight is threshold 0 of the account, which the
			// vocabulary records as signer-weight
			if typ == "signer_created" && created {
				return nil, "the master key of an account the same operation created; the vocabulary records no added signer for it"
			}
			f := map[string]any{"signer": signer, "after": int64(0)}
			if w, ok := d["weight"].(json.Number); ok && typ != "signer_removed" {
				n, err := w.Int64()
				if err != nil {
					return nil, fmt.Sprintf("weight %s: %v", w, err)
				}
				f["after"] = n
			}
			return change("signer-weight", f)
		}
		f := map[string]any{"signer": signer}
		if w, ok := d["weight"].(json.Number); ok {
			n, err := w.Int64()
			if err != nil {
				return nil, fmt.Sprintf("weight %s: %v", w, err)
			}
			if typ == "signer_updated" {
				f["after"] = n
			} else {
				f["weight"] = n
			}
		}
		kind := map[string]string{"signer_created": "signer-added", "signer_removed": "signer-removed", "signer_updated": "signer-weight"}[typ]
		return change(kind, f)
	case "trustline_created", "trustline_removed", "trustline_updated":
		a, err := asset(d)
		if err != nil {
			return nil, err.Error()
		}
		f := map[string]any{"asset": a}
		if typ != "trustline_removed" {
			s, _ := str(d, "limit")
			v, err := stroops(s)
			if err != nil {
				return nil, err.Error()
			}
			if typ == "trustline_created" {
				f["limit"] = v
			} else {
				f["after"] = v
			}
		}
		kind := map[string]string{"trustline_created": "trustline-created", "trustline_removed": "trustline-removed", "trustline_updated": "trustline-limit"}[typ]
		return change(kind, f)
	case "account_home_domain_updated":
		hd, _ := str(d, "home_domain")
		return change("home-domain", map[string]any{"after": hd})
	case "data_created", "data_updated", "data_removed":
		name, _ := str(d, "name")
		f := map[string]any{
			"action":   map[string]string{"data_created": "created", "data_updated": "updated", "data_removed": "removed"}[typ],
			"name_hex": hex.EncodeToString([]byte(name)),
		}
		if v, ok := str(d, "value"); ok && typ != "data_removed" {
			raw, err := base64.StdEncoding.DecodeString(v)
			if err != nil {
				return nil, fmt.Sprintf("data value %q: %v", v, err)
			}
			f["value_after_hex"] = hex.EncodeToString(raw)
		}
		return change("data-entry", f)
	}
	return nil, "stellar-etl effect type with no claimed vocabulary kind"
}

// tokenChange maps one token_transfer row to a token-event change.
func tokenChange(row map[string]any) (*wire.Change, string) {
	topic, _ := str(row, "event_topic")
	switch topic {
	case "transfer", "mint", "burn", "clawback", "fee":
	default:
		return nil, "token transfer event with no claimed vocabulary event"
	}
	f := map[string]any{"event": topic}
	if c, ok := str(row, "contract_id"); ok {
		f["contract"] = c
	}
	if v, ok := str(row, "from"); ok {
		f["from"] = v
	}
	if v, ok := str(row, "to"); ok {
		f["to"] = v
	}
	if v, ok := str(row, "amount_raw"); ok {
		f["amount"] = v
	}
	return &wire.Change{Kind: "token-event", Fields: f, Source: &wire.RowRef{Table: "token_transfer", Category: topic}}, ""
}

// cellText renders a decoded JSON value as dump cell text: strings as they
// are, numbers as written, booleans as true and false, objects and arrays as
// compact JSON.
func cellText(v any) (*string, string, error) {
	var s, t string
	switch x := v.(type) {
	case nil:
		return nil, "null", nil
	case string:
		s, t = x, "string"
	case json.Number:
		s, t = x.String(), "number"
	case bool:
		s, t = strconv.FormatBool(x), "boolean"
	default:
		b, err := json.Marshal(x)
		if err != nil {
			return nil, "", err
		}
		s, t = string(b), "json"
	}
	return &s, t, nil
}

// dumpTable turns export rows into a dump table: the union of their keys as
// columns, sorted, and the rows sorted by their cells.
func dumpTable(name string, rows []map[string]any) (wire.DumpTable, []wire.DumpRow, error) {
	keys := map[string]string{}
	for _, r := range rows {
		for k, v := range r {
			_, t, err := cellText(v)
			if err != nil {
				return wire.DumpTable{}, nil, err
			}
			if old, ok := keys[k]; !ok || old == "null" {
				keys[k] = t
			} else if t != "null" && old != t {
				keys[k] = "mixed"
			}
		}
	}
	names := make([]string, 0, len(keys))
	for k := range keys {
		names = append(names, k)
	}
	sort.Strings(names)
	table := wire.DumpTable{Name: name}
	for _, k := range names {
		table.Columns = append(table.Columns, wire.DumpColumn{Name: k, Type: keys[k]})
	}
	out := make([]wire.DumpRow, 0, len(rows))
	for _, r := range rows {
		cells := make([]*string, len(names))
		for i, k := range names {
			c, _, err := cellText(r[k])
			if err != nil {
				return wire.DumpTable{}, nil, err
			}
			cells[i] = c
		}
		out = append(out, wire.DumpRow{Table: name, Cells: cells})
	}
	sort.SliceStable(out, func(i, j int) bool { return lessCells(out[i].Cells, out[j].Cells) })
	return table, out, nil
}

func lessCells(a, b []*string) bool {
	for i := range a {
		switch {
		case a[i] == nil && b[i] == nil:
			continue
		case a[i] == nil:
			return true
		case b[i] == nil:
			return false
		case *a[i] != *b[i]:
			return *a[i] < *b[i]
		}
	}
	return false
}
