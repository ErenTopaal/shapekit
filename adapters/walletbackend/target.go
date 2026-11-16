package walletbackend

import (
	"bufio"
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// Images the target needs besides wallet-backend itself, by tag and digest as
// pins.toml lists them (checked by TestImagesMatchPins).
const (
	TimescaleImage = "timescale/timescaledb:2.28.2-pg17@sha256:909e2c7577c074517c8936b6d8799294b33f25fb5bad978a2199250665f1ce1a"
	SeaweedImage   = "chrislusf/seaweedfs:4.46@sha256:08d516132314207d10c8e37cbffc1f32b147d870169688734cc61c6231625b62"
)

// NetworkLabel marks every container the adapter starts with the harness's
// network name, so the harness can remove leftovers if it kills the adapter.
const NetworkLabel = "shapekit.docker_network"

const (
	dbName       = "wallet-backend"
	s3Bucket     = "ledgers"
	s3Region     = "us-east-1"
	s3Port       = "8333"
	ingestPort   = "8002"
	readyTimeout = 3 * time.Minute
)

// target is one running wallet-backend: database, optional object store and
// the ingest process.
type target struct {
	commit  string
	image   string
	params  wire.StartParams
	prefix  string
	cpus    string
	db      string
	ingest  string
	s3      string
	started time.Time
	store   *store
	s3url   string
	lake    *lakeCopier

	mu         sync.Mutex
	peak       uint64
	havePeak   bool
	peakErr    string
	stopSample chan struct{}
	sampleDone chan struct{}
}

func randomSuffix() (string, error) {
	b := make([]byte, 4)
	if _, err := rand.Read(b); err != nil {
		return "", fmt.Errorf("random container suffix: %w", err)
	}
	return hex.EncodeToString(b), nil
}

// runFlags are the labels every container gets, and the cpu limit when one
// is configured.
func (t *target) runFlags() []string {
	f := []string{"--label", NetworkLabel + "=" + t.params.DockerNetwork, "--label", "shapekit.adapter=wallet-backend"}
	if t.cpus != "" {
		f = append(f, "--cpus", t.cpus)
	}
	return f
}

// startTarget brings up the database, runs the migrations, starts the object
// store in sep54-s3 mode, and starts ingest. It returns once ingest runs.
func startTarget(ctx context.Context, src source, commit string, p wire.StartParams, dbMemory uint64, cpus string) (*target, error) {
	if p.DockerNetwork == "" {
		return nil, wire.Errorf(wire.ClassUnsupported, "wallet-backend runs in containers; the harness must pass docker_network")
	}
	switch p.Delivery.Mode {
	case wire.DeliveryRPC, wire.DeliverySep54S3:
	default:
		return nil, wire.Errorf(wire.ClassUnsupported, "delivery mode %q: wallet-backend reads ledgers from rpc or an s3 datastore only", p.Delivery.Mode)
	}
	if p.Limits.MemoryBytes == 0 {
		return nil, wire.Errorf(wire.ClassBadRequest, "limits.memory_bytes is 0")
	}
	if err := src.ensure(ctx, commit); err != nil {
		return nil, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	image, err := src.ensureImage(ctx, commit)
	if err != nil {
		return nil, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	needsEnv, err := src.needsStellarEnvironment(ctx, commit)
	if err != nil {
		return nil, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	suffix, err := randomSuffix()
	if err != nil {
		return nil, err
	}
	t := &target{commit: commit, image: image, params: p, prefix: "skwb-" + suffix, cpus: cpus}
	t.db, t.ingest, t.s3 = t.prefix+"-db", t.prefix+"-ingest", t.prefix+"-s3"
	if err := t.startDB(ctx, dbMemory); err != nil {
		return t, err
	}
	if p.Delivery.Mode == wire.DeliverySep54S3 {
		if err := t.startS3(ctx); err != nil {
			return t, err
		}
		if err := t.lake.copyUpTo(ctx, p.Ledgers.Bootstrap); err != nil {
			return t, wire.Errorf(wire.ClassTarget, "copying the lake up to ledger %d: %v", p.Ledgers.Bootstrap, err)
		}
	}
	env := t.env(needsEnv)
	if err := t.migrate(ctx, env); err != nil {
		return t, err
	}
	args := []string{"run", "-d", "--name", t.ingest, "--network", p.DockerNetwork,
		"--memory", strconv.FormatUint(p.Limits.MemoryBytes, 10),
		// equal to --memory: no swap, so the limit is a real ceiling
		"--memory-swap", strconv.FormatUint(p.Limits.MemoryBytes, 10),
		// wallet-backend gives up on a ledger after its retry ladder and exits;
		// deployments restart it, and it then retries the same ledger forever.
		// restarting here keeps that behavior, so a controlled give-up reads as
		// a halt and only a runtime crash or the memory limit reads as a death
		"--restart", "on-failure"}
	args = append(args, t.runFlags()...)
	for _, e := range env {
		args = append(args, "-e", e)
	}
	args = append(args, image, "ingest")
	t.started = time.Now()
	if _, err := docker(ctx, args...); err != nil {
		return t, wire.Errorf(wire.ClassTarget, "starting ingest: %v", err)
	}
	t.startSampler(ctx)
	return t, nil
}

func (t *target) env(needsStellarEnvironment bool) []string {
	p := t.params
	env := []string{
		"DATABASE_URL=postgres://postgres@" + t.db + ":5432/" + dbName + "?sslmode=disable",
		"NETWORK_PASSPHRASE=" + p.NetworkPassphrase,
		"RPC_URL=" + p.Delivery.RPCURL,
		"ARCHIVE_URL=" + p.Delivery.ArchiveURL,
		"CHECKPOINT_FREQUENCY=" + strconv.FormatUint(uint64(p.Ledgers.CheckpointFrequency), 10),
		"INGEST_SERVER_PORT=" + ingestPort,
		// older commits default to TRACE, which floods the log the adapter scans
		"LOG_LEVEL=INFO",
	}
	if needsStellarEnvironment {
		// required and only used to tag sentry events, which are off without a dsn
		env = append(env, "STELLAR_ENVIRONMENT=shapekit")
	}
	switch p.Delivery.Mode {
	case wire.DeliveryRPC:
		// set explicitly: commits before a9bac8d2 default to the datastore backend
		env = append(env, "LEDGER_BACKEND_TYPE=rpc")
	case wire.DeliverySep54S3:
		env = append(env,
			"LEDGER_BACKEND_TYPE=datastore",
			// seaweedfs runs without auth; the aws sdk still wants keys to sign with
			"AWS_ACCESS_KEY_ID=shapekit", "AWS_SECRET_ACCESS_KEY=shapekit", "AWS_REGION="+s3Region,
			"DATASTORE_BUCKET_PATH="+s3Bucket,
			"DATASTORE_REGION="+s3Region,
			"DATASTORE_ENDPOINT_URL=http://"+t.s3+":"+s3Port,
			"DATASTORE_LEDGERS_PER_FILE="+strconv.FormatUint(uint64(p.Delivery.Lake.LedgersPerBatch), 10),
			"DATASTORE_FILES_PER_PARTITION="+strconv.FormatUint(uint64(p.Delivery.Lake.BatchesPerPartition), 10),
		)
	}
	return env
}

func (t *target) startDB(ctx context.Context, memory uint64) error {
	p := t.params
	args := []string{"run", "-d", "--name", t.db, "--network", p.DockerNetwork, "--network-alias", t.db,
		"--memory", strconv.FormatUint(memory, 10), "--memory-swap", strconv.FormatUint(memory, 10),
		"-e", "POSTGRES_HOST_AUTH_METHOD=trust", "-e", "POSTGRES_DB=" + dbName}
	args = append(args, t.runFlags()...)
	// the settings wallet-backend's docker-compose.yaml passes, which its migrations need
	args = append(args, TimescaleImage, "postgres", "-c", "timescaledb.enable_chunk_skipping=on", "-c", "timescaledb.enable_sparse_index_bloom=on")
	if _, err := docker(ctx, args...); err != nil {
		return wire.Errorf(wire.ClassTarget, "starting timescaledb: %v", err)
	}
	ip, err := containerIP(ctx, t.db, p.DockerNetwork)
	if err != nil {
		return wire.Errorf(wire.ClassTarget, "%v", err)
	}
	deadline := time.Now().Add(readyTimeout)
	for {
		// tcp on 127.0.0.1 inside the container: the image's init phase runs a
		// socket-only server, so this only answers once the real server is up
		_, err := docker(ctx, "exec", t.db, "pg_isready", "-h", "127.0.0.1", "-U", "postgres", "-d", dbName)
		if err == nil {
			break
		}
		if time.Now().After(deadline) {
			return wire.Errorf(wire.ClassTarget, "timescaledb did not accept connections within %s: %v", readyTimeout, err)
		}
		select {
		case <-ctx.Done():
			return wire.Errorf(wire.ClassTarget, "waiting for timescaledb: %v", ctx.Err())
		case <-time.After(500 * time.Millisecond):
		}
	}
	st, err := openStore(ctx, "postgres://postgres@"+ip+":5432/"+dbName+"?sslmode=disable", p.NetworkPassphrase)
	if err != nil {
		return wire.Errorf(wire.ClassTarget, "%v", err)
	}
	t.store = st
	// the init script of wallet-backend's compose file; the extension must exist before migrate up
	if _, err := st.pool.Exec(ctx, "CREATE EXTENSION IF NOT EXISTS timescaledb"); err != nil {
		return wire.Errorf(wire.ClassTarget, "creating the timescaledb extension: %v", err)
	}
	return nil
}

func (t *target) migrate(ctx context.Context, env []string) error {
	args := []string{"run", "--rm", "--name", t.prefix + "-migrate", "--network", t.params.DockerNetwork}
	args = append(args, t.runFlags()...)
	for _, e := range env {
		args = append(args, "-e", e)
	}
	args = append(args, t.image, "migrate", "up")
	out, err := dockerCombined(ctx, args...)
	fmt.Fprintf(os.Stderr, "migrate up:\n%s\n", out)
	if err != nil {
		return wire.Errorf(wire.ClassTarget, "migrate up: %v", err)
	}
	return nil
}

// startSampler records the ingest container's cgroup memory.peak once a
// second while it runs; the cgroup goes away with the container.
func (t *target) startSampler(ctx context.Context) {
	t.stopSample = make(chan struct{})
	t.sampleDone = make(chan struct{})
	go func() {
		defer close(t.sampleDone)
		path := ""
		for {
			if path == "" {
				p, err := t.cgroupPeakPath(ctx)
				if err != nil {
					t.setPeakErr(err.Error())
				} else {
					path = p
				}
			}
			if path != "" {
				t.samplePeak(path)
			}
			select {
			case <-t.stopSample:
				return
			case <-ctx.Done():
				return
			case <-time.After(time.Second):
			}
		}
	}()
}

func (t *target) cgroupPeakPath(ctx context.Context) (string, error) {
	st, err := inspectState(ctx, t.ingest)
	if err != nil {
		return "", err
	}
	return cgroupPeakPathOf(st)
}

func cgroupPeakPathOf(st containerState) (string, error) {
	if !st.Running || st.Pid == 0 {
		return "", errors.New("ingest is not running, no cgroup to read")
	}
	b, err := os.ReadFile(filepath.Join("/proc", strconv.Itoa(st.Pid), "cgroup"))
	if err != nil {
		return "", fmt.Errorf("reading the ingest cgroup: %w", err)
	}
	for line := range strings.SplitSeq(strings.TrimSpace(string(b)), "\n") {
		// cgroup v2 has the single line 0::<path>
		if rest, ok := strings.CutPrefix(line, "0::"); ok {
			return filepath.Join("/sys/fs/cgroup", rest, "memory.peak"), nil
		}
	}
	return "", errors.New("ingest is not in a cgroup v2 hierarchy, memory.peak is unavailable")
}

func (t *target) samplePeak(path string) {
	b, err := os.ReadFile(path) //nolint:gosec // the cgroup file of the container this adapter started
	if err != nil {
		if !os.IsNotExist(err) {
			t.setPeakErr(err.Error())
		}
		return
	}
	v, err := strconv.ParseUint(strings.TrimSpace(string(b)), 10, 64)
	if err != nil {
		t.setPeakErr(fmt.Sprintf("memory.peak is %q: %v", strings.TrimSpace(string(b)), err))
		return
	}
	t.mu.Lock()
	if !t.havePeak || v > t.peak {
		t.peak, t.havePeak = v, true
	}
	t.mu.Unlock()
}

func (t *target) setPeakErr(msg string) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.peakErr != msg {
		t.peakErr = msg
		fmt.Fprintf(os.Stderr, "peak memory: %s\n", msg)
	}
}

func (t *target) status(ctx context.Context) (wire.StatusResult, error) {
	st, err := inspectState(ctx, t.ingest)
	if err != nil {
		return wire.StatusResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	if st.Running {
		if p, err := cgroupPeakPathOf(st); err == nil {
			t.samplePeak(p)
		}
	}
	cur, err := t.store.cursor(ctx)
	if err != nil {
		return wire.StatusResult{}, wire.Errorf(wire.ClassTarget, "%v", err)
	}
	res := wire.StatusResult{LastIngestedLedger: cur}
	logs, err := dockerCombined(ctx, "logs", "--tail", "5000", t.ingest)
	if err != nil {
		return wire.StatusResult{}, wire.Errorf(wire.ClassTarget, "reading ingest logs: %v", err)
	}
	if e := lastError(logs); e != "" {
		res.LastError = &e
	}
	alive := st.Running || st.Restarting
	switch {
	case st.OOMKilled:
		res.Health = wire.HealthOOM
	case runtimeCrash.MatchString(logs) || !alive:
		// with on-failure restarts the container only stays down after exit 0,
		// and live ingest never ends on its own
		res.Health = wire.HealthCrashed
	case cur == nil:
		res.Health = wire.HealthStarting
	default:
		res.Health = wire.HealthRunning
	}
	if !st.Running && st.Error != "" && res.LastError == nil {
		e := st.Error
		res.LastError = &e
	}
	t.mu.Lock()
	if t.havePeak {
		v := t.peak
		res.PeakMemoryBytes = &v
	}
	t.mu.Unlock()
	return res, nil
}

var (
	logLevel     = regexp.MustCompile(`\blevel=(\w+)`)
	logMsgError  = regexp.MustCompile(`\bmsg="Error `)
	exitErrorRow = regexp.MustCompile(`^(Error: |panic: )`)
	// a go runtime failure, as opposed to the "panic: executing root command"
	// wallet-backend prints when it gives up on a ledger
	runtimeCrash = regexp.MustCompile(`(?m)^(panic: runtime error|fatal error: |\[signal SIG)`)
)

// lastError picks the newest line in which wallet-backend reports an
// ingestion failure: logrus error, fatal and panic lines, its retry warnings
// ("Error ingesting data ...", "Error fetching ledger ..."), and the cobra
// "Error:" or "panic:" line it prints on exit. Other warnings (for example a
// failed blend price snapshot) do not stop ingestion and are not errors here.
func lastError(logs string) string {
	last := ""
	sc := bufio.NewScanner(strings.NewReader(logs))
	sc.Buffer(make([]byte, 64*1024), 8*1024*1024)
	for sc.Scan() {
		line := sc.Text()
		if exitErrorRow.MatchString(line) {
			last = line
			continue
		}
		m := logLevel.FindStringSubmatch(line)
		if m == nil {
			continue
		}
		switch m[1] {
		case "error", "fatal", "panic":
			last = line
		case "warning":
			if logMsgError.MatchString(line) {
				last = line
			}
		}
	}
	return last
}

// teardown removes every container and volume the target created and returns
// the tail of the ingest log.
func (t *target) teardown(ctx context.Context) (string, error) {
	if t.stopSample != nil {
		close(t.stopSample)
		<-t.sampleDone
		t.stopSample = nil
	}
	var logs string
	var errs []error
	if t.ingest != "" {
		if out, err := dockerLogsTail(ctx, t.ingest); err == nil {
			logs = out
		} else if !strings.Contains(err.Error(), "No such container") {
			errs = append(errs, err)
		}
	}
	if t.store != nil {
		t.store.close()
		t.store = nil
	}
	for _, name := range []string{t.ingest, t.prefix + "-migrate", t.s3, t.db} {
		if name == "" {
			continue
		}
		if _, err := docker(ctx, "rm", "-f", "-v", name); err != nil && !strings.Contains(err.Error(), "No such container") {
			errs = append(errs, err)
		}
	}
	return logs, errors.Join(errs...)
}

const maxStopLogs = 1 << 20

func dockerLogsTail(ctx context.Context, name string) (string, error) {
	out, err := dockerCombined(ctx, "logs", "--tail", "20000", name)
	if err != nil {
		return "", err
	}
	if len(out) > maxStopLogs {
		out = out[len(out)-maxStopLogs:]
	}
	return out, nil
}
