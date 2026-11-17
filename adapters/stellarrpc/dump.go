package stellarrpc

import (
	"context"
	"encoding/json"
	"fmt"
	"slices"
	"sort"
	"strconv"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// Tables are the dump's tables: what getLedgers, getTransactions and
// getEvents serve in their default base64 form, one row per item, and the
// same answers in xdrFormat json as one row per JSON leaf (key, path, value),
// so the carry check reads every address stellar-rpc's own XDR to JSON
// conversion wrote.
var Tables = []string{"events", "events_json", "ledgers", "ledgers_json", "transactions", "transactions_json"}

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

// dumpTable turns served items into a dump table: the union of their members
// as columns, sorted, and the rows sorted by their cells.
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
	table := wire.DumpTable{Name: name, Columns: []wire.DumpColumn{}}
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

// leaves flattens one served item into rows of (key, path, value): every
// scalar under it, objects by member name and arrays by index. Empty objects
// and arrays become a row whose value is their JSON text.
func leaves(key string, v any) []map[string]any {
	var out []map[string]any
	var walk func(path string, v any)
	walk = func(path string, v any) {
		switch x := v.(type) {
		case map[string]any:
			if len(x) == 0 {
				out = append(out, map[string]any{"key": key, "path": path, "value": "{}"})
				return
			}
			ks := make([]string, 0, len(x))
			for k := range x {
				ks = append(ks, k)
			}
			sort.Strings(ks)
			for _, k := range ks {
				p := k
				if path != "" {
					p = path + "." + k
				}
				walk(p, x[k])
			}
		case []any:
			if len(x) == 0 {
				out = append(out, map[string]any{"key": key, "path": path, "value": "[]"})
				return
			}
			for i, e := range x {
				walk(fmt.Sprintf("%s[%d]", path, i), e)
			}
		default:
			out = append(out, map[string]any{"key": key, "path": path, "value": x})
		}
	}
	walk("", v)
	return out
}

// dumpRows reads the rows of one table over ledgers from..to.
func dumpRows(ctx context.Context, c *client, name string, from, to uint32) ([]map[string]any, error) {
	format := ""
	if len(name) > 5 && name[len(name)-5:] == "_json" {
		format = "json"
	}
	var items []served
	var keyOf string
	var err error
	switch name {
	case "ledgers", "ledgers_json":
		items, err = c.ledgers(ctx, from, to, format)
		keyOf = "sequence"
	case "transactions", "transactions_json":
		items, err = c.transactions(ctx, from, to, format)
		keyOf = "txHash"
	case "events", "events_json":
		items, err = c.events(ctx, from, to, format)
		keyOf = "id"
	default:
		return nil, fmt.Errorf("no table %q", name)
	}
	if err != nil {
		return nil, err
	}
	rows := make([]map[string]any, 0, len(items))
	for _, it := range items {
		if format == "" {
			rows = append(rows, it.raw)
			continue
		}
		k, _, err := cellText(it.raw[keyOf])
		if err != nil || k == nil {
			return nil, fmt.Errorf("%s item without %s", name, keyOf)
		}
		rows = append(rows, leaves(*k, it.raw)...)
	}
	return rows, nil
}

// Dump answers dump: every table, over every ledger the current process
// holds.
func (a *Adapter) Dump(tables []string, table func(wire.DumpTable) error, row func(wire.DumpRow) error) (wire.DumpResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil {
		return wire.DumpResult{}, wire.Errorf(wire.ClassBadRequest, "dump before start")
	}
	c, err := a.target.rpc()
	if err != nil {
		return wire.DumpResult{}, wire.Errorf(wire.ClassBadRequest, "%v", err)
	}
	names := Tables
	if len(tables) > 0 {
		for _, n := range tables {
			if !slices.Contains(Tables, n) {
				return wire.DumpResult{}, wire.Errorf(wire.ClassBadRequest, "no table %q", n)
			}
		}
		names = tables
	}
	var latest latestLedger
	if err := c.call(a.ctx, "getLatestLedger", nil, &latest); err != nil {
		return wire.DumpResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	// the oldest ledger the process holds, from a one-ledger getLedgers page
	var p struct {
		OldestLedger uint32 `json:"oldestLedger"`
	}
	if err := c.call(a.ctx, "getLedgers", map[string]any{"startLedger": latest.Sequence, "pagination": map[string]any{"limit": 1}}, &p); err != nil {
		return wire.DumpResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	if latest.Sequence-p.OldestLedger >= 10000 {
		return wire.DumpResult{}, wire.Errorf(wire.ClassUnsupported, "a dump of %d ledgers; this adapter reads at most one 10000-ledger scan window", latest.Sequence-p.OldestLedger+1)
	}
	var res wire.DumpResult
	for _, name := range names {
		rows, err := dumpRows(a.ctx, c, name, p.OldestLedger, latest.Sequence)
		if err != nil {
			return res, wire.Errorf(wire.ClassTarget, "reading %s: %v", name, err)
		}
		head, body, err := dumpTable(name, rows)
		if err != nil {
			return res, wire.Errorf(wire.ClassInternal, "%v", err)
		}
		if err := table(head); err != nil {
			return res, err
		}
		res.Tables++
		for _, r := range body {
			if err := row(r); err != nil {
				return res, err
			}
			res.Rows++
		}
	}
	return res, nil
}
