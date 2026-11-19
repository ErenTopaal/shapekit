#!/usr/bin/env bash
# prints one value from pins.toml, so workflows never repeat a pinned version.
#
# usage: scripts/pin.sh image <name>    repository@digest of an [[image]]
#        scripts/pin.sh git <name>      commit sha of a [[git]] entry
#        scripts/pin.sh tag <name>      tag of a [[git]] entry (ref without refs/tags/)
#        scripts/pin.sh release <name>  "<version> <commit> <asset url> <sha256>" of a [[release]]
#        scripts/pin.sh maven <artifact> "<version> <file url> <sha256>" of the one [[maven]] pin
#                                       of that artifact
#        scripts/pin.sh maven-urls <artifact>  every url that serves that file, one per line:
#                                       maven central, its official mirrors, and for maven
#                                       itself the apache archive (the order pins verify uses)
#        scripts/pin.sh toolchain <name> "<version> <artifact> <sha256>" of a [[toolchain]]
#
# needs python3 >= 3.11 for tomllib.
set -euo pipefail

kind=${1:?usage: scripts/pin.sh image|git|tag|release|maven|maven-urls|toolchain <name>}
name=${2:?usage: scripts/pin.sh image|git|tag|release|maven|maven-urls|toolchain <name>}

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
    "maven-urls": "maven",
    "toolchain": "toolchain",
}.get(kind)
if table is None:
    sys.exit(f"unknown kind {kind}, expected image, git, tag, release, maven, maven-urls or toolchain")
key = "artifact" if table == "maven" else "name"
# maven central first, then its official mirrors; MAVEN_REPOSITORIES in
# crates/shapekit-cli/src/pins/mod.rs holds the same list
repositories = [
    "https://repo1.maven.org/maven2",
    "https://repo.maven.apache.org/maven2",
    "https://maven-central.storage-download.googleapis.com/maven2",
]
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
elif table == "maven":
    path = f'{e["group"].replace(".", "/")}/{e["artifact"]}/{e["version"]}/{e["file"]}'
    urls = [f"{base}/{path}" for base in repositories]
    if e["group"] == "org.apache.maven" and e["artifact"] == "apache-maven":
        major = e["version"].split(".")[0]
        urls.append(f'https://archive.apache.org/dist/maven/maven-{major}/{e["version"]}/binaries/{e["file"]}')
    if kind == "maven":
        print(e["version"], urls[0], e["sha256"])
    else:
        print("\n".join(urls))
elif kind == "toolchain":
    print(e["version"], e["artifact"], e["sha256"])
else:
    ref = e.get("ref", "")
    if not ref.startswith("refs/tags/"):
        sys.exit(f"{name} is not pinned to a tag")
    print(ref.removeprefix("refs/tags/"))
PY
