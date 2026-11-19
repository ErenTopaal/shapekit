// Package pinning decides two facts the source-built adapters (wallet-backend,
// stellar-etl) take from the target commit they build: the newest ledger
// protocol its go-stellar-sdk decodes, and the image each FROM of its
// Dockerfile is built on. Each adapter keeps a table of both. A pull request
// that bumps either past the table still runs: an unlisted sdk is held to the
// protocol of the newest listed release before it, and an unlisted base image
// is built on the digest its tag resolves to, which the run records. What could
// not be decided safely is an error naming the table to update.
package pinning

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os/exec"
	"sort"
	"strings"

	"golang.org/x/mod/semver"
)

// SDKProtocol returns the newest ledger protocol go-stellar-sdk version
// decodes, from table (version to protocol), and a note when the version is
// not listed. An unlisted version is held to the protocol of the newest listed
// version before it: a go-stellar-sdk release decodes every protocol an older
// one does, so that protocol is a floor, and chains of a newer protocol are
// judged out of scope instead of failing. A version that is not a semantic
// version, or is older than every listed one, has no floor and is an error.
// where names the table for the messages, for example "sdkProtocols in
// adapters/walletbackend/source.go".
func SDKProtocol(table map[string]uint32, version, where string) (uint32, string, error) {
	if p, ok := table[version]; ok {
		return p, "", nil
	}
	if !semver.IsValid(version) {
		return 0, "", fmt.Errorf("go-stellar-sdk %s is not a semantic version, so its protocol is unknown; add it to %s in a shapekit release", version, where)
	}
	floor := ""
	for v := range table {
		if semver.Compare(v, version) < 0 && (floor == "" || semver.Compare(v, floor) > 0) {
			floor = v
		}
	}
	if floor == "" {
		return 0, "", fmt.Errorf("go-stellar-sdk %s is older than every version in %s, so its protocol is unknown; add it to that table in a shapekit release", version, where)
	}
	p := table[floor]
	return p, fmt.Sprintf("go-stellar-sdk %s is not in %s; it is held to protocol %d, the protocol of %s, the newest listed release before it. Chains of a newer protocol are out of scope until a shapekit release lists %s", version, where, p, floor, version), nil
}

// Resolver returns the digest reference (<repository>@sha256:<hex>) an image
// reference resolves to now.
type Resolver func(ctx context.Context, ref string) (string, error)

// Contexts returns the buildx flags that build each base on a fixed digest, a
// stable digest of that set for an image label, and a note for each base the
// table does not list. A listed base is built on its pin (table maps the FROM
// reference to <repository>@sha256:<hex>, as pins.toml lists it). A base the
// Dockerfile already names by digest is built on that digest. Any other base is
// resolved by resolve and built on the digest it returns, which the note
// records. where names the table for the messages, for example "BaseImages in
// adapters/walletbackend/docker.go".
func Contexts(ctx context.Context, table map[string]string, where string, bases []string, resolve Resolver) ([]string, string, []string, error) {
	var flags, set, notes []string
	for _, b := range bases {
		pinned, ok := table[b]
		switch {
		case ok:
			flags = append(flags, "--build-context", b+"=docker-image://"+pinned)
		case strings.Contains(b, "@sha256:"):
			// the Dockerfile pins it itself, and the build reads that digest
			pinned = b
		default:
			r, err := resolve(ctx, b)
			if err != nil {
				return nil, "", nil, fmt.Errorf("base image %s is not in %s and its digest could not be resolved: %w; add it to that table and pins.toml in a shapekit release", b, where, err)
			}
			pinned = r
			flags = append(flags, "--build-context", b+"=docker-image://"+pinned)
			notes = append(notes, fmt.Sprintf("base image %s is not in %s; it is built on %s, the digest its tag resolves to now", b, where, pinned))
		}
		set = append(set, b+"="+pinned)
	}
	sort.Strings(set)
	sum := sha256.Sum256([]byte(strings.Join(set, "\n")))
	return flags, hex.EncodeToString(sum[:]), notes, nil
}

// Repository is the full repository name docker gives an image reference
// without its tag or digest: golang:1.25 is docker.io/library/golang.
func Repository(ref string) string {
	name := ref
	if i := strings.Index(name, "@"); i >= 0 {
		name = name[:i]
	}
	if i := strings.LastIndex(name, ":"); i > strings.LastIndex(name, "/") {
		name = name[:i]
	}
	first, _, hasSlash := strings.Cut(name, "/")
	switch {
	case !hasSlash:
		return "docker.io/library/" + name
	case strings.ContainsAny(first, ".:") || first == "localhost":
		return name
	default:
		return "docker.io/" + name
	}
}

// RegistryDigest resolves ref through the registry with docker buildx
// imagetools: the digest of the manifest (or index) its tag names now.
func RegistryDigest(ctx context.Context, ref string) (string, error) {
	cmd := exec.CommandContext(ctx, "docker", "buildx", "imagetools", "inspect", "--format", "{{json .Manifest}}", ref) //nolint:gosec // docker is the fixed program; ref comes from the target's Dockerfile
	var stderr strings.Builder
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("docker buildx imagetools inspect %s: %w: %s", ref, err, strings.TrimSpace(stderr.String()))
	}
	var m struct {
		Digest string `json:"digest"`
	}
	if err := json.Unmarshal(out, &m); err != nil {
		return "", fmt.Errorf("reading the manifest of %s: %w", ref, err)
	}
	if !strings.HasPrefix(m.Digest, "sha256:") || len(m.Digest) != len("sha256:")+64 {
		return "", fmt.Errorf("the manifest of %s has digest %q", ref, m.Digest)
	}
	return Repository(ref) + "@" + m.Digest, nil
}
