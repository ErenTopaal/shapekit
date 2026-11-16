package main

// FuzzXdrLaws is the coverage-guided target for go-stellar-sdk, run by
// `shapekit fuzz native --target go` through `go test -fuzz`. It checks the encoding laws of
// the law library: a value the sdk decodes under the corpus limits must encode back to exactly
// the input bytes, and the input with a word more or a byte less must not decode. The first
// input byte picks the type from the configured list; the rest is the xdr.
//
// The configuration comes from the json file named by SHAPEKIT_FUZZ_CONFIG: the types in
// order with their decode depth, a directory of seed inputs and a file of classes not to
// report again. A plain `go test` skips the target, which has nothing to run without it. Classes are named as in testees/rust/fuzz/fuzz_targets/xdr_laws.rs.

import (
	"bytes"
	"encoding"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"github.com/stellar/go-stellar-sdk/xdr"
)

type fuzzConfig struct {
	Types []struct {
		Name  string `json:"name"`
		Depth uint   `json:"depth"`
	} `json:"types"`
	Seeds string `json:"seeds"`
	Skip  string `json:"skip"`
}

func loadFuzzConfig(f *testing.F) (*fuzzConfig, map[string]bool) {
	path := os.Getenv("SHAPEKIT_FUZZ_CONFIG")
	if path == "" {
		return nil, nil
	}
	data, err := os.ReadFile(path) //nolint:gosec // the file the harness is pointed at
	if err != nil {
		f.Fatalf("reading %s: %v", path, err)
	}
	var c fuzzConfig
	if err := json.Unmarshal(data, &c); err != nil {
		f.Fatalf("parsing %s: %v", path, err)
	}
	for _, t := range c.Types {
		if _, ok := xdrTypes[t.Name]; !ok {
			f.Fatalf("the type table has no %s", t.Name)
		}
	}
	skip := map[string]bool{}
	if c.Skip != "" {
		data, err := os.ReadFile(c.Skip)
		if err != nil {
			f.Fatalf("reading %s: %v", c.Skip, err)
		}
		for line := range strings.SplitSeq(string(data), "\n") {
			if line = strings.TrimSpace(line); line != "" {
				skip[line] = true
			}
		}
	}
	return &c, skip
}

// decodes all of data as name under the corpus limits; nil when the sdk refuses it
func fuzzDecode(name string, depth uint, data []byte) any {
	value := xdrTypes[name]()
	read, err := xdr.UnmarshalWithOptions(bytes.NewReader(data), value, xdr.DecodeOptions{MaxDepth: depth, MaxInputLen: len(data)})
	if err != nil || read != len(data) {
		return nil
	}
	return value
}

func wordKind(w []byte) string {
	switch {
	case bytes.Equal(w, []byte{0, 0, 0, 0}):
		return "0"
	case bytes.Equal(w, []byte{0, 0, 0, 1}):
		return "1"
	}
	return "x"
}

func word(b []byte, i int) []byte {
	if i*4 >= len(b) {
		return nil
	}
	return b[i*4 : min(i*4+4, len(b))]
}

// <in>-><out> of the first 4-byte word in which the two encodings differ
func firstWordChange(input, again []byte) string {
	n := max(len(input), len(again))
	for i := 0; i*4 < n; i++ {
		a, b := word(input, i), word(again, i)
		if !bytes.Equal(a, b) {
			return wordKind(a) + "->" + wordKind(b)
		}
	}
	return "x->x"
}

func FuzzXdrLaws(f *testing.F) {
	config, skip := loadFuzzConfig(f)
	if config == nil {
		f.Skip("SHAPEKIT_FUZZ_CONFIG is not set; shapekit fuzz native --target go runs this target")
	}
	if config.Seeds != "" {
		entries, err := os.ReadDir(config.Seeds)
		if err != nil {
			f.Fatalf("reading %s: %v", config.Seeds, err)
		}
		names := make([]string, 0, len(entries))
		for _, e := range entries {
			names = append(names, e.Name())
		}
		sort.Strings(names)
		for _, n := range names {
			data, err := os.ReadFile(filepath.Join(config.Seeds, n)) //nolint:gosec // seeds the runner wrote
			if err != nil {
				f.Fatalf("reading seed %s: %v", n, err)
			}
			f.Add(data)
		}
	}
	f.Fuzz(func(t *testing.T, data []byte) {
		if len(data) == 0 {
			return
		}
		ty := config.Types[int(data[0])%len(config.Types)]
		input := data[1:]
		report := func(class, detail string) {
			if !skip[class] {
				t.Fatalf("shapekit-class %s: %s", class, detail)
			}
		}
		defer func() {
			if p := recover(); p != nil {
				report(ty.Name+":panic", fmt.Sprint(p))
			}
		}()
		value := fuzzDecode(ty.Name, ty.Depth, input)
		if value == nil {
			return
		}
		again, err := value.(encoding.BinaryMarshaler).MarshalBinary()
		if err != nil {
			report(ty.Name+":roundtrip:encode-error", err.Error())
			return
		}
		if !bytes.Equal(again, input) {
			report(ty.Name+":roundtrip:"+firstWordChange(input, again),
				fmt.Sprintf("re-encoding gives %d bytes differing from the %d input bytes", len(again), len(input)))
		}
		if fuzzDecode(ty.Name, ty.Depth, append(append([]byte{}, input...), 0, 0, 0, 0)) != nil {
			report(ty.Name+":noncanonical:longer", "the input plus a trailing zero word decodes")
		}
		if len(input) > 0 && fuzzDecode(ty.Name, ty.Depth, input[:len(input)-1]) != nil {
			report(ty.Name+":noncanonical:shorter", "the input without its last byte decodes")
		}
	})
}
