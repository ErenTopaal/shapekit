#!/usr/bin/env bash
# picks the families whose chains `shapekit gen verify-repro` regenerates on a pull request,
# and splits them into shards for a github actions matrix.
#
# usage: scripts/select-repro.sh <rotation> <changed-files> [rotating] [shards]
#        scripts/select-repro.sh --families
#        scripts/select-repro.sh --check-chains
#
#   <rotation>       a number that moves the rotating window, the pull request number in ci,
#                    or "all" for every family
#   <changed-files>  file with one changed path per line (may be empty)
#   [rotating]       families in the rotating window (default 4)
#   [shards]         matrix width when every family is selected (default 4)
#
# the selection is the rotating window plus every family the change touches. a change to
# code or data every recipe shares (the core driver, the generator framework, the registry,
# the contracts, the pins) selects every family. the output is the matrix as json on one
# line, {"include":[{"shard":"1","families":"a b"},...]}, plus a summary on stderr. the same
# inputs always give the same output.
#
# --families prints the family list the selection works from, one per line. --check-chains
# fails unless every directory under corpus/chains belongs to one of those families and every
# family owns at least one chain. pr.yml runs both, and also passes the list to the cli, which
# rejects a family no recipe records; together they keep this list equal to the recipes the
# binary registers. nightly.yml does not use this script: it regenerates every chain.
set -euo pipefail

mode=select
case "${1:-}" in
  --families) mode=families ;;
  --check-chains) mode=check ;;
esac

if [ "$mode" = select ]; then
  rotation=${1:?usage: scripts/select-repro.sh <rotation> <changed-files> [rotating] [shards]}
  changed_file=${2:?usage: scripts/select-repro.sh <rotation> <changed-files> [rotating] [shards]}
  rotating=${3:-4}
  shards=${4:-4}

  all=false
  if [ "$rotation" = all ]; then
    all=true
    rotation=0
  fi
  case "$rotation$rotating$shards" in
    *[!0-9]*) echo "rotation, rotating and shards must be whole numbers" >&2; exit 2 ;;
  esac
  if [ "$rotating" -lt 1 ] || [ "$shards" -lt 1 ]; then
    echo "rotating and shards must be at least 1" >&2
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

# the family a chain directory belongs to: the longest family name that prefixes it, followed
# by a dot or a dash (cap67.unified-events-backfill.p22 is a cap67.unified-events chain)
family_of_chain() {
  local chain=$1 best="" f
  for f in "${families[@]}"; do
    case "$chain" in
      "$f".* | "$f"-*) if [ "${#f}" -gt "${#best}" ]; then best=$f; fi ;;
    esac
  done
  echo "$best"
}

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
  done
  for f in "${families[@]}"; do
    if [ -z "${owned[$f]:-}" ]; then
      echo "family $f has no chain under corpus/chains" >&2
      bad=1
    fi
  done
  [ "$bad" = 0 ] && echo "${#families[@]} families, every chain under corpus/chains belongs to one" >&2
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

# round robin over the sorted selection, at most one shard per family. the rotating
# window alone already spreads over several runners, which keeps the wall clock down
width=$shards
if [ "${#selected[@]}" -lt "$width" ]; then width=${#selected[@]}; fi
declare -a shard_families=()
for i in "${!selected[@]}"; do
  s=$(( i % width ))
  shard_families[s]="${shard_families[s]:+${shard_families[s]} }${selected[$i]}"
done

json='{"include":['
for s in $(seq 0 $((width - 1))); do
  [ "$s" -gt 0 ] && json+=','
  json+="{\"shard\":\"$((s + 1))\",\"families\":\"${shard_families[s]}\"}"
  echo "shard $((s + 1)): ${shard_families[s]}" >&2
done
json+=']}'
echo "$json"
