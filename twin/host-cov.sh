#!/usr/bin/env bash
# measures which code of soroban-env-host a recorded chain's contract calls run.
#
# usage: twin/host-cov.sh <chain dir> <out dir>
#
# core's --enable-codecoverage build instruments only its c and c++ code, so the
# rust contract host inside it is measured here instead: shapekit-host-cov,
# built with -C instrument-coverage, replays every invocation of the chain
# through the host's e2e_invoke entry point with the inputs core used, and
# fails unless each replay returns, writes and emits exactly what the chain
# recorded. only then is the profile it leaves a measurement of the chain.
#
# writes into <out dir> (created, must not exist):
#   replay.txt          the replay's per-invocation comparison
#   host.profdata       the merged profile
#   host-coverage.json  region and line counts per soroban-env-host source file
# and prints the host's region totals.
set -euo pipefail

chain=${1:?usage: twin/host-cov.sh <chain dir> <out dir>}
out=${2:?usage: twin/host-cov.sh <chain dir> <out dir>}
if [ -e "$out" ]; then
    echo "$out already exists" >&2
    exit 2
fi
repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
chain=$(cd "$chain" && pwd -P)
mkdir -p "$out"
out=$(cd "$out" && pwd -P)

# the profile tools must come from the llvm the compiler instruments with
(cd "$repo" && rustup component add llvm-tools >/dev/null 2>&1)
sysroot=$(cd "$repo" && rustc --print sysroot)
host=$(cd "$repo" && rustc -vV | sed -n 's/^host: //p')
tools="$sysroot/lib/rustlib/$host/bin"
for t in llvm-profdata llvm-cov; do
    [ -x "$tools/$t" ] || { echo "missing $tools/$t (rustup component llvm-tools)" >&2; exit 1; }
done

# the host the replay links; the binary also carries older hosts through
# shapekit-core-driver, which are left out of the report
host_version=$(cd "$repo" && cargo tree -q --locked -p shapekit-twin --depth 1 -e normal --prefix none |
    sed -n 's/^soroban-env-host v\([0-9.]*\)$/\1/p')
[ -n "$host_version" ] || { echo "shapekit-twin does not depend on soroban-env-host" >&2; exit 1; }

target=${CARGO_TARGET_DIR:-$repo/target}/host-cov
# an explicit --target keeps RUSTFLAGS off build scripts and proc macros,
# which would otherwise be instrumented and drop profiles while compiling
(cd "$repo" && CARGO_TARGET_DIR="$target" RUSTFLAGS="-C instrument-coverage" \
    cargo build -q --release --locked --target "$host" -p shapekit-twin --bin shapekit-host-cov)
bin="$target/$host/release/shapekit-host-cov"

mkdir -p "$out/profraw"
status=0
LLVM_PROFILE_FILE="$out/profraw/host-%p.profraw" "$bin" "$chain" | tee "$out/replay.txt" || status=$?
if [ "$status" != 0 ]; then
    echo "the replay differs from the chain (exit $status); no coverage is reported for it" >&2
    exit 1
fi

"$tools/llvm-profdata" merge -sparse -o "$out/host.profdata" "$out"/profraw/*.profraw
rm -rf "$out/profraw"
"$tools/llvm-cov" export -summary-only -format=text -instr-profile="$out/host.profdata" "$bin" |
    python3 -I -c '
import json, re, sys
data = json.load(sys.stdin)["data"][0]
pat = re.compile(r"/soroban-env-host-(" + re.escape(sys.argv[2]) + r")/src/")
files, versions = [], set()
for f in data["files"]:
    m = pat.search(f["filename"])
    if not m:
        continue
    versions.add(m.group(1))
    s = f["summary"]
    files.append({
        "file": f["filename"][m.start() + 1:],
        "regions": s["regions"]["count"],
        "regions_covered": s["regions"]["covered"],
        "lines": s["lines"]["count"],
        "lines_covered": s["lines"]["covered"],
    })
if not files:
    sys.exit("no soroban-env-host source in the coverage export")
if len(versions) != 1:
    sys.exit(f"expected one soroban-env-host version, found {sorted(versions)}")
files.sort(key=lambda f: f["file"])
total = {k: sum(f[k] for f in files) for k in ("regions", "regions_covered", "lines", "lines_covered")}
report = {"crate": "soroban-env-host", "version": versions.pop(), "total": total, "files": files}
json.dump(report, open(sys.argv[1], "w"), indent=2)
open(sys.argv[1], "a").write("\n")
t = total
print("soroban-env-host %s: %d of %d regions, %d of %d lines, %d files" % (
    report["version"], t["regions_covered"], t["regions"], t["lines_covered"], t["lines"], len(files)))
' "$out/host-coverage.json" "$host_version"
