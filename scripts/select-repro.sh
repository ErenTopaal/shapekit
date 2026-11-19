#!/usr/bin/env bash
# picks the families whose chains `shapekit gen verify-repro` regenerates on a pull request,
# and splits their chains into shards for a github actions matrix.
#
# usage: scripts/select-repro.sh <rotation> <changed-files> [rotating] [budget]
#        scripts/select-repro.sh --families
#        scripts/select-repro.sh --check-chains
#
#   <rotation>       a number that moves the rotating window, the pull request number in ci,
#                    or "all" for every family
#   <changed-files>  file with one changed path per line (may be empty)
#   [rotating]       families in the rotating window (default 4)
#   [budget]         seconds of recording per shard (default 360), from the per-chain
#                    seconds in scripts/repro-seconds.tsv
#
# the selection is the rotating window plus every family the change touches. a change to
# code or data every recipe shares (the core driver, the generator framework, the oracle,
# the registry, the contracts, the pins) selects every family. the oracle is in that set
# because verify-repro reruns it over the regenerated chains and compares the oracle files
# too; the corpus job in pr.yml also reruns it over the whole corpus, golden and synthetic
# entries included, which the repro shards do not cover. the output is the matrix as json on one
# line, {"include":[{"shard":"1","families":"a b"},...]}, plus a summary on stderr. the same
# inputs always give the same output. a shard regenerates whole chains, so one family can
# spread over several shards; each entry is {"shard":"1","seconds":300,"chains":"a b"}.
#
# scripts/repro-seconds.tsv holds the seconds `gen verify-repro --jobs 2` takes per chain on a
# github ubuntu-24.04 runner (both passphrases at once, so the longer of the two).
#
# --families prints the family list the selection works from, one per line: the families of
# the recipe files and those of the chains corpus/recipes/chains.toml lists. --check-chains
# fails unless every directory under corpus/chains belongs to one of those families and every
# family owns at least one chain, and unless scripts/repro-seconds.tsv lists every chain
# directory exactly once and nothing else. pr.yml runs both, and also passes the list to the cli, which
# rejects a family no recipe records; together they keep this list equal to the recipes the
# binary registers. nightly.yml does not use this script: it regenerates every chain.
set -euo pipefail

mode=select
case "${1:-}" in
  --families) mode=families ;;
  --check-chains) mode=check ;;
esac

if [ "$mode" = select ]; then
  rotation=${1:?usage: scripts/select-repro.sh <rotation> <changed-files> [rotating] [budget]}
  changed_file=${2:?usage: scripts/select-repro.sh <rotation> <changed-files> [rotating] [budget]}
  rotating=${3:-4}
  budget=${4:-360}

  all=false
  if [ "$rotation" = all ]; then
    all=true
    rotation=0
  fi
  case "$rotation$rotating$budget" in
    *[!0-9]*) echo "rotation, rotating and budget must be whole numbers" >&2; exit 2 ;;
  esac
  if [ "$rotating" -lt 1 ] || [ "$budget" -lt 1 ]; then
    echo "rotating and budget must be at least 1" >&2
    exit 2
  fi
fi

cd "$(git rev-parse --show-toplevel)"
recipes=crates/shapekit-gen/src/recipe

# every family with a recipe: each `pub const <NAME>FAMILY: &str` in a recipe file. most files
# declare one FAMILY for the recipes they hold; a file may declare more under other names, as
# old_protocols.rs does with INFLATION_FAMILY
mapfile -t families < <(
  grep -h -o -E '^pub const [A-Z_]*FAMILY: &str = "[^"]+";' "$recipes"/*.rs |
    sed -E 's/^pub const [A-Z_]*FAMILY: &str = "([^"]+)";$/\1/' | sort -u
)
if [ "${#families[@]}" -eq 0 ]; then
  echo "no recipe declares a FAMILY constant in $recipes" >&2
  exit 1
fi

# chains recorded from recipe ir files, listed in corpus/recipes/chains.toml as chain<tab>family;
# their families are recipe families too
index=corpus/recipes/chains.toml
mapfile -t indexed < <(
  [ -f "$index" ] && awk -F' = ' '
    /^\[\[chain\]\]/ { id = ""; fam = "" }
    $1 == "id" { id = $2; gsub(/"/, "", id) }
    $1 == "family" { fam = $2; gsub(/"/, "", fam); print id "\t" fam }
  ' "$index"
)
mapfile -t families < <(
  { printf '%s\n' "${families[@]}"; for entry in "${indexed[@]}"; do echo "${entry#*$'\t'}"; done; } | sort -u
)

# the family a recipe file belongs to, as file<tab>family, only for files whose recipes all
# share the one FAMILY constant. a file with recipes of several families (old_protocols.rs
# holds failed-tx, cap03 and cap26 recipes) has no entry, so a change to it selects every family
mapfile -t file_family < <(
  for file in "$recipes"/*.rs; do
    names=$(grep -c -E '^pub const [A-Z_]*FAMILY: &str' "$file" || true)
    others=$(grep -c -E 'super::[a-z0-9_]+::FAMILY' "$file" || true)
    if [ "$names" = 1 ] && [ "$others" = 0 ]; then
      f=$(sed -n -E 's/^pub const FAMILY: &str = "([^"]+)";$/\1/p' "$file")
      [ -n "$f" ] && printf '%s\t%s\n' "$file" "$f"
    fi
  done
)

# the family a chain directory belongs to: its family in corpus/recipes/chains.toml, or else
# the longest family name that prefixes it, followed by a dot or a dash
# (cap67.unified-events-backfill.p22 is a cap67.unified-events chain)
family_of_chain() {
  local chain=$1 best="" f entry
  for entry in "${indexed[@]}"; do
    if [ "${entry%%$'\t'*}" = "$chain" ]; then
      echo "${entry#*$'\t'}"
      return
    fi
  done
  for f in "${families[@]}"; do
    case "$chain" in
      "$f".* | "$f"-*) if [ "${#f}" -gt "${#best}" ]; then best=$f; fi ;;
    esac
  done
  echo "$best"
}

# seconds per chain, from scripts/repro-seconds.tsv: chain<tab>seconds, '#' starts a comment
table=scripts/repro-seconds.tsv
declare -A seconds=()
table_bad=0
while IFS=$'\t' read -r chain secs extra || [ -n "$chain" ]; do
  case "$chain" in "" | "#"*) continue ;; esac
  if [ -n "${extra:-}" ] || [ -z "${secs:-}" ] || [ -n "${secs//[0-9]/}" ] || [ "$secs" -lt 1 ]; then
    echo "$table: '$chain' needs one whole number of seconds above 0" >&2
    table_bad=1
  elif [ -n "${seconds[$chain]:-}" ]; then
    echo "$table: $chain is listed twice" >&2
    table_bad=1
  else
    seconds[$chain]=$secs
  fi
done <"$table"

if [ "$mode" = families ]; then
  printf '%s\n' "${families[@]}"
  exit 0
fi

if [ "$mode" = check ]; then
  declare -A owned=()
  bad=0
  for dir in corpus/chains/*/; do
    chain=$(basename "$dir")
    f=$(family_of_chain "$chain")
    if [ -z "$f" ]; then
      echo "chain $chain belongs to no family this script finds in $recipes" >&2
      bad=1
    else
      owned[$f]=1
    fi
    if [ -z "${seconds[$chain]:-}" ]; then
      echo "chain $chain has no seconds in $table; add the seconds gen verify-repro prints for it" >&2
      bad=1
    fi
  done
  for chain in "${!seconds[@]}"; do
    if [ ! -d "corpus/chains/$chain" ]; then
      echo "$table lists $chain, which is not under corpus/chains" >&2
      bad=1
    fi
  done
  [ "$table_bad" = 0 ] || bad=1
  for f in "${families[@]}"; do
    if [ -z "${owned[$f]:-}" ]; then
      echo "family $f has no chain under corpus/chains" >&2
      bad=1
    fi
  done
  [ "$bad" = 0 ] && echo "${#families[@]} families, every chain under corpus/chains belongs to one and has its seconds" >&2
  exit "$bad"
fi

declare -A pick=()
why=("requested")
$all || why=()

# rotating window over the sorted family list
n=${#families[@]}
start=$(( (rotation * rotating) % n ))
for i in $(seq 0 $((rotating < n ? rotating - 1 : n - 1))); do
  pick[${families[$(( (start + i) % n ))]}]=1
done

family_of_file() {
  local path=$1 entry
  for entry in "${file_family[@]}"; do
    if [ "${entry%%$'\t'*}" = "$path" ]; then
      echo "${entry#*$'\t'}"
      return
    fi
  done
}

while IFS= read -r path || [ -n "$path" ]; do
  [ -z "$path" ] && continue
  f=""
  case "$path" in
    corpus/chains/*)
      chain=${path#corpus/chains/}
      f=$(family_of_chain "${chain%%/*}")
      ;;
    corpus/recipes/chains.toml)
      all=true
      why+=("$path")
      ;;
    corpus/recipes/*)
      # a recipe ir file of a listed chain selects that chain's family
      rel=${path#corpus/recipes/}
      fam=${rel%%/*}
      for entry in "${indexed[@]}"; do [ "${entry#*$'\t'}" = "$fam" ] && f=$fam; done
      ;;
    corpus/entries/*)
      fam=${path#corpus/entries/}
      fam=${fam%%/*}
      for g in "${families[@]}"; do [ "$g" = "$fam" ] && f=$fam; done
      ;;
    "$recipes"/*/*)
      # a recipe split into a directory belongs to the recipe file of the same name
      dir=${path#"$recipes"/}
      f=$(family_of_file "$recipes/${dir%%/*}.rs")
      [ -z "$f" ] && { all=true; why+=("$path"); }
      ;;
    "$recipes"/*.rs)
      f=$(family_of_file "$path")
      [ -z "$f" ] && { all=true; why+=("$path"); }
      ;;
    crates/shapekit-gen/* | crates/shapekit-core-driver/* | crates/shapekit-manifest/* | \
      crates/shapekit-oracle/* | crates/shapekit-pack/* | crates/shapekit-registry/* | \
      registry/* | contracts/* | pins.toml | Cargo.toml | Cargo.lock | rust-toolchain.toml)
      all=true
      why+=("$path")
      ;;
  esac
  if [ -n "$f" ]; then pick[$f]=1; fi
done <"$changed_file"

if $all; then
  selected=("${families[@]}")
  echo "every family: ${why[*]:0:5}${why[5]:+ ...}" >&2
else
  mapfile -t selected < <(printf '%s\n' "${!pick[@]}" | sort)
  echo "rotation $rotation: ${#selected[@]} of $n families" >&2
fi

# the chains of the selected families, each with its seconds from the durations table,
# packed into shards: longest first, each onto the shard with the least seconds so far
# (ties to the lower shard). a shard gets at most $budget seconds unless one chain alone is
# longer, so the shard count follows the work, not the family count
[ "$table_bad" = 0 ] || exit 1
declare -A chosen=()
for g in "${selected[@]}"; do chosen[$g]=1; done
lines=()
for dir in corpus/chains/*/; do
  chain=$(basename "$dir")
  f=$(family_of_chain "$chain")
  [ -n "${chosen[$f]:-}" ] || continue
  if [ -z "${seconds[$chain]:-}" ]; then
    echo "chain $chain has no seconds in $table" >&2
    exit 1
  fi
  lines+=("$chain"$'\t'"${seconds[$chain]}")
done
python3 -I - "$budget" "${lines[@]}" <<'PY'
import json, math, sys
budget = int(sys.argv[1])
chains = [(c, int(s)) for c, s in (a.split("\t") for a in sys.argv[2:])]
total = sum(s for _, s in chains)
width = max(1, min(len(chains), math.ceil(total / budget)))
while True:
    shards = [[0, []] for _ in range(width)]
    for c, s in sorted(chains, key=lambda x: (-x[1], x[0])):
        least = min(range(width), key=lambda i: (shards[i][0], i))
        shards[least][0] += s
        shards[least][1].append(c)
    if width >= len(chains) or max(x[0] for x in shards) <= max(budget, max(s for _, s in chains)):
        break
    width += 1
include = []
for i, (s, cs) in enumerate(shards, 1):
    include.append({"shard": str(i), "seconds": s, "chains": " ".join(sorted(cs))})
    print(f"shard {i}: {len(cs)} chains, {s} s", file=sys.stderr)
print(f"{len(chains)} chains, {total} s in {width} shards of at most {budget} s", file=sys.stderr)
print(json.dumps({"include": include}, separators=(",", ":")))
PY
