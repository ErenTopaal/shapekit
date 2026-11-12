// Package consumercheck checks that shapekit's ledger delivery is accepted by the
// go-stellar-sdk ingestion code real indexers use: BufferedStorageBackend over a
// filesystem SEP-54 lake, RPCLedgerBackend against `shapekit serve`, and the history
// archive client against an archive written as it stood at one checkpoint.
//
// The checks run the shapekit binary. Build it first (`cargo build -p shapekit-cli`); the
// tests look for SHAPEKIT_BIN, then for target/debug/shapekit at the repository root, and
// fail when neither exists.
package consumercheck

import (
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// LedgerRecord is one entry of chain.json's ledgers list.
type LedgerRecord struct {
	LedgerSeq uint32 `json:"ledger_seq"`
	SHA256    string `json:"sha256"`
}

// Chain is the part of a committed chain's chain.json the checks use.
type Chain struct {
	Dir         string
	Schema      uint32 `json:"schema"`
	Passphrase  string `json:"passphrase"`
	LedgerRange struct {
		First uint32 `json:"first"`
		Last  uint32 `json:"last"`
	} `json:"ledger_range"`
	Checkpoints []uint32 `json:"checkpoints"`
	Lake        struct {
		Dir                 string `json:"dir"`
		LedgersPerBatch     uint32 `json:"ledgers_per_batch"`
		BatchesPerPartition uint32 `json:"batches_per_partition"`
	} `json:"lake"`
	Ledgers []LedgerRecord `json:"ledgers"`
}

// LakeDir is the chain's SEP-54 lake.
func (c Chain) LakeDir() string { return filepath.Join(c.Dir, c.Lake.Dir) }

// SHA256 returns the canonical hash chain.json records for seq.
func (c Chain) SHA256(seq uint32) (string, error) {
	i := int(seq) - int(c.LedgerRange.First)
	if i < 0 || i >= len(c.Ledgers) || c.Ledgers[i].LedgerSeq != seq {
		return "", fmt.Errorf("%s: chain.json has no record for ledger %d", c.Dir, seq)
	}

	return c.Ledgers[i].SHA256, nil
}

// RepoRoot is the repository root, two levels above the adapters module.
func RepoRoot() (string, error) {
	wd, err := os.Getwd()
	if err != nil {
		return "", err
	}

	// this package lives at adapters/protocol/consumercheck
	return filepath.Abs(filepath.Join(wd, "..", "..", ".."))
}

// ReadChain reads a committed chain directory's chain.json.
func ReadChain(dir string) (Chain, error) {
	b, err := os.ReadFile(filepath.Join(dir, "chain.json")) //nolint:gosec // dir is a chain directory the checks enumerate
	if err != nil {
		return Chain{}, err
	}

	var c Chain
	if err := json.Unmarshal(b, &c); err != nil {
		return Chain{}, fmt.Errorf("%s: %w", dir, err)
	}

	if c.Schema != 2 {
		return Chain{}, fmt.Errorf("%s: chain.json schema %d is not the committed form; run `shapekit pack sep54`", dir, c.Schema)
	}

	c.Dir = dir

	return c, nil
}

// CorpusChains lists every chain directory under corpus/chains, sorted.
func CorpusChains(root string) ([]string, error) {
	matches, err := filepath.Glob(filepath.Join(root, "corpus", "chains", "*", "*", "chain.json"))
	if err != nil {
		return nil, err
	}

	dirs := make([]string, 0, len(matches))
	for _, m := range matches {
		dirs = append(dirs, filepath.Dir(m))
	}

	sort.Strings(dirs)

	if len(dirs) == 0 {
		return nil, errors.New("no chains under corpus/chains")
	}

	return dirs, nil
}

// CheckpointChain is the recorded chain that runs past two checkpoints, kept with the
// shapekit-pack tests; the corpus chains end before the first checkpoint.
func CheckpointChain(root string) string {
	return filepath.Join(root, "crates", "shapekit-pack", "tests", "fixtures", "checkpoints.p30", "testnet")
}

// Binary locates the shapekit binary.
func Binary(root string) (string, error) {
	if b := os.Getenv("SHAPEKIT_BIN"); b != "" {
		return b, nil
	}

	b := filepath.Join(root, "target", "debug", "shapekit")
	if _, err := os.Stat(b); err != nil {
		return "", fmt.Errorf("no shapekit binary at %s and SHAPEKIT_BIN is unset; run `cargo build -p shapekit-cli`: %w", b, err)
	}

	return b, nil
}

// Hash is the lowercase hex SHA-256 the chain.json records use.
func Hash(b []byte) string {
	h := sha256.Sum256(b)
	return hex.EncodeToString(h[:])
}

// Server is a running `shapekit serve`.
type Server struct {
	URL string
	cmd *exec.Cmd
}

// Serve starts `shapekit serve` for a chain directory on a free loopback port and waits
// for the line that names its address.
func Serve(ctx context.Context, bin, chainDir string, extra ...string) (*Server, error) {
	args := append([]string{"serve", "--chain", chainDir, "--listen", "127.0.0.1:0"}, extra...)
	cmd := exec.CommandContext(ctx, bin, args...) //nolint:gosec // bin is the shapekit binary under test
	cmd.Stderr = os.Stderr

	out, err := cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}

	if err := cmd.Start(); err != nil {
		return nil, err
	}

	lines := make(chan string, 1)
	go func() {
		s := bufio.NewScanner(out)
		for s.Scan() {
			if _, url, ok := strings.Cut(s.Text(), " at "); ok && strings.HasPrefix(s.Text(), "serving ") {
				lines <- url
				break
			}
		}
		// keep draining so the server never blocks on a full pipe
		_, _ = io.Copy(io.Discard, out)
	}()

	select {
	case url := <-lines:
		return &Server{URL: url, cmd: cmd}, nil
	case <-time.After(60 * time.Second):
		_ = cmd.Process.Kill()
		_ = cmd.Wait()

		return nil, fmt.Errorf("shapekit serve %s did not report its address within a minute", chainDir)
	}
}

// Stop kills the server and reaps it.
func (s *Server) Stop() error {
	if err := s.cmd.Process.Kill(); err != nil {
		return err
	}

	// a killed process reports the signal as its exit error
	var exitErr *exec.ExitError
	if err := s.cmd.Wait(); err != nil && !errors.As(err, &exitErr) {
		return err
	}

	return nil
}
