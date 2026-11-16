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

// sdkProtocols is the newest ledger protocol each go-stellar-sdk release that
// a pinned wallet-backend commit uses can decode: v0.7.3 is the protocol 28
// sdk, v0.8.0 the protocol 30 one. An unknown version is an error, not a guess.
var sdkProtocols = map[string]uint32{
	"v0.7.3": 28,
	"v0.8.0": 30,
}

func (s source) maxProtocol(ctx context.Context, commit string) (uint32, string, error) {
	mod, err := s.show(ctx, commit, "go.mod")
	if err != nil {
		return 0, "", err
	}
	sc := bufio.NewScanner(bytes.NewReader(mod))
	for sc.Scan() {
		f := strings.Fields(sc.Text())
		if len(f) >= 2 && f[0] == "github.com/stellar/go-stellar-sdk" {
			p, ok := sdkProtocols[f[1]]
			if !ok {
				return 0, f[1], fmt.Errorf("go-stellar-sdk %s at %s has no known protocol; add it to sdkProtocols", f[1], commit)
			}
			return p, f[1], nil
		}
	}
	return 0, "", fmt.Errorf("go.mod at %s does not require go-stellar-sdk", commit)
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
// from it on the same pinned base images already exists. Every FROM is
// replaced by its digest in BaseImages. builder names the buildx builder (one
// of the docker-container driver can carry memory and cpu limits); the current
// builder when empty.
func (s source) ensureImage(ctx context.Context, commit, builder string) (string, error) {
	tag := imageTag(commit)
	dockerfile, err := s.show(ctx, commit, "Dockerfile")
	if err != nil {
		return "", err
	}
	bases, err := dockerfileBases(dockerfile)
	if err != nil {
		return "", fmt.Errorf("the Dockerfile at %s: %w", commit, err)
	}
	contexts, baseSet, err := pinnedContexts(bases)
	if err != nil {
		return "", fmt.Errorf("the Dockerfile at %s: %w", commit, err)
	}
	if got, err := docker(ctx, "image", "inspect", "--format", "{{index .Config.Labels \""+baseLabel+"\"}}", tag); err == nil && got == baseSet {
		return tag, nil
	}
	archive := exec.CommandContext(ctx, "git", "-C", s.dir, "archive", "--format=tar", commit) //nolint:gosec // commit was checked to be 40 hex digits
	out, err := archive.StdoutPipe()
	if err != nil {
		return "", fmt.Errorf("git archive: %w", err)
	}
	var archErr bytes.Buffer
	archive.Stderr = &archErr
	if err := archive.Start(); err != nil {
		return "", fmt.Errorf("git archive: %w", err)
	}
	args := []string{"buildx", "build", "--load"}
	if builder != "" {
		args = append(args, "--builder", builder)
	}
	args = append(args, contexts...)
	args = append(args, "--build-arg", "GIT_COMMIT="+commit,
		"--label", "shapekit.wallet-backend.commit="+commit,
		"--label", baseLabel+"="+baseSet,
		"-t", tag, "-")
	buildErr := dockerStream(ctx, out, args...)
	// drain whatever docker did not read so git archive can exit
	_, drainErr := io.Copy(io.Discard, out)
	waitErr := archive.Wait()
	if buildErr != nil {
		return "", fmt.Errorf("building %s: %w", commit, buildErr)
	}
	if waitErr != nil {
		return "", fmt.Errorf("git archive %s: %w: %s", commit, waitErr, strings.TrimSpace(archErr.String()))
	}
	if drainErr != nil {
		return "", fmt.Errorf("git archive %s: %w", commit, drainErr)
	}
	return tag, nil
}
