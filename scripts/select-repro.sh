#!/usr/bin/env bash
# picks the families whose chains `shapekit gen verify-repro` regenerates on a pull request,
# and splits them into shards for a github actions matrix.
#
# usage: scripts/select-repro.sh <rotation> <changed-files> [rotating] [shards]
#
#   <rotation>       a number that moves the rotating window, the pull request number in ci
#   <changed-files>  file with one changed path per line (may be empty)
#   [rotating]       families in the rotating window (default 4)
#   [shards]         matrix width when every family is selected (default 4)
#
# the selection is the rotating window plus every family the change touches. a change to
# code or data every recipe shares (the core driver, the generator framework, the registry,
# the contracts, the pins) selects every family. the output is the matrix as json on one
# line, {"include":[{"shard":"1","families":"a b"},...]}, plus a summary on stderr. the same
# inputs always give the same output.
set -euo pipefail

rotation=${1:?usage: scripts/select-repro.sh <rotation> <changed-files> [rotating] [shards]}
changed_file=${2:?usage: scripts/select-repro.sh <rotation> <changed-files> [rotating] [shards]}
rotating=${3:-4}
shards=${4:-4}

case "$rotation$rotating$shards" in
  *[!0-9]*) echo "rotation, rotating and shards must be whole numbers" >&2; exit 2 ;;
esac
if [ "$rotating" -lt 1 ] || [ "$shards" -lt 1 ]; then
  echo "rotating and shards must be at least 1" >&2
  exit 2
fi

cd "$(git rev-parse --show-toplevel)"
recipes=crates/shapekit-gen/src/recipe

# every family with a recipe, from the FAMILY constant in each recipe file, as file<tab>family
mapfile -t file_family < <(
  grep -H -o -E '^pub const FAMILY: &str = "[^"]+";' "$recipes"/*.rs |
    sed -E 's/^([^:]+):pub const FAMILY: &str = "([^"]+)";$/\1\t\2/' | sort -t$'\t' -k2
)
mapfile -t families < <(printf '%s\n' "${file_family[@]}" | cut -f2 | sort -u)
if [ "${#families[@]}" -eq 0 ]; then
  echo "no recipe declares a FAMILY constant in $recipes" >&2
  exit 1
fi

declare -A pick=()
all=false
why=()

# rotating window over the sorted family list
n=${#families[@]}
start=$(( (rotation * rotating) % n ))
for i in $(seq 0 $((rotating < n ? rotating - 1 : n - 1))); do
  pick[${families[$(( (start + i) % n ))]}]=1
done

# the family a chain directory belongs to: the longest family name that prefixes it
family_of_chain() {
  local chain=$1 best="" f
  for f in "${families[@]}"; do
    case "$chain" in
      "$f".*) if [ "${#f}" -gt "${#best}" ]; then best=$f; fi ;;
    esac
  done
  echo "$best"
}

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
  echo "every family: shared generator inputs changed (${why[*]:0:5}${why[5]:+ ...})" >&2
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
