package walletbackend

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// run runs git in dir and returns its trimmed stdout.
func run(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...) //nolint:gosec // test-only git calls on temporary repositories
	cmd.Env = append(os.Environ(), "GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@t", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@t")
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %s: %v: %s", strings.Join(args, " "), err, out)
	}
	return strings.TrimSpace(string(out))
}

// upstream makes a repository with two commits, each writing go.mod, that
// serves any reachable commit by hash the way GitHub does.
func upstream(t *testing.T) (url, first, second string) {
	t.Helper()
	dir := t.TempDir()
	run(t, dir, "init", "--quiet")
	run(t, dir, "config", "uploadpack.allowReachableSHA1InWant", "true")
	for i, mod := range []string{"module first\n", "module second\n"} {
		if err := os.WriteFile(filepath.Join(dir, "go.mod"), []byte(mod), 0o600); err != nil {
			t.Fatal(err)
		}
		run(t, dir, "add", "go.mod")
		run(t, dir, "commit", "--quiet", "-m", mod)
		if i == 0 {
			first = run(t, dir, "rev-parse", "HEAD")
		}
	}
	// the file scheme, since git ignores --depth for a plain path
	return "file://" + dir, first, run(t, dir, "rev-parse", "HEAD")
}

func TestEnsureFetchesOnlyTheCommit(t *testing.T) {
	url, first, second := upstream(t)
	s := source{dir: filepath.Join(t.TempDir(), "cache", "wallet-backend"), url: url}
	ctx := context.Background()
	if err := s.ensure(ctx, first); err != nil {
		t.Fatal(err)
	}
	if got := run(t, s.dir, "rev-parse", "--is-shallow-repository"); got != "true" {
		t.Fatalf("the new clone is not shallow: %s", got)
	}
	if mod, err := s.show(ctx, first, "go.mod"); err != nil || string(mod) != "module first\n" {
		t.Fatalf("go.mod at the first commit: %q %v", mod, err)
	}
	if _, err := s.git(ctx, "cat-file", "-e", second+"^{commit}"); err == nil {
		t.Fatal("the clone holds a commit it was not asked for")
	}
	// a later commit is fetched into the same clone, also at depth 1
	if err := s.ensure(ctx, second); err != nil {
		t.Fatal(err)
	}
	if mod, err := s.show(ctx, second, "go.mod"); err != nil || string(mod) != "module second\n" {
		t.Fatalf("go.mod at the second commit: %q %v", mod, err)
	}
	if n := run(t, s.dir, "rev-list", "--count", second); n != "1" {
		t.Fatalf("the second fetch brought %s commits of history", n)
	}
}

func TestEnsureLeavesNoCloneWhenTheFetchFails(t *testing.T) {
	url, _, _ := upstream(t)
	parent := t.TempDir()
	s := source{dir: filepath.Join(parent, "wallet-backend"), url: url}
	if err := s.ensure(context.Background(), strings.Repeat("ab", 20)); err == nil {
		t.Fatal("fetching a commit upstream does not have succeeded")
	}
	entries, err := os.ReadDir(parent)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("a failed fetch left %v", entries)
	}
}
