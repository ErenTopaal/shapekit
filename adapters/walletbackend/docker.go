package walletbackend

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
)

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
