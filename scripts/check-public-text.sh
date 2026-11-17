#!/usr/bin/env bash
# fails if a tracked file holds text that belongs to the machine it was made on rather than to
# the project: absolute home directories, scratch and worktree directory names, private and
# tailnet addresses, the build machine's name, or a name from the private pattern file.
#
# the private patterns name people, so they are not tracked: one perl regex per line ('#'
# starts a comment) in the file $SHAPEKIT_TEXT_PATTERNS, by default info/text-patterns in the
# repository's git directory, which every worktree of the clone shares.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

patterns_file=${SHAPEKIT_TEXT_PATTERNS:-$(git rev-parse --path-format=absolute --git-common-dir)/info/text-patterns}
if [ ! -f "$patterns_file" ]; then
  echo "no private pattern file at $patterns_file; create it or point SHAPEKIT_TEXT_PATTERNS at one" >&2
  exit 2
fi

# github's runner home is the one absolute home path a workflow has to name
generic=(
  '/home/(?!runner\b)'
  '/Users/'
  'shapekit-(tmp|wt)\b'
  '(?i)\boffice\b'
  '\b192\.168\.[0-9]{1,3}\.[0-9]{1,3}\b'
  '\b100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.[0-9]{1,3}\.[0-9]{1,3}\b'
)
mapfile -t private < <(grep -v -e '^[[:space:]]*#' -e '^[[:space:]]*$' "$patterns_file")
if [ "${#private[@]}" -eq 0 ]; then
  echo "$patterns_file holds no pattern" >&2
  exit 2
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
  # the matching line is cut short so a private name in it is shown once, not in full context
  echo "$hits" | cut -c1-200 >&2
  exit 1
fi
echo "no machine-specific or personal text in $(git ls-files | wc -l) tracked files"
