#!/usr/bin/env bash
# prints one value from pins.toml, so workflows never repeat a pinned version.
#
# usage: scripts/pin.sh image <name>    repository@digest of an [[image]]
#        scripts/pin.sh git <name>      commit sha of a [[git]] entry
#        scripts/pin.sh tag <name>      tag of a [[git]] entry (ref without refs/tags/)
#        scripts/pin.sh release <name>  "<version> <commit> <asset url> <sha256>" of a [[release]]
#        scripts/pin.sh maven <artifact> "<version> <file url> <sha256>" of the one [[maven]] pin
#                                       of that artifact
#        scripts/pin.sh toolchain <name> "<version> <artifact> <sha256>" of a [[toolchain]]
#
# needs python3 >= 3.11 for tomllib.
set -euo pipefail

kind=${1:?usage: scripts/pin.sh image|git|tag|release|maven|toolchain <name>}
name=${2:?usage: scripts/pin.sh image|git|tag|release|maven|toolchain <name>}

python3 -I - "$(git rev-parse --show-toplevel)/pins.toml" "$kind" "$name" <<'PY'
import sys, tomllib

path, kind, name = sys.argv[1:]
with open(path, "rb") as f:
    pins = tomllib.load(f)

table = {
    "image": "image",
    "git": "git",
    "tag": "git",
    "release": "release",
    "maven": "maven",
    "toolchain": "toolchain",
}.get(kind)
if table is None:
    sys.exit(f"unknown kind {kind}, expected image, git, tag, release, maven or toolchain")
key = "artifact" if kind == "maven" else "name"
found = [e for e in pins.get(table, []) if e.get(key) == name]
if len(found) != 1:
    sys.exit(f"pins.toml has {len(found)} [[{table}]] entries named {name}")
e = found[0]
if kind == "image":
    print(f'{e["repository"]}@{e["digest"]}')
elif kind == "git":
    print(e["sha"])
elif kind == "release":
    tag = e["tag"]
    url = f'https://github.com/{e["repository"]}/releases/download/{tag.replace("+", "%2B")}/{e["asset"]}'
    print(tag.removeprefix("v"), e["commit"], url, e["sha256"])
elif kind == "maven":
    group = e["group"].replace(".", "/")
    url = f'https://repo1.maven.org/maven2/{group}/{e["artifact"]}/{e["version"]}/{e["file"]}'
    print(e["version"], url, e["sha256"])
elif kind == "toolchain":
    print(e["version"], e["artifact"], e["sha256"])
else:
    ref = e.get("ref", "")
    if not ref.startswith("refs/tags/"):
        sys.exit(f"{name} is not pinned to a tag")
    print(ref.removeprefix("refs/tags/"))
PY
