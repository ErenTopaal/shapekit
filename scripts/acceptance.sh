#!/usr/bin/env bash
# the acceptance gate: the checks every change to main must pass.
#
# usage: scripts/acceptance.sh           quick mode, the gate for every change
#        scripts/acceptance.sh --full    quick mode, then the nightly checks: pins against the
#                                        live sources, every sdk version and the upstream
#                                        sightings, the wallet-backend reproductions and linter
#                                        baseline, findings with drafts allowed and every chain
#                                        regenerated under both passphrases
#
# needs cargo, go and node on PATH; full mode also needs docker and network access. runs from
# the repository root wherever it is started. run outputs go to target/acceptance/.
# docs/maintenance.md, "Acceptance gate", describes both modes.
set -euo pipefail

mode=quick
case "${1:-}" in
  "" | --quick) ;;
  --full) mode=full ;;
  -h | --help)
    sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  *)
    echo "usage: scripts/acceptance.sh [--quick|--full]" >&2
    exit 2
    ;;
esac

cd "$(git rev-parse --show-toplevel)"

for tool in cargo go node; do
  if ! command -v "$tool" >/dev/null; then
    echo "acceptance: $tool is not on PATH" >&2
    exit 2
  fi
done
if [ "$mode" = full ] && ! command -v docker >/dev/null; then
  echo "acceptance: full mode needs docker" >&2
  exit 2
fi

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

# quick mode

step cargo fmt --all --check
step cargo clippy --workspace --all-targets --locked -- -D warnings
step cargo test --workspace --locked
step cargo build --release --locked -p shapekit-cli
step "$shapekit" registry check
step "$shapekit" manifest validate --file corpus/manifest.json --registry registry
step "$shapekit" manifest validate --file candidate/manifest.json --registry registry
step "$shapekit" contracts schema --check
step "$shapekit" coverage --check
step "$shapekit" guide check
step "$shapekit" goldens agree
step "$shapekit" pack verify
for sdk in rust go js; do
  step "$shapekit" conformance run --sdk "$sdk" --out "$out/conformance/$sdk"
done
step "$shapekit" findings check
step scripts/check-no-em-dash.sh

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
# the all-versions runs above built every testee the reproductions use
step "$shapekit" conformance reproduce --no-build

current="$shapekit harness reproduce"
echo "== $current"
"$shapekit" harness reproduce --table adapters/walletbackend/reproductions.toml \
  --out "$out/harness-reproduce" | tee "$out/harness-reproduce.txt"
rows=$(grep -c ': holds$\|: does not hold$' "$out/harness-reproduce.txt" || true)
held=$(grep -c ': holds$' "$out/harness-reproduce.txt" || true)
echo "harness reproduce: $held of $rows rows hold"
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
(cd "$wb" && "$OLDPWD/$out/bin/indexerlint" -baseline "$OLDPWD/lint/baselines/wallet-backend.json" ./...)

step "$shapekit" findings check --allow-draft
step "$shapekit" gen verify-repro --jobs 2

trap - ERR
echo "acceptance (full): every check passed"
