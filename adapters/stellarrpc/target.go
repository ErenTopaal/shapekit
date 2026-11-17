package stellarrpc

import (
	"bufio"
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/stellar/go-stellar-sdk/keypair"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// NetworkLabel marks every container with the harness's network name, so the
// harness can remove leftovers.
const NetworkLabel = "shapekit.docker_network"

const (
	rpcPort = "8000"
	// how long start waits for the first process to answer getVersionInfo
	readyTimeout = 5 * time.Minute
)

// target is one stellar-rpc deployment: a datastore directory the adapter
// fills with released batches, and the stellar-rpc process that serves them.
// Every release starts a new process on an empty database (see relaunch).
type target struct {
	params  wire.StartParams
	cpus    string
	prefix  string
	lake    string // the datastore directory stellar-rpc reads
	data    string // its database and captive core directory
	etc     string // its configuration files
	copied  map[string]bool
	runs    int
	current string // the container of the current process

	mu       sync.Mutex
	url      string
	answered bool
	last     *uint32
	lastErr  *string
	peak     uint64
	havePeak bool
	logs     strings.Builder // the logs of processes already replaced
}

func randomSuffix() (string, error) {
	b := make([]byte, 4)
	if _, err := rand.Read(b); err != nil {
		return "", fmt.Errorf("random container suffix: %w", err)
	}
	return hex.EncodeToString(b), nil
}

func startTarget(ctx context.Context, cpus string, p wire.StartParams) (*target, error) {
	if p.DockerNetwork == "" {
		return nil, wire.Errorf(wire.ClassUnsupported, "stellar-rpc runs in a container; the harness must pass docker_network")
	}
	if p.Delivery.Mode != wire.DeliverySep54Filesystem {
		return nil, wire.Errorf(wire.ClassUnsupported, "delivery mode %q: this adapter gives stellar-rpc the lake as a filesystem datastore", p.Delivery.Mode)
	}
	if p.Limits.MemoryBytes == 0 {
		return nil, wire.Errorf(wire.ClassBadRequest, "limits.memory_bytes is 0")
	}
	suffix, err := randomSuffix()
	if err != nil {
		return nil, err
	}
	t := &target{
		params: p, cpus: cpus, prefix: "skrpc-" + suffix, copied: map[string]bool{},
		lake: filepath.Join(p.Workdir, "lake"), data: filepath.Join(p.Workdir, "data"), etc: filepath.Join(p.Workdir, "etc"),
	}
	for _, d := range []string{t.lake, t.etc} {
		if err := os.MkdirAll(d, 0o750); err != nil {
			return t, wire.Errorf(wire.ClassInternal, "creating %s: %v", d, err)
		}
	}
	if err := t.writeConfig(); err != nil {
		return t, err
	}
	if err := t.copyUpTo(p.Ledgers.Bootstrap); err != nil {
		return t, err
	}
	if err := t.relaunch(ctx); err != nil {
		return t, err
	}
	return t, nil
}

// writeConfig writes stellar-rpc's TOML file and the captive core file it
// requires.
//
// stellar-rpc ingests from the datastore only through backfill
// (cmd/stellar-rpc/internal/rpcv1/ingest/backfill.go): BACKFILL needs
// SERVE_LEDGERS_FROM_DATASTORE, and the frontfill stops at the newest
// checkpoint at or below the datastore's newest ledger. CHECKPOINT_FREQUENCY
// 1 makes every ledger a checkpoint, so the frontfill reaches the newest
// released ledger. Live ingestion after the backfill runs captive core, which
// follows the network over SCP; on the harness's network there is none, so
// captive core never ingests a ledger (stellar-rpc logs "could not run
// ingestion. Retrying"). Its configuration only has to pass stellar-rpc's
// checks: a quorum of one key generated here, which signs nothing.
func (t *target) writeConfig() error {
	kp, err := keypair.Random()
	if err != nil {
		return wire.Errorf(wire.ClassInternal, "generating the captive core quorum key: %v", err)
	}
	core := fmt.Sprintf(`NETWORK_PASSPHRASE=%s
UNSAFE_QUORUM=true
FAILURE_SAFETY=0

[[HOME_DOMAINS]]
HOME_DOMAIN="shapekit.invalid"
QUALITY="MEDIUM"

[[VALIDATORS]]
NAME="quorum"
HOME_DOMAIN="shapekit.invalid"
PUBLIC_KEY=%q
ADDRESS="127.0.0.1:11625"
HISTORY="curl -sf %s/{0} -o {1}"
`, strconv.Quote(t.params.NetworkPassphrase), kp.Address(), t.params.Delivery.ArchiveURL)
	rpc := fmt.Sprintf(`NETWORK_PASSPHRASE = %s
HISTORY_ARCHIVE_URLS = [%q]
CAPTIVE_CORE_CONFIG_PATH = "/etc/stellar-rpc/captive-core.cfg"
CAPTIVE_CORE_STORAGE_PATH = "/data/captive-core"
DB_PATH = "/data/stellar-rpc.sqlite"
ENDPOINT = "0.0.0.0:%s"
LOG_LEVEL = "info"
BACKFILL = true
SERVE_LEDGERS_FROM_DATASTORE = true
CHECKPOINT_FREQUENCY = 1

[datastore_config]
type = "Filesystem"

[datastore_config.params]
destination_path = "/lake"

[datastore_config.schema]
ledgers_per_file = %d
files_per_partition = %d
`, strconv.Quote(t.params.NetworkPassphrase), t.params.Delivery.ArchiveURL, rpcPort,
		t.params.Delivery.Lake.LedgersPerBatch, t.params.Delivery.Lake.BatchesPerPartition)
	for name, body := range map[string]string{"captive-core.cfg": core, "stellar-rpc.toml": rpc} {
		if err := os.WriteFile(filepath.Join(t.etc, name), []byte(body), 0o644); err != nil { //nolint:gosec // read by the container's user
			return wire.Errorf(wire.ClassInternal, "writing %s: %v", name, err)
		}
	}
	return nil
}

// copyUpTo copies the lake's .config.json and every batch whose last ledger
// is at or below head into the datastore directory, skipping what is there.
func (t *target) copyUpTo(head uint32) error {
	src := t.params.Delivery.Lake.Path
	return filepath.WalkDir(src, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		dst := filepath.Join(t.lake, rel)
		if d.IsDir() {
			return os.MkdirAll(dst, 0o750)
		}
		if t.copied[rel] {
			return nil
		}
		if rel != ".config.json" {
			last, ok := batchLast(d.Name())
			if !ok {
				return wire.Errorf(wire.ClassInternal, "lake file %s has no ledger in its name", path)
			}
			if last > head {
				return nil
			}
		}
		b, err := os.ReadFile(path) //nolint:gosec // a file of the lake the harness named
		if err != nil {
			return wire.Errorf(wire.ClassInternal, "reading %s: %v", path, err)
		}
		if err := os.WriteFile(dst, b, 0o644); err != nil { //nolint:gosec // read by the container's user
			return wire.Errorf(wire.ClassInternal, "writing %s: %v", dst, err)
		}
		t.copied[rel] = true
		return nil
	})
}

// batchLast reads the last ledger of a SEP-54 batch file name:
// FFFFFFFD--2.xdr.zst holds ledger 2, FFFFFFFD--2-5.xdr.zst ledgers 2 to 5.
func batchLast(name string) (uint32, bool) {
	stem, ok := strings.CutSuffix(name, ".xdr.zst")
	if !ok {
		return 0, false
	}
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

// relaunch replaces the stellar-rpc process: it keeps the old container's
// logs, removes it, empties the database directory and starts a new one,
// whose backfill ingests every batch in the datastore directory.
//
// A new process on the old database would not ingest a single new ledger:
// with the database at ledger H and the datastore at H+1, the frontfill range
// is [H+1, H+1], and runFrontfill fills only when its first ledger is below
// its last (backfill.go, `if bounds.frontfill.First < bounds.frontfill.Last`).
// The harness releases one ledger at a time, so each release starts over.
func (t *target) relaunch(ctx context.Context) error {
	if t.current != "" {
		t.keepLogs(ctx)
		if err := removeContainer(ctx, t.current); err != nil {
			return wire.Errorf(wire.ClassTarget, "%v", err)
		}
	}
	if err := os.RemoveAll(t.data); err != nil {
		return wire.Errorf(wire.ClassInternal, "emptying %s: %v", t.data, err)
	}
	if err := os.MkdirAll(t.data, 0o750); err != nil {
		return wire.Errorf(wire.ClassInternal, "creating %s: %v", t.data, err)
	}
	t.runs++
	name := fmt.Sprintf("%s-%d", t.prefix, t.runs)
	mem := strconv.FormatUint(t.params.Limits.MemoryBytes, 10)
	args := []string{"run", "-d", "--name", name,
		"--network", t.params.DockerNetwork,
		"--label", NetworkLabel + "=" + t.params.DockerNetwork, "--label", "shapekit.adapter=stellar-rpc",
		"--memory", mem, "--memory-swap", mem,
		"--user", strconv.Itoa(os.Getuid()) + ":" + strconv.Itoa(os.Getgid()),
		"-e", "HOME=/data",
		"-v", t.data + ":/data",
		"-v", t.lake + ":/lake:ro",
		"-v", t.etc + ":/etc/stellar-rpc:ro",
	}
	if t.cpus != "" {
		args = append(args, "--cpus", t.cpus)
	}
	args = append(args, Image, "--config-path", "/etc/stellar-rpc/stellar-rpc.toml")
	if _, err := docker(ctx, args...); err != nil {
		return wire.Errorf(wire.ClassTarget, "starting stellar-rpc: %v", err)
	}
	t.current = name
	ip, err := containerIP(ctx, name, t.params.DockerNetwork)
	if err != nil {
		return wire.Errorf(wire.ClassTarget, "%v", err)
	}
	t.mu.Lock()
	t.url = "http://" + ip + ":" + rpcPort
	t.mu.Unlock()
	return nil
}

// waitReady waits until the current process answers getVersionInfo, and
// checks that it is the pinned build. A process that exits first is left to
// status, which reports it.
func (t *target) waitReady(ctx context.Context) error {
	deadline := time.Now().Add(readyTimeout)
	for {
		st, err := inspectState(ctx, t.current)
		if err != nil {
			return wire.Errorf(wire.ClassTarget, "%v", err)
		}
		if !st.Running {
			return nil
		}
		var v versionInfo
		cctx, cancel := context.WithTimeout(ctx, 5*time.Second)
		err = newClient(t.url).call(cctx, "getVersionInfo", nil, &v)
		cancel()
		if err == nil {
			if v.CommitHash != Commit || v.Version != Version30 {
				return wire.Errorf(wire.ClassTarget, "the image runs stellar-rpc %s at %s, not %s at %s", v.Version, v.CommitHash, Version30, Commit)
			}
			return nil
		}
		if time.Now().After(deadline) {
			return wire.Errorf(wire.ClassTarget, "stellar-rpc did not answer getVersionInfo within %s: %v", readyTimeout, err)
		}
		time.Sleep(200 * time.Millisecond)
	}
}

func (t *target) release(ctx context.Context, head uint32) error {
	if err := t.copyUpTo(head); err != nil {
		return err
	}
	return t.relaunch(ctx)
}

// ignoredError says whether an error line of stellar-rpc's log comes from its
// live ingestion through captive core, which cannot follow a network here
// (see writeConfig). Those lines are not errors of the ingestion under test.
func ignoredError(line string) bool {
	return strings.Contains(line, `msg="could not run ingestion. Retrying"`) ||
		strings.Contains(line, "subservice=stellar-core")
}

// newestError returns the last line at level error or fatal, or the
// "panic:" line of a crash, in a log.
func newestError(log string) *string {
	var last *string
	sc := bufio.NewScanner(strings.NewReader(log))
	sc.Buffer(make([]byte, 64*1024), 4*1024*1024)
	for sc.Scan() {
		l := sc.Text()
		if (strings.Contains(l, "level=error") || strings.Contains(l, "level=fatal") || strings.HasPrefix(l, "panic:")) && !ignoredError(l) {
			s := l
			last = &s
		}
	}
	return last
}

func (t *target) status(ctx context.Context) (wire.StatusResult, error) {
	st, err := inspectState(ctx, t.current)
	if err != nil {
		return wire.StatusResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	if st.Running && st.Pid != 0 {
		t.samplePeak(st.Pid)
	}
	if log, err := dockerCombined(ctx, "logs", t.current); err == nil {
		if e := newestError(log); e != nil {
			t.mu.Lock()
			t.lastErr = e
			t.mu.Unlock()
		}
	}
	if st.Running {
		var l latestLedger
		cctx, cancel := context.WithTimeout(ctx, 5*time.Second)
		err := newClient(t.url).call(cctx, "getLatestLedger", nil, &l)
		cancel()
		if err == nil {
			t.mu.Lock()
			t.answered = true
			seq := l.Sequence
			t.last = &seq
			t.mu.Unlock()
		}
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	res := wire.StatusResult{LastIngestedLedger: t.last, LastError: t.lastErr}
	switch {
	case st.OOMKilled:
		res.Health = wire.HealthOOM
	case !st.Running:
		res.Health = wire.HealthCrashed
		if res.LastError == nil && st.Error != "" {
			e := st.Error
			res.LastError = &e
		}
	case !t.answered:
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

// keepLogs appends the tail of the current container's log to the logs stop
// returns.
func (t *target) keepLogs(ctx context.Context) {
	out, _ := dockerCombined(ctx, "logs", "--tail", "200", t.current)
	t.mu.Lock()
	defer t.mu.Unlock()
	fmt.Fprintf(&t.logs, "== %s\n%s", t.current, out)
	// stop answers at most 1 MiB; keep the newest part
	if t.logs.Len() > 900*1024 {
		s := t.logs.String()
		t.logs.Reset()
		t.logs.WriteString(s[len(s)-800*1024:])
	}
}

func (t *target) teardown(ctx context.Context) (string, error) {
	if t.current == "" {
		return "", nil
	}
	t.keepLogs(ctx)
	err := removeContainer(ctx, t.current)
	t.current = ""
	return t.logs.String(), err
}

func removeContainer(ctx context.Context, name string) error {
	if _, err := docker(ctx, "rm", "-f", "-v", name); err != nil && !strings.Contains(err.Error(), "No such container") {
		return err
	}
	return nil
}

func docker(ctx context.Context, args ...string) (string, error) {
	var stdout, stderr bytes.Buffer
	cmd := exec.CommandContext(ctx, "docker", args...) //nolint:gosec // docker is the fixed program; the args are built by this adapter
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return stdout.String(), fmt.Errorf("docker %s: %w: %s", args[0], err, strings.TrimSpace(stderr.String()))
	}
	return strings.TrimSpace(stdout.String()), nil
}

// dockerCombined returns stdout and stderr in one buffer, for logs.
func dockerCombined(ctx context.Context, args ...string) (string, error) {
	var out bytes.Buffer
	cmd := exec.CommandContext(ctx, "docker", args...) //nolint:gosec // docker is the fixed program; the args are built by this adapter
	cmd.Stdout = &out
	cmd.Stderr = &out
	err := cmd.Run()
	return out.String(), err
}

func dockerStream(ctx context.Context, args ...string) error {
	cmd := exec.CommandContext(ctx, "docker", args...) //nolint:gosec // docker is the fixed program; the args are built by this adapter
	cmd.Stdout = os.Stderr
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("docker %s: %w", args[0], err)
	}
	return nil
}

type containerState struct {
	Running   bool
	ExitCode  int
	OOMKilled bool
	Pid       int
	Error     string
}

func inspectState(ctx context.Context, name string) (containerState, error) {
	out, err := docker(ctx, "inspect", "--format", "{{json .State}}", name)
	if err != nil {
		return containerState{}, err
	}
	var st containerState
	if err := json.Unmarshal([]byte(out), &st); err != nil {
		return containerState{}, fmt.Errorf("decoding state of %s: %w", name, err)
	}
	return st, nil
}

func containerIP(ctx context.Context, name, network string) (string, error) {
	out, err := docker(ctx, "inspect", "--format", "{{json .NetworkSettings.Networks}}", name)
	if err != nil {
		return "", err
	}
	var nets map[string]struct {
		IPAddress string
	}
	if err := json.Unmarshal([]byte(out), &nets); err != nil {
		return "", fmt.Errorf("decoding networks of %s: %w", name, err)
	}
	n, ok := nets[network]
	if !ok || n.IPAddress == "" {
		return "", fmt.Errorf("%s has no address on %s", name, network)
	}
	return n.IPAddress, nil
}

// ensureImage pulls the pinned image unless it is present.
func ensureImage(ctx context.Context) error {
	if _, err := docker(ctx, "image", "inspect", Image); err == nil {
		return nil
	}
	if err := dockerStream(ctx, "pull", "--quiet", Image); err != nil {
		return fmt.Errorf("pulling %s: %w", Image, err)
	}
	return nil
}

var errNoProcess = errors.New("no stellar-rpc process")

func (t *target) rpc() (*client, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.url == "" {
		return nil, errNoProcess
	}
	return newClient(t.url), nil
}
