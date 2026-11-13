package consumercheck

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"github.com/stellar/go-stellar-sdk/historyarchive"
	"github.com/stellar/go-stellar-sdk/ingest"
	"github.com/stellar/go-stellar-sdk/ingest/ledgerbackend"
	"github.com/stellar/go-stellar-sdk/support/datastore"
	"github.com/stellar/go-stellar-sdk/xdr"
)

func setup(t *testing.T) (root, bin string, chains []string) {
	t.Helper()

	root, err := RepoRoot()
	if err != nil {
		t.Fatal(err)
	}

	bin, err = Binary(root)
	if err != nil {
		t.Fatal(err)
	}

	chains, err = CorpusChains(root)
	if err != nil {
		t.Fatal(err)
	}

	cp := CheckpointChain(root)
	if _, err := os.Stat(filepath.Join(cp, "chain.json")); err != nil {
		t.Fatalf("checkpoint chain missing: %v", err)
	}

	return root, bin, append(chains, cp)
}

// readAll pulls every ledger of the chain through a backend and compares its bytes with the
// canonical hash in chain.json.
func readAll(ctx context.Context, t *testing.T, c Chain, b ledgerbackend.LedgerBackend) {
	t.Helper()

	r := ledgerbackend.BoundedRange(c.LedgerRange.First, c.LedgerRange.Last)
	if err := b.PrepareRange(ctx, r); err != nil {
		t.Fatalf("%s: PrepareRange: %v", c.Dir, err)
	}

	for seq := c.LedgerRange.First; seq <= c.LedgerRange.Last; seq++ {
		lcm, err := b.GetLedger(ctx, seq)
		if err != nil {
			t.Fatalf("%s: GetLedger(%d): %v", c.Dir, seq, err)
		}

		raw, err := lcm.MarshalBinary()
		if err != nil {
			t.Fatalf("%s: ledger %d: %v", c.Dir, seq, err)
		}

		want, err := c.SHA256(seq)
		if err != nil {
			t.Fatal(err)
		}

		if got := Hash(raw); got != want {
			t.Fatalf("%s: ledger %d: sha256 %s, chain.json says %s", c.Dir, seq, got, want)
		}

		if lcm.LedgerSequence() != seq {
			t.Fatalf("%s: asked for %d, got %d", c.Dir, seq, lcm.LedgerSequence())
		}
	}
}

// Check 1: go-stellar-sdk reads every chain's lake through a Filesystem datastore and gets
// the recorded canonical bytes back.
func TestBufferedStorageBackendReadsEveryLake(t *testing.T) {
	_, _, chains := setup(t)
	ctx := context.Background()

	for _, dir := range chains {
		c, err := ReadChain(dir)
		if err != nil {
			t.Fatal(err)
		}

		cfg := datastore.DataStoreConfig{
			Type:              "Filesystem",
			Params:            map[string]string{"destination_path": c.LakeDir()},
			NetworkPassphrase: c.Passphrase,
			Compression:       "zstd",
		}

		store, err := datastore.NewDataStore(ctx, cfg)
		if err != nil {
			t.Fatal(err)
		}

		// the schema comes from the lake's .config.json, which the sdk also checks against
		// the passphrase and compression given above
		schema, err := datastore.LoadSchema(ctx, store, cfg)
		if err != nil {
			t.Fatalf("%s: LoadSchema: %v", dir, err)
		}

		if schema.LedgersPerFile != c.Lake.LedgersPerBatch || schema.FilesPerPartition != c.Lake.BatchesPerPartition {
			t.Fatalf("%s: lake schema %+v disagrees with chain.json", dir, schema)
		}

		backend, err := ledgerbackend.NewBufferedStorageBackend(ledgerbackend.BufferedStorageBackendConfig{
			BufferSize: 10,
			NumWorkers: 2,
			RetryLimit: 0,
			RetryWait:  time.Second,
		}, store, schema)
		if err != nil {
			t.Fatal(err)
		}

		readAll(ctx, t, c, backend)

		if err := backend.Close(); err != nil {
			t.Fatal(err)
		}

		t.Logf("%s: ledgers %d to %d read from the lake", c.Dir, c.LedgerRange.First, c.LedgerRange.Last)
	}
}

// Check 2: go-stellar-sdk's RPC backend reads every chain from `shapekit serve` and gets the
// same bytes.
func TestRPCLedgerBackendReadsEveryChain(t *testing.T) {
	_, bin, chains := setup(t)
	ctx := context.Background()

	for _, dir := range chains {
		c, err := ReadChain(dir)
		if err != nil {
			t.Fatal(err)
		}

		srv, err := Serve(ctx, bin, dir)
		if err != nil {
			t.Fatal(err)
		}

		// a failed check must not leave the server running
		t.Cleanup(func() {
			if err := srv.Stop(); err != nil {
				t.Error(err)
			}
		})

		backend := ledgerbackend.NewRPCLedgerBackend(ledgerbackend.RPCLedgerBackendOptions{
			RPCServerURL: srv.URL,
			BufferSize:   10,
		})

		readAll(ctx, t, c, backend)

		// the backend only answers this once a range is prepared
		latest, err := backend.GetLatestLedgerSequence(ctx)
		if err != nil {
			t.Fatalf("%s: GetLatestLedgerSequence: %v", dir, err)
		}

		if latest != c.LedgerRange.Last {
			t.Fatalf("%s: latest ledger %d, chain ends at %d", dir, latest, c.LedgerRange.Last)
		}

		if err := backend.Close(); err != nil {
			t.Fatal(err)
		}

		if err := srv.Stop(); err != nil {
			t.Fatal(err)
		}

		t.Logf("%s: ledgers %d to %d read over rpc", c.Dir, c.LedgerRange.First, c.LedgerRange.Last)
	}
}

// lakeHeader is the ledger header the lake holds for seq, read through the sdk.
func lakeHeader(ctx context.Context, t *testing.T, c Chain, seq uint32) xdr.LedgerHeaderHistoryEntry {
	t.Helper()

	cfg := datastore.DataStoreConfig{
		Type:              "Filesystem",
		Params:            map[string]string{"destination_path": c.LakeDir()},
		NetworkPassphrase: c.Passphrase,
		Compression:       "zstd",
	}

	store, err := datastore.NewDataStore(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}

	schema, err := datastore.LoadSchema(ctx, store, cfg)
	if err != nil {
		t.Fatal(err)
	}

	backend, err := ledgerbackend.NewBufferedStorageBackend(ledgerbackend.BufferedStorageBackendConfig{
		BufferSize: 1, NumWorkers: 1, RetryWait: time.Second,
	}, store, schema)
	if err != nil {
		t.Fatal(err)
	}

	defer func() { _ = backend.Close() }()

	if err := backend.PrepareRange(ctx, ledgerbackend.BoundedRange(seq, seq)); err != nil {
		t.Fatal(err)
	}

	lcm, err := backend.GetLedger(ctx, seq)
	if err != nil {
		t.Fatal(err)
	}

	return lcm.LedgerHeaderHistoryEntry()
}

// checkArchive connects the sdk's history archive client to url and requires the archive to
// stand at checkpoint: root state, ledger headers equal to the lake's, a bucket list whose
// hash is the one the checkpoint ledger's header commits to, and every bucket entry readable
// by the checkpoint change reader (what an indexer bootstraps its state from).
func checkArchive(ctx context.Context, t *testing.T, c Chain, url string, checkpoint uint32) {
	t.Helper()

	arch, err := historyarchive.Connect(url, historyarchive.ArchiveOptions{
		NetworkPassphrase:   c.Passphrase,
		CheckpointFrequency: historyarchive.DefaultCheckpointFrequency,
	})
	if err != nil {
		t.Fatalf("%s: Connect: %v", url, err)
	}

	has, err := arch.GetRootHAS()
	if err != nil {
		t.Fatalf("%s: GetRootHAS: %v", url, err)
	}

	if has.CurrentLedger != checkpoint {
		t.Fatalf("%s: root state at %d, want %d", url, has.CurrentLedger, checkpoint)
	}

	latest, err := arch.GetLatestLedgerSequence()
	if err != nil || latest != checkpoint {
		t.Fatalf("%s: GetLatestLedgerSequence = %d, %v; want %d", url, latest, err, checkpoint)
	}

	ledgers, err := arch.GetLedgers(c.LedgerRange.First, checkpoint)
	if err != nil {
		t.Fatalf("%s: GetLedgers: %v", url, err)
	}

	for seq := c.LedgerRange.First; seq <= checkpoint; seq++ {
		l, ok := ledgers[seq]
		if !ok {
			t.Fatalf("%s: no ledger %d in the archive", url, seq)
		}

		if err := arch.VerifyLedgerHeaderHistoryEntry(&l.Header); err != nil {
			t.Fatalf("%s: ledger %d: %v", url, seq, err)
		}
	}

	want := lakeHeader(ctx, t, c, checkpoint)
	got := ledgers[checkpoint].Header

	if got.Hash != want.Hash {
		t.Fatalf("%s: archive header %d hash %x, lake says %x", url, checkpoint, got.Hash, want.Hash)
	}

	bl, err := has.BucketListHash()
	if err != nil {
		t.Fatal(err)
	}

	if bl != want.Header.BucketListHash {
		t.Fatalf("%s: bucket list hash %x, ledger %d commits to %x", url, bl, checkpoint, want.Header.BucketListHash)
	}

	reader, err := ingest.NewCheckpointChangeReader(ctx, arch, checkpoint)
	if err != nil {
		t.Fatalf("%s: NewCheckpointChangeReader: %v", url, err)
	}

	entries := 0

	for {
		_, err := reader.Read()
		if errors.Is(err, io.EOF) {
			break
		}

		if err != nil {
			t.Fatalf("%s: reading checkpoint %d: %v", url, checkpoint, err)
		}

		entries++
	}

	_ = reader.Close()

	if entries == 0 {
		t.Fatalf("%s: checkpoint %d holds no ledger entries", url, checkpoint)
	}

	t.Logf("%s: checkpoint %d accepted, %d entries", url, checkpoint, entries)
}

// The history archive as written by archive_at(checkpoint) is accepted by the sdk's client,
// which reports the chosen checkpoint as current.
func TestHistoryArchiveAtCheckpoint(t *testing.T) {
	root, bin, _ := setup(t)
	ctx := context.Background()

	c, err := ReadChain(CheckpointChain(root))
	if err != nil {
		t.Fatal(err)
	}

	if len(c.Checkpoints) < 2 {
		t.Fatalf("%s: checkpoints %v, the check needs at least two", c.Dir, c.Checkpoints)
	}

	for _, cp := range c.Checkpoints {
		out := filepath.Join(t.TempDir(), "archive")
		//nolint:gosec // bin is the shapekit binary under test and the args are fixed flags
		cmd := exec.CommandContext(ctx, bin, "pack", "archive-at", "--chain", c.Dir,
			"--checkpoint", fmt.Sprint(cp), "--out", out)
		cmd.Stderr = os.Stderr

		if err := cmd.Run(); err != nil {
			t.Fatalf("archive-at %d: %v", cp, err)
		}

		checkArchive(ctx, t, c, "file://"+out, cp)
	}
}

// The archive `shapekit serve` publishes moves with the head: it stands at the newest
// checkpoint at or below the visible ledger.
func TestServedArchiveFollowsHead(t *testing.T) {
	root, bin, _ := setup(t)
	ctx := context.Background()

	c, err := ReadChain(CheckpointChain(root))
	if err != nil {
		t.Fatal(err)
	}

	srv, err := Serve(ctx, bin, c.Dir)
	if err != nil {
		t.Fatal(err)
	}

	defer func() {
		if err := srv.Stop(); err != nil {
			t.Error(err)
		}
	}()

	for _, cp := range c.Checkpoints {
		// one ledger past the checkpoint still shows that checkpoint
		head := min(cp+1, c.LedgerRange.Last)
		body, _ := json.Marshal(map[string]uint32{"ledger": head})

		req, err := http.NewRequestWithContext(ctx, http.MethodPost, srv.URL+"/control/head", bytes.NewReader(body))
		if err != nil {
			t.Fatal(err)
		}

		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}

		_ = resp.Body.Close()

		if resp.StatusCode != http.StatusOK {
			t.Fatalf("moving the head to %d: %s", head, resp.Status)
		}

		checkArchive(ctx, t, c, srv.URL+"/archive", cp)
	}
}

// lakeLedgers reads every ledger of the chain from its lake through the sdk.
func lakeLedgers(ctx context.Context, t *testing.T, c Chain) []xdr.LedgerCloseMeta {
	t.Helper()

	cfg := datastore.DataStoreConfig{
		Type:              "Filesystem",
		Params:            map[string]string{"destination_path": c.LakeDir()},
		NetworkPassphrase: c.Passphrase,
		Compression:       "zstd",
	}

	store, err := datastore.NewDataStore(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}

	schema, err := datastore.LoadSchema(ctx, store, cfg)
	if err != nil {
		t.Fatalf("%s: LoadSchema: %v", c.Dir, err)
	}

	backend, err := ledgerbackend.NewBufferedStorageBackend(ledgerbackend.BufferedStorageBackendConfig{
		BufferSize: 10, NumWorkers: 2, RetryWait: time.Second,
	}, store, schema)
	if err != nil {
		t.Fatal(err)
	}

	defer func() { _ = backend.Close() }()

	if err := backend.PrepareRange(ctx, ledgerbackend.BoundedRange(c.LedgerRange.First, c.LedgerRange.Last)); err != nil {
		t.Fatalf("%s: PrepareRange: %v", c.Dir, err)
	}

	out := make([]xdr.LedgerCloseMeta, 0, c.LedgerRange.Last-c.LedgerRange.First+1)

	for seq := c.LedgerRange.First; seq <= c.LedgerRange.Last; seq++ {
		lcm, err := backend.GetLedger(ctx, seq)
		if err != nil {
			t.Fatalf("%s: GetLedger(%d): %v", c.Dir, seq, err)
		}

		out = append(out, lcm)
	}

	return out
}

// guarded runs f and turns a panic into an error, so one bad list names its place instead
// of ending the whole test binary.
func guarded(f func() error) (err error) {
	defer func() {
		if r := recover(); r != nil {
			err = fmt.Errorf("panic: %v", r)
		}
	}()

	return f()
}

// rawLists are the change lists of one transaction's meta, as stored, by name.
func rawLists(tx ingest.LedgerTransaction) (map[string]xdr.LedgerEntryChanges, error) {
	lists := map[string]xdr.LedgerEntryChanges{
		"fee":            tx.FeeChanges,
		"post-apply-fee": tx.PostTxApplyFeeChanges,
	}

	var ops [][]xdr.LedgerEntryChange

	switch tx.UnsafeMeta.V {
	case 0:
		for _, op := range *tx.UnsafeMeta.Operations {
			ops = append(ops, op.Changes)
		}
	case 1:
		m := tx.UnsafeMeta.MustV1()
		lists["tx"] = m.TxChanges

		for _, op := range m.Operations {
			ops = append(ops, op.Changes)
		}
	case 2:
		m := tx.UnsafeMeta.MustV2()
		lists["tx-before"], lists["tx-after"] = m.TxChangesBefore, m.TxChangesAfter

		for _, op := range m.Operations {
			ops = append(ops, op.Changes)
		}
	case 3:
		m := tx.UnsafeMeta.MustV3()
		lists["tx-before"], lists["tx-after"] = m.TxChangesBefore, m.TxChangesAfter

		for _, op := range m.Operations {
			ops = append(ops, op.Changes)
		}
	case 4:
		m := tx.UnsafeMeta.MustV4()
		lists["tx-before"], lists["tx-after"] = m.TxChangesBefore, m.TxChangesAfter

		for _, op := range m.Operations {
			ops = append(ops, op.Changes)
		}
	default:
		return nil, fmt.Errorf("transaction meta v%d", tx.UnsafeMeta.V)
	}

	for i, op := range ops {
		lists[fmt.Sprintf("op %d", i)] = op
	}

	return lists, nil
}

// checkLedgerIngest runs the sdk's ingest readers over one ledger: the change reader to
// the end, and per transaction the fee, transaction and operation changes plus
// GetChangesFromLedgerEntryChanges over every stored list. Indexers built on the sdk do
// exactly this, and a list in an order core never emits makes it panic.
func checkLedgerIngest(c Chain, lcm xdr.LedgerCloseMeta) []error {
	seq := lcm.LedgerSequence()
	where := func(format string, args ...any) string {
		return fmt.Sprintf("%s: ledger %d: ", c.Dir, seq) + fmt.Sprintf(format, args...)
	}

	var errs []error

	if err := guarded(func() error {
		r, err := ingest.NewLedgerChangeReaderFromLedgerCloseMeta(c.Passphrase, lcm)
		if err != nil {
			return err
		}

		defer func() { _ = r.Close() }()

		for {
			_, err := r.Read()
			if errors.Is(err, io.EOF) {
				return nil
			}

			if err != nil {
				return err
			}
		}
	}); err != nil {
		errs = append(errs, fmt.Errorf("%s: %w", where("change reader"), err))
	}

	txs, err := ingest.NewLedgerTransactionReaderFromLedgerCloseMeta(c.Passphrase, lcm)
	if err != nil {
		return append(errs, fmt.Errorf("%s: %w", where("transaction reader"), err))
	}

	defer func() { _ = txs.Close() }()

	for {
		tx, err := txs.Read()
		if errors.Is(err, io.EOF) {
			break
		}

		if err != nil {
			return append(errs, fmt.Errorf("%s: %w", where("reading a transaction"), err))
		}

		at := func(what string) string { return where("tx %d %s", tx.Index, what) }

		if err := guarded(func() error {
			tx.GetFeeChanges()
			tx.GetPostApplyFeeChanges()

			if _, err := tx.GetChanges(); err != nil {
				return err
			}

			for i := range tx.Envelope.Operations() {
				if _, err := tx.GetOperationChanges(uint32(i)); err != nil {
					return fmt.Errorf("op %d: %w", i, err)
				}
			}

			return nil
		}); err != nil {
			errs = append(errs, fmt.Errorf("%s: %w", at("changes"), err))
		}

		lists, err := rawLists(tx)
		if err != nil {
			errs = append(errs, fmt.Errorf("%s: %w", at("meta"), err))
			continue
		}

		for name, l := range lists {
			if err := guarded(func() error {
				ingest.GetChangesFromLedgerEntryChanges(l)
				return nil
			}); err != nil {
				errs = append(errs, fmt.Errorf("%s: %w", at(name), err))
			}
		}
	}

	return errs
}

// Check 3: go-stellar-sdk's ingest readers take every transaction of every chain without a
// panic or an error.
func TestIngestReadsEveryTransaction(t *testing.T) {
	_, _, chains := setup(t)
	ctx := context.Background()

	for _, dir := range chains {
		c, err := ReadChain(dir)
		if err != nil {
			t.Fatal(err)
		}

		txs := 0

		for _, lcm := range lakeLedgers(ctx, t, c) {
			for _, err := range checkLedgerIngest(c, lcm) {
				t.Error(err)
			}

			txs += lcm.CountTransactions()
		}

		t.Logf("%s: %d transactions read by ingest", c.Dir, txs)
	}
}
