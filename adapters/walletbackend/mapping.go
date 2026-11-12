package walletbackend

import (
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"math/big"
	"sort"
	"strconv"
	"strings"

	"github.com/stellar/go-stellar-sdk/strkey"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// row is one stored row keyed by column name, with the values pgx decoded.
// Columns are read by name so a column a commit lacks reads as absent.
type row map[string]any

func (r row) text(col string) (string, bool) {
	v, ok := r[col].(string)
	return v, ok
}

func (r row) bytes(col string) ([]byte, bool) {
	v, ok := r[col].([]byte)
	return v, ok && v != nil
}

func (r row) integer(col string) (int64, bool) {
	switch v := r[col].(type) {
	case int16:
		return int64(v), true
	case int32:
		return int64(v), true
	case int64:
		return v, true
	}
	return 0, false
}

func (r row) object(col string) (map[string]any, bool) {
	v, ok := r[col].(map[string]any)
	return v, ok
}

// exportRow renders a row for an unmapped entry: byte columns as lowercase hex,
// everything else as pgx decoded it.
func exportRow(r row) map[string]any {
	out := make(map[string]any, len(r))
	for k, v := range r {
		out[k] = jsonValue(v)
	}
	return out
}

// state change categories and reasons as wallet-backend writes them
// (internal/indexer/types/types.go).
const (
	catBalance       = "BALANCE"
	catAccount       = "ACCOUNT"
	catSigner        = "SIGNER"
	catThreshold     = "SIGNATURE_THRESHOLD"
	catDataEntry     = "DATA_ENTRY"
	catHomeDomain    = "HOME_DOMAIN"
	catAllowance     = "ALLOWANCE"
	catFlags         = "FLAGS"
	catTrustline     = "TRUSTLINE"
	catBalanceAuth   = "BALANCE_AUTHORIZATION"
	reasonCreate     = "CREATE"
	reasonMerge      = "MERGE"
	reasonDebit      = "DEBIT"
	reasonCredit     = "CREDIT"
	reasonMint       = "MINT"
	reasonBurn       = "BURN"
	reasonAdd        = "ADD"
	reasonRemove     = "REMOVE"
	reasonUpdate     = "UPDATE"
	reasonSet        = "SET"
	reasonClear      = "CLEAR"
	stateChangeTable = "state_changes"
)

// wallet-backend's own trustline flag bits (types.go flag bitmask), translated
// to the TrustLineFlags values core stores.
const (
	wbFlagAuthorized               = 1 << 0
	wbFlagClawbackEnabled          = 1 << 5
	wbFlagAuthorizedMaintainLiabil = 1 << 6

	coreTrustlineAuthorized          = 1
	coreTrustlineMaintainLiabilities = 2
	coreTrustlineClawbackEnabled     = 4
)

func coreTrustlineFlags(wb int64) uint32 {
	var f uint32
	if wb&wbFlagAuthorized != 0 {
		f |= coreTrustlineAuthorized
	}
	if wb&wbFlagAuthorizedMaintainLiabil != 0 {
		f |= coreTrustlineMaintainLiabilities
	}
	if wb&wbFlagClawbackEnabled != 0 {
		f |= coreTrustlineClawbackEnabled
	}
	return f
}

// mapper turns one transaction's state change rows into vocabulary changes.
type mapper struct {
	assets assetIndex
}

type txMapping struct {
	changes  []wire.Change
	unmapped []wire.Unmapped
}

// opIndexOf returns the 0-based operation index of a TOID operation id, or
// false for the transaction-level id 0 (fee rows).
func opIndexOf(operationID int64) (int, bool) {
	if operationID == 0 {
		return 0, false
	}
	return int(operationID&0xfff) - 1, true
}

func (m *mapper) mapTransaction(rows []row) txMapping {
	var out txMapping
	sort.SliceStable(rows, func(i, j int) bool {
		oi, _ := rows[i].integer("operation_id")
		oj, _ := rows[j].integer("operation_id")
		if oi != oj {
			return oi < oj
		}
		si, _ := rows[i].integer("state_change_id")
		sj, _ := rows[j].integer("state_change_id")
		return si < sj
	})
	for start := 0; start < len(rows); {
		op, _ := rows[start].integer("operation_id")
		end := start
		for end < len(rows) {
			o, _ := rows[end].integer("operation_id")
			if o != op {
				break
			}
			end++
		}
		m.mapOperation(op, rows[start:end], &out)
		start = end
	}
	return out
}

// built is a change under construction with the addresses it read.
type built struct {
	kind   string
	fields map[string]any
	raw    map[string]wire.RawAddress
	source wire.RowRef
	bad    bool
}

func newBuilt(kind string, r row) *built {
	cat, _ := r.text("state_change_category")
	reason, _ := r.text("state_change_reason")
	return &built{
		kind:   kind,
		fields: map[string]any{},
		raw:    map[string]wire.RawAddress{},
		source: wire.RowRef{Table: stateChangeTable, Category: cat, Reason: reason},
	}
}

// addr reads an address column into field. A missing column leaves the field
// absent; bytes that do not decode set the field to null and keep the raw form.
func (b *built) addr(field, col string, r row, kind columnKind) {
	raw, ok := r.bytes(col)
	if !ok {
		return
	}
	ra, s := rawAddress(col, raw, kind)
	b.raw[field] = ra
	if s == nil {
		b.fields[field] = nil
		b.bad = true
		return
	}
	b.fields[field] = *s
}

// limit reads a trustline limit column. wallet-backend stores limits in
// horizon's amount form ("922337203685.4775807"), unlike token amounts, which
// it stores in stroops; the vocabulary wants stroops. A missing column leaves
// the field absent; a value that is not such an amount reports false.
func (b *built) limit(field, col string, r row) bool {
	v, ok := r.text(col)
	if !ok {
		return true
	}
	s, ok := stroops(v)
	if !ok {
		return false
	}
	b.fields[field] = s
	return true
}

// stroops converts a decimal amount with at most 7 fraction digits into the
// integer number of stroops, exactly.
func stroops(v string) (string, bool) {
	whole, frac, _ := strings.Cut(v, ".")
	if whole == "" || len(frac) > 7 || strings.HasPrefix(whole, "-") {
		return "", false
	}
	for _, c := range whole + frac {
		if c < '0' || c > '9' {
			return "", false
		}
	}
	n, ok := new(big.Int).SetString(whole+frac+strings.Repeat("0", 7-len(frac)), 10)
	if !ok {
		return "", false
	}
	return n.String(), true
}

func (b *built) change(op int64) wire.Change {
	if idx, ok := opIndexOf(op); ok {
		b.fields["scope"] = "operation"
		b.fields["op_index"] = idx
	} else {
		b.fields["scope"] = "fee-processing"
	}
	src := b.source
	return wire.Change{Kind: b.kind, Fields: b.fields, Raw: b.raw, Source: &src}
}

func unmappedRow(r row, note string) wire.Unmapped {
	cat, _ := r.text("state_change_category")
	reason, _ := r.text("state_change_reason")
	return wire.Unmapped{Table: stateChangeTable, Category: cat, Reason: reason, Row: exportRow(r), Note: note}
}

func (m *mapper) mapOperation(op int64, rows []row, out *txMapping) {
	used := make([]bool, len(rows))
	emit := func(b *built) { out.changes = append(out.changes, b.change(op)) }
	skip := func(i int, note string) {
		used[i] = true
		out.unmapped = append(out.unmapped, unmappedRow(rows[i], note))
	}

	// trustline creations first, so the authorization row wallet-backend writes
	// alongside each one (initial trustline flags) merges into it
	for i, r := range rows {
		if cat(r) != catTrustline {
			continue
		}
		used[i] = true
		asset, note := m.trustlineAsset(r)
		if note != "" {
			used[i] = false
			skip(i, note)
			continue
		}
		var b *built
		switch reason(r) {
		case reasonAdd:
			b = newBuilt("trustline-created", r)
			if !b.limit("limit", "trustline_limit_new", r) {
				used[i] = false
				skip(i, "trustline_limit_new is not an amount with at most 7 decimals")
				continue
			}
			if j := companionAuth(rows, used, i); j >= 0 {
				used[j] = true
				if f, ok := rows[j].integer("flags"); ok {
					b.fields["flags"] = coreTrustlineFlags(f)
				}
			}
		case reasonUpdate:
			b = newBuilt("trustline-limit", r)
			if !b.limit("before", "trustline_limit_old", r) || !b.limit("after", "trustline_limit_new", r) {
				used[i] = false
				skip(i, "a trustline limit is not an amount with at most 7 decimals")
				continue
			}
		case reasonRemove:
			b = newBuilt("trustline-removed", r)
		default:
			used[i] = false
			skip(i, "TRUSTLINE with a reason other than ADD, UPDATE or REMOVE")
			continue
		}
		b.addr("account", "account_id", r, accountColumn)
		b.fields["asset"] = asset
		emit(b)
	}

	m.mapBalances(op, rows, used, out)

	// rows that wallet-backend splits per flag or per threshold merge into one change per account
	type key struct{ kind, account, token string }
	merged := map[key]*built{}
	var order []key
	for i, r := range rows {
		if used[i] {
			continue
		}
		switch cat(r) {
		case catFlags, catThreshold, catBalanceAuth:
		default:
			continue
		}
		acct, _ := r.bytes("account_id")
		tok, _ := r.bytes("token_id")
		lp, _ := r.text("liquidity_pool_id")
		k := key{kind: cat(r), account: hex.EncodeToString(acct), token: hex.EncodeToString(tok) + lp}
		if cat(r) == catBalanceAuth && len(acct) > 0 && acct[0] != versionByte('G') {
			// a contract holder's authorization comes from a SAC set_authorized
			// event and has no trustline behind it
			used[i] = true
			b := newBuilt("token-event", r)
			b.fields["event"] = "set_authorized"
			b.addr("contract", "token_id", r, accountColumn)
			b.addr("to", "account_id", r, accountColumn)
			b.fields["authorized"] = reason(r) == reasonSet
			emit(b)
			continue
		}
		used[i] = true
		if _, ok := merged[k]; ok {
			continue
		}
		var b *built
		switch cat(r) {
		case catFlags:
			b = newBuilt("account-flags", r)
		case catThreshold:
			b = newBuilt("thresholds", r)
		case catBalanceAuth:
			asset, note := m.trustlineAsset(r)
			if note != "" {
				used[i] = false
				skip(i, note)
				continue
			}
			b = newBuilt("trustline-flags", r)
			b.fields["asset"] = asset
		}
		b.addr("account", "account_id", r, accountColumn)
		merged[k] = b
		order = append(order, k)
	}
	for _, k := range order {
		emit(merged[k])
	}

	for i, r := range rows {
		if used[i] {
			continue
		}
		used[i] = true
		switch cat(r) {
		case catAccount:
			m.mapAccount(r, emit, func(note string) { used[i] = false; skip(i, note) })
		case catSigner:
			m.mapSigner(r, emit, func(note string) { used[i] = false; skip(i, note) })
		case catHomeDomain:
			b := newBuilt("home-domain", r)
			b.addr("account", "account_id", r, accountColumn)
			kv, _ := r.object("key_value")
			b.fields["before"] = stringOr(kv["old"])
			b.fields["after"] = stringOr(kv["new"])
			emit(b)
		case catDataEntry:
			if b, note := dataEntry(r); note != "" {
				used[i] = false
				skip(i, note)
			} else {
				emit(b)
			}
		case catAllowance:
			b := newBuilt("token-event", r)
			b.fields["event"] = "approve"
			b.addr("contract", "token_id", r, accountColumn)
			b.addr("from", "account_id", r, accountColumn)
			b.addr("spender", "spender_account_id", r, accountColumn)
			if a, ok := r.text("amount"); ok {
				b.fields["amount"] = a
			}
			if kv, ok := r.object("key_value"); ok {
				if lu, ok := uintOf(kv["live_until_ledger"]); ok {
					b.fields["live_until_ledger"] = lu
				}
			}
			emit(b)
		default:
			used[i] = false
			skip(i, "no vocabulary kind for this category: blend protocol state is recorded by the oracle only as raw contract events, which wallet-backend does not store")
		}
	}
	for i := range rows {
		if !used[i] {
			skip(i, "row left after mapping; adapter gap")
		}
	}
}

func cat(r row) string    { s, _ := r.text("state_change_category"); return s }
func reason(r row) string { s, _ := r.text("state_change_reason"); return s }

func versionByte(prefix byte) byte {
	switch prefix {
	case 'G':
		return 6 << 3
	case 'C':
		return 2 << 3
	}
	return 0
}

// companionAuth finds the BALANCE_AUTHORIZATION SET row wallet-backend writes
// right after a TRUSTLINE ADD for the same holder and asset.
func companionAuth(rows []row, used []bool, at int) int {
	acct, _ := rows[at].bytes("account_id")
	tok, _ := rows[at].bytes("token_id")
	lp, _ := rows[at].text("liquidity_pool_id")
	for j := at + 1; j < len(rows); j++ {
		r := rows[j]
		if used[j] || cat(r) != catBalanceAuth || reason(r) != reasonSet {
			continue
		}
		a, _ := r.bytes("account_id")
		t, _ := r.bytes("token_id")
		l, _ := r.text("liquidity_pool_id")
		if hex.EncodeToString(a) == hex.EncodeToString(acct) && hex.EncodeToString(t) == hex.EncodeToString(tok) && l == lp {
			return j
		}
	}
	return -1
}

// trustlineAsset resolves a row's token_id (a SAC contract id) or
// liquidity_pool_id to the vocabulary's asset string.
func (m *mapper) trustlineAsset(r row) (string, string) {
	if lp, ok := r.text("liquidity_pool_id"); ok && lp != "" {
		// wallet-backend stores the pool id as hex; the oracle names pools by
		// their L strkey
		id, err := hex.DecodeString(lp)
		if err != nil || len(id) != 32 {
			return "", "liquidity_pool_id is not a 32-byte hex pool id"
		}
		l, err := strkey.Encode(strkey.VersionByteLiquidityPool, id)
		if err != nil {
			return "", fmt.Sprintf("liquidity_pool_id: %v", err)
		}
		return "pool-share:" + l, ""
	}
	tok, ok := r.bytes("token_id")
	if !ok {
		return "", "row names neither a token nor a liquidity pool"
	}
	if len(tok) != 33 {
		return "", fmt.Sprintf("token_id holds %d bytes, not a 33-byte contract address", len(tok))
	}
	var id [32]byte
	copy(id[:], tok[1:])
	if a, ok := m.assets.byContract[id]; ok {
		return a, ""
	}
	return "", "token_id is not the asset contract of any asset in trustline_assets or contract_tokens"
}

func (m *mapper) mapAccount(r row, emit func(*built), skip func(string)) {
	acct, _ := r.bytes("account_id")
	isContract := len(acct) > 0 && acct[0] == versionByte('C')
	switch reason(r) {
	case reasonCreate:
		if isContract {
			b := newBuilt("contract-deployed", r)
			b.addr("contract", "account_id", r, accountColumn)
			emit(b)
			return
		}
		b := newBuilt("account-created", r)
		b.addr("account", "account_id", r, accountColumn)
		emit(b)
	case reasonMerge:
		b := newBuilt("account-removed", r)
		b.addr("account", "account_id", r, accountColumn)
		emit(b)
	default:
		skip("ACCOUNT with a reason other than CREATE or MERGE")
	}
}

func (m *mapper) mapSigner(r row, emit func(*built), skip func(string)) {
	var b *built
	switch reason(r) {
	case reasonAdd:
		b = newBuilt("signer-added", r)
		if w, ok := r.integer("signer_weight_new"); ok {
			b.fields["weight"] = w
		}
	case reasonRemove:
		b = newBuilt("signer-removed", r)
		if w, ok := r.integer("signer_weight_old"); ok {
			b.fields["weight"] = w
		}
	case reasonUpdate:
		b = newBuilt("signer-weight", r)
		if w, ok := r.integer("signer_weight_old"); ok {
			b.fields["before"] = w
		}
		if w, ok := r.integer("signer_weight_new"); ok {
			b.fields["after"] = w
		}
	default:
		skip("SIGNER with a reason other than ADD, UPDATE or REMOVE")
		return
	}
	b.addr("account", "account_id", r, accountColumn)
	b.addr("signer", "signer_account_id", r, signerColumn)
	emit(b)
}

func dataEntry(r row) (*built, string) {
	var action string
	switch reason(r) {
	case reasonAdd:
		action = "created"
	case reasonUpdate:
		action = "updated"
	case reasonRemove:
		action = "removed"
	default:
		return nil, "DATA_ENTRY with a reason other than ADD, UPDATE or REMOVE"
	}
	name, ok := r.text("data_entry_name")
	if !ok {
		return nil, "DATA_ENTRY without data_entry_name"
	}
	b := newBuilt("data-entry", r)
	b.addr("account", "account_id", r, accountColumn)
	b.fields["action"] = action
	b.fields["name_hex"] = hex.EncodeToString([]byte(name))
	kv, _ := r.object("key_value")
	for field, key := range map[string]string{"value_before_hex": "old", "value_after_hex": "new"} {
		v, present := kv[key]
		if !present {
			continue
		}
		s, ok := v.(string)
		if !ok {
			return nil, fmt.Sprintf("key_value.%s is %T, not a base64 string", key, v)
		}
		raw, err := base64.StdEncoding.DecodeString(s)
		if err != nil {
			return nil, fmt.Sprintf("key_value.%s is not base64: %v", key, err)
		}
		b.fields[field] = hex.EncodeToString(raw)
	}
	return b, ""
}

// balanceRow is a BALANCE row reduced to what pairing needs.
type balanceRow struct {
	i      int
	reason string
	token  string
	amount string
}

// mapBalances turns wallet-backend's per-holder BALANCE rows back into token
// events. wallet-backend writes a transfer as a DEBIT row for the sender and a
// CREDIT row for the receiver, a classic mint as MINT on the issuer plus CREDIT
// on the receiver, a burn or clawback as BURN on the issuer plus DEBIT on the
// holder, and a SEP-41 mint or burn as one row. Rows pair up in state_change_id
// order on equal token and amount; a row with no partner keeps only the side
// wallet-backend stored.
func (m *mapper) mapBalances(op int64, rows []row, used []bool, out *txMapping) {
	var bal []balanceRow
	for i, r := range rows {
		if used[i] || cat(r) != catBalance {
			continue
		}
		tok, _ := r.bytes("token_id")
		amt, _ := r.text("amount")
		bal = append(bal, balanceRow{i: i, reason: reason(r), token: hex.EncodeToString(tok), amount: amt})
	}
	emit := func(b *built) { out.changes = append(out.changes, b.change(op)) }
	if op == 0 {
		// fee rows: one net row per fee payer, debit for a net charge, credit for a net refund
		for _, br := range bal {
			r := rows[br.i]
			used[br.i] = true
			b := newBuilt("token-event", r)
			b.fields["event"] = "fee"
			b.addr("contract", "token_id", r, accountColumn)
			b.addr("from", "account_id", r, accountColumn)
			switch br.reason {
			case reasonDebit:
				b.fields["amount"] = br.amount
			case reasonCredit:
				neg, ok := negate(br.amount)
				if !ok {
					used[br.i] = false
					out.unmapped = append(out.unmapped, unmappedRow(r, "fee refund amount is not a decimal integer"))
					continue
				}
				b.fields["amount"] = neg
			default:
				used[br.i] = false
				out.unmapped = append(out.unmapped, unmappedRow(r, "transaction-level BALANCE row that is neither DEBIT nor CREDIT"))
				continue
			}
			emit(b)
		}
		return
	}
	taken := make([]bool, len(bal))
	partner := func(from int, want string, like balanceRow) int {
		for j := from; j < len(bal); j++ {
			if !taken[j] && bal[j].reason == want && bal[j].token == like.token && bal[j].amount == like.amount {
				return j
			}
		}
		return -1
	}
	for k, br := range bal {
		if taken[k] {
			continue
		}
		taken[k] = true
		r := rows[br.i]
		used[br.i] = true
		b := newBuilt("token-event", r)
		b.addr("contract", "token_id", r, accountColumn)
		b.fields["amount"] = br.amount
		switch br.reason {
		case reasonMint:
			b.fields["event"] = "mint"
			if j := partner(k+1, reasonCredit, br); j >= 0 {
				taken[j] = true
				used[bal[j].i] = true
				b.addr("to", "account_id", rows[bal[j].i], accountColumn)
				setMux(b, rows[bal[j].i])
			} else {
				b.addr("to", "account_id", r, accountColumn)
				setMux(b, r)
			}
		case reasonBurn:
			b.fields["event"] = "burn"
			if j := partner(k+1, reasonDebit, br); j >= 0 {
				taken[j] = true
				used[bal[j].i] = true
				b.addr("from", "account_id", rows[bal[j].i], accountColumn)
			} else {
				b.addr("from", "account_id", r, accountColumn)
			}
		case reasonDebit:
			b.fields["event"] = "transfer"
			b.addr("from", "account_id", r, accountColumn)
			if j := partner(k+1, reasonCredit, br); j >= 0 {
				taken[j] = true
				used[bal[j].i] = true
				b.addr("to", "account_id", rows[bal[j].i], accountColumn)
				setMux(b, rows[bal[j].i])
			}
		case reasonCredit:
			b.fields["event"] = "transfer"
			b.addr("to", "account_id", r, accountColumn)
			setMux(b, r)
		default:
			used[br.i] = false
			out.unmapped = append(out.unmapped, unmappedRow(r, "BALANCE row with a reason other than DEBIT, CREDIT, MINT or BURN"))
			continue
		}
		emit(b)
	}
}

// setMux copies wallet-backend's to_muxed_id (a u64, written only for id-typed
// muxed info) into the vocabulary's typed form.
func setMux(b *built, r row) {
	if v, ok := r.text("to_muxed_id"); ok && v != "" {
		b.fields["to_muxed_id"] = map[string]any{"type": "u64", "value": v}
	}
}

func negate(s string) (string, bool) {
	n, ok := new(big.Int).SetString(s, 10)
	if !ok {
		return "", false
	}
	return n.Neg(n).String(), true
}

func stringOr(v any) string {
	s, _ := v.(string)
	return s
}

func uintOf(v any) (uint32, bool) {
	switch n := v.(type) {
	case float64:
		if n >= 0 && n <= 4294967295 && n == float64(uint32(n)) {
			return uint32(n), true
		}
	case string:
		u, err := strconv.ParseUint(n, 10, 32)
		if err == nil {
			return uint32(u), true
		}
	}
	return 0, false
}
