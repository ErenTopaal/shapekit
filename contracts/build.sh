#!/bin/sh
# builds the test contracts with the same compile step `stellar contract build` runs in stellar-cli 30.0.1
# (cmd/soroban-cli/src/commands/contract/build.rs). stellar-cli's post-processing (spec shaking, cliver
# meta, wasm-opt) is not applied, so hashes differ from an official stellar-cli build. the spec shaking
# env var is what stellar-cli itself sets; soroban-sdk 29 refuses to build without it.
set -eu
cd "$(dirname "$0")"
for p in auth_tree echo factory; do
  CARGO_BUILD_RUSTFLAGS="--remap-path-prefix=$HOME/.cargo/registry/src=" \
  SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1 \
  cargo rustc --locked --manifest-path="$p/Cargo.toml" --crate-type=cdylib --target=wasm32v1-none --release
  cp "target/wasm32v1-none/release/$p.wasm" wasm/
done
(cd wasm && sha256sum auth_tree.wasm echo.wasm factory.wasm > SHA256SUMS)
