// Package walletbackend is the shapekit reference adapter for
// stellar/wallet-backend: it builds a commit, runs TimescaleDB, migrate and
// live ingest in containers on the harness's internal network, and reads
// what wallet-backend stored back in the oracle vocabulary.
package walletbackend

import (
	"context"
	"errors"
	"fmt"
	"os"
	"strings"
	"sync"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// Version of this adapter.
const Version = "0.2.0"

// VocabVersion is the oracle vocabulary version the claims and exports follow
// (ORACLE_VOCAB_VERSION in crates/shapekit-oracle/src/vocab.rs).
const VocabVersion = 2

// Config is how the adapter is invoked.
type Config struct {
	// Commit is the full wallet-backend commit hash to build and run.
	Commit string
	// RepoDir is a wallet-backend clone; created from UpstreamURL when missing.
	RepoDir string
	// DBMemoryBytes caps the TimescaleDB container.
	DBMemoryBytes uint64
	// CPUs is docker's --cpus for every container the adapter starts; no
	// limit when empty.
	CPUs string
	// Builder is the buildx builder an image is built with; the current one
	// when empty.
	Builder string
}

// Adapter implements wire.Handler.
type Adapter struct {
	cfg Config
	src source
	ctx context.Context
	// the signal path in main can call Close while a request runs
	mu     sync.Mutex
	target *target
}

// New checks the configuration and returns an adapter.
func New(ctx context.Context, cfg Config) (*Adapter, error) {
	if !fullSHA.MatchString(cfg.Commit) {
		return nil, fmt.Errorf("--commit must be a full 40-character lowercase commit hash, got %q", cfg.Commit)
	}
	if cfg.RepoDir == "" {
		return nil, fmt.Errorf("no wallet-backend clone directory")
	}
	return &Adapter{cfg: cfg, src: source{dir: cfg.RepoDir, url: UpstreamURL}, ctx: ctx}, nil
}

// Claims are what wallet-backend records, in the oracle vocabulary. Every item
// lists only the fields wallet-backend stores; see the wallet-backend section
// of docs/adapter-interface.md for the row each one comes from.
func Claims() wire.Claims {
	return wire.Claims{
		// transactions_accounts holds G and C keys. Only accounts are claimed:
		// wallet-backend does not record the contracts that emit events (every
		// fee event's native asset contract among them) as participants.
		Participants: &wire.ParticipantClaim{AddressKinds: []string{"account"}, MuxIDs: false},
		Changes: []wire.ChangeClaim{
			{Kind: "account-created", Fields: []string{"account"}},
			{Kind: "account-removed", Fields: []string{"account"}},
			{Kind: "contract-deployed", Fields: []string{"contract"}},
			{Kind: "signer-added", Fields: []string{"account", "signer", "weight"}},
			{Kind: "signer-removed", Fields: []string{"account", "signer", "weight"}},
			{Kind: "signer-weight", Fields: []string{"account", "signer", "before", "after"}},
			{Kind: "thresholds", Fields: []string{"account"}},
			{Kind: "account-flags", Fields: []string{"account"}},
			{Kind: "home-domain", Fields: []string{"account", "before", "after"}},
			{Kind: "data-entry", Fields: []string{"account", "action", "name_hex", "value_before_hex", "value_after_hex"}},
			{Kind: "trustline-created", Fields: []string{"account", "asset", "limit"}},
			{Kind: "trustline-limit", Fields: []string{"account", "asset", "before", "after"}},
			{Kind: "trustline-removed", Fields: []string{"account", "asset"}},
			{Kind: "trustline-flags", Fields: []string{"account", "asset"}},
			{
				Kind:   "token-event",
				Where:  map[string][]any{"event": {"transfer", "mint", "burn", "clawback"}},
				Fields: []string{"event", "contract", "from", "to", "amount", "to_muxed_id"},
			},
			{
				// wallet-backend keeps one net fee row per transaction: the charge
				// event is claimed without its amount, and a soroban refund event
				// (after-tx or after-all-txs) is not claimed at all
				Kind:   "token-event",
				Where:  map[string][]any{"event": {"fee"}, "stage": {"before-all-txs"}},
				Fields: []string{"event", "contract", "from"},
			},
			{
				Kind:   "token-event",
				Where:  map[string][]any{"event": {"approve"}},
				Fields: []string{"event", "contract", "from", "spender", "amount", "live_until_ledger"},
			},
		},
	}
}

// Describe answers describe.
func (a *Adapter) Describe(protocol int) (wire.DescribeResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if protocol < wire.MinProtocolVersion || protocol > wire.ProtocolVersion {
		return wire.DescribeResult{}, wire.Errorf(wire.ClassUnsupported, "protocol %d; this adapter speaks %d to %d", protocol, wire.MinProtocolVersion, wire.ProtocolVersion)
	}
	if err := a.src.ensure(a.ctx, a.cfg.Commit); err != nil {
		return wire.DescribeResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	maxProto, _, note, err := a.src.maxProtocol(a.ctx, a.cfg.Commit)
	if err != nil {
		return wire.DescribeResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	if note != "" {
		fmt.Fprintf(os.Stderr, "wb-adapter: note: %s\n", note)
	}
	var caps []string
	if protocol >= 2 {
		caps = []string{wire.CapabilityDump, wire.CapabilityRestart}
	}
	return wire.DescribeResult{
		Protocol:        protocol,
		Adapter:         "shapekit-wallet-backend-adapter",
		AdapterVersion:  Version,
		Target:          "wallet-backend",
		TargetVersion:   a.cfg.Commit,
		DeliveryModes:   []string{wire.DeliveryRPC, wire.DeliverySep54S3},
		DefaultDelivery: wire.DeliveryRPC,
		Bootstrap:       "checkpoint",
		VocabVersion:    VocabVersion,
		// go-stellar-sdk decodes every older LedgerCloseMeta version, so only the
		// top of the range depends on the commit
		Protocols:    wire.ProtocolRange{Min: 1, Max: maxProto},
		Claims:       Claims(),
		Capabilities: caps,
	}, nil
}

// Start answers start.
func (a *Adapter) Start(p wire.StartParams) (wire.StartResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassBadRequest, "start sent twice")
	}
	t, err := startTarget(a.ctx, a.src, a.cfg.Commit, p, a.cfg.DBMemoryBytes, a.cfg.CPUs, a.cfg.Builder)
	a.target = t
	if err != nil {
		return wire.StartResult{}, err
	}
	_, sdk, sdkNote, err := a.src.maxProtocol(a.ctx, a.cfg.Commit)
	if err != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	imageID, err := docker(a.ctx, "image", "inspect", "--format", "{{.Id}}", t.image)
	if err != nil {
		return wire.StartResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	comps := map[string]string{
		"wallet-backend.commit": a.cfg.Commit,
		"wallet-backend.image":  imageID,
		"go-stellar-sdk":        sdk,
		"timescaledb":           TimescaleImage,
	}
	// a version or base the tables do not list is recorded with the run
	if sdkNote != "" {
		comps["go-stellar-sdk.note"] = sdkNote
	}
	if len(t.baseNotes) > 0 {
		comps["base-images.note"] = strings.Join(t.baseNotes, "; ")
	}
	if p.Delivery.Mode == wire.DeliverySep54S3 {
		comps["seaweedfs"] = SeaweedImage
	}
	return wire.StartResult{Components: comps}, nil
}

// Advance answers advance.
func (a *Adapter) Advance(head uint32) (wire.AdvanceResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil {
		return wire.AdvanceResult{}, wire.Errorf(wire.ClassBadRequest, "advance before start")
	}
	if a.target.lake != nil {
		if err := a.target.lake.copyUpTo(a.ctx, head); err != nil {
			return wire.AdvanceResult{}, wire.Errorf(wire.ClassTarget, "copying the lake up to ledger %d: %v", head, err)
		}
	}
	return wire.AdvanceResult{Head: head}, nil
}

// Status answers status.
func (a *Adapter) Status() (wire.StatusResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil || a.target.store == nil {
		return wire.StatusResult{}, wire.Errorf(wire.ClassBadRequest, "status before a successful start")
	}
	return a.target.status(a.ctx)
}

// Export answers export.
func (a *Adapter) Export(from, to uint32, emit func(any) error) (wire.ExportResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil || a.target.store == nil {
		return wire.ExportResult{}, wire.Errorf(wire.ClassBadRequest, "export before a successful start")
	}
	n, err := a.target.store.export(a.ctx, from, to, emit)
	if err != nil {
		return wire.ExportResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	return wire.ExportResult{Records: n}, nil
}

// Dump answers dump: every table wallet-backend's migrations created, as
// stored.
func (a *Adapter) Dump(tables []string, table func(wire.DumpTable) error, row func(wire.DumpRow) error) (wire.DumpResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil || a.target.store == nil {
		return wire.DumpResult{}, wire.Errorf(wire.ClassBadRequest, "dump before a successful start")
	}
	res, err := a.target.store.dump(a.ctx, tables, table, row)
	if err != nil {
		if _, ok := errors.AsType[*wire.Error](err); ok {
			return res, err
		}
		return res, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	return res, nil
}

// Restart answers restart: docker restarts the ingest container, which keeps
// the database, so wallet-backend resumes from its stored cursor as it does
// after a deployment restart.
func (a *Adapter) Restart() (wire.RestartResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.target == nil || a.target.store == nil {
		return wire.RestartResult{}, wire.Errorf(wire.ClassBadRequest, "restart before a successful start")
	}
	if _, err := docker(a.ctx, "restart", a.target.ingest); err != nil {
		return wire.RestartResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	return wire.RestartResult{}, nil
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
