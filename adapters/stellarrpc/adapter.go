// Package stellarrpc is the shapekit adapter for stellar/stellar-rpc: it runs
// the pinned release image with the chain's lake as a filesystem datastore,
// lets stellar-rpc's backfill ingest every released ledger, and reads back
// what getTransactions, getEvents and getLedgers serve, in the oracle
// vocabulary.
package stellarrpc

import (
	"context"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"slices"
	"sync"

	"github.com/stellar/go-stellar-sdk/xdr"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// Version of this adapter.
const Version = "0.2.0"

// VocabVersion is the oracle vocabulary version the claims and exports follow.
const VocabVersion = 2

// Image is the stellar-rpc release image, by tag and digest as pins.toml
// lists it. Its stellar-rpc reports Version30 at Commit (checked when a
// process first answers).
const Image = "stellar/stellar-rpc:30.0.1@sha256:b6d41d3148087adf5d6856380a40854bd1e25b29d62a7ae484258e290e73b096"

// Version30 and Commit are what getVersionInfo answers for Image: release
// v30.0.1, the commit of its tag.
const (
	Version30 = "30.0.1-6b6ec60c8bde5a93984b6290065d3882d384b25d"
	Commit    = "6b6ec60c8bde5a93984b6290065d3882d384b25d"
)

// MaxProtocol is the newest ledger protocol the release decodes (its
// go-stellar-sdk v0.9.0 and soroban-env-host 30.0.0).
const MaxProtocol = 30

// Event sources: which method the export reads a transaction's events from.
const (
	FromEvents       = "events"
	FromTransactions = "transactions"
	FromLedgers      = "ledgers"
)

// Config is how the adapter is invoked.
type Config struct {
	// CPUs is docker's --cpus for the container; no limit when empty.
	CPUs string
	// EventsFrom is the method the export reads events from: FromEvents
	// (getEvents), FromTransactions (getTransactions) or FromLedgers
	// (getLedgers).
	EventsFrom string
}

// Adapter implements wire.Handler and wire.Dumper.
type Adapter struct {
	cfg    Config
	ctx    context.Context
	mu     sync.Mutex
	target *target
}

// New checks the configuration and returns an adapter.
func New(ctx context.Context, cfg Config) (*Adapter, error) {
	if !slices.Contains([]string{FromEvents, FromTransactions, FromLedgers}, cfg.EventsFrom) {
		return nil, fmt.Errorf("--events-from must be events, transactions or ledgers, not %q", cfg.EventsFrom)
	}
	return &Adapter{cfg: cfg, ctx: ctx}, nil
}

// Claims are the consensus events every method serves: token events in the
// CAP-67 and SEP-41 shapes and every other contract or system event. The
// asset of a token event is not claimed: the oracle proves it from the
// assets of the ledger's entries, which none of these methods serve.
// Participants and entry changes are not claimed: stellar-rpc derives no
// participants and serves entry changes only inside the meta XDR.
func Claims() wire.Claims {
	return wire.Claims{
		Changes: []wire.ChangeClaim{
			{
				Kind: "token-event",
				Fields: []string{"event", "stage", "contract", "from", "to", "spender", "amount",
					"to_muxed_id", "authorized", "live_until_ledger", "asset_topic"},
			},
			{Kind: "contract-event", Fields: []string{"contract", "stage", "topics_xdr", "data_xdr"}},
		},
	}
}

// Describe answers describe.
func (a *Adapter) Describe(protocol int) (wire.DescribeResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if protocol < 2 || protocol > wire.ProtocolVersion {
		return wire.DescribeResult{}, wire.Errorf(wire.ClassUnsupported, "protocol %d; this adapter speaks 2", protocol)
	}
	return wire.DescribeResult{
		Protocol:        protocol,
		Adapter:         "shapekit-stellar-rpc-adapter",
		AdapterVersion:  Version,
		Target:          "stellar-rpc",
		TargetVersion:   Version30,
		DeliveryModes:   []string{wire.DeliverySep54Filesystem},
		DefaultDelivery: wire.DeliverySep54Filesystem,
		// on an empty database stellar-rpc's live ingestion starts at the
		// archive's newest checkpoint (rpcv1/ingest/service.go,
		// getNextLedgerSequence), and its backfill ends on one
		Bootstrap:    "checkpoint",
		VocabVersion: VocabVersion,
		Protocols:    wire.ProtocolRange{Min: 1, Max: MaxProtocol},
		Claims:       Claims(),
		Capabilities: []string{wire.CapabilityDump},
	}, nil
}

// Start answers start.
func (a *Adapter) Start(p wire.StartParams) (wire.StartResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassBadRequest, "start sent twice")
	}
	if err := ensureImage(a.ctx); err != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	t, err := startTarget(a.ctx, a.cfg.CPUs, p)
	a.target = t
	if err != nil {
		return wire.StartResult{}, err
	}
	if err := t.waitReady(a.ctx); err != nil {
		return wire.StartResult{}, err
	}
	imageID, err := docker(a.ctx, "image", "inspect", "--format", "{{.Id}}", Image)
	if err != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	return wire.StartResult{Components: map[string]string{
		"stellar-rpc.version": Version30,
		"stellar-rpc.image":   Image,
		"stellar-rpc.imageid": imageID,
		"events-from":         a.cfg.EventsFrom,
	}}, nil
}

// Advance answers advance: the batches up to head go into the datastore and a
// new stellar-rpc process backfills them.
func (a *Adapter) Advance(head uint32) (wire.AdvanceResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil {
		return wire.AdvanceResult{}, wire.Errorf(wire.ClassBadRequest, "advance before start")
	}
	if err := a.target.release(a.ctx, head); err != nil {
		if we, ok := errors.AsType[*wire.Error](err); ok {
			return wire.AdvanceResult{}, we
		}
		return wire.AdvanceResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	return wire.AdvanceResult{Head: head}, nil
}

// Status answers status.
func (a *Adapter) Status() (wire.StatusResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil || a.target.current == "" {
		return wire.StatusResult{}, wire.Errorf(wire.ClassBadRequest, "status before a successful start")
	}
	return a.target.status(a.ctx)
}

type txRecord struct {
	rec wire.TxRecord
}

func newRecord(ledger uint32, hash string) *txRecord {
	return &txRecord{rec: wire.TxRecord{Ledger: ledger, Hash: hash,
		Participants: []wire.Participant{}, Changes: []wire.Change{}, Unmapped: []wire.Unmapped{}}}
}

// add maps one event into the record, or into its unmapped rows when it does
// not map.
func (r *txRecord) add(ev rpcEvent, table string, row map[string]any) {
	c, err := ev.change()
	switch {
	case err != nil:
		r.rec.Unmapped = append(r.rec.Unmapped, wire.Unmapped{Table: table, Row: row, Note: err.Error()})
	case c == nil:
		r.rec.Unmapped = append(r.rec.Unmapped, wire.Unmapped{Table: table, Category: "diagnostic", Row: row,
			Note: "a diagnostic event; the vocabulary records consensus events only"})
	default:
		c.Source = &wire.RowRef{Table: table}
		r.rec.Changes = append(r.rec.Changes, *c)
	}
}

// Export answers export: one record per transaction the method served in
// ledgers from..to, with the events it served for it.
func (a *Adapter) Export(from, to uint32, emit func(any) error) (wire.ExportResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil {
		return wire.ExportResult{}, wire.Errorf(wire.ClassBadRequest, "export before start")
	}
	c, err := a.target.rpc(a.ctx)
	if err != nil {
		return wire.ExportResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	// every method scans at most 10000 ledgers per call (get_events.go and
	// get_transactions.go, LedgerScanLimit); a short page past that window
	// would read as the end of the data
	if to-from >= 10000 {
		return wire.ExportResult{}, wire.Errorf(wire.ClassUnsupported, "export of %d ledgers; this adapter reads at most one 10000-ledger scan window", to-from+1)
	}
	var (
		order   []*txRecord
		orphans []wire.Unmapped
		readErr error
	)
	switch a.cfg.EventsFrom {
	case FromEvents:
		order, orphans, readErr = exportFromEvents(a.ctx, c, from, to)
	case FromTransactions:
		order, readErr = exportFromTransactions(a.ctx, c, from, to)
	case FromLedgers:
		order, readErr = exportFromLedgers(a.ctx, c, from, to)
	}
	if readErr != nil {
		return wire.ExportResult{}, wire.Errorf(wire.ClassTarget, "%v", readErr)
	}
	var n uint64
	for _, r := range order {
		if err := emit(r.rec); err != nil {
			return wire.ExportResult{Records: n}, err
		}
		n++
	}
	if len(orphans) > 0 {
		if err := emit(wire.UnattributedRecord{Unmapped: orphans}); err != nil {
			return wire.ExportResult{Records: n}, err
		}
		n++
	}
	return wire.ExportResult{Records: n}, nil
}

func txRecords(ctx context.Context, c *client, from, to uint32) ([]served, []*txRecord, map[string]*txRecord, error) {
	txs, err := c.transactions(ctx, from, to, "")
	if err != nil {
		return nil, nil, nil, err
	}
	var order []*txRecord
	byHash := map[string]*txRecord{}
	for _, t := range txs {
		hash, err := t.str("txHash")
		if err != nil {
			return nil, nil, nil, fmt.Errorf("getTransactions: %w", err)
		}
		l, err := t.uint("ledger")
		if err != nil {
			return nil, nil, nil, fmt.Errorf("getTransactions: %w", err)
		}
		r := newRecord(l, hash)
		order = append(order, r)
		byHash[hash] = r
	}
	return txs, order, byHash, nil
}

var eventTypes = map[string]xdr.ContractEventType{
	"contract":   xdr.ContractEventTypeContract,
	"system":     xdr.ContractEventTypeSystem,
	"diagnostic": xdr.ContractEventTypeDiagnostic,
}

func exportFromEvents(ctx context.Context, c *client, from, to uint32) ([]*txRecord, []wire.Unmapped, error) {
	_, order, byHash, err := txRecords(ctx, c, from, to)
	if err != nil {
		return nil, nil, err
	}
	evs, err := c.events(ctx, from, to, "")
	if err != nil {
		return nil, nil, err
	}
	var orphans []wire.Unmapped
	for _, e := range evs {
		ev, err := servedEvent(e)
		hash, _ := e.str("txHash")
		r := byHash[hash]
		if err != nil {
			u := wire.Unmapped{Table: "getEvents", Row: e.raw, Note: err.Error()}
			if r == nil {
				orphans = append(orphans, u)
			} else {
				r.rec.Unmapped = append(r.rec.Unmapped, u)
			}
			continue
		}
		if r == nil {
			orphans = append(orphans, wire.Unmapped{Table: "getEvents", Row: e.raw,
				Note: "an event of a transaction getTransactions did not serve"})
			continue
		}
		r.add(ev, "getEvents", e.raw)
	}
	return order, orphans, nil
}

// servedEvent reads one getEvents item: its type, contract and topics and
// value as served, and its stage from where its id puts it.
func servedEvent(e served) (rpcEvent, error) {
	typ, err := e.str("type")
	if err != nil {
		return rpcEvent{}, err
	}
	t, ok := eventTypes[typ]
	if !ok {
		return rpcEvent{}, fmt.Errorf("event type %q", typ)
	}
	ev := rpcEvent{typ: t}
	if v, ok := e.raw["contractId"]; ok && v != "" {
		if ev.contract, err = e.str("contractId"); err != nil {
			return rpcEvent{}, err
		}
	}
	tx, err := e.uint("transactionIndex")
	if err != nil {
		return rpcEvent{}, err
	}
	op, err := e.uint("operationIndex")
	if err != nil {
		return rpcEvent{}, err
	}
	ev.stage, ev.opIndex = idStage(tx, op)
	if ev.topicsB64, err = e.strings("topic"); err != nil {
		return rpcEvent{}, err
	}
	if ev.dataB64, err = e.str("value"); err != nil {
		return rpcEvent{}, err
	}
	return ev, nil
}

func exportFromTransactions(ctx context.Context, c *client, from, to uint32) ([]*txRecord, error) {
	txs, order, _, err := txRecords(ctx, c, from, to)
	if err != nil {
		return nil, err
	}
	for i, t := range txs {
		r := order[i]
		evs, _ := t.raw["events"].(map[string]any)
		ev := served{evs}
		txEvents, err := ev.strings("transactionEventsXdr")
		if err != nil {
			return nil, fmt.Errorf("getTransactions %s: %w", r.rec.Hash, err)
		}
		for _, s := range txEvents {
			var te xdr.TransactionEvent
			row := map[string]any{"txHash": r.rec.Hash, "transactionEventsXdr": s}
			if err := xdr.SafeUnmarshalBase64(s, &te); err != nil {
				r.rec.Unmapped = append(r.rec.Unmapped, wire.Unmapped{Table: "getTransactions", Row: row, Note: err.Error()})
				continue
			}
			stage, err := stageName(te.Stage)
			if err == nil {
				var e rpcEvent
				if e, err = fromContractEvent(te.Event, stage, 0); err == nil {
					r.add(e, "getTransactions", row)
					continue
				}
			}
			r.rec.Unmapped = append(r.rec.Unmapped, wire.Unmapped{Table: "getTransactions", Row: row, Note: err.Error()})
		}
		var perOp [][]string
		if list, ok := evs["contractEventsXdr"].([]any); ok {
			for i, op := range list {
				s, err := served{map[string]any{"op": op}}.strings("op")
				if err != nil {
					return nil, fmt.Errorf("getTransactions %s contractEventsXdr[%d]: %w", r.rec.Hash, i, err)
				}
				perOp = append(perOp, s)
			}
		}
		for op, list := range perOp {
			for _, s := range list {
				var ce xdr.ContractEvent
				row := map[string]any{"txHash": r.rec.Hash, "operation": op, "contractEventsXdr": s}
				if err := xdr.SafeUnmarshalBase64(s, &ce); err != nil {
					r.rec.Unmapped = append(r.rec.Unmapped, wire.Unmapped{Table: "getTransactions", Row: row, Note: err.Error()})
					continue
				}
				e, err := fromContractEvent(ce, StageOperation, uint32(op))
				if err != nil {
					r.rec.Unmapped = append(r.rec.Unmapped, wire.Unmapped{Table: "getTransactions", Row: row, Note: err.Error()})
					continue
				}
				r.add(e, "getTransactions", row)
			}
		}
		diag, err := t.strings("diagnosticEventsXdr")
		if err != nil {
			return nil, fmt.Errorf("getTransactions %s: %w", r.rec.Hash, err)
		}
		for _, s := range diag {
			r.rec.Unmapped = append(r.rec.Unmapped, wire.Unmapped{Table: "getTransactions", Category: "diagnostic",
				Row:  map[string]any{"txHash": r.rec.Hash, "diagnosticEventsXdr": s},
				Note: "a diagnostic event; the vocabulary records consensus events only"})
		}
	}
	return order, nil
}

func exportFromLedgers(ctx context.Context, c *client, from, to uint32) ([]*txRecord, error) {
	ledgers, err := c.ledgers(ctx, from, to, "")
	if err != nil {
		return nil, err
	}
	var order []*txRecord
	for _, l := range ledgers {
		seq, err := l.uint("sequence")
		if err != nil {
			return nil, fmt.Errorf("getLedgers: %w", err)
		}
		meta, err := l.str("metadataXdr")
		if err != nil {
			return nil, fmt.Errorf("getLedgers %d: %w", seq, err)
		}
		var lcm xdr.LedgerCloseMeta
		if err := xdr.SafeUnmarshalBase64(meta, &lcm); err != nil {
			return nil, fmt.Errorf("getLedgers %d: metadataXdr does not decode: %w", seq, err)
		}
		txs, err := lcmTransactions(lcm)
		if err != nil {
			return nil, fmt.Errorf("getLedgers %d: %w", seq, err)
		}
		for _, tx := range txs {
			r := newRecord(seq, hex.EncodeToString(tx.hash[:]))
			evs, err := metaEvents(tx.meta)
			if err != nil {
				return nil, fmt.Errorf("getLedgers %d tx %s: %w", seq, r.rec.Hash, err)
			}
			for _, e := range evs {
				r.add(e, "getLedgers", map[string]any{"sequence": seq, "txHash": r.rec.Hash})
			}
			order = append(order, r)
		}
	}
	return order, nil
}

type lcmTx struct {
	hash xdr.Hash
	meta xdr.TransactionMeta
}

// lcmTransactions lists a ledger's transactions in apply order with the hash
// core recorded in their result pair.
func lcmTransactions(lcm xdr.LedgerCloseMeta) ([]lcmTx, error) {
	var out []lcmTx
	switch lcm.V {
	case 0:
		for _, t := range lcm.V0.TxProcessing {
			out = append(out, lcmTx{t.Result.TransactionHash, t.TxApplyProcessing})
		}
	case 1:
		for _, t := range lcm.V1.TxProcessing {
			out = append(out, lcmTx{t.Result.TransactionHash, t.TxApplyProcessing})
		}
	case 2:
		for _, t := range lcm.V2.TxProcessing {
			out = append(out, lcmTx{t.Result.TransactionHash, t.TxApplyProcessing})
		}
	default:
		return nil, fmt.Errorf("LedgerCloseMeta version %d", lcm.V)
	}
	return out, nil
}

// Stop answers stop.
func (a *Adapter) Stop() (wire.StopResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil {
		return wire.StopResult{Logs: ""}, nil
	}
	logs, err := a.target.teardown(context.WithoutCancel(a.ctx))
	a.target = nil
	if err != nil {
		return wire.StopResult{}, wire.Errorf(wire.ClassInternal, "removing the container: %v", err)
	}
	return wire.StopResult{Logs: logs}, nil
}

// Close tears the target down when the harness closed stdin without stop.
func (a *Adapter) Close() error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil {
		return nil
	}
	_, err := a.target.teardown(context.WithoutCancel(a.ctx))
	a.target = nil
	if err != nil {
		fmt.Fprintf(os.Stderr, "teardown: %v\n", err)
	}
	return err
}
