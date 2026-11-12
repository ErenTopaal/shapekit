// Package testmod prepares the analysistest testdata modules.
//
// Each rule's testdata directory is its own Go module that imports the real
// go-stellar-sdk, so the analyzers are tested against the real types.
// analysistest loads a module testdata directory with GOPROXY=off, so the
// module cache has to hold the sdk before it runs. Download fills it, which
// needs the network only the first time.
package testmod

import (
	"os"
	"os/exec"
	"testing"
)

// Download runs `go mod download` in dir and fails the test if it fails.
func Download(t *testing.T, dir string) {
	t.Helper()

	cmd := exec.CommandContext(t.Context(), "go", "mod", "download")
	cmd.Dir = dir
	// the repository's go.work does not list testdata modules
	cmd.Env = append(os.Environ(), "GOWORK=off")

	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("go mod download in %s: %v\n%s", dir, err, out)
	}
}
