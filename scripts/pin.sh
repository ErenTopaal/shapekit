#!/usr/bin/env bash
# prints one value from pins.toml, so workflows never repeat a pinned version.
#
# usage: scripts/pin.sh image <name>    repository@digest of an [[image]]
#        scripts/pin.sh git <name>      commit sha of a [[git]] entry
#        scripts/pin.sh tag <name>      tag of a [[git]] entry (ref without refs/tags/)
#
# needs python3 >= 3.11 for tomllib.
set -euo pipefail

kind=${1:?usage: scripts/pin.sh image|git|tag <name>}
name=${2:?usage: scripts/pin.sh image|git|tag <name>}

python3 -I - "$(git rev-parse --show-toplevel)/pins.toml" "$kind" "$name" <<'PY'
import sys, tomllib

path, kind, name = sys.argv[1:]
with open(path, "rb") as f:
    pins = tomllib.load(f)

table = {"image": "image", "git": "git", "tag": "git"}.get(kind)
if table is None:
    sys.exit(f"unknown kind {kind}, expected image, git or tag")
found = [e for e in pins.get(table, []) if e.get("name") == name]
if len(found) != 1:
    sys.exit(f"pins.toml has {len(found)} [[{table}]] entries named {name}")
e = found[0]
if kind == "image":
    print(f'{e["repository"]}@{e["digest"]}')
elif kind == "git":
    print(e["sha"])
else:
    ref = e.get("ref", "")
    if not ref.startswith("refs/tags/"):
        sys.exit(f"{name} is not pinned to a tag")
    print(ref.removeprefix("refs/tags/"))
PY
