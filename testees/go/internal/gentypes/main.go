// Command gentypes writes tables_gen.go for the go testee: the go-stellar-sdk xdr type behind
// every type the xdr schema defines (schema/xdr/<tag>/xdr.json) and every type the corpus
// manifest uses, and the gxdr enum that carries the xdr identifiers of every xdr enum.
//
// The tables are read from the sdk source of every version in versions/*.mod. Types every
// version has go into tables_gen.go, which every build compiles. A type only some versions
// have goes into tables_gen_<version>.go, behind the build tag shapekit_sdk_<version> (dots as
// underscores) that the version's build in conformance/versions.toml sets, so each build
// decodes every type its own sdk version has. A manifest type that some version lacks stops
// the generator, because the runner would then send a type the testee cannot answer; a schema
// type no version has is listed in the generated file.
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
	extra := flag.String("extra", "", "comma separated xdr types to add to the manifest's")
	schema := flag.String("schema", "", "xdr.json of the schema whose every type the table covers")
	flag.Parse()

	if err := run(*manifest, *schema, *versions, *out, *extra); err != nil {
		fmt.Fprintln(os.Stderr, "gentypes:", err)
		os.Exit(1)
	}
}

func run(manifestPath, schemaPath, versionsDir, out, extra string) error {
	required, err := manifestTypes(manifestPath)
	if err != nil {
		return err
	}
	for name := range strings.SplitSeq(extra, ",") {
		if name = strings.TrimSpace(name); name != "" && !slices.Contains(required, name) {
			required = append(required, name)
		}
	}
	optional := []string{}
	if schemaPath != "" {
		if optional, err = schemaTypes(schemaPath); err != nil {
			return err
		}
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

	types, err := mapTypes(required, optional, sdks)
	if err != nil {
		return err
	}
	enums, err := mapEnums(sdks)
	if err != nil {
		return err
	}

	files, err := render(types, enums, sdks, schemaPath)
	if err != nil {
		return err
	}
	// stale per-version files of versions no longer in versions/ go away
	stale, err := filepath.Glob(filepath.Join(filepath.Dir(out), "tables_gen_*.go"))
	if err != nil {
		return err
	}
	for _, f := range stale {
		if _, keep := files[filepath.Base(f)]; !keep {
			if err := os.Remove(f); err != nil {
				return err
			}
		}
	}
	for name, code := range files {
		path := filepath.Join(filepath.Dir(out), name)
		if name == "tables_gen.go" {
			path = out
		}
		if err := os.WriteFile(path, code, 0o644); err != nil { //nolint:gosec // generated source is meant to be world readable
			return err
		}
	}
	return nil
}

// schemaTypes lists the struct, union, enum and typedef names of an xdr.json schema.
func schemaTypes(path string) ([]string, error) {
	data, err := os.ReadFile(path) //nolint:gosec // the path is the generator's own flag
	if err != nil {
		return nil, err
	}
	var doc struct {
		Definitions []struct {
			Kind string `json:"kind"`
			Name string `json:"name"`
		} `json:"definitions"`
	}
	if err := json.Unmarshal(data, &doc); err != nil {
		return nil, fmt.Errorf("%s: %w", path, err)
	}
	var out []string
	for _, d := range doc.Definitions {
		switch d.Kind {
		case "struct", "union", "enum", "typedef":
			out = append(out, d.Name)
		}
	}
	if len(out) == 0 {
		return nil, fmt.Errorf("%s defines no type", path)
	}
	slices.Sort(out)
	return out, nil
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
// typeTable is where each xdr type name lives: in every version, in some, or in none.
type typeTable struct {
	// common maps names every version has to the go type name
	common map[string]string
	// only maps a version to the names only some versions have, and their go type names
	only map[string]map[string]string
	// absent lists schema names no version has
	absent []string
}

func mapTypes(required, optional []string, sdks []sdkSource) (typeTable, error) {
	t := typeTable{common: map[string]string{}, only: map[string]map[string]string{}}
	var problems []string
	names := slices.Clone(required)
	for _, n := range optional {
		if !slices.Contains(names, n) {
			names = append(names, n)
		}
	}
	slices.Sort(names)
	for _, name := range names {
		goName := ""
		var have []string
		for _, s := range sdks {
			found, err := findType(name, s)
			if errors.Is(err, errNoType) {
				continue
			}
			if err != nil {
				problems = append(problems, fmt.Sprintf("%s %s: %v", sdkModule, s.version, err))
				continue
			}
			if goName != "" && goName != found {
				problems = append(problems, fmt.Sprintf("%s is xdr.%s in one version and xdr.%s in %s", name, goName, found, s.version))
			}
			goName = found
			have = append(have, s.version)
		}
		// the table is keyed by the name the testee protocol uses, the stellar-xdr crate's
		key := rustName(goName)
		switch {
		case len(have) == len(sdks):
			t.common[key] = goName
		case slices.Contains(required, name):
			problems = append(problems, fmt.Sprintf("the manifest uses %s, but only %v of the sdk versions have it", name, have))
		case len(have) == 0:
			t.absent = append(t.absent, name)
		default:
			for _, v := range have {
				if t.only[v] == nil {
					t.only[v] = map[string]string{}
				}
				t.only[v][key] = goName
			}
		}
	}
	if len(problems) > 0 {
		return t, errors.New(strings.Join(problems, "\n"))
	}
	return t, nil
}

var errNoType = errors.New("package xdr has no such type")

// rustName is the stellar-xdr crate's name for a go-stellar-sdk xdr type. Both generators
// camel-case the .x names the same way (SCPQuorumSet is ScpQuorumSet in both); the one
// difference is the overlay's Error, which stellar-xdr renames SError. A test of the
// conformance crate checks the table's names against the crate's own list.
func rustName(goName string) string {
	if goName == "Error" {
		return "SError"
	}
	return goName
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
		return "", fmt.Errorf("%w: %s", errNoType, name)
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

// buildTag is the tag a version's build sets: shapekit_sdk_v0_9_0 for v0.9.0.
func buildTag(version string) string {
	return "shapekit_sdk_" + strings.NewReplacer(".", "_", "-", "_", "+", "_").Replace(version)
}

func render(t typeTable, enums map[string]string, sdks []sdkSource, schemaPath string) (map[string][]byte, error) {
	files := map[string][]byte{}
	versions := make([]string, 0, len(sdks))
	for _, s := range sdks {
		versions = append(versions, s.version)
	}
	source := "corpus/manifest.json"
	if schemaPath != "" {
		source += ", " + filepath.ToSlash(strings.TrimPrefix(schemaPath, "../../"))
	}
	header := fmt.Sprintf("// Code generated by internal/gentypes from %s and %s %s. DO NOT EDIT.\n\n", source, sdkModule, strings.Join(versions, ", "))

	var b bytes.Buffer
	b.WriteString(header)
	b.WriteString("package main\n\n")
	b.WriteString("import (\n\t\"reflect\"\n\n\t\"github.com/stellar/go-stellar-sdk/gxdr\"\n\t\"github.com/stellar/go-stellar-sdk/xdr\"\n)\n\n")
	b.WriteString("// xdrTypes maps every xdr type name that every sdk version has to a new value of the sdk type;\n")
	b.WriteString("// tables_gen_<version>.go adds the types only some versions have.\n")
	b.WriteString("var xdrTypes = map[string]func() any{\n")
	for _, name := range sortedKeys(t.common) {
		fmt.Fprintf(&b, "\t%q: func() any { return new(xdr.%s) },\n", name, t.common[name])
	}
	b.WriteString("}\n\n")
	b.WriteString("// absentTypes are schema types no sdk version has a go type for.\n")
	b.WriteString("var absentTypes = []string{")
	for i, name := range t.absent {
		if i > 0 {
			b.WriteString(", ")
		}
		fmt.Fprintf(&b, "%q", name)
	}
	b.WriteString("}\n\n")
	b.WriteString("// enumIdentifiers maps every xdr enum to the xdr identifiers of its values, as gxdr has them.\n")
	b.WriteString("var enumIdentifiers = map[reflect.Type]map[int32]string{\n")
	for _, goName := range sortedKeys(enums) {
		fmt.Fprintf(&b, "\treflect.TypeFor[xdr.%s](): gxdr.%s(0).XdrEnumNames(),\n", goName, enums[goName])
	}
	b.WriteString("}\n")
	code, err := format.Source(b.Bytes())
	if err != nil {
		return nil, err
	}
	files["tables_gen.go"] = code

	for _, v := range versions {
		only := t.only[v]
		if len(only) == 0 {
			continue
		}
		var f bytes.Buffer
		fmt.Fprintf(&f, "//go:build %s\n\n", buildTag(v))
		f.WriteString(header)
		f.WriteString("package main\n\n")
		f.WriteString("import \"github.com/stellar/go-stellar-sdk/xdr\"\n\n")
		fmt.Fprintf(&f, "// the types %s %s has that not every version has\n", sdkModule, v)
		f.WriteString("func init() {\n")
		for _, name := range sortedKeys(only) {
			fmt.Fprintf(&f, "\txdrTypes[%q] = func() any { return new(xdr.%s) }\n", name, only[name])
		}
		f.WriteString("}\n")
		code, err := format.Source(f.Bytes())
		if err != nil {
			return nil, err
		}
		files["tables_gen_"+strings.TrimPrefix(buildTag(v), "shapekit_sdk_")+".go"] = code
	}
	return files, nil
}

func sortedKeys(m map[string]string) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	slices.Sort(keys)
	return keys
}
