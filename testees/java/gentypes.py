"""writes src/main/java/org/shapekit/testee/TypesGen.java, the java testee's xdr type table.

the table maps the name the testee protocol uses for an xdr type (the stellar-xdr crate's, as
crates/shapekit-schema rust_type_name derives it) to the java-stellar-sdk class of that type,
for every struct, union, enum and typedef of the xdr schema and every xdr_type of the corpus
manifest. the classes come from the sdk jar of every version under versions/: types every
version has go into one table, types only some versions have into a table per version, and
schema types no version has into a list. a manifest type that some version lacks stops the
generator, because the runner would send a type the testee cannot answer.

run with python3 -I from testees/java after the jars are in the local maven repository (any
build of the versions does that):

    python3 -I gentypes.py

a development build of the sdk (docs/sdk-ci.md) reports a version this table may not know, so
its build writes a table of its own, for that one version and jar:

    python3 -I gentypes.py --version <version> --jar <sdk jar> --out <file>
"""

import argparse
import json
import os
import struct
import sys
import zipfile

SDK_JAR = "~/.m2/repository/network/lightsail/stellar-sdk/{v}/stellar-sdk-{v}.jar"
XDR_PREFIX = "org/stellar/sdk/xdr/"
DECODE = "(Lorg/stellar/sdk/xdr/XdrDataInputStream;I)"
ENCODE = "(Lorg/stellar/sdk/xdr/XdrDataOutputStream;)V"
ACC_PUBLIC = 0x0001
ACC_STATIC = 0x0008


def heck_words(s):
    """the words of heck 0.5's transform (heck/src/lib.rs), which rs-stellar-xdr's generator
    uses to camel-case xdr names"""
    words = []
    for word in "".join(c if c.isalnum() else " " for c in s).split(" "):
        init = 0
        mode = "boundary"
        i = 0
        while i < len(word):
            c = word[i]
            if i + 1 < len(word):
                nxt = word[i + 1]
                if c.islower():
                    next_mode = "lower"
                elif c.isupper():
                    next_mode = "upper"
                else:
                    next_mode = mode
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
                break
            i += 1
    return words


def rust_type_name(xdr):
    """crates/shapekit-schema/src/names.rs rust_type_name: heck's upper camel case after the
    generator's renames"""
    escaped = {"type": "type_", "Error": "SError"}.get(xdr, xdr)
    return "".join(w[:1].upper() + w[1:].lower() for w in heck_words(escaped) if w)


def class_methods(data):
    """the (access flags, name, descriptor) of every method a class file declares"""
    if data[:4] != b"\xca\xfe\xba\xbe":
        raise ValueError("not a class file")
    count = struct.unpack_from(">H", data, 8)[0]
    pool = [None] * count
    pos = 10
    i = 1
    while i < count:
        tag = data[pos]
        if tag == 1:
            n = struct.unpack_from(">H", data, pos + 1)[0]
            pool[i] = data[pos + 3 : pos + 3 + n].decode("utf-8", "surrogatepass")
            pos += 3 + n
        elif tag in (7, 8, 16, 19, 20):
            pos += 3
        elif tag == 15:
            pos += 4
        elif tag in (3, 4, 9, 10, 11, 12, 17, 18):
            pos += 5
        elif tag in (5, 6):
            pos += 9
            i += 1
        else:
            raise ValueError(f"constant pool tag {tag}")
        i += 1
    pos += 6
    interfaces = struct.unpack_from(">H", data, pos)[0]
    pos += 2 + 2 * interfaces

    def members(pos):
        out = []
        n = struct.unpack_from(">H", data, pos)[0]
        pos += 2
        for _ in range(n):
            flags, name, desc, attrs = struct.unpack_from(">HHHH", data, pos)
            pos += 8
            for _ in range(attrs):
                length = struct.unpack_from(">I", data, pos + 2)[0]
                pos += 6 + length
            out.append((flags, pool[name], pool[desc]))
        return out, pos

    _, pos = members(pos)
    methods, _ = members(pos)
    return methods


def sdk_types(jar):
    """simple class name, lowercased, to the binary name of every xdr class with the sdk's
    decode(XdrDataInputStream, int) and encode(XdrDataOutputStream)"""
    out = {}
    with zipfile.ZipFile(jar) as z:
        for entry in z.namelist():
            if not (entry.startswith(XDR_PREFIX) and entry.endswith(".class")):
                continue
            binary = entry[len(XDR_PREFIX) : -len(".class")]
            simple = binary.split("$")[-1]
            methods = class_methods(z.read(entry))
            decodes = any(
                n == "decode" and d.startswith(DECODE) and f & ACC_STATIC and f & ACC_PUBLIC
                for f, n, d in methods
            )
            encodes = any(n == "encode" and d == ENCODE and f & ACC_PUBLIC for f, n, d in methods)
            if not (decodes and encodes):
                continue
            key = simple.lower()
            if key in out:
                raise SystemExit(f"{jar}: {binary} and {out[key]} differ only in case")
            out[key] = binary
    if not out:
        raise SystemExit(f"{jar} has no xdr classes")
    return out


def manifest_types(path):
    seen = set()

    def walk(v):
        if isinstance(v, dict):
            for k, child in v.items():
                if k == "xdr_type":
                    if not isinstance(child, str):
                        raise SystemExit(f"{path}: xdr_type {child!r} is not a string")
                    seen.add(child)
                else:
                    walk(child)
        elif isinstance(v, list):
            for child in v:
                walk(child)

    with open(path, encoding="utf-8") as f:
        walk(json.load(f))
    if not seen:
        raise SystemExit(f"{path} names no xdr_type")
    return seen


def java_string(s):
    return json.dumps(s)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--schema", default="../../schema/xdr/v30.0/xdr.json")
    p.add_argument("--manifest", default="../../corpus/manifest.json")
    p.add_argument("--versions", default="versions")
    p.add_argument("--extra", default="TransactionMeta")
    p.add_argument("--version", help="write the table of this one version, read from --jar, instead of versions/")
    p.add_argument("--jar", help="the sdk jar of the --version build")
    p.add_argument("--out", default="src/main/java/org/shapekit/testee/TypesGen.java")
    args = p.parse_args()
    if (args.version is None) != (args.jar is None):
        raise SystemExit("--version and --jar go together")

    with open(args.schema, encoding="utf-8") as f:
        schema = json.load(f)
    names = sorted(
        d["name"] for d in schema["definitions"] if d["kind"] in ("struct", "union", "enum", "typedef")
    )
    if not names:
        raise SystemExit(f"{args.schema} defines no type")
    by_rust = {}
    for n in names:
        r = rust_type_name(n)
        if r in by_rust:
            raise SystemExit(f"{n} and {by_rust[r]} have the same rust name {r}")
        by_rust[r] = n
    required = manifest_types(args.manifest) | {e for e in args.extra.split(",") if e}
    unknown = sorted(required - set(by_rust))
    if unknown:
        raise SystemExit(f"the manifest names types the schema does not define: {unknown}")

    if args.version is not None:
        versions = [args.version]
        jars = {args.version: args.jar}
    else:
        versions = sorted(os.listdir(args.versions), key=lambda v: tuple(int(x) for x in v.split(".")))
        jars = {v: os.path.expanduser(SDK_JAR.format(v=v)) for v in versions}
    if not versions:
        raise SystemExit(f"no versions in {args.versions}")
    tables = {}
    for v in versions:
        jar = jars[v]
        if not os.path.isfile(jar):
            raise SystemExit(f"{jar} is missing; build versions/{v}/pom.xml first")
        tables[v] = sdk_types(jar)

    common, only, absent, problems = {}, {v: {} for v in versions}, [], []
    for rust in sorted(by_rust):
        xdr = by_rust[rust]
        found = {v: tables[v][xdr.lower()] for v in versions if xdr.lower() in tables[v]}
        if len(set(found.values())) > 1:
            problems.append(f"{rust} is {sorted(set(found.values()))} across versions")
        if len(found) == len(versions):
            common[rust] = next(iter(found.values()))
        elif rust in required:
            problems.append(f"the manifest uses {rust}, but only {sorted(found)} of the sdk versions have it")
        elif not found:
            absent.append(rust)
        else:
            for v, binary in found.items():
                only[v][rust] = binary
    if problems:
        raise SystemExit("\n".join(problems))

    lines = [
        "// Code generated by testees/java/gentypes.py from "
        + args.schema.replace("../../", "")
        + ", "
        + args.manifest.replace("../../", "")
        + " and network.lightsail:stellar-sdk "
        + ", ".join(versions)
        + ". DO NOT EDIT.",
        "",
        "package org.shapekit.testee;",
        "",
        "import java.util.List;",
        "import java.util.Map;",
        "",
        "/** the xdr types of the sdk, keyed by the stellar-xdr crate's type names. */",
        "final class TypesGen {",
        "  private TypesGen() {}",
        "",
        "  /** binary names under org.stellar.sdk.xdr of the types every sdk version has. */",
        "  static final Map<String, String> COMMON =",
        "      Map.ofEntries(",
    ]
    entries = [f"          Map.entry({java_string(k)}, {java_string(v)})" for k, v in sorted(common.items())]
    lines.append(",\n".join(entries) + ");")
    lines += ["", "  /** the types only some sdk versions have, per version. */", "  static final Map<String, Map<String, String>> ONLY ="]
    per = []
    for v in versions:
        inner = ", ".join(f"Map.entry({java_string(k)}, {java_string(b)})" for k, b in sorted(only[v].items()))
        per.append(f"          Map.entry({java_string(v)}, Map.ofEntries({inner}))")
    lines.append("      Map.ofEntries(\n" + ",\n".join(per) + ");")
    lines += [
        "",
        "  /** schema types no sdk version has a class for. */",
        "  static final List<String> ABSENT = List.of(" + ", ".join(java_string(a) for a in absent) + ");",
        "}",
        "",
    ]
    with open(args.out, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"{args.out}: {len(common)} common types, {', '.join(f'{v} +{len(only[v])}' for v in versions)}, {len(absent)} absent")


if __name__ == "__main__":
    sys.exit(main())
