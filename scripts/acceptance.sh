#!/usr/bin/env bash
# the acceptance gate: the checks every change to main must pass.
#
# usage: scripts/acceptance.sh           quick mode, the gate for every change
#        scripts/acceptance.sh --full    quick mode, then the nightly checks: pins against the
#                                        live sources, every sdk version and the upstream
#                                        sightings, the wallet-backend reproductions and linter
#                                        baseline, the stellar-rpc harness runs, findings with
#                                        drafts allowed, every chain
#                                        regenerated under both passphrases, the test
#                                        contracts rebuilt and the site built
#
# needs cargo, go, golangci-lint, node and npm on PATH, and network access once to fetch the python
# and java testees' pinned uv, cpython, jdk and maven into target/toolchains/; full mode also
# needs docker, flutter, php, composer, dotnet and network
# access, and the wasm32v1-none rust target for the test contracts. runs from
# the repository root wherever it is started, builds without debug info, and writes run
# outputs to target/acceptance/.
# docs/maintenance.md, "Acceptance gate", describes both modes.
set -euo pipefail

mode=quick
case "${1:-}" in
  "" | --quick) ;;
  --full) mode=full ;;
  -h | --help)
    sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  *)
    echo "usage: scripts/acceptance.sh [--quick|--full]" >&2
    exit 2
    ;;
esac

cd "$(git rev-parse --show-toplevel)"
root=$PWD

# without debug info a workspace build stays a few gigabytes; with it, tens
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0

for tool in cargo go node npm golangci-lint; do
  if ! command -v "$tool" >/dev/null; then
    echo "acceptance: $tool is not on PATH" >&2
    exit 2
  fi
done
if [ "$mode" = full ] && ! command -v docker >/dev/null; then
  echo "acceptance: full mode needs docker" >&2
  exit 2
fi
for tool in flutter php composer dotnet; do
  if [ "$mode" = full ] && ! command -v "$tool" >/dev/null; then
    echo "acceptance: full mode needs $tool (the dart, php and dotnet testees' builds)" >&2
    exit 2
  fi
done

out=target/acceptance
mkdir -p "$out"
shapekit="${CARGO_TARGET_DIR:-target}/release/shapekit"
current=""
trap 'echo "acceptance ($mode): failed at: $current" >&2' ERR

step() {
  current="$*"
  echo "== $*"
  "$@"
}

# runs "$@" and fails when it leaves tracked files under $1 changed: the committed copy must
# be what the tool writes
unchanged() {
  local dir=$1
  shift
  step "$@"
  local changed
  changed=$(git status --porcelain -- "$dir")
  if [ -n "$changed" ]; then
    echo "$changed" >&2
    echo "acceptance ($mode): committed $dir differs from what $* writes" >&2
    exit 1
  fi
}

# go vet, go test and golangci-lint in one module of go.work
go_module() {
  (cd "$1" && go vet ./... && go test ./... && golangci-lint run --config "$root/.golangci.yml" ./...)
}

# the site's install, typecheck, linter and prerendered build
site_build() {
  (cd site && npm ci --no-audit --no-fund && npm run typecheck && npm run lint && npm run build)
}

# the javascript testee's typecheck, build and unit tests
js_testee() {
  (cd testees/js && npm ci --no-audit --no-fund && npx tsc --noEmit && npx tsc && node --test "dist/**/*.test.js")
}

# a testee outside the workspace, at the sdk's current version: rustfmt, clippy and its unit
# tests, in the target directory its conformance build uses
cargo_testee() {
  local sdk=$1 version
  version=$(sed -n "/^\[sdk\.$sdk\]/,/^current/s/^current = \"\(.*\)\"/\1/p" conformance/versions.toml)
  local manifest="testees/$sdk/versions/$version/Cargo.toml" target="target/testees/$sdk/$version"
  rustfmt --edition 2024 --check "testees/$sdk"/src/*.rs "testees/$sdk"/*.rs
  cargo clippy --locked --all-targets --manifest-path "$manifest" --target-dir "$target" -- -D warnings
  cargo test --locked --manifest-path "$manifest" --target-dir "$target"
}

# the python testee's unit tests, against the current version's venv that its conformance run
# below has built
python_testee() {
  local v
  v=$(python3 -I -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["sdk"]["python"]["current"])' conformance/versions.toml)
  "target/testees/python/$v/venv/bin/python" -B -E -s -m unittest discover -s testees/python/tests -t testees/python
}

# the java testee's unit tests (maven surefire) at the current version
java_testee() {
  local v
  v=$(python3 -I -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["sdk"]["java"]["current"])' conformance/versions.toml)
  mvn -B -q -f "testees/java/versions/$v/pom.xml" test
}

# the dart testee as its conformance build lays it out under target/testees/dart/<version>/
# (run that build first): dart analyze over the sources and the unit tests. needs flutter on
# PATH, so it runs in full mode only
dart_testee() {
  local version
  version=$(sed -n "/^\[sdk\.dart\]/,/^current/s/^current = \"\(.*\)\"/\1/p" conformance/versions.toml)
  (cd "target/testees/dart/$version" && dart analyze --fatal-infos bin lib test tool && dart test)
}

# the php testee's unit checks in its conformance build directory (run that build first);
# needs php and composer on PATH, so it runs in full mode only
php_testee() {
  local version
  version=$(sed -n "/^\[sdk\.php\]/,/^current/s/^current = \"\(.*\)\"/\1/p" conformance/versions.toml)
  php "target/testees/php/$version/tests/run.php"
}

# quick mode

step cargo fmt --all --check
step cargo clippy --workspace --all-targets --locked -- -D warnings
step cargo test --workspace --locked
step cargo build --release --locked -p shapekit-cli
# the consumer checks in adapters/protocol serve chains with the cli
export SHAPEKIT_BIN="$root/$shapekit"
case "$shapekit" in /*) SHAPEKIT_BIN=$shapekit ;; esac
# every module of go.work, so a module added later is not missed
for dir in $(go list -m -f '{{.Dir}}'); do
  step go_module "$dir"
done
step go build -C lint -o "$root/$out/bin/indexerlint" ./cmd/indexerlint
current="indexerlint over adapters/"
echo "== $current"
(cd adapters && "$root/$out/bin/indexerlint" ./...)
step js_testee
step cargo_testee soroban
step "$shapekit" registry check
step "$shapekit" manifest validate --file corpus/manifest.json --registry registry
unchanged corpus sh -c "\"$shapekit\" synth build && \"$shapekit\" manifest build"
unchanged corpus "$shapekit" oracle run
step "$shapekit" manifest validate --file candidate/manifest.json --registry registry
step "$shapekit" contracts schema --check
step "$shapekit" coverage --check
step "$shapekit" guide check
step "$shapekit" goldens agree
step "$shapekit" pack verify
current="repro selection families"
echo "== $current"
diff <(scripts/select-repro.sh --families | sort) <("$shapekit" gen families | sort)
step scripts/select-repro.sh --check-chains
step "$shapekit" harness schema
rm -rf "$out/dist"
step "$shapekit" corpus pack --out "$out/dist"
for sdk in rust go js; do
  step "$shapekit" conformance run --sdk "$sdk" --out "$out/conformance/$sdk"
done
# the python testee's pinned uv and cpython, fetched once into target/toolchains/
current="python toolchain"
echo "== $current"
python_path=$(scripts/python-toolchain.sh)
PATH="$python_path:$PATH" step "$shapekit" conformance run --sdk python --out "$out/conformance/python"
step python_testee
# the java testee's pinned jdk and maven, fetched once into target/toolchains/
current="java toolchain"
echo "== $current"
java_path=$(scripts/java-toolchain.sh)
PATH="$java_path:$PATH" step "$shapekit" conformance run --sdk java --out "$out/conformance/java"
PATH="$java_path:$PATH" step java_testee
step "$shapekit" findings check
step scripts/check-no-em-dash.sh
step scripts/check-public-text.sh

if [ "$mode" = quick ]; then
  trap - ERR
  echo "acceptance (quick): every check passed"
  exit 0
fi

# full mode

step "$shapekit" pins verify
for sdk in rust go js; do
  step "$shapekit" conformance run --sdk "$sdk" --all-versions --out "$out/conformance-all/$sdk"
done
step "$shapekit" conformance run --sdk soroban --all-versions --out "$out/conformance-all/soroban"
PATH="$python_path:$PATH" step "$shapekit" conformance run --sdk python --all-versions --out "$out/conformance-all/python"
# the type table of the python testee is what its generator writes from every version's venv
unchanged testees/python sh -c "python3 -I testees/python/gentypes.py"
PATH="$java_path:$PATH" step "$shapekit" conformance run --sdk java --all-versions --out "$out/conformance-all/java"
# the type table of the java testee is what its generator writes from every version's jars
unchanged testees/java sh -c "cd testees/java && python3 -I gentypes.py"
step "$shapekit" conformance run --sdk dart --all-versions --out "$out/conformance-all/dart"
step dart_testee
step "$shapekit" conformance run --sdk php --all-versions --out "$out/conformance-all/php"
step php_testee
step "$shapekit" conformance run --sdk dotnet --all-versions --out "$out/conformance-all/dotnet"
PATH="$java_path:$PATH" step "$shapekit" conformance run --sdk kmp --all-versions --out "$out/conformance-all/kmp"
# the type table of the kmp testee is what its generator writes from every version's jars
unchanged testees/kmp sh -c "cd testees/kmp && python3 -I gentypes.py"
step "$shapekit" conformance run --sdk stellar-cli --all-versions --out "$out/conformance-all/stellar-cli"
# the all-versions runs above built every testee the reproductions use
step "$shapekit" conformance reproduce --no-build

current="$shapekit harness reproduce"
echo "== $current"
"$shapekit" harness reproduce --table adapters/walletbackend/reproductions.toml \
  --out "$out/harness-reproduce" | tee "$out/harness-reproduce.txt"
# the table's rows: one per entry verdict and one per carry check of each case
table="$out/harness-reproduce/REPRODUCTIONS.md"
rows=$(grep -c '^| wallet-backend' "$table" || true)
held=$(grep -c '^| wallet-backend.* | yes |$' "$table" || true)
carry=$(grep -c '^| wallet-backend[^|]*|[^|]*|[^|]*| carry check |' "$table" || true)
echo "harness reproduce: $held of $rows table rows hold ($((rows - carry)) entry verdicts, $carry carry checks)"
if [ "$rows" -eq 0 ] || [ "$held" -ne "$rows" ]; then
  echo "acceptance (full): failed at: $current" >&2
  exit 1
fi

# the linter on wallet-backend at the pinned commit, with the committed baseline
pin=$(scripts/pin.sh git wallet-backend)
wb="$out/wallet-backend"
if [ "$(git -C "$wb" rev-parse HEAD 2>/dev/null || true)" != "$pin" ]; then
  current="fetching wallet-backend $pin"
  echo "== $current"
  rm -rf "$wb"
  git init --quiet "$wb"
  git -C "$wb" fetch --quiet --depth 1 https://github.com/stellar/wallet-backend "$pin"
  git -C "$wb" checkout --quiet FETCH_HEAD
fi
step go build -C lint -o "$PWD/$out/bin/indexerlint" ./cmd/indexerlint
current="indexerlint -baseline lint/baselines/wallet-backend.json on wallet-backend $pin"
echo "== $current"
# the checkout sits under this repository, whose go.work must not claim it
(cd "$wb" && GOWORK=off "$OLDPWD/$out/bin/indexerlint" -baseline "$OLDPWD/lint/baselines/wallet-backend.json" ./...)

# the stellar-rpc adapter over every chain of both networks, once per method the export reads
# events from, with the verdicts of the committed expectations file
step docker pull --quiet "$(scripts/pin.sh image stellar-rpc)"
step go build -C adapters -o "$root/$out/bin/rpc-adapter" ./stellarrpc/cmd/rpc-adapter
for src in events transactions ledgers; do
  step "$shapekit" harness run --adapter "$out/bin/rpc-adapter --cpus 2 --events-from $src" \
    --network both --expect crates/shapekit-harness/expect/stellar-rpc.toml \
    --out "$out/harness-stellar-rpc/$src"
done

step "$shapekit" findings check --allow-draft
step "$shapekit" gen verify-repro --jobs 2

# the test contracts with the pinned stellar-cli, byte for byte against the committed wasm
unchanged contracts sh -c "contracts/build.sh && contracts/build-legacy.sh"

# the site as site.yml builds it, from data derived from this checkout
step "$shapekit" site data --out site/public/data --commit "$(git rev-parse HEAD)"
step site_build

trap - ERR
echo "acceptance (full): every check passed"
