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
	out := flag.String("out", "", "path of the generated go file")
	flag.Parse()

	if *in == "" || *out == "" {
		fmt.Fprintln(os.Stderr, "usage: gen -in fields.toml -out table_gen.go")
		os.Exit(2)
	}

	src, err := os.ReadFile(*in)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	code, err := generate.Generate(src)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	if err := os.WriteFile(*out, code, 0o600); err != nil { //nolint:gosec // the output path is go generate's own argument
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
