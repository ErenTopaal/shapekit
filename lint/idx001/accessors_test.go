package idx001

import (
	"go/types"
	"os"
	"testing"

	"golang.org/x/tools/go/packages"

	"github.com/ErenTopaal/shapekit/lint/internal/fieldtable"
	"github.com/ErenTopaal/shapekit/lint/internal/testmod"
)

// a misspelled accessor in fields.toml, or one an sdk release renames,
// would make the rule silently stop following that field, so every name is
// resolved against the go-stellar-sdk the testdata module pins.
func TestAccessorsExistInSDK(t *testing.T) {
	dir := "testdata"
	if d := os.Getenv("IDX001_SDK_MODULE"); d != "" {
		// lets the same check run against another sdk release
		dir = d
	}

	testmod.Download(t, dir)

	cfg := &packages.Config{
		Mode: packages.NeedTypes,
		Dir:  dir,
		Env:  append(os.Environ(), "GOWORK=off", "GOFLAGS=-mod=mod"),
	}

	pkgs, err := packages.Load(cfg, "github.com/stellar/go-stellar-sdk/xdr")
	if err != nil || len(pkgs) != 1 || pkgs[0].Types == nil {
		t.Fatalf("loading the xdr package: %v", err)
	}

	scope := pkgs[0].Types.Scope()

	check := func(a fieldtable.Accessor, where string) {
		obj, ok := scope.Lookup(a.Type).(*types.TypeName)
		if !ok {
			t.Errorf("%s: xdr.%s is not a type", where, a.Type)

			return
		}

		if m, _, _ := types.LookupFieldOrMethod(types.NewPointer(obj.Type()), true, obj.Pkg(), a.Member); m == nil {
			t.Errorf("%s: xdr.%s has no field or method %s", where, a.Type, a.Member)
		}
	}

	for _, f := range fieldtable.Fields {
		for _, a := range f.Accessors {
			check(a, f.ID)
		}
	}

	for c := range carriers {
		check(c, "carrier")
	}

	for u := range unmuxers {
		check(u, "unmuxer")
	}
}
