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
