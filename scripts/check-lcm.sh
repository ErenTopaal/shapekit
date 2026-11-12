#!/usr/bin/env bash
# runs stellar-core's own golden check (`test '[tx]' --check-lcm`) in the pinned -buildtests
# image against a stellar-core checkout, the literal half of golden agreement.
#
# usage: scripts/check-lcm.sh <core-src> [jobs] [batch]
#
# <core-src> must hold test-lcm-current/. the tests are split round-robin into <jobs>
# containers (default 2), and each container runs its share <batch> tests per process
# (default 5). one process for all 144 tests is not an option: memory grows across test
# cases until the oom killer stops it (seen at about 6 GB), which is why core's own ci-build.sh
# runs the same command through selftest-parallel in batches of 5. separate containers give
# each job its own network namespace, so the ports the tests open cannot collide.
set -euo pipefail

core_src=$(realpath "${1:?usage: scripts/check-lcm.sh <core-src> [jobs] [batch]}")
jobs=${2:-2}
batch=${3:-5}

cd "$(git rev-parse --show-toplevel)"

if [ ! -d "$core_src/test-lcm-current" ]; then
  echo "$core_src has no test-lcm-current directory" >&2
  exit 2
fi

# the image is taken from pins.toml by digest, never by tag
image=$(awk '
  /^\[\[image\]\]/ { name = ""; repo = ""; digest = "" }
  /^name = /       { name = $3 }
  /^repository = / { repo = $3 }
  /^digest = /     { digest = $3; if (name == "\"stellar-core-buildtests\"") { gsub(/"/, "", repo); gsub(/"/, "", digest); print repo "@" digest } }
' pins.toml)
if [ -z "$image" ]; then
  echo "pins.toml has no stellar-core-buildtests image" >&2
  exit 2
fi

work=$(mktemp -d)
containers=()
cleanup() {
  for c in "${containers[@]}"; do docker rm -f -v "$c" >/dev/null 2>&1 || true; done
  rm -rf "$work"
}
trap cleanup EXIT

docker run --rm --network none "$image" test --ll FATAL --list-test-names-only '[tx]' 2>/dev/null \
  | grep -v -e '^Warning: running non-release version' -e '^$' >"$work/tests.txt"
total=$(wc -l <"$work/tests.txt")
if [ "$total" -eq 0 ]; then
  echo "stellar-core lists no [tx] tests" >&2
  exit 1
fi
# same whitelist as core's selftest-parallel: catch splits test specs on commas
if grep -n '[^-A-Za-z0-9 _():/]' "$work/tests.txt" >&2; then
  echo "test names above hold characters a catch test spec cannot carry" >&2
  exit 1
fi
for j in $(seq 0 $((jobs - 1))); do
  awk -v j="$j" -v n="$jobs" '(NR - 1) % n == j' "$work/tests.txt" >"$work/part-$j.txt"
done

# runs inside the container: one stellar-core process per batch of test names
runner='
set -eu
n=0
batch=""
count=0
run() {
  n=$((n + 1))
  echo "batch $n: $batch"
  stellar-core test --ll fatal -r simple --disable-dots --rng-seed 12345 --check-lcm /golden "$batch"
}
while IFS= read -r name; do
  if [ -z "$batch" ]; then batch=$name; else batch="$batch,$name"; fi
  count=$((count + 1))
  if [ "$count" -eq "$BATCH" ]; then run; batch=""; count=0; fi
done </part.txt
if [ -n "$batch" ]; then run; fi
'

echo "image $image"
echo "$total [tx] tests in $jobs containers, $batch per process"
start=$(date +%s)
pids=()
for j in $(seq 0 $((jobs - 1))); do
  name="shapekit-check-lcm-$$-$j"
  containers+=("$name")
  mkdir -p "$work/run-$j"
  docker run --name "$name" --network none --user "$(id -u):$(id -g)" \
    -e HOME=/run -e BATCH="$batch" \
    -v "$core_src/test-lcm-current:/golden/test-lcm-current:ro" \
    -v "$work/part-$j.txt:/part.txt:ro" \
    -v "$work/run-$j:/run" -w /run \
    --entrypoint sh "$image" -c "$runner" >"$work/log-$j.txt" 2>&1 &
  pids+=($!)
done

failed=0
for j in "${!pids[@]}"; do
  if ! wait "${pids[$j]}"; then
    failed=1
    oom=$(docker inspect -f '{{.State.OOMKilled}}' "${containers[$j]}" 2>/dev/null || echo unknown)
    echo "container $j failed (oom killed: $oom), last lines of its log:" >&2
    tail -n 40 "$work/log-$j.txt" >&2
  fi
done
elapsed=$(($(date +%s) - start))

passed=$(cat "$work"/log-*.txt | grep -c '^All tests passed' || true)
batches=$(cat "$work"/log-*.txt | grep -c '^batch [0-9]*:' || true)
echo "$batches processes, $passed passed, ${elapsed}s"
if [ "$failed" -ne 0 ] || [ "$passed" -ne "$batches" ]; then
  echo "check-lcm failed" >&2
  exit 1
fi
echo "check-lcm passed: every [tx] test's ledger close meta matches $core_src/test-lcm-current"
