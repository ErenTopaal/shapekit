package main

import (
	"encoding/json"
	"maps"
	"os"
	"slices"
	"testing"
)

// the types the go:generate line in tables.go adds to the manifest's
var extraTypes = []string{"TransactionMeta"}

// the type table must name exactly the manifest's xdr types and the extra ones, or the
// generated file is stale
func TestTypeTableMatchesManifest(t *testing.T) {
	names, err := manifestTypeNames()
	if err != nil {
		t.Fatal(err)
	}
	for _, extra := range extraTypes {
		if !slices.Contains(names, extra) {
			names = append(names, extra)
		}
	}
	slices.Sort(names)
	have := slices.Sorted(maps.Keys(xdrTypes))
	if !slices.Equal(have, names) {
		t.Errorf("tables_gen.go has %v, the manifest and the extra types are %v; run go generate", have, names)
	}
	for name, newValue := range xdrTypes {
		if _, ok := newValue().(interface{ MarshalBinary() ([]byte, error) }); !ok {
			t.Errorf("%s has no MarshalBinary", name)
		}
	}
}

func manifestTypeNames() ([]string, error) {
	data, err := os.ReadFile("../../corpus/manifest.json")
	if err != nil {
		return nil, err
	}
	var doc any
	if err := json.Unmarshal(data, &doc); err != nil {
		return nil, err
	}
	seen := map[string]bool{}
	var walk func(v any)
	walk = func(v any) {
		switch t := v.(type) {
		case map[string]any:
			for k, child := range t {
				if name, ok := child.(string); ok && k == "xdr_type" {
					seen[name] = true
				}
				walk(child)
			}
		case []any:
			for _, child := range t {
				walk(child)
			}
		}
	}
	walk(doc)
	return slices.Sorted(maps.Keys(seen)), nil
}
