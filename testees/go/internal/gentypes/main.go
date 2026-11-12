// Command gentypes writes tables_gen.go for the go testee: the go-stellar-sdk xdr type behind
// every xdr type name the corpus manifest uses, and the gxdr enum that carries the xdr
// identifiers of every xdr enum.
//
// The tables are read from the sdk source of every version in versions/*.mod, and only names
// that exist in all of them are written, so the one generated file compiles against each
// version. A manifest type that some version lacks stops the generator instead of being
// dropped, because the runner would then send a type the testee cannot answer.
package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"go/ast"
	"go/format"
	"go/parser"
	"go/token"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
)

const sdkModule = "github.com/stellar/go-stellar-sdk"

func main() {
	manifest := flag.String("manifest", "../../corpus/manifest.json", "corpus manifest to read xdr_type names from")
	versions := flag.String("versions", "versions", "directory holding one <version>.mod per sdk version")
	out := flag.String("out", "tables_gen.go", "file to write")
	flag.Parse()

	if err := run(*manifest, *versions, *out); err != nil {
		fmt.Fprintln(os.Stderr, "gentypes:", err)
		os.Exit(1)
	}
}

func run(manifestPath, versionsDir, out string) error {
	names, err := manifestTypes(manifestPath)
	if err != nil {
		return err
	}

	modfiles, err := filepath.Glob(filepath.Join(versionsDir, "*.mod"))
	if err != nil {
		return err
	}
	if len(modfiles) == 0 {
		return fmt.Errorf("no module files in %s", versionsDir)
	}
	slices.Sort(modfiles)

	var sdks []sdkSource
	for _, mf := range modfiles {
		dir, version, err := sdkDir(mf)
		if err != nil {
			return err
		}
		src, err := readSDK(dir)
		if err != nil {
			return fmt.Errorf("%s %s: %w", sdkModule, version, err)
		}
		src.version = version
		sdks = append(sdks, src)
	}

	types, err := mapTypes(names, sdks)
	if err != nil {
		return err
	}
	enums, err := mapEnums(sdks)
	if err != nil {
		return err
	}

	code, err := render(types, enums, sdks)
	if err != nil {
		return err
	}
	return os.WriteFile(out, code, 0o644) //nolint:gosec // generated source is meant to be world readable
}

// manifestTypes collects every xdr_type value anywhere in the manifest.
func manifestTypes(path string) ([]string, error) {
	data, err := os.ReadFile(path) //nolint:gosec // the path is the generator's own flag
	if err != nil {
		return nil, err
	}
	var doc any
	if err := json.Unmarshal(data, &doc); err != nil {
		return nil, fmt.Errorf("%s: %w", path, err)
	}
	seen := map[string]bool{}
	var walk func(v any) error
	walk = func(v any) error {
		switch t := v.(type) {
		case map[string]any:
			for k, child := range t {
				if k == "xdr_type" {
					name, ok := child.(string)
					if !ok {
						return fmt.Errorf("%s: xdr_type %v is not a string", path, child)
					}
					seen[name] = true
					continue
				}
				if err := walk(child); err != nil {
					return err
				}
			}
		case []any:
			for _, child := range t {
				if err := walk(child); err != nil {
					return err
				}
			}
		}
		return nil
	}
	if err := walk(doc); err != nil {
		return nil, err
	}
	if len(seen) == 0 {
		return nil, fmt.Errorf("%s names no xdr_type", path)
	}
	names := make([]string, 0, len(seen))
	for n := range seen {
		names = append(names, n)
	}
	slices.Sort(names)
	return names, nil
}

// sdkDir finds the sdk source a module file resolves to, downloading it when the module cache
// does not have it yet.
func sdkDir(modfile string) (dir, version string, err error) {
	type module struct {
		Version string
		Dir     string
	}
	list := func() (module, error) {
		var m module
		outb, err := goCmd("list", "-modfile="+modfile, "-m", "-json", sdkModule)
		if err != nil {
			return m, err
		}
		return m, json.Unmarshal(outb, &m)
	}
	m, err := list()
	if err != nil {
		return "", "", fmt.Errorf("%s: %w", modfile, err)
	}
	if m.Dir == "" {
		if _, err := goCmd("mod", "download", "-modfile="+modfile, sdkModule); err != nil {
			return "", "", fmt.Errorf("%s: %w", modfile, err)
		}
		if m, err = list(); err != nil {
			return "", "", fmt.Errorf("%s: %w", modfile, err)
		}
	}
	if m.Dir == "" {
		return "", "", fmt.Errorf("%s: %s has no source directory after download", modfile, sdkModule)
	}
	return m.Dir, m.Version, nil
}

func goCmd(args ...string) ([]byte, error) {
	cmd := exec.Command("go", args...) //nolint:gosec // only list and mod download, with module file paths the generator globbed
	// -modfile cannot be combined with the repository's go.work
	cmd.Env = append(os.Environ(), "GOWORK=off")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	outb, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("go %s: %w: %s", strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return outb, nil
}

type sdkSource struct {
	version string
	// exported type names of package xdr, each with its method names
	xdrMethods map[string]map[string]bool
	// gxdr types with XdrEnumNames, which are its enums
	gxdrEnums map[string]bool
}

func readSDK(dir string) (sdkSource, error) {
	src := sdkSource{xdrMethods: map[string]map[string]bool{}, gxdrEnums: map[string]bool{}}
	if err := parseDir(filepath.Join(dir, "xdr"), func(f *ast.File) {
		collect(f, src.xdrMethods)
	}); err != nil {
		return src, err
	}
	gx := map[string]map[string]bool{}
	if err := parseDir(filepath.Join(dir, "gxdr"), func(f *ast.File) {
		collect(f, gx)
	}); err != nil {
		return src, err
	}
	for name, methods := range gx {
		if methods["XdrEnumNames"] {
			src.gxdrEnums[name] = true
		}
	}
	return src, nil
}

func parseDir(dir string, each func(*ast.File)) error {
	files, err := filepath.Glob(filepath.Join(dir, "*.go"))
	if err != nil {
		return err
	}
	fset := token.NewFileSet()
	parsed := 0
	for _, path := range files {
		if strings.HasSuffix(path, "_test.go") {
			continue
		}
		f, err := parser.ParseFile(fset, path, nil, parser.SkipObjectResolution)
		if err != nil {
			return err
		}
		each(f)
		parsed++
	}
	if parsed == 0 {
		return fmt.Errorf("no go files in %s", dir)
	}
	return nil
}

func collect(f *ast.File, types map[string]map[string]bool) {
	ensure := func(name string) map[string]bool {
		if types[name] == nil {
			types[name] = map[string]bool{}
		}
		return types[name]
	}
	for _, decl := range f.Decls {
		switch d := decl.(type) {
		case *ast.GenDecl:
			if d.Tok != token.TYPE {
				continue
			}
			for _, spec := range d.Specs {
				ts, ok := spec.(*ast.TypeSpec)
				if ok && ts.Name.IsExported() && ts.Assign == token.NoPos {
					ensure(ts.Name.Name)
				}
			}
		case *ast.FuncDecl:
			if d.Recv == nil || len(d.Recv.List) != 1 {
				continue
			}
			recv := d.Recv.List[0].Type
			if star, ok := recv.(*ast.StarExpr); ok {
				recv = star.X
			}
			if id, ok := recv.(*ast.Ident); ok && id.IsExported() {
				ensure(id.Name)[d.Name.Name] = true
			}
		}
	}
}

// mapTypes pairs each manifest name with the xdr type of that name. go-stellar-sdk's xdrgen
// and stellar-xdr spell most names alike; where they differ only in case (SCVal, ScVal) the
// one type whose name matches without case is taken.
func mapTypes(names []string, sdks []sdkSource) (map[string]string, error) {
	out := map[string]string{}
	var problems []string
	for _, name := range names {
		goName := ""
		for _, s := range sdks {
			found, err := findType(name, s)
			if err != nil {
				problems = append(problems, fmt.Sprintf("%s %s: %v", sdkModule, s.version, err))
				continue
			}
			if goName != "" && goName != found {
				problems = append(problems, fmt.Sprintf("%s is xdr.%s in one version and xdr.%s in %s", name, goName, found, s.version))
			}
			goName = found
		}
		out[name] = goName
	}
	if len(problems) > 0 {
		return nil, errors.New(strings.Join(problems, "\n"))
	}
	return out, nil
}

func findType(name string, s sdkSource) (string, error) {
	candidates := []string{}
	if _, ok := s.xdrMethods[name]; ok {
		candidates = append(candidates, name)
	} else {
		for goName := range s.xdrMethods {
			if strings.EqualFold(goName, name) {
				candidates = append(candidates, goName)
			}
		}
	}
	switch len(candidates) {
	case 0:
		return "", fmt.Errorf("package xdr has no type for %s", name)
	case 1:
	default:
		slices.Sort(candidates)
		return "", fmt.Errorf("%s matches several xdr types: %s", name, strings.Join(candidates, ", "))
	}
	methods := s.xdrMethods[candidates[0]]
	for _, m := range []string{"DecodeFrom", "MarshalBinary"} {
		if !methods[m] {
			return "", fmt.Errorf("xdr.%s has no %s method", candidates[0], m)
		}
	}
	return candidates[0], nil
}

// mapEnums pairs every xdr enum (a type with ValidEnum) with the gxdr enum of the same xdr name.
// gxdr spells types as the .x files do (SCValType) and xdr camel-cases them (ScValType), so the
// match ignores case and must be unique.
func mapEnums(sdks []sdkSource) (map[string]string, error) {
	out := map[string]string{}
	var problems []string
	for i, s := range sdks {
		names := map[string]string{}
		for goName, methods := range s.xdrMethods {
			if !methods["ValidEnum"] {
				continue
			}
			var matches []string
			for g := range s.gxdrEnums {
				if strings.EqualFold(g, goName) {
					matches = append(matches, g)
				}
			}
			if len(matches) != 1 {
				problems = append(problems, fmt.Sprintf("%s %s: xdr.%s matches gxdr enums %v", sdkModule, s.version, goName, matches))
				continue
			}
			names[goName] = matches[0]
		}
		if i == 0 {
			out = names
			continue
		}
		// keep only enums every version has, so the table compiles against each of them
		for goName, g := range out {
			if names[goName] != g {
				delete(out, goName)
			}
		}
	}
	if len(problems) > 0 {
		return nil, errors.New(strings.Join(problems, "\n"))
	}
	if len(out) == 0 {
		return nil, errors.New("no xdr enum has a gxdr counterpart")
	}
	return out, nil
}

func render(types, enums map[string]string, sdks []sdkSource) ([]byte, error) {
	var b bytes.Buffer
	versions := make([]string, 0, len(sdks))
	for _, s := range sdks {
		versions = append(versions, s.version)
	}
	fmt.Fprintf(&b, "// Code generated by internal/gentypes from corpus/manifest.json and %s %s. DO NOT EDIT.\n\n", sdkModule, strings.Join(versions, ", "))
	b.WriteString("package main\n\n")
	b.WriteString("import (\n\t\"reflect\"\n\n\t\"github.com/stellar/go-stellar-sdk/gxdr\"\n\t\"github.com/stellar/go-stellar-sdk/xdr\"\n)\n\n")

	b.WriteString("// xdrTypes maps every xdr type name in the corpus manifest to a new value of the sdk type.\n")
	b.WriteString("var xdrTypes = map[string]func() any{\n")
	for _, name := range sortedKeys(types) {
		fmt.Fprintf(&b, "\t%q: func() any { return new(xdr.%s) },\n", name, types[name])
	}
	b.WriteString("}\n\n")

	b.WriteString("// enumIdentifiers maps every xdr enum to the xdr identifiers of its values, as gxdr has them.\n")
	b.WriteString("var enumIdentifiers = map[reflect.Type]map[int32]string{\n")
	for _, goName := range sortedKeys(enums) {
		fmt.Fprintf(&b, "\treflect.TypeFor[xdr.%s](): gxdr.%s(0).XdrEnumNames(),\n", goName, enums[goName])
	}
	b.WriteString("}\n")

	return format.Source(b.Bytes())
}

func sortedKeys(m map[string]string) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	slices.Sort(keys)
	return keys
}
