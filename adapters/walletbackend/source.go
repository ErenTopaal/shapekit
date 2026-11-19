package walletbackend

import (
	"bufio"
	"bytes"
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/ErenTopaal/shapekit/adapters/pinning"
)

// UpstreamURL is where wallet-backend's source is cloned from when no local
// clone is given.
const UpstreamURL = "https://github.com/stellar/wallet-backend"

var fullSHA = regexp.MustCompile(`^[0-9a-f]{40}$`)

// source is a git clone of wallet-backend used to read and build commits.
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

// ensure makes sure the clone exists and holds commit. A clone the adapter
// creates fetches only the commits it is asked for, at depth 1: the adapter
// reads nothing but a commit's tree, and a full clone (over 200 MB) can take
// longer than the harness gives describe. A full clone passed with --repo stays
// full.
func (s source) ensure(ctx context.Context, commit string) error {
	if _, err := os.Stat(filepath.Join(s.dir, ".git")); os.IsNotExist(err) {
		return s.create(ctx, commit)
	} else if err != nil {
		return fmt.Errorf("checking %s: %w", s.dir, err)
	}
	if _, err := s.git(ctx, "cat-file", "-e", commit+"^{commit}"); err == nil {
		return nil
	}
	args := []string{"fetch", "--quiet", "origin", commit}
	shallow, err := s.git(ctx, "rev-parse", "--is-shallow-repository")
	if err != nil {
		return err
	}
	if strings.TrimSpace(string(shallow)) == "true" {
		args = append(args, "--depth", "1")
	}
	if _, err := s.git(ctx, args...); err != nil {
		return fmt.Errorf("commit %s is not in %s and could not be fetched: %w", commit, s.dir, err)
	}
	return nil
}

// create makes a clone holding only commit, at depth 1. It is fetched under a
// temporary name and renamed into place, so a run stopped halfway leaves no
// directory that looks like a clone.
func (s source) create(ctx context.Context, commit string) error {
	parent := filepath.Dir(s.dir)
	if err := os.MkdirAll(parent, 0o750); err != nil {
		return fmt.Errorf("creating %s: %w", parent, err)
	}
	tmp, err := os.MkdirTemp(parent, "."+filepath.Base(s.dir)+"-")
	if err != nil {
		return fmt.Errorf("creating a directory in %s: %w", parent, err)
	}
	defer os.RemoveAll(tmp) //nolint:errcheck // a no-op after the rename; otherwise it drops the partial fetch
	staged := source{dir: tmp, url: s.url}
	for _, args := range [][]string{
		{"init", "--quiet"},
		{"remote", "add", "origin", s.url},
		{"fetch", "--quiet", "--depth", "1", "origin", commit},
	} {
		if _, err := staged.git(ctx, args...); err != nil {
			return fmt.Errorf("fetching %s from %s: %w", commit, s.url, err)
		}
	}
	if err := os.Rename(tmp, s.dir); err != nil {
		return fmt.Errorf("moving the new clone to %s: %w", s.dir, err)
	}
	return nil
}

func (s source) show(ctx context.Context, commit, path string) ([]byte, error) {
	return s.git(ctx, "show", commit+":"+path)
}

// sdkProtocols is the newest ledger protocol each go-stellar-sdk release that
// a pinned wallet-backend commit uses can decode: v0.7.3 is the protocol 28
// sdk, v0.8.0 the protocol 30 one. An unlisted version is held to the protocol
// of the newest listed release before it (pinning.SDKProtocol).
var sdkProtocols = map[string]uint32{
	"v0.7.3": 28,
	"v0.8.0": 30,
}

// maxProtocol returns the newest ledger protocol the commit's go-stellar-sdk
// decodes, the sdk version, and a note when the version is not listed.
func (s source) maxProtocol(ctx context.Context, commit string) (uint32, string, string, error) {
	mod, err := s.show(ctx, commit, "go.mod")
	if err != nil {
		return 0, "", "", err
	}
	sc := bufio.NewScanner(bytes.NewReader(mod))
	for sc.Scan() {
		f := strings.Fields(sc.Text())
		if len(f) >= 2 && f[0] == "github.com/stellar/go-stellar-sdk" {
			p, note, err := pinning.SDKProtocol(sdkProtocols, f[1], "sdkProtocols in adapters/walletbackend/source.go")
			if err != nil {
				return 0, f[1], "", fmt.Errorf("go.mod at %s: %w", commit, err)
			}
			return p, f[1], note, nil
		}
	}
	return 0, "", "", fmt.Errorf("go.mod at %s does not require go-stellar-sdk", commit)
}

// needsStellarEnvironment says whether the commit's ingest command still has
// the required --stellar-environment option (removed before a9bac8d2).
func (s source) needsStellarEnvironment(ctx context.Context, commit string) (bool, error) {
	src, err := s.show(ctx, commit, "cmd/ingest.go")
	if err != nil {
		return false, err
	}
	return bytes.Contains(src, []byte("StellarEnvironmentOption")), nil
}

// imageTag is the image built from commit, cached by commit.
func imageTag(commit string) string { return "shapekit-wallet-backend:" + commit }

// ensureImage builds the commit with its own Dockerfile unless an image built
// from it on the same base digests already exists. Every FROM is replaced by
// its digest (pinnedContexts). builder names the buildx builder (one of the
// docker-container driver can carry memory and cpu limits); the current
// builder when empty. The notes name each base BaseImages does not list.
func (s source) ensureImage(ctx context.Context, commit, builder string) (string, []string, error) {
	tag := imageTag(commit)
	dockerfile, err := s.show(ctx, commit, "Dockerfile")
	if err != nil {
		return "", nil, err
	}
	bases, err := dockerfileBases(dockerfile)
	if err != nil {
		return "", nil, fmt.Errorf("the Dockerfile at %s: %w", commit, err)
	}
	// a base resolved for an earlier chain is built on the same digest, without asking the
	// registry again; a missing image or label leaves nothing recorded
	recorded, _ := docker(ctx, "image", "inspect", "--format", "{{index .Config.Labels \""+resolvedLabel+"\"}}", tag)
	plan, err := pinnedContexts(ctx, bases, pinning.Cached(pinning.Recorded(recorded), pinning.RegistryDigest))
	if err != nil {
		return "", nil, fmt.Errorf("the Dockerfile at %s: %w", commit, err)
	}
	if got, err := docker(ctx, "image", "inspect", "--format", "{{index .Config.Labels \""+baseLabel+"\"}}", tag); err == nil && got == plan.Set {
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
	args := []string{"buildx", "build", "--load"}
	if builder != "" {
		args = append(args, "--builder", builder)
	}
	args = append(args, plan.Flags...)
	args = append(args, "--build-arg", "GIT_COMMIT="+commit,
		"--label", "shapekit.wallet-backend.commit="+commit,
		"--label", baseLabel+"="+plan.Set,
		"--label", resolvedLabel+"="+pinning.Label(plan.Resolved),
		"-t", tag, "-")
	buildErr := dockerStream(ctx, out, args...)
	// drain whatever docker did not read so git archive can exit
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
