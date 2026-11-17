"""writes src/main/kotlin/org/shapekit/testee/TypesGen.kt, the kmp testee's xdr type table.

the table maps the name the testee protocol uses for an xdr type (the stellar-xdr crate's, as
crates/shapekit-schema rust_type_name derives it) to the class of that type in
com.soneso.stellar:stellar-sdk-jvm, for every struct, union, enum and typedef of the xdr schema
and every xdr_type of the corpus manifest. a class counts when its companion has
decode(XdrReader) and fromXdrJsonElement(JsonElement). types every version under versions/ has
go into one table, types only some versions have into a table per version, and schema types no
version has into a list. a manifest type some version lacks stops the generator, because the
runner would send a type the testee cannot answer.

the name rules are the java testee's (testees/java/gentypes.py). run
with python3 -I from testees/kmp after the jars are in the local maven repository (any build of
the versions does that):

    python3 -I gentypes.py
"""

import argparse
import json
import os
import struct
import sys
import zipfile

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "java"))
import gentypes as java  # noqa: E402

SDK_JAR = "~/.m2/repository/com/soneso/stellar/stellar-sdk-jvm/{v}/stellar-sdk-jvm-{v}.jar"
XDR_PREFIX = "com/soneso/stellar/sdk/xdr/"
DECODE = "(Lcom/soneso/stellar/sdk/xdr/XdrReader;)"
FROM_JSON = "(Lkotlinx/serialization/json/JsonElement;)"


def class_methods(data):
    """the (access flags, name, descriptor) of every method a class file declares. kotlin's
    metadata strings are modified utf-8 (a nul is c0 80), which python does not decode, so bytes
    that are not utf-8 are replaced: no name or descriptor read here has any"""
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
            pool[i] = data[pos + 3 : pos + 3 + n].decode("utf-8", "replace")
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


def named(method, name):
    """a typedef's class is a kotlin value class, whose functions taking or returning it carry a
    mangled jvm name: decode-_jjEnWU is decode"""
    return method == name or method.startswith(name + "-")


def sdk_types(jar):
    """xdr name, lowercased, to the simple name of every <Name>Xdr class whose companion
    decodes from an XdrReader and reads XDR-JSON"""
    out = {}
    with zipfile.ZipFile(jar) as z:
        entries = set(z.namelist())
        for entry in sorted(entries):
            if not (entry.startswith(XDR_PREFIX) and entry.endswith("Xdr$Companion.class")):
                continue
            simple = entry[len(XDR_PREFIX) : -len("$Companion.class")]
            if "/" in simple or "$" in simple:
                continue
            methods = class_methods(z.read(entry))
            decodes = any(named(n, "decode") and d.startswith(DECODE) for _, n, d in methods)
            reads = any(named(n, "fromXdrJsonElement") and d.startswith(FROM_JSON) for _, n, d in methods)
            if not (decodes and reads):
                continue
            key = simple[: -len("Xdr")].lower()
            if key in out:
                raise SystemExit(f"{jar}: {simple} and {out[key]} differ only in case")
            out[key] = simple
    if not out:
        raise SystemExit(f"{jar} has no xdr classes")
    return out


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--schema", default="../../schema/xdr/v30.0/xdr.json")
    p.add_argument("--manifest", default="../../corpus/manifest.json")
    p.add_argument("--versions", default="versions")
    p.add_argument("--extra", default="TransactionMeta")
    p.add_argument("--out", default="src/main/kotlin/org/shapekit/testee/TypesGen.kt")
    args = p.parse_args()

    with open(args.schema, encoding="utf-8") as f:
        schema = json.load(f)
    names = sorted(
        d["name"] for d in schema["definitions"] if d["kind"] in ("struct", "union", "enum", "typedef")
    )
    if not names:
        raise SystemExit(f"{args.schema} defines no type")
    by_rust = {}
    for n in names:
        r = java.rust_type_name(n)
        if r in by_rust:
            raise SystemExit(f"{n} and {by_rust[r]} have the same rust name {r}")
        by_rust[r] = n
    required = java.manifest_types(args.manifest) | {e for e in args.extra.split(",") if e}
    unknown = sorted(required - set(by_rust))
    if unknown:
        raise SystemExit(f"the manifest names types the schema does not define: {unknown}")

    versions = sorted(os.listdir(args.versions), key=lambda v: tuple(int(x) for x in v.split(".")))
    if not versions:
        raise SystemExit(f"no versions in {args.versions}")
    tables = {}
    for v in versions:
        jar = os.path.expanduser(SDK_JAR.format(v=v))
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
            for v, simple in found.items():
                only[v][rust] = simple
    if problems:
        raise SystemExit("\n".join(problems))

    q = json.dumps
    lines = [
        "// Code generated by testees/kmp/gentypes.py from "
        + args.schema.replace("../../", "")
        + ", "
        + args.manifest.replace("../../", "")
        + " and com.soneso.stellar:stellar-sdk-jvm "
        + ", ".join(versions)
        + ". DO NOT EDIT.",
        "",
        "package org.shapekit.testee",
        "",
        "/** The xdr types of the sdk, keyed by the stellar-xdr crate's type names. */",
        "object TypesGen {",
        "    /** Simple names under com.soneso.stellar.sdk.xdr of the types every sdk version has. */",
        "    val COMMON: Map<String, String> = mapOf(",
    ]
    lines += [f"        {q(k)} to {q(v)}," for k, v in sorted(common.items())]
    lines += ["    )", "", "    /** The types only some sdk versions have, per version. */", "    val ONLY: Map<String, Map<String, String>> = mapOf("]
    for v in versions:
        inner = ", ".join(f"{q(k)} to {q(b)}" for k, b in sorted(only[v].items()))
        lines.append(f"        {q(v)} to mapOf({inner}),")
    lines += [
        "    )",
        "",
        "    /** Schema types no sdk version has a class for. */",
        "    val ABSENT: List<String> = listOf(" + ", ".join(q(a) for a in absent) + ")",
        "}",
        "",
    ]
    with open(args.out, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"{args.out}: {len(common)} common types, {', '.join(f'{v} +{len(only[v])}' for v in versions)}, {len(absent)} absent")


if __name__ == "__main__":
    sys.exit(main())
