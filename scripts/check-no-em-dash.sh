#!/usr/bin/env bash
# fails if any tracked file contains an em dash (U+2014). project rule, see README.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# the pattern is built from its utf-8 bytes so this script does not trip itself
pattern=$(printf '\xe2\x80\x94')

errors=$(mktemp)
trap 'rm -f "$errors"' EXIT

# xargs folds "no match" and "could not read" into one exit code,
# so read errors are caught through stderr instead
hits=$(git ls-files -z | xargs -0 -r grep -n -I -F -e "$pattern" -- 2>"$errors" || true)

if [ -s "$errors" ]; then
  cat "$errors" >&2
  exit 2
fi
if [ -n "$hits" ]; then
  echo "em dash (U+2014) found in tracked files:" >&2
  echo "$hits" >&2
  exit 1
fi
echo "no em dashes in $(git ls-files | wc -l) tracked files"
