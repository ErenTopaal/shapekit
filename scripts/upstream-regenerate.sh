#!/usr/bin/env bash
# runs the commands that regenerate what one new upstream item changes, against the new
# version, and prints the result as markdown for its tracking issue. nothing is committed:
# outputs go to a scratch directory or to the workflow's checkout.
#
# usage: scripts/upstream-regenerate.sh <item> <version>
#   xdr <tag>          stellar-xdr definitions: generate schema/xdr/<tag>/xdr.json, then
#                      `shapekit schema diff` against the pinned tag and `shapekit schema families`
#   core <tag>         stellar-core: `shapekit twin gates` over the new tag's source, compared with
#                      registry/generated/gates.json when that file exists
#   js|go|rust <ver>   an sdk release: `shapekit conformance sync-versions`, then
#                      `shapekit conformance run` of the new version over the corpus
#   caps <commit>      stellar-protocol: the CAP texts changed between the pinned commit and
#                      <commit>, with the protocol each names
#
# a command that fails is reported with its output and makes the script exit 1, so the issue
# says what broke instead of leaving the section out.
# needs: git, python3 >= 3.11; shapekit from $SHAPEKIT (default target/release/shapekit, built
# when missing); for xdr also cargo; for an sdk also the sdk's toolchain (cargo, go or node).
set -uo pipefail

item=${1:?usage: scripts/upstream-regenerate.sh <xdr|core|js|go|rust|caps> <version>}
version=${2:?usage: scripts/upstream-regenerate.sh <xdr|core|js|go|rust|caps> <version>}
root=$(git rev-parse --show-toplevel)
cd "$root" || exit 2
shapekit=${SHAPEKIT:-target/release/shapekit}
if [ ! -x "$shapekit" ]; then
  cargo build --release --locked -q -p shapekit-cli || exit 2
fi
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
status=0

# runs a command, prints it and its output in a fenced block, records a failure
run() {
  local out code
  out=$("$@" 2>&1)
  code=$?
  printf "\`%s\`" "$*"
  if [ "$code" -ne 0 ]; then
    printf ' failed with exit code %d' "$code"
    status=1
  fi
  printf ":\n\n\`\`\`\n%s\n\`\`\`\n\n" "$(printf '%s\n' "$out" | tail -n 60)"
  return "$code"
}

pinned_git() {
  python3 -I - "$root/pins.toml" "$1" "$2" <<'PY'
import sys, tomllib
path, name, field = sys.argv[1:]
with open(path, "rb") as f:
    pins = tomllib.load(f)
[e] = [g for g in pins["git"] if g["name"] == name]
v = e.get(field, "")
print(v.removeprefix("refs/tags/"))
PY
}

echo "### Regenerated against $item $version"
echo
case "$item" in
  xdr)
    pinned=$(pinned_git stellar-xdr ref)
    run scripts/fetch-xdr-definitions.sh "$version" "refs/tags/$version" &&
      run "$shapekit" schema diff "$pinned" "$version"
    run "$shapekit" schema families
    ;;
  core)
    if ! { git init -q "$scratch/core" &&
      git -C "$scratch/core" fetch -q --depth 1 https://github.com/stellar/stellar-core "refs/tags/$version" &&
      git -C "$scratch/core" checkout -q FETCH_HEAD; }; then
      echo "fetching stellar-core $version failed"
      exit 1
    fi
    sha=$(git -C "$scratch/core" rev-parse HEAD)
    # the gate list checks the checkout against pins.toml, so it reads a copy pinned to the
    # new tag
    python3 -I - "$root/pins.toml" "$scratch/pins.toml" "$version" "$sha" <<'PY'
import re, sys
src, dst, tag, sha = sys.argv[1:]
text = open(src).read()
block = re.search(r'\[\[git\]\]\nname = "stellar-core"\n(?:[^\[]*\n)', text).group(0)
new = re.sub(r'ref = "[^"]*"', f'ref = "refs/tags/{tag}"', block)
new = re.sub(r'sha = "[^"]*"', f'sha = "{sha}"', new)
open(dst, "w").write(text.replace(block, new))
PY
    if run "$shapekit" twin gates --core-src "$scratch/core" --out "$scratch/gates.json" --pins "$scratch/pins.toml" &&
      [ -f registry/generated/gates.json ]; then
      echo "Gate list against the committed one (file, line, column, comparison, protocol):"
      echo
      echo '```'
      python3 -I - registry/generated/gates.json "$scratch/gates.json" <<'PY'
import json, sys
def gates(p):
    return {(g["file"], g["line"], g["column"], g["comparison"], g["version"]) for g in json.load(open(p))["gates"]}
old, new = gates(sys.argv[1]), gates(sys.argv[2])
print(f"{len(old)} committed, {len(new)} at the new tag, {len(new - old)} added, {len(old - new)} gone")
for g in sorted(new - old)[:40]:
    print("+", *g)
for g in sorted(old - new)[:40]:
    print("-", *g)
PY
      echo '```'
      echo
    fi
    ;;
  js | go | rust)
    run "$shapekit" conformance sync-versions &&
      run "$shapekit" conformance run --sdk "$item" --version "$version" --out "$scratch/conformance"
    ;;
  caps)
    pinned=$(pinned_git stellar-protocol sha)
    # a partial clone needs a named remote to fetch the CAP texts it reads on demand
    if ! { git init -q "$scratch/protocol" &&
      git -C "$scratch/protocol" remote add origin https://github.com/stellar/stellar-protocol &&
      git -C "$scratch/protocol" fetch -q --filter=blob:none origin "$pinned" "$version"; }; then
      echo "fetching stellar-protocol failed"
      exit 1
    fi
    changed=$(git -C "$scratch/protocol" diff --name-only "$pinned" "$version" -- 'core/cap-*.md')
    if [ -z "$changed" ]; then
      echo "No CAP text changed between $pinned and $version."
    else
      echo "| CAP text | protocol it names |"
      echo "|---|---|"
      for f in $changed; do
        proto=$(git -C "$scratch/protocol" show "$version:$f" 2>/dev/null |
          grep -m1 -i '^protocol version:' | sed 's/^[^:]*: *//')
        echo "| \`$f\` | ${proto:-removed or none} |"
      done
      echo
      echo "No command regenerates registry/caps.toml from the texts; update the records by hand (runbook step 4)."
    fi
    ;;
  *)
    echo "unknown item $item" >&2
    exit 2
    ;;
esac
exit "$status"
