// Command gen writes table_gen.go from registry/fields.toml. It runs from
// go generate in package fieldtable.
package main

import (
	"flag"
	"fmt"
	"os"

	"github.com/ErenTopaal/shapekit/lint/internal/fieldtable/generate"
)

func main() {
	in := flag.String("in", "", "path to registry/fields.toml")
	verdicts := flag.String("verdicts", "", "path to registry/generated/field-verdicts.json")
	out := flag.String("out", "", "path of the generated go file")
	flag.Parse()

	if *in == "" || *verdicts == "" || *out == "" {
		fmt.Fprintln(os.Stderr, "usage: gen -in fields.toml -verdicts field-verdicts.json -out table_gen.go")
		os.Exit(2)
	}

	src, err := os.ReadFile(*in)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	measured, err := os.ReadFile(*verdicts)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	code, err := generate.Generate(src, measured)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	if err := os.WriteFile(*out, code, 0o600); err != nil { //nolint:gosec // the output path is go generate's own argument
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
