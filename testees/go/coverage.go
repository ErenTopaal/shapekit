package main

// coverage_snapshot: go's statement coverage of the sdk, for a testee built with -cover,
// -covermode=atomic and -coverpkg=github.com/stellar/go-stellar-sdk/...,. (shapekit builds that
// variant for coverage runs; the testee's own package has to be in the list, or the runtime has
// no meta-data to write, and atomic is the mode runtime/coverage writes counters for). runtime/coverage writes the counters of this process; `go tool covdata textfmt` turns
// them into blocks. a build without -cover has no coverage meta-data, and hello does not list
// the op.

import (
	"bufio"
	"bytes"
	"encoding/binary"
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

// coverageNew answers coverage_new: how many counters of the instrumented packages have become
// nonzero since the previous call, and how many are nonzero. runtime/coverage writes the live
// counters into memory and they are read from there, with no files and no covdata, so it is
// cheap enough to ask after every input. The layout read is the counter data file of
// internal/coverage (defs.go, encodecounter): a 32-byte file header, a 16-byte segment header
// with the function count and the lengths of the string and argument tables that follow, then per
// live function uleb128 values: the counter count, the package id, the function id and the
// counters. The footer after the last function is not read.
var coverSeen = map[[3]uint32]bool{}

func coverageNew(request) (map[string]any, error) {
	if !instrumented() {
		return nil, unsupported("the testee was built without -cover, so it has no coverage counters")
	}
	var buf bytes.Buffer
	if err := coverage.WriteCounters(&buf); err != nil {
		return nil, internal("writing coverage counters: %v", err)
	}
	b := buf.Bytes()
	const fileHeader, segHeader = 32, 16
	if len(b) < fileHeader+segHeader || string(b[:4]) != "\x00\x63\x77\x6d" {
		return nil, internal("the counter data has an unknown header")
	}
	if b[24] != 2 { // CtrULeb128
		return nil, internal("the counter data uses counter flavor %d, not uleb128", b[24])
	}
	seg := b[fileHeader:]
	funcs := binary.LittleEndian.Uint64(seg[0:8])
	strtab := binary.LittleEndian.Uint32(seg[8:12])
	args := binary.LittleEndian.Uint32(seg[12:16])
	p := fileHeader + segHeader + int(strtab) + int(args)
	if p > len(b) {
		return nil, internal("the counter data is shorter than its tables")
	}
	read := func() (uint32, error) {
		var v uint64
		for shift := uint(0); ; shift += 7 {
			if p >= len(b) || shift > 28 {
				return 0, internal("the counter data ends inside a value")
			}
			c := b[p]
			p++
			v |= uint64(c&0x7f) << shift
			if c&0x80 == 0 {
				return uint32(v), nil
			}
		}
	}
	added := 0
	for f := uint64(0); f < funcs; f++ {
		n, err := read()
		if err != nil {
			return nil, err
		}
		pkg, err := read()
		if err != nil {
			return nil, err
		}
		fn, err := read()
		if err != nil {
			return nil, err
		}
		for i := uint32(0); i < n; i++ {
			c, err := read()
			if err != nil {
				return nil, err
			}
			k := [3]uint32{pkg, fn, i}
			if c != 0 && !coverSeen[k] {
				coverSeen[k] = true
				added++
			}
		}
	}
	return map[string]any{"new": added, "total": len(coverSeen)}, nil
}
