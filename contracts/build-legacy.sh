#!/bin/sh
# builds the test contracts against older soroban-sdk releases for chains below protocol 29. the host
# refuses a wasm whose contractenvmetav0 interface protocol is above the ledger's, and soroban-sdk 29
# writes 29. each contracts/legacy/p<N> workspace compiles the same sources as build.sh, pinned to the
# soroban-sdk whose interface protocol is N, with the same compile step as build.sh:
#   p22  soroban-sdk 22.0.11  protocol 22 chains (echo needs its sdk22 feature, see echo/Cargo.toml)
#   p23  soroban-sdk 23.5.3   protocol 24 chains; no soroban-sdk release targets interface 24
#   p28  soroban-sdk 28.0.0   protocol 28 chains
# outputs go to wasm/p<N>/ with a SHA256SUMS each.
set -eu
cd "$(dirname "$0")"
build() {
  dir=$1
  shift
  mkdir -p "wasm/$dir"
  for p in "$@"; do
    CARGO_BUILD_RUSTFLAGS="--remap-path-prefix=$HOME/.cargo/registry/src=" \
    SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1 \
    cargo rustc --locked --manifest-path="legacy/$dir/$p/Cargo.toml" --crate-type=cdylib --target=wasm32v1-none --release
    cp "legacy/$dir/target/wasm32v1-none/release/$p.wasm" "wasm/$dir/"
  done
  (cd "wasm/$dir" && for p in "$@"; do sha256sum "$p.wasm"; done > SHA256SUMS)
}
build p22 auth_tree echo
build p23 auth_tree echo
build p28 auth_tree echo factory
