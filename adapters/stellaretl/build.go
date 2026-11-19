package stellaretl

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/ErenTopaal/shapekit/adapters/pinning"
)

// UpstreamURL is where stellar-etl's source is cloned from when no local clone
// is given.
const UpstreamURL = "https://github.com/stellar/stellar-etl"

var fullSHA = regexp.MustCompile(`^[0-9a-f]{40}$`)

// BaseImages maps every FROM reference in stellar-etl's docker/Dockerfile at
// the pinned commit to the same image by digest as pins.toml lists it
// (checked by TestBaseImagesMatchPins). A FROM missing here is built on the
// digest its tag resolves to at build time, which the run records
// (pinning.Contexts).
var BaseImages = map[string]string{
	"golang:1.27.3": "docker.io/library/golang@sha256:e519d3b0a5b285e5aa53ab9eef02bc6bbb9cb04608389b743a136a9ce75f0d1e",
	"ubuntu:24.04":  "docker.io/library/ubuntu@sha256:73b999393d07f2bf65a40373778a44dc9a6d54ad7718d29247d8ff9413c4d6a8",
}

// GCSImage is the GCS emulator stellar-etl reads the lake from, by tag and
// digest as pins.toml lists it.
const GCSImage = "fsouza/fake-gcs-server:1.56.1@sha256:797ce226d62f947c009dc40246b30cfb456b8473d8241407f9d6f2c04e4d69ef"

const baseLabel = "shapekit.stellar-etl.bases"

// resolvedLabel records the digest each base BaseImages does not list resolved
// to (pinning.Label), so later chains build on the same one.
const resolvedLabel = "shapekit.stellar-etl.resolved"

// source is a git clone of stellar-etl.
type source struct {
	dir string
	url string
}

func (s source) git(ctx context.Context, args ...string) ([]byte, error) {
	var stdout, stderr bytes.Buffer
	cmd := exec.CommandContext(ctx, "git", append([]string{"-C", s.dir}, args...)...) //nolint:gosec // git is the fixed program; the clone dir is the operator's --repo
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("git %s: %w: %s", args[0], err, strings.TrimSpace(stderr.String()))
	}
	return stdout.Bytes(), nil
}

// ensure makes sure the clone exists and holds commit.
func (s source) ensure(ctx context.Context, commit string) error {
	if _, err := os.Stat(filepath.Join(s.dir, ".git")); os.IsNotExist(err) {
		if err := os.MkdirAll(filepath.Dir(s.dir), 0o750); err != nil {
			return fmt.Errorf("creating %s: %w", filepath.Dir(s.dir), err)
		}
		cmd := exec.CommandContext(ctx, "git", "clone", "--quiet", s.url, s.dir) //nolint:gosec // git is the fixed program; url is a constant and dir the operator's --repo
		cmd.Stdout = os.Stderr
		cmd.Stderr = os.Stderr
		if err := cmd.Run(); err != nil {
			return fmt.Errorf("cloning %s: %w", s.url, err)
		}
	} else if err != nil {
		return fmt.Errorf("checking %s: %w", s.dir, err)
	}
	if _, err := s.git(ctx, "cat-file", "-e", commit+"^{commit}"); err == nil {
		return nil
	}
	if _, err := s.git(ctx, "fetch", "--quiet", "origin", commit); err != nil {
		return fmt.Errorf("commit %s is not in %s and could not be fetched: %w", commit, s.dir, err)
	}
	return nil
}

func (s source) show(ctx context.Context, commit, path string) ([]byte, error) {
	return s.git(ctx, "show", commit+":"+path)
}

// sdkProtocols is the newest ledger protocol the go-stellar-sdk version a
// pinned stellar-etl commit requires can decode. An unlisted version is held
// to the protocol of the newest listed release before it
// (pinning.SDKProtocol).
var sdkProtocols = map[string]uint32{
	// the protocol 30 sdk line (v0.8.x) at the commit stellar-etl v2.8.34 requires
	"v0.8.1-0.20261106081502-e31e3b146d00": 30,
	// the release stellar-etl v2.8.35 requires; its xdr is v30.0 (xdr/xdr_commit_generated.txt)
	"v0.9.0": 30,
}

// sdk returns the commit's go-stellar-sdk version, the newest ledger protocol
// it decodes, and a note when the version is not listed.
func (s source) sdk(ctx context.Context, commit string) (string, uint32, string, error) {
	mod, err := s.show(ctx, commit, "go.mod")
	if err != nil {
		return "", 0, "", err
	}
	sc := bufio.NewScanner(bytes.NewReader(mod))
	for sc.Scan() {
		f := strings.Fields(sc.Text())
		if len(f) >= 2 && f[0] == "github.com/stellar/go-stellar-sdk" {
			p, note, err := pinning.SDKProtocol(sdkProtocols, f[1], "sdkProtocols in adapters/stellaretl/build.go")
			if err != nil {
				return f[1], 0, "", fmt.Errorf("go.mod at %s: %w", commit, err)
			}
			return f[1], p, note, nil
		}
	}
	return "", 0, "", fmt.Errorf("go.mod at %s does not require go-stellar-sdk", commit)
}

// dockerfileBases returns the external images a Dockerfile builds FROM, in
// order and without repeats.
func dockerfileBases(dockerfile []byte) ([]string, error) {
	var bases []string
	seen := map[string]bool{}
	stages := map[string]bool{}
	sc := bufio.NewScanner(bytes.NewReader(dockerfile))
	for sc.Scan() {
		f := strings.Fields(sc.Text())
		if len(f) == 0 || !strings.EqualFold(f[0], "FROM") {
			continue
		}
		args := f[1:]
		for len(args) > 0 && strings.HasPrefix(args[0], "--") {
			args = args[1:]
		}
		if len(args) == 0 {
			return nil, fmt.Errorf("FROM with no image: %q", sc.Text())
		}
		ref := args[0]
		if strings.Contains(ref, "$") {
			return nil, fmt.Errorf("FROM %s uses a build argument; the base cannot be pinned", ref)
		}
		if !stages[strings.ToLower(ref)] && ref != "scratch" && !seen[ref] {
			seen[ref] = true
			bases = append(bases, ref)
		}
		if len(args) >= 3 && strings.EqualFold(args[1], "AS") {
			stages[strings.ToLower(args[2])] = true
		}
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}
	if len(bases) == 0 {
		return nil, fmt.Errorf("no FROM line")
	}
	return bases, nil
}

// pinnedContexts plans the build on each base's digest: its pin in BaseImages,
// or for a base BaseImages does not list, the digest resolve gives.
func pinnedContexts(ctx context.Context, bases []string, resolve pinning.Resolver) (pinning.Plan, error) {
	return pinning.Contexts(ctx, BaseImages, "BaseImages in adapters/stellaretl/build.go", bases, resolve)
}

func imageTag(commit string) string { return "shapekit-stellar-etl:" + commit }

// ensureImage builds the commit with its own docker/Dockerfile on fixed base
// digests (pinnedContexts) unless such an image exists. builder names a buildx
// builder (one of the docker-container driver can carry memory and cpu
// limits); the default builder when empty. The notes name each base
// BaseImages does not list.
func (s source) ensureImage(ctx context.Context, commit, builder string) (string, []string, error) {
	tag := imageTag(commit)
	dockerfile, err := s.show(ctx, commit, "docker/Dockerfile")
	if err != nil {
		return "", nil, err
	}
	bases, err := dockerfileBases(dockerfile)
	if err != nil {
		return "", nil, fmt.Errorf("docker/Dockerfile at %s: %w", commit, err)
	}
	// a base resolved for an earlier chain is built on the same digest, without asking the
	// registry again; a missing image or label leaves nothing recorded
	recorded, _ := docker(ctx, "image", "inspect", "--format", "{{index .Config.Labels \""+resolvedLabel+"\"}}", tag)
	plan, err := pinnedContexts(ctx, bases, pinning.Cached(pinning.Recorded(recorded), pinning.RegistryDigest))
	if err != nil {
		return "", nil, fmt.Errorf("docker/Dockerfile at %s: %w", commit, err)
	}
	// an image built before the resolutions were recorded is built again (from the
	// build cache), so it records them and later chains skip the registry
	if got, err := docker(ctx, "image", "inspect", "--format", "{{index .Config.Labels \""+baseLabel+"\"}}", tag); err == nil && got == plan.Set &&
		(len(plan.Resolved) == 0 || recorded == pinning.Label(plan.Resolved)) {
		return tag, plan.Notes, nil
	}
	archive := exec.CommandContext(ctx, "git", "-C", s.dir, "archive", "--format=tar", commit) //nolint:gosec // commit was checked to be 40 hex digits
	out, err := archive.StdoutPipe()
	if err != nil {
		return "", nil, fmt.Errorf("git archive: %w", err)
	}
	var archErr bytes.Buffer
	archive.Stderr = &archErr
	if err := archive.Start(); err != nil {
		return "", nil, fmt.Errorf("git archive: %w", err)
	}
	args := []string{"buildx", "build", "--load", "-f", "docker/Dockerfile"}
	if builder != "" {
		args = append(args, "--builder", builder)
	}
	args = append(args, plan.Flags...)
	args = append(args,
		"--label", "shapekit.stellar-etl.commit="+commit,
		"--label", baseLabel+"="+plan.Set,
		"--label", resolvedLabel+"="+pinning.Label(plan.Resolved),
		"-t", tag, "-")
	buildErr := dockerStream(ctx, out, args...)
	_, drainErr := io.Copy(io.Discard, out)
	waitErr := archive.Wait()
	if buildErr != nil {
		return "", nil, fmt.Errorf("building %s: %w", commit, buildErr)
	}
	if waitErr != nil {
		return "", nil, fmt.Errorf("git archive %s: %w: %s", commit, waitErr, strings.TrimSpace(archErr.String()))
	}
	if drainErr != nil {
		return "", nil, fmt.Errorf("git archive %s: %w", commit, drainErr)
	}
	return tag, plan.Notes, nil
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

func dockerStream(ctx context.Context, in io.Reader, args ...string) error {
	cmd := exec.CommandContext(ctx, "docker", args...) //nolint:gosec // docker is the fixed program; the args are built by this adapter
	cmd.Stdin = in
	cmd.Stdout = os.Stderr
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("docker %s: %w", args[0], err)
	}
	return nil
}

// dockerCombined returns stdout and stderr in one buffer, for exec and logs.
func dockerCombined(ctx context.Context, args ...string) (string, error) {
	var out bytes.Buffer
	cmd := exec.CommandContext(ctx, "docker", args...) //nolint:gosec // docker is the fixed program; the args are built by this adapter
	cmd.Stdout = &out
	cmd.Stderr = &out
	err := cmd.Run()
	return out.String(), err
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
