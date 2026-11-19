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

// Plan is how an image is built on its bases.
type Plan struct {
	// Flags are the buildx --build-context flags that build each base on its
	// digest.
	Flags []string
	// Set is a stable digest of every base and its digest, for an image label.
	Set string
	// Notes name each base the table does not list and the digest it is built on.
	Notes []string
	// Resolved maps each base the table does not list to the digest it
	// resolved to, for Label.
	Resolved map[string]string
}

// Contexts plans the build of an image on bases. A listed base is built on its
// pin (table maps the FROM reference to <repository>@sha256:<hex>, as
// pins.toml lists it). A base the Dockerfile already names by digest is built
// on that digest. Any other base is resolved by resolve and built on the
// digest it returns, which the plan's notes record. where names the table for
// the messages, for example "BaseImages in adapters/walletbackend/docker.go".
func Contexts(ctx context.Context, table map[string]string, where string, bases []string, resolve Resolver) (Plan, error) {
	var flags, set, notes []string
	resolved := map[string]string{}
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
				return Plan{}, fmt.Errorf("base image %s is not in %s and its digest could not be resolved: %w; add it to that table and pins.toml in a shapekit release", b, where, err)
			}
			pinned = r
			resolved[b] = r
			flags = append(flags, "--build-context", b+"=docker-image://"+pinned)
			notes = append(notes, fmt.Sprintf("base image %s is not in %s; it is built on %s, the digest its tag resolved to", b, where, pinned))
		}
		set = append(set, b+"="+pinned)
	}
	sort.Strings(set)
	sum := sha256.Sum256([]byte(strings.Join(set, "\n")))
	return Plan{Flags: flags, Set: hex.EncodeToString(sum[:]), Notes: notes, Resolved: resolved}, nil
}

// Label is the value of an image label recording resolved, which Recorded
// reads back.
func Label(resolved map[string]string) string {
	refs := make([]string, 0, len(resolved))
	for r := range resolved {
		refs = append(refs, r)
	}
	sort.Strings(refs)
	parts := make([]string, 0, len(refs))
	for _, r := range refs {
		parts = append(parts, r+"="+resolved[r])
	}
	return strings.Join(parts, " ")
}

// Recorded reads a Label value back. Entries it cannot read are left out, so
// they are resolved again.
func Recorded(label string) map[string]string {
	out := map[string]string{}
	for f := range strings.FieldsSeq(label) {
		ref, digest, ok := strings.Cut(f, "=")
		if ok && ref != "" && strings.Contains(digest, "@sha256:") {
			out[ref] = digest
		}
	}
	return out
}

// Cached resolves a reference from recorded first, so every chain of a run
// builds on the digest the first one resolved and asks the registry once; the
// rest through resolve.
func Cached(recorded map[string]string, resolve Resolver) Resolver {
	return func(ctx context.Context, ref string) (string, error) {
		if d, ok := recorded[ref]; ok {
			return d, nil
		}
		return resolve(ctx, ref)
	}
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
// imagetools: the digest of the manifest (or index) its tag names now, which is
// the sha256 of the raw manifest bytes. --raw fetches that one manifest and not
// the per-platform ones, so a resolution costs one request against a
// registry's pull limit.
func RegistryDigest(ctx context.Context, ref string) (string, error) {
	cmd := exec.CommandContext(ctx, "docker", "buildx", "imagetools", "inspect", "--raw", ref) //nolint:gosec // docker is the fixed program; ref comes from the target's Dockerfile
	var stderr strings.Builder
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("docker buildx imagetools inspect %s: %w: %s", ref, err, strings.TrimSpace(stderr.String()))
	}
	var m struct {
		SchemaVersion int `json:"schemaVersion"`
	}
	if err := json.Unmarshal(out, &m); err != nil {
		return "", fmt.Errorf("reading the manifest of %s: %w", ref, err)
	}
	if m.SchemaVersion != 2 {
		return "", fmt.Errorf("the manifest of %s has schema version %d, not 2", ref, m.SchemaVersion)
	}
	sum := sha256.Sum256(out)
	return Repository(ref) + "@sha256:" + hex.EncodeToString(sum[:]), nil
}
