#!/usr/bin/env bash
# fails if a tracked file holds text that belongs to the machine it was made on rather than to
# the project: absolute home directories and private or tailnet addresses, and every pattern of
# the private pattern file.
#
# the private patterns name people, machines and local directories, so they are not tracked:
# one perl regex per line ('#' starts a comment) in the file $SHAPEKIT_TEXT_PATTERNS, by default
# info/text-patterns in the repository's git directory, which every worktree of the clone
# shares. a clone without that file, such as a fresh one, is checked against the generic
# patterns only, and the script says so; a pattern file that is named but missing, or that
# holds no pattern, is an error.
#
#   scripts/check-public-text.sh [--locations]
#
# --locations prints only the file and line of each hit, not the text, for logs others can read
# (ci): a hit of a private pattern would otherwise print the private text it matched.
set -euo pipefail

locations=0
case "${1:-}" in
  "") ;;
  --locations) locations=1 ;;
  *)
    echo "usage: $0 [--locations]" >&2
    exit 2
    ;;
esac

cd "$(git rev-parse --show-toplevel)"

# github's runner home is the one absolute home path a workflow has to name
generic=(
  '/home/(?!runner\b)'
  '/Users/'
  '\b192\.168\.[0-9]{1,3}\.[0-9]{1,3}\b'
  '\b100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.[0-9]{1,3}\.[0-9]{1,3}\b'
)

private=()
if [ -n "${SHAPEKIT_TEXT_PATTERNS:-}" ]; then
  patterns_file=$SHAPEKIT_TEXT_PATTERNS
  if [ ! -f "$patterns_file" ]; then
    echo "SHAPEKIT_TEXT_PATTERNS names $patterns_file, which does not exist" >&2
    exit 2
  fi
else
  patterns_file=$(git rev-parse --path-format=absolute --git-common-dir)/info/text-patterns
fi
if [ -f "$patterns_file" ]; then
  mapfile -t private < <(grep -v -e '^[[:space:]]*#' -e '^[[:space:]]*$' "$patterns_file")
  if [ "${#private[@]}" -eq 0 ]; then
    echo "$patterns_file holds no pattern" >&2
    exit 2
  fi
fi

args=()
for p in "${generic[@]}" "${private[@]}"; do
  args+=(-e "$p")
done

# this script names the generic patterns, so it is left out of its own scan
set +e
hits=$(git grep -n -I -P "${args[@]}" -- . ':!scripts/check-public-text.sh')
status=$?
set -e
if [ "$status" -gt 1 ]; then
  echo "git grep failed with status $status" >&2
  exit 2
fi
if [ -n "$hits" ]; then
  echo "machine-specific or personal text in tracked files:" >&2
  if [ "$locations" -eq 1 ]; then
    echo "$hits" | cut -d: -f1,2 >&2
  else
    # the matching line is cut short so a private name in it is shown once, not in full context
    echo "$hits" | cut -c1-200 >&2
  fi
  exit 1
fi
files=$(git ls-files | wc -l)
if [ "${#private[@]}" -eq 0 ]; then
  echo "no home directory or private address in $files tracked files; the private patterns were not checked, there is no pattern file at $patterns_file"
else
  echo "no machine-specific or personal text in $files tracked files (${#private[@]} private patterns)"
fi
