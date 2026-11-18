package walletbackend

import (
	"bytes"
	"context"
	"fmt"
	"io/fs"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"time"

	"github.com/ErenTopaal/shapekit/adapters/protocol"
	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// startS3 runs seaweedfs the way wallet-backend's integration tests do
// (internal/integrationtests/infrastructure/containers.go): one `weed server
// -s3` without auth, the bucket created through `weed shell` once the filer is
// up.
// s3Memory is the memory and swap limit of the seaweedfs container.
const s3Memory = "1g"

func (t *target) startS3(ctx context.Context) error {
	p := t.params
	script := fmt.Sprintf("weed server -dir=/data -s3 & "+
		"until echo s3.bucket.list | weed shell -master localhost:9333 2>/dev/null | grep -q %[1]s; do "+
		"echo s3.bucket.create -name %[1]s | weed shell -master localhost:9333 2>/dev/null; sleep 1; done; wait", s3Bucket)
	// seaweedfs holds a chain's lake, a few megabytes; the cap keeps it from growing unbounded
	args := []string{"run", "-d", "--name", t.s3, "--network", p.DockerNetwork, "--network-alias", t.s3,
		"--memory", s3Memory, "--memory-swap", s3Memory}
	args = append(args, t.runFlags()...)
	args = append(args, "--entrypoint", "sh", SeaweedImage, "-c", script)
	if _, err := docker(ctx, args...); err != nil {
		return wire.Errorf(wire.ClassTarget, "starting seaweedfs: %v", err)
	}
	ip, err := containerIP(ctx, t.s3, p.DockerNetwork)
	if err != nil {
		return wire.Errorf(wire.ClassTarget, "%v", err)
	}
	t.s3url = "http://" + ip + ":" + s3Port + "/" + s3Bucket
	client := &http.Client{Timeout: 5 * time.Second}
	deadline := time.Now().Add(readyTimeout)
	for {
		ok, err := bucketReady(ctx, client, t.s3url)
		if ok {
			break
		}
		if time.Now().After(deadline) {
			return wire.Errorf(wire.ClassTarget, "seaweedfs bucket %s not ready within %s: %v", s3Bucket, readyTimeout, err)
		}
		select {
		case <-ctx.Done():
			return wire.Errorf(wire.ClassTarget, "waiting for seaweedfs: %v", ctx.Err())
		case <-time.After(time.Second):
		}
	}
	lc, err := newLakeCopier(p.Delivery.Lake, t.s3url)
	if err != nil {
		return wire.Errorf(wire.ClassTarget, "%v", err)
	}
	t.lake = lc
	return nil
}

func bucketReady(ctx context.Context, c *http.Client, url string) (bool, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, http.NoBody)
	if err != nil {
		return false, err
	}
	resp, err := c.Do(req)
	if err != nil {
		return false, err
	}
	if cerr := resp.Body.Close(); cerr != nil {
		return false, fmt.Errorf("GET %s: %w", url, cerr)
	}
	if resp.StatusCode != http.StatusOK {
		return false, fmt.Errorf("GET %s: %s", url, resp.Status)
	}
	return true, nil
}

var batchName = regexp.MustCompile(`^[0-9A-F]{8}--(\d+)(?:-(\d+))?\.xdr\.zst$`)

type lakeBatch struct {
	key        string
	path       string
	start, end uint32
}

// lakeCopier releases a chain's SEP-54 lake into the bucket batch by batch,
// so the target's datastore sees only ledgers the harness made visible.
type lakeCopier struct {
	bucketURL string
	root      string
	batches   []lakeBatch
	next      int
	config    bool
	client    *http.Client
}

func newLakeCopier(lake wire.Lake, bucketURL string) (*lakeCopier, error) {
	layout, err := protocol.NewLayout(lake.LedgersPerBatch, lake.BatchesPerPartition)
	if err != nil {
		return nil, err
	}
	lc := &lakeCopier{bucketURL: bucketURL, root: lake.Path, client: &http.Client{Timeout: time.Minute}}
	err = filepath.WalkDir(lake.Path, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		m := batchName.FindStringSubmatch(d.Name())
		if m == nil {
			return nil
		}
		start, err := strconv.ParseUint(m[1], 10, 32)
		if err != nil {
			return fmt.Errorf("batch %s: %w", path, err)
		}
		end := start
		if m[2] != "" {
			if end, err = strconv.ParseUint(m[2], 10, 32); err != nil {
				return fmt.Errorf("batch %s: %w", path, err)
			}
		}
		rel, err := filepath.Rel(lake.Path, path)
		if err != nil {
			return err
		}
		// the key must be the one the target computes from the layout
		if want := layout.ObjectKey(uint32(start)); filepath.ToSlash(rel) != want {
			return fmt.Errorf("batch %s is not at its sep-54 key %s", rel, want)
		}
		lc.batches = append(lc.batches, lakeBatch{key: filepath.ToSlash(rel), path: path, start: uint32(start), end: uint32(end)})
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("reading the lake at %s: %w", lake.Path, err)
	}
	if len(lc.batches) == 0 {
		return nil, fmt.Errorf("the lake at %s has no batches", lake.Path)
	}
	sort.Slice(lc.batches, func(i, j int) bool { return lc.batches[i].start < lc.batches[j].start })
	return lc, nil
}

// copyUpTo uploads .config.json and every batch whose last ledger is at or
// below head that is not in the bucket yet.
func (lc *lakeCopier) copyUpTo(ctx context.Context, head uint32) error {
	if !lc.config {
		if err := lc.put(ctx, ".config.json", filepath.Join(lc.root, ".config.json")); err != nil {
			return err
		}
		lc.config = true
	}
	for lc.next < len(lc.batches) && lc.batches[lc.next].end <= head {
		b := lc.batches[lc.next]
		if err := lc.put(ctx, b.key, b.path); err != nil {
			return err
		}
		lc.next++
	}
	return nil
}

func (lc *lakeCopier) put(ctx context.Context, key, path string) error {
	body, err := os.ReadFile(path) //nolint:gosec // a batch file found under the lake path the harness passed
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPut, lc.bucketURL+"/"+key, bytes.NewReader(body))
	if err != nil {
		return err
	}
	resp, err := lc.client.Do(req)
	if err != nil {
		return fmt.Errorf("PUT %s: %w", key, err)
	}
	if cerr := resp.Body.Close(); cerr != nil {
		return fmt.Errorf("PUT %s: %w", key, cerr)
	}
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("PUT %s: %s", key, resp.Status)
	}
	return nil
}
