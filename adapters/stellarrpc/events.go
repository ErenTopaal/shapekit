package stellarrpc

import (
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"maps"
	"math/big"
	"unicode/utf8"

	"github.com/stellar/go-stellar-sdk/strkey"
	"github.com/stellar/go-stellar-sdk/xdr"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// Event stages as the oracle vocabulary names them.
const (
	StageBeforeAllTxs = "before-all-txs"
	StageOperation    = "operation"
	StageAfterTx      = "after-tx"
	StageAfterAllTxs  = "after-all-txs"
)

// rpcEvent is one consensus event as one of stellar-rpc's methods served it:
// the event's own fields, where it sits in the transaction, and the base64
// strings of its topics and data exactly as they were served.
type rpcEvent struct {
	// typ is the ContractEventType.
	typ xdr.ContractEventType
	// contract is the event's contract strkey, empty when the event has none.
	contract string
	stage    string
	// opIndex is the operation index; set for StageOperation only.
	opIndex   uint32
	topicsB64 []string
	dataB64   string
}

// fromContractEvent builds an rpcEvent from a decoded ContractEvent. Its
// topics and data are encoded again, as the oracle encodes core's (key_xdr in
// crates/shapekit-oracle/src/interpret.rs).
func fromContractEvent(e xdr.ContractEvent, stage string, opIndex uint32) (rpcEvent, error) {
	ev := rpcEvent{typ: e.Type, stage: stage, opIndex: opIndex}
	if e.ContractId != nil {
		s, err := strkey.Encode(strkey.VersionByteContract, e.ContractId[:])
		if err != nil {
			return rpcEvent{}, err
		}
		ev.contract = s
	}
	body, ok := e.Body.GetV0()
	if !ok {
		return rpcEvent{}, fmt.Errorf("event body version %d", e.Body.V)
	}
	for _, t := range body.Topics {
		b, err := t.MarshalBinary()
		if err != nil {
			return rpcEvent{}, err
		}
		ev.topicsB64 = append(ev.topicsB64, base64.StdEncoding.EncodeToString(b))
	}
	b, err := body.Data.MarshalBinary()
	if err != nil {
		return rpcEvent{}, err
	}
	ev.dataB64 = base64.StdEncoding.EncodeToString(b)
	return ev, nil
}

// scope is the vocabulary change scope of an event stage, as the oracle
// assigns it.
func scope(stage string) string {
	switch stage {
	case StageBeforeAllTxs:
		return "fee-processing"
	case StageAfterTx:
		return "tx-after"
	case StageAfterAllTxs:
		return "post-apply-fee"
	}
	return "operation"
}

// change maps one event to the vocabulary: a token-event when its topics and
// data have the shape of a CAP-67 or SEP-41 token event, a contract-event
// otherwise. A diagnostic event is not a consensus fact and maps to nothing
// (ok is false with no error). The rules are those of the oracle's
// token_event (crates/shapekit-oracle/src/interpret.rs), so that an event
// served unchanged maps to the oracle's own change.
func (ev rpcEvent) change() (*wire.Change, error) {
	if ev.typ == xdr.ContractEventTypeDiagnostic {
		return nil, nil
	}
	topics := make([]xdr.ScVal, len(ev.topicsB64))
	for i, s := range ev.topicsB64 {
		if err := xdr.SafeUnmarshalBase64(s, &topics[i]); err != nil {
			return nil, fmt.Errorf("topic %d %q does not decode: %w", i, s, err)
		}
	}
	var data xdr.ScVal
	if err := xdr.SafeUnmarshalBase64(ev.dataB64, &data); err != nil {
		return nil, fmt.Errorf("data %q does not decode: %w", ev.dataB64, err)
	}
	fields := map[string]any{"stage": ev.stage, "scope": scope(ev.stage)}
	if ev.stage == StageOperation {
		fields["op_index"] = ev.opIndex
	}
	te, err := tokenEvent(ev.contract, topics, data)
	if err != nil {
		return nil, err
	}
	if te != nil {
		maps.Copy(fields, te)
		return &wire.Change{Kind: "token-event", Fields: fields}, nil
	}
	if ev.contract != "" {
		fields["contract"] = ev.contract
	}
	fields["topics_xdr"] = append([]string{}, ev.topicsB64...)
	fields["data_xdr"] = ev.dataB64
	return &wire.Change{Kind: "contract-event", Fields: fields}, nil
}

// address is the oracle's sc_address: the base strkey of an address, a muxed
// account or muxed contract reduced to its base.
func address(a xdr.ScAddress) (string, error) {
	switch a.Type {
	case xdr.ScAddressTypeScAddressTypeAccount:
		return a.AccountId.GetAddress()
	case xdr.ScAddressTypeScAddressTypeMuxedAccount:
		return strkey.Encode(strkey.VersionByteAccountID, a.MuxedAccount.Ed25519[:])
	case xdr.ScAddressTypeScAddressTypeContract:
		return strkey.Encode(strkey.VersionByteContract, a.ContractId[:])
	case xdr.ScAddressTypeScAddressTypeMuxedContract:
		return strkey.Encode(strkey.VersionByteContract, a.MuxedContract.ContractId[:])
	case xdr.ScAddressTypeScAddressTypeClaimableBalance:
		cb := a.ClaimableBalanceId
		if cb.Type != xdr.ClaimableBalanceIdTypeClaimableBalanceIdTypeV0 {
			return "", fmt.Errorf("claimable balance id type %d", cb.Type)
		}
		return strkey.Encode(strkey.VersionByteClaimableBalance, append([]byte{byte(cb.Type)}, cb.V0[:]...))
	case xdr.ScAddressTypeScAddressTypeLiquidityPool:
		return strkey.Encode(strkey.VersionByteLiquidityPool, a.LiquidityPoolId[:])
	}
	return "", fmt.Errorf("sc address type %d", a.Type)
}

func i128(p xdr.Int128Parts) string {
	v := new(big.Int).Lsh(big.NewInt(int64(p.Hi)), 64)
	return v.Add(v, new(big.Int).SetUint64(uint64(p.Lo))).String()
}

// lossy decodes bytes as UTF-8 the way Rust's String::from_utf8_lossy does:
// every maximal invalid subpart becomes one U+FFFD.
func lossy(b []byte) string {
	out := make([]rune, 0, len(b))
	for len(b) > 0 {
		r, n := utf8.DecodeRune(b)
		if r != utf8.RuneError || n > 1 {
			out = append(out, r)
			b = b[n:]
			continue
		}
		out = append(out, utf8.RuneError)
		b = b[maximalSubpart(b):]
	}
	return string(out)
}

// maximalSubpart is the length of the longest prefix of b that starts a
// well-formed UTF-8 sequence without completing it, at least 1 (Unicode
// "substitution of maximal subparts", which Rust follows).
func maximalSubpart(b []byte) int {
	c := b[0]
	var need int
	lo, hi := byte(0x80), byte(0xBF)
	switch {
	case c >= 0xC2 && c <= 0xDF:
		need = 1
	case c == 0xE0:
		need, lo = 2, 0xA0
	case c >= 0xE1 && c <= 0xEC, c == 0xEE, c == 0xEF:
		need = 2
	case c == 0xED:
		need, hi = 2, 0x9F
	case c == 0xF0:
		need, lo = 3, 0x90
	case c >= 0xF1 && c <= 0xF3:
		need = 3
	case c == 0xF4:
		need, hi = 3, 0x8F
	default:
		return 1
	}
	n := 1
	for i := 0; i < need && n < len(b); i++ {
		x := b[n]
		if i == 0 && (x < lo || x > hi) || i > 0 && (x < 0x80 || x > 0xBF) {
			break
		}
		n++
	}
	return n
}

// amountData is the oracle's amount_data: an i128 amount, or a map of exactly
// the keys amount and to_muxed_id. ok is false for any other shape.
func amountData(d xdr.ScVal) (string, map[string]any, bool) {
	switch d.Type {
	case xdr.ScValTypeScvI128:
		return i128(*d.I128), nil, true
	case xdr.ScValTypeScvMap:
		if d.Map == nil || *d.Map == nil {
			return "", nil, false
		}
		var amount string
		var have bool
		var mux map[string]any
		for _, en := range **d.Map {
			if en.Key.Type != xdr.ScValTypeScvSymbol {
				return "", nil, false
			}
			k := string(*en.Key.Sym)
			switch {
			case k == "amount" && en.Val.Type == xdr.ScValTypeScvI128:
				amount, have = i128(*en.Val.I128), true
			case k == "to_muxed_id":
				switch en.Val.Type {
				case xdr.ScValTypeScvU64:
					mux = map[string]any{"type": "u64", "value": fmt.Sprint(uint64(*en.Val.U64))}
				case xdr.ScValTypeScvString:
					mux = map[string]any{"type": "string", "value": hex.EncodeToString([]byte(*en.Val.Str))}
				case xdr.ScValTypeScvBytes:
					mux = map[string]any{"type": "bytes", "value": hex.EncodeToString(*en.Val.Bytes)}
				default:
					return "", nil, false
				}
			default:
				return "", nil, false
			}
		}
		if !have {
			return "", nil, false
		}
		return amount, mux, true
	}
	return "", nil, false
}

// tokenEvent is the oracle's token_event without the asset field (the oracle
// proves that field from the ledger's assets, which no method here serves).
// It returns nil when the event is not a token event.
func tokenEvent(contract string, topics []xdr.ScVal, data xdr.ScVal) (map[string]any, error) {
	if len(topics) == 0 || topics[0].Type != xdr.ScValTypeScvSymbol {
		return nil, nil
	}
	nameBytes := []byte(*topics[0].Sym)
	if !utf8.Valid(nameBytes) {
		return nil, nil
	}
	name := string(nameBytes)
	if contract == "" {
		return nil, nil
	}
	var addrs []string
	for _, t := range topics[1:] {
		if t.Type != xdr.ScValTypeScvAddress {
			continue
		}
		s, err := address(*t.Address)
		if err != nil {
			return nil, err
		}
		addrs = append(addrs, s)
	}
	te := map[string]any{"event": name, "contract": contract}
	if len(topics) >= 2 && topics[len(topics)-1].Type == xdr.ScValTypeScvString {
		te["asset_topic"] = lossy([]byte(*topics[len(topics)-1].Str))
	}
	switch name {
	case "transfer":
		if len(addrs) != 2 {
			return nil, nil
		}
		amount, mux, ok := amountData(data)
		if !ok {
			return nil, nil
		}
		te["from"], te["to"], te["amount"] = addrs[0], addrs[1], amount
		if mux != nil {
			te["to_muxed_id"] = mux
		}
	case "mint":
		amount, mux, ok := amountData(data)
		if !ok || len(addrs) == 0 {
			return nil, nil
		}
		te["to"], te["amount"] = addrs[len(addrs)-1], amount
		if mux != nil {
			te["to_muxed_id"] = mux
		}
	case "burn", "clawback", "fee":
		amount, _, ok := amountData(data)
		if !ok || len(addrs) == 0 {
			return nil, nil
		}
		from := addrs[len(addrs)-1]
		if name == "burn" {
			from = addrs[0]
		}
		te["from"], te["amount"] = from, amount
	case "set_authorized":
		if data.Type != xdr.ScValTypeScvBool || len(addrs) == 0 {
			return nil, nil
		}
		te["to"], te["authorized"] = addrs[len(addrs)-1], *data.B
	case "approve":
		if len(addrs) != 2 || data.Type != xdr.ScValTypeScvVec || data.Vec == nil || *data.Vec == nil {
			return nil, nil
		}
		v := **data.Vec
		if len(v) < 2 || v[0].Type != xdr.ScValTypeScvI128 || v[1].Type != xdr.ScValTypeScvU32 {
			return nil, nil
		}
		te["from"], te["spender"] = addrs[0], addrs[1]
		te["amount"], te["live_until_ledger"] = i128(*v[0].I128), uint32(*v[1].U32)
	default:
		return nil, nil
	}
	return te, nil
}

// metaEvents lists the consensus events of a TransactionMeta in the order the
// oracle reads them (crates/shapekit-oracle/src/view.rs, TxView::events): v3
// Soroban events at operation 0; v4 fee events before all transactions, then
// every operation's events, then the remaining staged events. Earlier
// versions carry none.
func metaEvents(m xdr.TransactionMeta) ([]rpcEvent, error) {
	var out []rpcEvent
	add := func(e xdr.ContractEvent, stage string, op uint32) error {
		ev, err := fromContractEvent(e, stage, op)
		if err != nil {
			return err
		}
		out = append(out, ev)
		return nil
	}
	switch m.V {
	case 3:
		if m.V3.SorobanMeta != nil {
			for _, e := range m.V3.SorobanMeta.Events {
				if err := add(e, StageOperation, 0); err != nil {
					return nil, err
				}
			}
		}
	case 4:
		for _, e := range m.V4.Events {
			if e.Stage == xdr.TransactionEventStageTransactionEventStageBeforeAllTxs {
				if err := add(e.Event, StageBeforeAllTxs, 0); err != nil {
					return nil, err
				}
			}
		}
		for i, op := range m.V4.Operations {
			for _, e := range op.Events {
				if err := add(e, StageOperation, uint32(i)); err != nil {
					return nil, err
				}
			}
		}
		for _, e := range m.V4.Events {
			if e.Stage == xdr.TransactionEventStageTransactionEventStageBeforeAllTxs {
				continue
			}
			stage, err := stageName(e.Stage)
			if err != nil {
				return nil, err
			}
			if err := add(e.Event, stage, 0); err != nil {
				return nil, err
			}
		}
	}
	return out, nil
}

func stageName(s xdr.TransactionEventStage) (string, error) {
	switch s {
	case xdr.TransactionEventStageTransactionEventStageBeforeAllTxs:
		return StageBeforeAllTxs, nil
	case xdr.TransactionEventStageTransactionEventStageAfterTx:
		return StageAfterTx, nil
	case xdr.TransactionEventStageTransactionEventStageAfterAllTxs:
		return StageAfterAllTxs, nil
	}
	return "", fmt.Errorf("transaction event stage %d", s)
}

// TOID masks stellar-rpc puts into an event id for the events that belong to
// no operation (cmd/stellar-rpc/internal/store/event.go, StageSentinels):
// before all transactions tx 0 and operation 0, after one transaction
// operation OperationMask, after all transactions tx TransactionMask.
const (
	toidTransactionMask = 1<<20 - 1
	toidOperationMask   = 1<<12 - 1
)

// idStage is the stage and operation index of an event as getEvents placed it,
// from its transactionIndex and operationIndex members.
func idStage(tx, op uint32) (string, uint32) {
	switch {
	case tx == 0 && op == 0:
		return StageBeforeAllTxs, 0
	case tx == toidTransactionMask:
		return StageAfterAllTxs, 0
	case op == toidOperationMask:
		return StageAfterTx, 0
	}
	return StageOperation, op
}
