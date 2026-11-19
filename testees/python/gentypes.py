"""writes shapekit_testee/types_gen.py: the stellar_sdk.xdr class behind every type the xdr
schema defines (schema/xdr/<tag>/xdr.json) and every type the corpus manifest uses, for every
sdk version in versions/.

the classes are read from the sdk source each version's venv holds (the build in
conformance/versions.toml makes target/testees/python/<version>/venv), parsed, never imported.
the table is keyed by the stellar-xdr crate's type names, which the testee protocol uses:
heck's upper camel case of the .x name, as crates/shapekit-schema derives it, with the overlay
Error renamed SError. types every version has go into COMMON, the others into ONLY under the
versions that have them. a manifest type some version lacks stops the generator, because the
runner would then send a type the testee cannot answer; schema types no version has are
listed in ABSENT. crates/shapekit-conformance/tests/python_type_table.rs checks the keys
against the crate's own list.

run from the repository root:
    python3 -I testees/python/gentypes.py

a development build of the sdk (docs/sdk-ci.md) reports a version this table may not know, so
its build writes a table of its own, for that one version and venv:
    python3 -I testees/python/gentypes.py --version <version> --venv <venv> --out <file>
"""

import argparse
import ast
import json
import sys
from pathlib import Path


def heck_words(s: str) -> list[str]:
    """heck 0.5's word split (transform in heck/src/lib.rs)."""
    words = []
    for word in "".join(c if c.isalnum() else " " for c in s).split(" "):
        init = 0
        mode = "boundary"
        for i, c in enumerate(word):
            if i + 1 < len(word):
                nxt = word[i + 1]
                next_mode = "lower" if c.islower() else "upper" if c.isupper() else mode
                if next_mode == "lower" and nxt.isupper():
                    words.append(word[init : i + 1])
                    init = i + 1
                    mode = "boundary"
                elif mode == "upper" and c.isupper() and nxt.islower():
                    words.append(word[init:i])
                    init = i
                    mode = "boundary"
                else:
                    mode = next_mode
            else:
                words.append(word[init:])
    return [w for w in words if w]


def rust_type_name(xdr_name: str) -> str:
    """shapekit_schema::names::rust_type_name: heck's to_upper_camel_case of the .x name."""
    escaped = {"type": "type_", "Error": "SError"}.get(xdr_name, xdr_name)
    return "".join(w[:1].upper() + w[1:].lower() for w in heck_words(escaped))


def schema_types(path: Path) -> list[str]:
    doc = json.loads(path.read_text())
    names = sorted(d["name"] for d in doc["definitions"] if d["kind"] in ("struct", "union", "enum", "typedef"))
    if not names:
        sys.exit(f"gentypes: {path} defines no type")
    return names


def manifest_types(path: Path) -> list[str]:
    seen = set()

    def walk(v: object) -> None:
        if isinstance(v, dict):
            for k, child in v.items():
                if k == "xdr_type":
                    if not isinstance(child, str):
                        sys.exit(f"gentypes: {path}: xdr_type {child!r} is not a string")
                    seen.add(child)
                else:
                    walk(child)
        elif isinstance(v, list):
            for child in v:
                walk(child)

    walk(json.loads(path.read_text()))
    if not seen:
        sys.exit(f"gentypes: {path} names no xdr_type")
    return sorted(seen)


def sdk_classes(xdr_dir: Path) -> set[str]:
    """top-level classes of stellar_sdk/xdr that decode and encode xdr bytes."""
    out = set()
    for f in sorted(xdr_dir.glob("*.py")):
        tree = ast.parse(f.read_text(), str(f))
        for node in tree.body:
            if isinstance(node, ast.ClassDef):
                methods = {n.name for n in node.body if isinstance(n, ast.FunctionDef)}
                if {"from_xdr_bytes", "to_xdr_bytes", "from_json", "to_json"} <= methods:
                    out.add(node.name)
    if not out:
        sys.exit(f"gentypes: no xdr classes in {xdr_dir}")
    return out


def find(name: str, classes: set[str]) -> str | None:
    if name in classes:
        return name
    matches = sorted(c for c in classes if c.lower() == name.lower())
    if len(matches) > 1:
        sys.exit(f"gentypes: {name} matches several xdr classes: {', '.join(matches)}")
    return matches[0] if matches else None


def version_key(v: str) -> list[int]:
    return [int(p) for p in v.split(".")]


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--manifest", default="corpus/manifest.json")
    p.add_argument("--schema", default="schema/xdr/v30.0/xdr.json")
    p.add_argument("--versions", default="testees/python/versions")
    p.add_argument("--venvs", default="target/testees/python")
    p.add_argument("--extra", default="TransactionMeta", help="comma separated types to add to the manifest's")
    p.add_argument("--out", default="testees/python/shapekit_testee/types_gen.py")
    p.add_argument("--version", help="write the table of this one version, read from --venv, instead of versions/")
    p.add_argument("--venv", help="the venv holding the --version build")
    args = p.parse_args()
    if (args.version is None) != (args.venv is None):
        sys.exit("gentypes: --version and --venv go together")

    required = manifest_types(Path(args.manifest))
    required += [n for n in args.extra.split(",") if n and n not in required]
    names = sorted(set(required) | set(schema_types(Path(args.schema))))

    if args.version is not None:
        versions = [args.version]
        venvs = {args.version: Path(args.venv)}
    else:
        versions = sorted((d.name for d in Path(args.versions).iterdir() if d.is_dir()), key=version_key)
        venvs = {v: Path(args.venvs, v, "venv") for v in versions}
    if not versions:
        sys.exit(f"gentypes: no versions in {args.versions}")
    classes = {}
    for v in versions:
        dirs = list(venvs[v].glob("lib/python*/site-packages/stellar_sdk/xdr"))
        if len(dirs) != 1:
            sys.exit(f"gentypes: build the {v} testee first; {venvs[v]} holds {len(dirs)} stellar_sdk/xdr")
        classes[v] = sdk_classes(dirs[0])

    common: dict[str, str] = {}
    only: dict[str, dict[str, str]] = {}
    absent: list[str] = []
    problems = []
    for name in names:
        found = {v: find(name, classes[v]) for v in versions}
        have = [v for v in versions if found[v]]
        spelled = {found[v] for v in have}
        if len(spelled) > 1:
            problems.append(f"{name} is spelled {sorted(spelled)} across versions")
            continue
        key = rust_type_name(name)
        if len(have) == len(versions):
            common[key] = found[versions[0]]
        elif name in required:
            problems.append(f"the manifest uses {name}, but only {have} of the sdk versions have it")
        elif not have:
            absent.append(key)
        else:
            for v in have:
                only.setdefault(v, {})[key] = found[v]
    if problems:
        sys.exit("gentypes: " + "\n".join(problems))

    lines = [
        f"# generated by testees/python/gentypes.py from {args.manifest}, {args.schema} and "
        f"stellar-sdk {', '.join(versions)}. do not edit.",
        "",
        f"VERSIONS = {versions!r}",
        "",
        "# the stellar-xdr crate's name of every xdr type every sdk version has, and the stellar_sdk.xdr class",
        "COMMON = {",
        *(f"    {k!r}: {v!r}," for k, v in sorted(common.items())),
        "}",
        "",
        "# the types only some sdk versions have",
        "ONLY: dict[str, dict[str, str]] = {",
    ]
    for v in versions:
        if only.get(v):
            lines.append(f"    {v!r}: {{")
            lines += [f"        {k!r}: {c!r}," for k, c in sorted(only[v].items())]
            lines.append("    },")
    lines += [
        "}",
        "",
        "# schema types no sdk version has a class for",
        f"ABSENT = {sorted(absent)!r}",
        "",
    ]
    Path(args.out).write_text("\n".join(lines).replace("'", '"'))


if __name__ == "__main__":
    main()
