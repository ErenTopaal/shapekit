package fieldtable

import (
	"bytes"
	"os"
	"testing"

	"github.com/ErenTopaal/shapekit/lint/internal/fieldtable/generate"
)

func TestTableMatchesRegistry(t *testing.T) {
	src, err := os.ReadFile("../../../registry/fields.toml")
	if err != nil {
		t.Fatal(err)
	}

	want, err := generate.Generate(src)
	if err != nil {
		t.Fatal(err)
	}

	got, err := os.ReadFile("table_gen.go")
	if err != nil {
		t.Fatal(err)
	}

	if !bytes.Equal(got, want) {
		t.Fatal("table_gen.go is out of date with registry/fields.toml; run go generate ./internal/fieldtable")
	}
}

func TestLookup(t *testing.T) {
	f, ok := Lookup("tx.memo.text")
	if !ok {
		t.Fatal("tx.memo.text missing")
	}

	if f.ValidatedByCore != "no" || len(f.Accessors) == 0 {
		t.Errorf("unexpected row %+v", f)
	}
}
