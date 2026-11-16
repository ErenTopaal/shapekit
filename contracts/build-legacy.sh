#!/bin/sh
# builds the test contracts against older soroban-sdk releases for chains below protocol 29. the host
# refuses a wasm whose contractenvmetav0 interface protocol is above the ledger's, and soroban-sdk 29
# writes 29. each contracts/legacy/p<N> workspace compiles the same sources as build.sh, pinned to the
# soroban-sdk whose interface protocol is N, with the same `stellar contract build` as build.sh:
#   p21  soroban-sdk 21.7.7   protocol 21 chains, echo only (with the sdk22 feature: sdk 21 has no
#                             events::Event either)
#   p22  soroban-sdk 22.0.11  protocol 22 chains (echo needs its sdk22 feature, see echo/Cargo.toml)
#   p23  soroban-sdk 23.5.3   protocol 24 chains; no soroban-sdk release targets interface 24
#   p28  soroban-sdk 28.0.0   protocol 28 chains
# stellar-cli shakes the spec only of a wasm whose sdk records how (rssdk_spec_shaking 2, which
# soroban-sdk 28 writes and 21 to 23 do not), so p21 to p23 keep their full spec; the cliver
# meta and wasm-opt apply to all four. outputs go to wasm/p<N>/ with a SHA256SUMS each.
set -eu
cd "$(dirname "$0")"
stellar=$(../scripts/fetch-stellar-cli.sh)
# see build.sh: the committed wasm is the build without extra rust flags
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_RUSTFLAGS
build() {
  dir=$1
  shift
  mkdir -p "wasm/$dir"
  for p in "$@"; do
    "$stellar" contract build --locked --manifest-path "legacy/$dir/Cargo.toml" --package "$p" --out-dir "wasm/$dir"
  done
  (cd "wasm/$dir" && for p in "$@"; do sha256sum "$p.wasm"; done > SHA256SUMS)
}
build p21 echo
build p22 auth_tree echo
build p23 auth_tree echo
build p28 auth_tree echo factory
