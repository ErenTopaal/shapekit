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

// every type the manifest uses and the extra ones decode in every build, and no type is in the
// table twice or both in it and in absentTypes; a build without a version tag has the types
// every version shares
func TestTypeTableCoversTheManifest(t *testing.T) {
	names, err := manifestTypeNames()
	if err != nil {
		t.Fatal(err)
	}
	names = append(names, extraTypes...)
	for _, name := range names {
		if _, ok := xdrTypes[name]; !ok {
			t.Errorf("tables_gen.go has no %s; run go generate", name)
		}
	}
	for _, name := range absentTypes {
		if _, ok := xdrTypes[name]; ok {
			t.Errorf("%s is in the table and listed as absent", name)
		}
	}
	have := slices.Sorted(maps.Keys(xdrTypes))
	if len(have) < 400 {
		t.Errorf("the table has %d types; it is generated from the whole schema", len(have))
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
