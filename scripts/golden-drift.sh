#!/usr/bin/env bash
# compares stellar-core's golden index files (test-lcm-current/<dir>/index.json) at an upstream
# ref with the committed snapshot index. prints one line per difference and nothing when the
# two agree. only the small index files are fetched, never the golden xdr.
#
# usage: scripts/golden-drift.sh <ref> [stamps]
#
#   <ref>     a stellar-core tag or branch
#   stamps    compare only the stamps (protocol, rng seed, versions), not the index hashes;
#             used for the default branch, whose test indexes change with every new test
#
# needs git and python3 >= 3.11.
set -euo pipefail

ref=${1:?usage: scripts/golden-drift.sh <ref> [stamps]}
mode=${2:-all}
case "$mode" in all|stamps) ;; *) echo "second argument must be stamps" >&2; exit 2 ;; esac

root=$(git rev-parse --show-toplevel)
tag=$("$root/scripts/pin.sh" tag stellar-core)
index="$root/corpus/goldens/core-$tag/index.json"
if [ ! -f "$index" ]; then
  echo "no committed golden index at $index" >&2
  exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

git clone --quiet --depth 1 --branch "$ref" --filter=blob:none --no-checkout \
  https://github.com/stellar/stellar-core "$work/core"
git -C "$work/core" sparse-checkout set --no-cone '/test-lcm-current/*/index.json'
git -C "$work/core" checkout --quiet

python3 -I - "$index" "$work/core" "$ref" "$mode" <<'PY'
import hashlib, json, os, sys

index_path, core, ref, mode = sys.argv[1:]
with open(index_path) as f:
    committed = {d["name"]: d for d in json.load(f)["directories"]}

def stamps(raw):
    v = json.loads(raw)
    return {
        "protocol": v.get("!cfg protocol version"),
        "rng_seed": v.get("!rng seed"),
        "test_all_versions": v.get("!test all versions"),
        "versions_to_test": v.get("!versions to test"),
    }

tier = os.path.join(core, "test-lcm-current")
upstream = {}
if os.path.isdir(tier):
    for name in sorted(os.listdir(tier)):
        p = os.path.join(tier, name, "index.json")
        if os.path.isfile(p):
            with open(p, "rb") as f:
                raw = f.read()
            upstream[name] = (hashlib.sha256(raw).hexdigest(), stamps(raw))

for name in sorted(set(committed) | set(upstream)):
    if name not in upstream:
        print(f"{ref}: {name} is no longer in test-lcm-current")
        continue
    if name not in committed:
        print(f"{ref}: new golden directory {name}, stamps {upstream[name][1]}")
        continue
    digest, st = upstream[name]
    want = committed[name]["stamps"]
    if st != want:
        print(f"{ref}: {name} stamps changed from {want} to {st}")
    elif mode == "all" and digest != committed[name]["index_sha256"]:
        print(f"{ref}: {name} index.json changed (sha256 {digest[:16]})")
PY
