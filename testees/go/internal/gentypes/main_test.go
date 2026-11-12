package main

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

func TestManifestTypes(t *testing.T) {
	path := filepath.Join(t.TempDir(), "manifest.json")
	doc := `{"entries":[{"variants":{"any":{"xdr_type":"ScVal"}}},{"variants":{"a":{"xdr_type":"Asset"},"b":{"xdr_type":"ScVal"}}}]}`
	if err := os.WriteFile(path, []byte(doc), 0o600); err != nil {
		t.Fatal(err)
	}
	got, err := manifestTypes(path)
	if err != nil {
		t.Fatal(err)
	}
	if want := []string{"Asset", "ScVal"}; !slices.Equal(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}

	if err := os.WriteFile(path, []byte(`{"entries":[]}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := manifestTypes(path); err == nil {
		t.Error("a manifest without xdr types was accepted")
	}
}

func source(version string, types map[string][]string, gxdrEnums ...string) sdkSource {
	s := sdkSource{version: version, xdrMethods: map[string]map[string]bool{}, gxdrEnums: map[string]bool{}}
	for name, methods := range types {
		s.xdrMethods[name] = map[string]bool{}
		for _, m := range methods {
			s.xdrMethods[name][m] = true
		}
	}
	for _, g := range gxdrEnums {
		s.gxdrEnums[g] = true
	}
	return s
}

func TestMapTypes(t *testing.T) {
	codec := []string{"DecodeFrom", "MarshalBinary"}
	a := source("v1", map[string][]string{"ScVal": codec, "Asset": codec})
	b := source("v2", map[string][]string{"ScVal": codec, "Asset": {"DecodeFrom"}})

	got, err := mapTypes([]string{"SCVal"}, []sdkSource{a, b})
	if err != nil || got["SCVal"] != "ScVal" {
		t.Errorf("case-insensitive match: %v, %v", got, err)
	}
	if _, err := mapTypes([]string{"Asset"}, []sdkSource{a, b}); err == nil || !strings.Contains(err.Error(), "MarshalBinary") {
		t.Errorf("a version without MarshalBinary: %v", err)
	}
	if _, err := mapTypes([]string{"LedgerCloseMeta"}, []sdkSource{a}); err == nil {
		t.Error("a type the sdk lacks was mapped")
	}
}

func TestMapEnums(t *testing.T) {
	enum := []string{"ValidEnum"}
	a := source("v1", map[string][]string{"ScValType": enum, "AssetType": enum}, "SCValType", "AssetType")
	b := source("v2", map[string][]string{"ScValType": enum}, "SCValType")

	got, err := mapEnums([]sdkSource{a, b})
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 1 || got["ScValType"] != "SCValType" {
		t.Errorf("got %v, want only the enum every version has", got)
	}
}
