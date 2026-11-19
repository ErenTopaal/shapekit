// Package stellaretl is the shapekit adapter for stellar/stellar-etl: it
// builds a commit, serves the chain's lake to it from a GCS emulator on the
// harness's internal network, runs its exports ledger by ledger the way an
// orchestrator runs batches, and reads the exported rows back in the oracle
// vocabulary.
package stellaretl

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"slices"
	"sort"
	"strings"
	"sync"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// Version of this adapter.
const Version = "0.2.0"

// VocabVersion is the oracle vocabulary version the claims and exports follow.
const VocabVersion = 2

// Config is how the adapter is invoked.
type Config struct {
	// Commit is the full stellar-etl commit hash to build and run.
	Commit string
	// RepoDir is a stellar-etl clone; created from UpstreamURL when missing.
	RepoDir string
	// CPUs is docker's --cpus for every container; no limit when empty.
	CPUs string
	// Builder is the buildx builder the image is built with; the default when
	// empty.
	Builder string
}

// Adapter implements wire.Handler, wire.Dumper.
type Adapter struct {
	cfg    Config
	src    source
	ctx    context.Context
	mu     sync.Mutex
	target *target
}

// New checks the configuration and returns an adapter.
func New(ctx context.Context, cfg Config) (*Adapter, error) {
	if !fullSHA.MatchString(cfg.Commit) {
		return nil, fmt.Errorf("--commit must be a full 40-character lowercase commit hash, got %q", cfg.Commit)
	}
	if cfg.RepoDir == "" {
		return nil, fmt.Errorf("no stellar-etl clone directory")
	}
	return &Adapter{cfg: cfg, src: source{dir: cfg.RepoDir, url: UpstreamURL}, ctx: ctx}, nil
}

// Claims are what the exports hold, in the oracle vocabulary: the classic
// changes stellar-etl's effects record, and the token events of its token
// transfer export. Participants are not claimed: no export lists a
// transaction's participants.
func Claims() wire.Claims {
	return wire.Claims{
		Changes: []wire.ChangeClaim{
			{Kind: "account-created", Fields: []string{"account", "starting_balance"}},
			{Kind: "account-removed", Fields: []string{"account"}},
			{Kind: "signer-added", Fields: []string{"account", "signer", "weight"}},
			{Kind: "signer-removed", Fields: []string{"account", "signer"}},
			{Kind: "signer-weight", Fields: []string{"account", "signer", "after"}},
			{Kind: "trustline-created", Fields: []string{"account", "asset", "limit"}},
			{Kind: "trustline-removed", Fields: []string{"account", "asset"}},
			{Kind: "trustline-limit", Fields: []string{"account", "asset", "after"}},
			{Kind: "home-domain", Fields: []string{"account", "after"}},
			{Kind: "data-entry", Fields: []string{"account", "action", "name_hex", "value_after_hex"}},
			{
				Kind:   "token-event",
				Where:  map[string][]any{"event": {"transfer", "mint", "burn", "clawback"}},
				Fields: []string{"event", "contract", "from", "to", "amount"},
			},
			{
				Kind:   "token-event",
				Where:  map[string][]any{"event": {"fee"}, "stage": {"before-all-txs"}},
				Fields: []string{"event", "contract", "from"},
			},
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
	if err := a.src.ensure(a.ctx, a.cfg.Commit); err != nil {
		return wire.DescribeResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	_, maxProto, note, err := a.src.sdk(a.ctx, a.cfg.Commit)
	if err != nil {
		return wire.DescribeResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	if note != "" {
		fmt.Fprintf(os.Stderr, "etl-adapter: note: %s\n", note)
	}
	return wire.DescribeResult{
		Protocol:        protocol,
		Adapter:         "shapekit-stellar-etl-adapter",
		AdapterVersion:  Version,
		Target:          "stellar-etl",
		TargetVersion:   a.cfg.Commit,
		DeliveryModes:   []string{wire.DeliverySep54GCS},
		DefaultDelivery: wire.DeliverySep54GCS,
		Bootstrap:       "ledger",
		VocabVersion:    VocabVersion,
		Protocols:       wire.ProtocolRange{Min: 1, Max: maxProto},
		Claims:          Claims(),
		Capabilities:    []string{wire.CapabilityDump},
	}, nil
}

// Start answers start.
func (a *Adapter) Start(p wire.StartParams) (wire.StartResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassBadRequest, "start sent twice")
	}
	if err := a.src.ensure(a.ctx, a.cfg.Commit); err != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	image, baseNotes, err := a.src.ensureImage(a.ctx, a.cfg.Commit, a.cfg.Builder)
	if err != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	for _, n := range baseNotes {
		fmt.Fprintf(os.Stderr, "etl-adapter: note: %s\n", n)
	}
	t, err := startTarget(a.ctx, image, a.cfg.CPUs, p)
	a.target = t
	if err != nil {
		return wire.StartResult{}, err
	}
	sdk, _, sdkNote, err := a.src.sdk(a.ctx, a.cfg.Commit)
	if err != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	imageID, err := docker(a.ctx, "image", "inspect", "--format", "{{.Id}}", image)
	if err != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	comps := map[string]string{
		"stellar-etl.commit": a.cfg.Commit,
		"stellar-etl.image":  imageID,
		"go-stellar-sdk":     sdk,
		"fake-gcs-server":    GCSImage,
	}
	// a version or base the tables do not list is recorded with the run
	if sdkNote != "" {
		comps["go-stellar-sdk.note"] = sdkNote
	}
	if len(baseNotes) > 0 {
		comps["base-images.note"] = strings.Join(baseNotes, "; ")
	}
	return wire.StartResult{Components: comps}, nil
}

// Advance answers advance: the batches up to head go into the bucket and the
// worker may export them.
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
	if a.target == nil || a.target.etl == "" {
		return wire.StatusResult{}, wire.Errorf(wire.ClassBadRequest, "status before a successful start")
	}
	return a.target.status(a.ctx)
}

type txRows struct {
	ledger  uint32
	hash    string
	changes []wire.Change
	unmap   []wire.Unmapped
}

// Export answers export: one record per exported transaction, with the
// changes of its effects and token transfers.
func (a *Adapter) Export(from, to uint32, emit func(any) error) (wire.ExportResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil {
		return wire.ExportResult{}, wire.Errorf(wire.ClassBadRequest, "export before start")
	}
	t := a.target
	read := func(name string) ([]map[string]any, error) {
		r, err := t.readRows(name, from, to)
		if err != nil {
			return nil, wire.Errorf(wire.ClassTarget, "reading %s: %v", name, err)
		}
		return r, nil
	}
	txs, err := read("transactions")
	if err != nil {
		return wire.ExportResult{}, err
	}
	byID := map[int64]*txRows{}
	byHash := map[string]*txRows{}
	var order []*txRows
	for _, r := range txs {
		hash, _ := str(r, "transaction_hash")
		idText, _ := str(r, "id")
		id, err := json.Number(idText).Int64()
		if err != nil {
			return wire.ExportResult{}, wire.Errorf(wire.ClassTarget, "transaction %s has id %q", hash, idText)
		}
		l, _ := str(r, "ledger_sequence")
		ln, err := json.Number(l).Int64()
		if err != nil {
			return wire.ExportResult{}, wire.Errorf(wire.ClassTarget, "transaction %s has ledger %q", hash, l)
		}
		x := &txRows{ledger: uint32(ln), hash: hash} //nolint:gosec // a ledger sequence
		byID[id], byHash[hash] = x, x
		order = append(order, x)
	}
	var orphans []wire.Unmapped
	effects, err := read("effects")
	if err != nil {
		return wire.ExportResult{}, err
	}
	created := map[string]bool{}
	for _, r := range effects {
		if typ, _ := str(r, "type_string"); typ == "account_created" {
			op, _ := str(r, "operation_id")
			addr, _ := str(r, "address")
			created[op+" "+addr] = true
		}
	}
	for _, r := range effects {
		op, _ := str(r, "operation_id")
		opID, err := json.Number(op).Int64()
		x := byID[toidTx(opID)]
		if err != nil || x == nil {
			orphans = append(orphans, unmapped("effects", r, "effect of a transaction stellar-etl did not export"))
			continue
		}
		addr, _ := str(r, "address")
		c, note := effectChange(r, created[op+" "+addr])
		if c == nil {
			x.unmap = append(x.unmap, unmapped("effects", r, note))
			continue
		}
		x.changes = append(x.changes, *c)
	}
	tokens, err := read("token_transfer")
	if err != nil {
		return wire.ExportResult{}, err
	}
	for _, r := range tokens {
		hash, _ := str(r, "transaction_hash")
		x := byHash[hash]
		if x == nil {
			orphans = append(orphans, unmapped("token_transfer", r, "token transfer of a transaction stellar-etl did not export"))
			continue
		}
		c, note := tokenChange(r)
		if c == nil {
			x.unmap = append(x.unmap, unmapped("token_transfer", r, note))
			continue
		}
		x.changes = append(x.changes, *c)
	}
	var n uint64
	for _, x := range order {
		rec := wire.TxRecord{Ledger: x.ledger, Hash: x.hash, Participants: []wire.Participant{}, Changes: x.changes, Unmapped: x.unmap}
		if rec.Changes == nil {
			rec.Changes = []wire.Change{}
		}
		if rec.Unmapped == nil {
			rec.Unmapped = []wire.Unmapped{}
		}
		if err := emit(rec); err != nil {
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

// Dump answers dump: every export of every exported ledger, one table per
// export.
func (a *Adapter) Dump(tables []string, table func(wire.DumpTable) error, row func(wire.DumpRow) error) (wire.DumpResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil {
		return wire.DumpResult{}, wire.Errorf(wire.ClassBadRequest, "dump before start")
	}
	t := a.target
	t.mu.Lock()
	var through uint32
	if t.done != nil {
		through = *t.done
	}
	t.mu.Unlock()
	names := make([]string, 0, len(Exports))
	for _, e := range Exports {
		names = append(names, e.Name)
	}
	sort.Strings(names)
	if len(tables) > 0 {
		for _, n := range tables {
			if !slices.Contains(names, n) {
				return wire.DumpResult{}, wire.Errorf(wire.ClassBadRequest, "no table %q", n)
			}
		}
		names = tables
	}
	var res wire.DumpResult
	for _, name := range names {
		var rows []map[string]any
		if through >= t.params.Ledgers.Bootstrap {
			r, err := t.readRows(name, t.params.Ledgers.Bootstrap, through)
			if err != nil {
				return res, wire.Errorf(wire.ClassTarget, "reading %s: %v", name, err)
			}
			rows = r
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
		return wire.StopResult{}, wire.Errorf(wire.ClassInternal, "removing containers: %v", err)
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
