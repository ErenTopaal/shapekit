#!/bin/sh
# builds the test contracts with `stellar contract build` from the stellar-cli release pinned in
# pins.toml, fetched and checked against its sha256 by scripts/fetch-stellar-cli.sh. stellar-cli
# compiles each contract with `cargo rustc --crate-type=cdylib --target=wasm32v1-none --release`,
# remapping the cargo registry path out of the output so the bytes do not depend on where the
# registry lives, then post-processes the wasm: a `cliver` entry (stellar-cli version and commit)
# in contractmetav0, spec shaking of contractspecv0 (soroban-sdk 28 and later record
# rssdk_spec_shaking 2), and wasm-opt. so the bytes depend on the stellar-cli release as well as
# on the rust toolchain and soroban-sdk; the same three give the same bytes.
set -eu
cd "$(dirname "$0")"
stellar=$(../scripts/fetch-stellar-cli.sh)
# stellar-cli drops its path remapping when RUSTFLAGS or CARGO_ENCODED_RUSTFLAGS is set, and
# merges CARGO_BUILD_RUSTFLAGS into it; the committed wasm is the build with none of them
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_RUSTFLAGS
for p in auth_tree delegating_account echo exec_ref factory; do
  "$stellar" contract build --locked --package "$p" --out-dir wasm
done
(cd wasm && sha256sum auth_tree.wasm delegating_account.wasm echo.wasm exec_ref.wasm factory.wasm > SHA256SUMS)
