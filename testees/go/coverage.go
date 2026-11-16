package main

// coverage_snapshot: go's statement coverage of the sdk, for a testee built with -cover,
// -covermode=atomic and -coverpkg=github.com/stellar/go-stellar-sdk/...,. (shapekit builds that
// variant for coverage runs; the testee's own package has to be in the list, or the runtime has
// no meta-data to write, and atomic is the mode runtime/coverage writes counters for). runtime/coverage writes the counters of this process; `go tool covdata textfmt` turns
// them into blocks. a build without -cover has no coverage meta-data, and hello does not list
// the op.

import (
	"bufio"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime/coverage"
	"sort"
	"strings"
)

// instrumented tells whether the binary carries coverage meta-data
func instrumented() bool {
	return coverage.WriteMeta(io.Discard) == nil
}

func coverageSnapshot(request) (map[string]any, error) {
	if !instrumented() {
		return nil, unsupported("the testee was built without -cover, so it has no coverage counters")
	}
	dir, err := os.MkdirTemp("", "shapekit-cover-")
	if err != nil {
		return nil, internal("creating a coverage directory: %v", err)
	}
	defer os.RemoveAll(dir) //nolint:errcheck // a leftover temporary directory changes no answer
	if err := coverage.WriteMetaDir(dir); err != nil {
		return nil, internal("writing coverage meta-data: %v", err)
	}
	if err := coverage.WriteCountersDir(dir); err != nil {
		return nil, internal("writing coverage counters: %v", err)
	}
	text := filepath.Join(dir, "cover.txt")
	cmd := exec.Command("go", "tool", "covdata", "textfmt", "-i="+dir, "-o="+text) //nolint:gosec // fixed tool, paths of our own temporary directory
	if out, err := cmd.CombinedOutput(); err != nil {
		return nil, internal("go tool covdata textfmt: %v: %s", err, strings.TrimSpace(string(out)))
	}
	f, err := os.Open(text) //nolint:gosec // the file covdata just wrote
	if err != nil {
		return nil, internal("reading %s: %v", text, err)
	}
	defer f.Close() //nolint:errcheck // read only
	ranges := map[string]bool{}
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		// mode: set|count|atomic, then <file>:<line>.<col>,<line>.<col> <statements> <count>
		line := sc.Text()
		if strings.HasPrefix(line, "mode:") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) != 3 || fields[2] == "0" {
			continue
		}
		block, ok := strings.CutPrefix(fields[0], sdkModule+"/")
		if !ok {
			continue
		}
		ranges[block] = true
	}
	if err := sc.Err(); err != nil {
		return nil, internal("reading %s: %v", text, err)
	}
	out := make([]string, 0, len(ranges))
	for r := range ranges {
		out = append(out, r)
	}
	sort.Strings(out)
	return map[string]any{"tool": "go -cover statement blocks", "ranges": out}, nil
}
