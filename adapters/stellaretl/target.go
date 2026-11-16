package stellaretl

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// NetworkLabel marks every container with the harness's network name, so the
// harness can remove leftovers.
const NetworkLabel = "shapekit.docker_network"

const (
	gcsPort      = "4443"
	gcsAlias     = "gcs"
	bucket       = "ledgers"
	readyTimeout = 2 * time.Minute
	// how long a failed export of a ledger waits before the next attempt; an
	// orchestrator retries a failed batch the same way
	retryWait = 2 * time.Second
)

// Exports are the stellar-etl commands run for every ledger, each writing
// {ledger}-{ledger}-{name}.txt into its folder.
var Exports = []struct{ Command, Name string }{
	{"export_transactions", "transactions"},
	{"export_operations", "operations"},
	{"export_effects", "effects"},
	{"export_token_transfer", "token_transfer"},
	{"export_contract_events", "contract_events"},
}

// target is one stellar-etl deployment: the GCS emulator holding the lake,
// and a container in which the adapter runs the exports ledger by ledger.
type target struct {
	params  wire.StartParams
	image   string
	cpus    string
	prefix  string
	gcs     string
	etl     string
	gcsURL  string
	outDir  string
	network string

	mu       sync.Mutex
	released uint32
	done     *uint32
	lastErr  *string
	peak     uint64
	havePeak bool
	work     chan struct{}
	stop     chan struct{}
	finished chan struct{}
	working  bool
}

func randomSuffix() (string, error) {
	b := make([]byte, 4)
	if _, err := rand.Read(b); err != nil {
		return "", fmt.Errorf("random container suffix: %w", err)
	}
	return hex.EncodeToString(b), nil
}

func (t *target) runFlags() []string {
	f := []string{"--network", t.params.DockerNetwork, "--label", NetworkLabel + "=" + t.params.DockerNetwork, "--label", "shapekit.adapter=stellar-etl"}
	if t.cpus != "" {
		f = append(f, "--cpus", t.cpus)
	}
	return f
}

// etlNetwork is the name stellar-etl gives the passphrase it was told: its
// datastore path ends in it, and only testnet and pubnet are reachable.
func etlNetwork(passphrase string) (string, error) {
	switch passphrase {
	case "Test SDF Network ; September 2015":
		return "testnet", nil
	case "Public Global Stellar Network ; September 2015":
		return "pubnet", nil
	}
	return "", wire.Errorf(wire.ClassUnsupported, "stellar-etl knows only the testnet and pubnet passphrases, not %q", passphrase)
}

func startTarget(ctx context.Context, image, cpus string, p wire.StartParams) (*target, error) {
	if p.DockerNetwork == "" {
		return nil, wire.Errorf(wire.ClassUnsupported, "stellar-etl runs in containers; the harness must pass docker_network")
	}
	if p.Delivery.Mode != wire.DeliverySep54GCS {
		return nil, wire.Errorf(wire.ClassUnsupported, "delivery mode %q: this adapter serves the lake to stellar-etl from a GCS emulator, filled from the lake directory", p.Delivery.Mode)
	}
	if p.Limits.MemoryBytes == 0 {
		return nil, wire.Errorf(wire.ClassBadRequest, "limits.memory_bytes is 0")
	}
	net, err := etlNetwork(p.NetworkPassphrase)
	if err != nil {
		return nil, err
	}
	suffix, err := randomSuffix()
	if err != nil {
		return nil, err
	}
	t := &target{
		params: p, image: image, cpus: cpus, prefix: "sketl-" + suffix, network: net,
		outDir: filepath.Join(p.Workdir, "out"),
		work:   make(chan struct{}, 1), stop: make(chan struct{}), finished: make(chan struct{}),
	}
	t.gcs, t.etl = t.prefix+"-gcs", t.prefix+"-etl"
	if err := os.MkdirAll(t.outDir, 0o750); err != nil {
		return t, wire.Errorf(wire.ClassInternal, "creating %s: %v", t.outDir, err)
	}
	if err := t.startGCS(ctx); err != nil {
		return t, err
	}
	if err := t.copyUpTo(ctx, p.Ledgers.Bootstrap); err != nil {
		return t, err
	}
	args := append([]string{"run", "-d", "--name", t.etl,
		"--memory", strconv.FormatUint(p.Limits.MemoryBytes, 10),
		"--memory-swap", strconv.FormatUint(p.Limits.MemoryBytes, 10),
		"--user", strconv.Itoa(os.Getuid()) + ":" + strconv.Itoa(os.Getgid()),
		"-v", t.outDir + ":/etl/out",
		"-e", "STORAGE_EMULATOR_HOST=http://" + gcsAlias + ":" + gcsPort,
		"-e", "HOME=/tmp",
	}, t.runFlags()...)
	args = append(args, image, "sleep", "infinity")
	if _, err := docker(ctx, args...); err != nil {
		return t, wire.Errorf(wire.ClassTarget, "starting the stellar-etl container: %v", err)
	}
	t.released = p.Ledgers.Bootstrap
	t.working = true
	go t.worker(context.WithoutCancel(ctx), p.Ledgers.Bootstrap)
	t.kick()
	return t, nil
}

func (t *target) startGCS(ctx context.Context) error {
	args := append([]string{"run", "-d", "--name", t.gcs, "--network-alias", gcsAlias, "--memory", "1g"}, t.runFlags()...)
	args = append(args, GCSImage, "-scheme", "http", "-port", gcsPort, "-public-host", gcsAlias+":"+gcsPort, "-backend", "memory")
	if _, err := docker(ctx, args...); err != nil {
		return wire.Errorf(wire.ClassTarget, "starting the GCS emulator: %v", err)
	}
	ip, err := containerIP(ctx, t.gcs, t.params.DockerNetwork)
	if err != nil {
		return wire.Errorf(wire.ClassTarget, "%v", err)
	}
	t.gcsURL = "http://" + ip + ":" + gcsPort
	client := &http.Client{Timeout: 5 * time.Second}
	deadline := time.Now().Add(readyTimeout)
	body := []byte(`{"name":"` + bucket + `"}`)
	for {
		req, err := http.NewRequestWithContext(ctx, http.MethodPost, t.gcsURL+"/storage/v1/b?project=shapekit", bytes.NewReader(body))
		if err != nil {
			return wire.Errorf(wire.ClassInternal, "%v", err)
		}
		req.Header.Set("Content-Type", "application/json")
		resp, err := client.Do(req)
		if err == nil {
			msg, _ := io.ReadAll(resp.Body)
			_ = resp.Body.Close()
			if resp.StatusCode == http.StatusOK || resp.StatusCode == http.StatusConflict {
				return nil
			}
			err = fmt.Errorf("creating bucket %s: %s %s", bucket, resp.Status, strings.TrimSpace(string(msg)))
		}
		if time.Now().After(deadline) {
			return wire.Errorf(wire.ClassTarget, "the GCS emulator did not accept a bucket within %s: %v", readyTimeout, err)
		}
		time.Sleep(time.Second)
	}
}

func (t *target) upload(ctx context.Context, name string, data []byte) error {
	u := t.gcsURL + "/upload/storage/v1/b/" + bucket + "/o?uploadType=media&name=" + url.QueryEscape(name)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, u, bytes.NewReader(data))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/octet-stream")
	resp, err := (&http.Client{Timeout: 30 * time.Second}).Do(req)
	if err != nil {
		return err
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusOK {
		msg, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("uploading %s: %s %s", name, resp.Status, strings.TrimSpace(string(msg)))
	}
	return nil
}

// copyUpTo uploads the lake's .config.json and every batch whose ledgers are
// at or below head and not uploaded yet, under <network>/ in the bucket.
func (t *target) copyUpTo(ctx context.Context, head uint32) error {
	lake := t.params.Delivery.Lake
	cfg, err := os.ReadFile(filepath.Join(lake.Path, ".config.json"))
	if err != nil {
		return wire.Errorf(wire.ClassInternal, "reading the lake config: %v", err)
	}
	if err := t.upload(ctx, t.network+"/.config.json", cfg); err != nil {
		return wire.Errorf(wire.ClassTarget, "%v", err)
	}
	return filepath.WalkDir(lake.Path, func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() || !strings.HasSuffix(path, ".xdr.zst") {
			return err
		}
		last, ok := batchLast(d.Name())
		if !ok {
			return wire.Errorf(wire.ClassInternal, "lake file %s has no ledger in its name", path)
		}
		if last > head {
			return nil
		}
		rel, err := filepath.Rel(lake.Path, path)
		if err != nil {
			return err
		}
		data, err := os.ReadFile(path) //nolint:gosec // a file of the lake the harness named
		if err != nil {
			return wire.Errorf(wire.ClassInternal, "reading %s: %v", path, err)
		}
		if err := t.upload(ctx, t.network+"/"+filepath.ToSlash(rel), data); err != nil {
			return wire.Errorf(wire.ClassTarget, "%v", err)
		}
		return nil
	})
}

// batchLast reads the last ledger of a SEP-54 batch file name:
// FFFFFFFD--2.xdr.zst holds ledger 2, FFFFFFFD--2-5.xdr.zst ledgers 2 to 5.
func batchLast(name string) (uint32, bool) {
	stem := strings.TrimSuffix(name, ".xdr.zst")
	_, rng, ok := strings.Cut(stem, "--")
	if !ok {
		return 0, false
	}
	if _, hi, ok := strings.Cut(rng, "-"); ok {
		rng = hi
	}
	n, err := strconv.ParseUint(rng, 10, 32)
	if err != nil {
		return 0, false
	}
	return uint32(n), true
}

func (t *target) kick() {
	select {
	case t.work <- struct{}{}:
	default:
	}
}

// release makes ledgers up to head available to the worker.
func (t *target) release(ctx context.Context, head uint32) error {
	t.mu.Lock()
	from := t.released
	t.mu.Unlock()
	if head > from {
		if err := t.copyUpTo(ctx, head); err != nil {
			return err
		}
	}
	t.mu.Lock()
	if head > t.released {
		t.released = head
	}
	t.mu.Unlock()
	t.kick()
	return nil
}

// worker exports released ledgers in order, starting at first. A failed
// export is retried until it succeeds or the target stops; the error stays in
// lastErr, so a ledger that never exports reads as a halt.
func (t *target) worker(ctx context.Context, first uint32) {
	defer close(t.finished)
	next := first
	for {
		t.mu.Lock()
		ready := next <= t.released
		t.mu.Unlock()
		if !ready {
			select {
			case <-t.stop:
				return
			case <-t.work:
				continue
			}
		}
		if err := t.exportLedger(ctx, next); err != nil {
			msg := err.Error()
			t.mu.Lock()
			t.lastErr = &msg
			t.mu.Unlock()
			fmt.Fprintf(os.Stderr, "ledger %d: %s\n", next, msg)
			select {
			case <-t.stop:
				return
			case <-time.After(retryWait):
			}
			continue
		}
		t.mu.Lock()
		done := next
		t.done = &done
		t.mu.Unlock()
		next++
	}
}

// exportLedger runs every export for one ledger. Each command writes its file
// only when it finishes, so a ledger counts as exported once all five did.
func (t *target) exportLedger(ctx context.Context, ledger uint32) error {
	l := strconv.FormatUint(uint64(ledger), 10)
	for _, e := range Exports {
		args := []string{"exec", t.etl, "stellar-etl", e.Command,
			"--start-ledger", l, "--end-ledger", l, "--batch-size", "1",
			"--datastore-path", bucket, "--output", "/etl/out/" + e.Name + "/",
			"--retry-limit", "0", "--num-workers", "1", "--buffer-size", "1"}
		if t.network == "testnet" {
			args = append(args, "--testnet")
		}
		out, err := dockerCombined(ctx, args...)
		if err != nil {
			return fmt.Errorf("%s of ledger %d: %w: %s", e.Command, ledger, err, lastLine(out))
		}
		file := filepath.Join(t.outDir, e.Name, l+"-"+l+"-"+e.Name+".txt")
		if _, err := os.Stat(file); err != nil {
			return fmt.Errorf("%s of ledger %d exited 0 without writing %s: %w", e.Command, ledger, file, err)
		}
	}
	return nil
}

func lastLine(s string) string {
	s = strings.TrimSpace(s)
	if i := strings.LastIndexByte(s, '\n'); i >= 0 {
		return s[i+1:]
	}
	return s
}

func (t *target) status(ctx context.Context) (wire.StatusResult, error) {
	st, err := inspectState(ctx, t.etl)
	if err != nil {
		return wire.StatusResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	if st.Running && st.Pid != 0 {
		t.samplePeak(st.Pid)
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	res := wire.StatusResult{LastIngestedLedger: t.done, LastError: t.lastErr}
	switch {
	case st.OOMKilled:
		res.Health = wire.HealthOOM
	case !st.Running:
		res.Health = wire.HealthCrashed
		if res.LastError == nil && st.Error != "" {
			e := st.Error
			res.LastError = &e
		}
	case t.done == nil:
		res.Health = wire.HealthStarting
	default:
		res.Health = wire.HealthRunning
	}
	if t.havePeak {
		v := t.peak
		res.PeakMemoryBytes = &v
	}
	return res, nil
}

// samplePeak reads memory.peak of the container's cgroup (v2).
func (t *target) samplePeak(pid int) {
	b, err := os.ReadFile(filepath.Join("/proc", strconv.Itoa(pid), "cgroup"))
	if err != nil {
		return
	}
	for line := range strings.SplitSeq(strings.TrimSpace(string(b)), "\n") {
		rest, ok := strings.CutPrefix(line, "0::")
		if !ok {
			continue
		}
		v, err := os.ReadFile(filepath.Join("/sys/fs/cgroup", rest, "memory.peak")) //nolint:gosec // the cgroup file of the container this adapter started
		if err != nil {
			return
		}
		n, err := strconv.ParseUint(strings.TrimSpace(string(v)), 10, 64)
		if err != nil {
			return
		}
		t.mu.Lock()
		if !t.havePeak || n > t.peak {
			t.peak, t.havePeak = n, true
		}
		t.mu.Unlock()
	}
}

// teardown stops the worker, keeps the etl container's log, and removes both
// containers.
func (t *target) teardown(ctx context.Context) (string, error) {
	select {
	case <-t.stop:
	default:
		close(t.stop)
	}
	logs, _ := dockerCombined(ctx, "logs", "--tail", "2000", t.etl)
	var errs []error
	for _, c := range []string{t.etl, t.gcs} {
		if _, err := docker(ctx, "rm", "-f", "-v", c); err != nil && !strings.Contains(err.Error(), "No such container") {
			errs = append(errs, err)
		}
	}
	if t.working {
		select {
		case <-t.finished:
		case <-time.After(time.Minute):
			errs = append(errs, errors.New("the export worker did not stop within a minute"))
		}
	}
	return logs, errors.Join(errs...)
}

// readRows reads every JSON line of the export files of name in ledgers
// from..to, in ledger order.
func (t *target) readRows(name string, from, to uint32) ([]map[string]any, error) {
	var out []map[string]any
	for l := from; l <= to; l++ {
		s := strconv.FormatUint(uint64(l), 10)
		path := filepath.Join(t.outDir, name, s+"-"+s+"-"+name+".txt")
		b, err := os.ReadFile(path) //nolint:gosec // an export file this adapter had written
		if errors.Is(err, os.ErrNotExist) {
			continue
		}
		if err != nil {
			return nil, err
		}
		dec := json.NewDecoder(bytes.NewReader(b))
		dec.UseNumber()
		for dec.More() {
			var row map[string]any
			if err := dec.Decode(&row); err != nil {
				return nil, fmt.Errorf("%s: %w", path, err)
			}
			out = append(out, row)
		}
	}
	return out, nil
}
