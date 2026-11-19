package walletbackend

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"

	"github.com/ErenTopaal/shapekit/adapters/pinning"
)

// BaseImages maps every FROM reference in wallet-backend's Dockerfile, at the
// commits this repository runs, to the same image by digest as pins.toml lists
// it (checked by TestBaseImagesMatchPins). The build replaces each FROM with
// the pinned digest through a buildx named context, so the tag a Dockerfile
// names never decides which bytes are built on. A FROM missing here is built
// on the digest its tag resolves to at build time, which the run records
// (pinning.Contexts).
var BaseImages = map[string]string{
	"golang:1.25.9-bookworm": "docker.io/library/golang@sha256:298734aec230b5f3e8cee450ce6d7eccc39f1797ba548ee90d57e9803030c6c3",
	"debian:bookworm-slim":   "docker.io/library/debian@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc",
	"ubuntu:jammy":           "docker.io/library/ubuntu@sha256:9abcaffc92a5279fff3a6a41635c0800ea33e528425fb389658806476e7defde",
}

// baseLabel is the image label recording the pinned bases an image was built
// on, so an image built before the pins (or on other pins) is rebuilt.
const baseLabel = "shapekit.wallet-backend.bases"

// dockerfileBases returns the external images a Dockerfile builds FROM, in
// order and without repeats. Stage names defined earlier in the file and
// scratch are not images.
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
		// the stage name counts from the next FROM on
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

// pinnedContexts returns the --build-context flags that replace each base with
// its digest, a stable digest of that set for the image label, and a note for
// each base BaseImages does not list.
func pinnedContexts(ctx context.Context, bases []string, resolve pinning.Resolver) ([]string, string, []string, error) {
	return pinning.Contexts(ctx, BaseImages, "BaseImages in adapters/walletbackend/docker.go", bases, resolve)
}

// docker runs the docker cli. Errors carry the command's stderr so a failure
// is reported with docker's own words.
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

// dockerStream runs the docker cli with stdin from in and its output copied
// to the adapter's stderr (the harness keeps it as the adapter log).
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

// dockerCombined runs the docker cli with stdout and stderr in one buffer, for
// commands that pass a container's two streams through (logs, run --rm).
func dockerCombined(ctx context.Context, args ...string) (string, error) {
	var out bytes.Buffer
	cmd := exec.CommandContext(ctx, "docker", args...) //nolint:gosec // docker is the fixed program; the args are built by this adapter
	cmd.Stdout = &out
	cmd.Stderr = &out
	if err := cmd.Run(); err != nil {
		return out.String(), fmt.Errorf("docker %s: %w: %s", args[0], err, lastLine(out.String()))
	}
	return out.String(), nil
}

func lastLine(s string) string {
	s = strings.TrimSpace(s)
	if i := strings.LastIndexByte(s, '\n'); i >= 0 {
		return s[i+1:]
	}
	return s
}

type containerState struct {
	Running    bool
	Restarting bool
	ExitCode   int
	OOMKilled  bool
	Pid        int
	Error      string
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
