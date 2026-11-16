#!/usr/bin/env bash
# writes schema/xdr/<label>/xdr.json, the machine-readable form of the stellar-xdr definitions,
# by running the upstream generator (rs-stellar-xdr, xdr-generator-rust/generator-definitions-json)
# at the commit pinned in pins.toml ([[git]] rs-stellar-xdr) over the .x files of a stellar-xdr ref.
# this is the same generator and the same invocation the rs-stellar-xdr Makefile uses for the
# xdr-definitions-json/xdr.json file it ships in the stellar-xdr crate.
#
# usage: scripts/fetch-xdr-definitions.sh [--check] [<label> <ref>]...
#   with no pair, writes the pinned stellar-xdr tag ([[git]] stellar-xdr) under its tag name.
#   <ref> is any stellar-xdr tag, branch or commit; <label> names the output directory.
#   --check  regenerate into a scratch directory and compare with the committed files instead
#            of writing them; exits 1 on any difference.
#
# schema/xdr/ holds every stellar-xdr release tag from v20 and the next branch, written with
#   scripts/fetch-xdr-definitions.sh <tag> <tag> ... next refs/heads/next
# v19, the oldest tag, uses union default arms, which the pinned generator refuses
# (UnsupportedDefaultArm at Stellar-transaction.x:851), so it has no definitions file.
#
# sources and the built generator are cached in target/xdr-definitions/.
# needs: git, cargo, tar, cmp, python3 >= 3.11 (scripts/pin.sh).
set -euo pipefail

root=$(git rev-parse --show-toplevel)
check=false
if [ "${1:-}" = "--check" ]; then
  check=true
  shift
fi
if [ $(($# % 2)) -ne 0 ]; then
  echo "usage: $0 [--check] [<label> <ref>]..." >&2
  exit 2
fi
if [ $# -eq 0 ]; then
  set -- "$("$root/scripts/pin.sh" tag stellar-xdr)" "$("$root/scripts/pin.sh" git stellar-xdr)"
fi

gen_sha=$("$root/scripts/pin.sh" git rs-stellar-xdr)
work="$root/target/xdr-definitions"
mkdir -p "$work"

# a bare mirror per upstream, updated once per run; the sources are exported from it with
# git archive, so nothing from upstream runs as part of the checkout
mirror() {
  local url=$1 dir=$2
  if [ -d "$dir" ]; then
    git -C "$dir" fetch -q --prune --tags origin '+refs/heads/*:refs/heads/*'
  else
    git clone -q --mirror "$url" "$dir"
  fi
}
mirror https://github.com/stellar/rs-stellar-xdr "$work/rs-stellar-xdr.git"
mirror https://github.com/stellar/stellar-xdr "$work/stellar-xdr.git"

gen_src="$work/generator-$gen_sha"
gen="$work/generator-target/release/generator-definitions-json"
if [ ! -f "$gen_src/.complete" ]; then
  rm -rf "$gen_src"
  mkdir -p "$gen_src"
  git -C "$work/rs-stellar-xdr.git" archive "$gen_sha" xdr-generator-rust | tar -x -C "$gen_src"
  touch "$gen_src/.complete"
fi
# the generator workspace carries its own Cargo.lock; --locked keeps its dependencies as upstream
# pinned them
CARGO_TARGET_DIR="$work/generator-target" cargo build -q --release --locked \
  --manifest-path "$gen_src/xdr-generator-rust/generator-definitions-json/Cargo.toml"

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
status=0
while [ $# -gt 0 ]; do
  label=$1 ref=$2
  shift 2
  sha=$(git -C "$work/stellar-xdr.git" rev-parse --verify -q "$ref^{commit}") || {
    echo "$label: stellar-xdr has no ref $ref" >&2
    exit 1
  }
  # the generator records each input path as given, so it runs where the .x files sit under xdr/,
  # the layout of the stellar-xdr submodule in rs-stellar-xdr
  run="$scratch/$label"
  mkdir -p "$run/xdr"
  git -C "$work/stellar-xdr.git" archive "$sha" | tar -x -C "$run/xdr"
  inputs=()
  while IFS= read -r f; do
    inputs+=(--input "xdr/$f")
  done < <(cd "$run/xdr" && ls -- *.x | LC_ALL=C sort)
  out="$run/xdr.json"
  if ! (cd "$run" && "$gen" "${inputs[@]}" --output "$out" 2>"$run/stderr"); then
    echo "$label ($ref, $sha): the generator failed:" >&2
    cat "$run/stderr" >&2
    exit 1
  fi
  dest="$root/schema/xdr/$label/xdr.json"
  if $check; then
    if ! cmp -s "$out" "$dest"; then
      echo "$label ($ref, $sha): $dest differs from the generator output" >&2
      status=1
    else
      echo "$label ($ref, $sha): unchanged" >&2
    fi
  else
    mkdir -p "${dest%/*}"
    cp "$out" "$dest"
    echo "$label ($ref, $sha): wrote $dest" >&2
  fi
done
exit $status
