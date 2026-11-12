#!/usr/bin/env bash
# compares the newest upstream releases with what pins.toml pins, and keeps one
# open issue in this repo up to date while they differ.
#
# usage: scripts/upstream-releases.sh [--dry-run]
#   --dry-run  print the report and skip every gh call
#
# needs: git, curl, python3 >= 3.11 (for tomllib). without --dry-run also gh,
# with GH_TOKEN and GH_REPO set (the drift workflow sets both).
set -euo pipefail

dry_run=false
case "${1:-}" in
  "") ;;
  --dry-run) dry_run=true ;;
  *) echo "usage: $0 [--dry-run]" >&2; exit 2 ;;
esac

root=$(git rev-parse --show-toplevel)
pins="$root/pins.toml"
issue_title="upstream releases newer than pins.toml"

# crates.io rejects requests without a user agent
ua="shapekit-upstream-releases (https://github.com/${GH_REPO:-shapekit})"

# newest tag matching an anchored regex, by version sort. ls-remote needs no token
# and is not rate limited the way the releases api is.
newest_tag() {
  local repo=$1 re=$2 tag
  tag=$(git ls-remote --tags --refs "https://github.com/$repo" |
    awk -F/ '{print $3}' | grep -E "$re" | sort -V | tail -n 1)
  if [ -z "$tag" ]; then
    echo "no tag in $repo matches $re" >&2
    return 1
  fi
  echo "$tag"
}

json_field() {
  python3 -I -c 'import json, sys
v = json.load(sys.stdin)
for k in sys.argv[1].split("."):
    v = v[k]
print(v)' "$1"
}

upstream_core=$(newest_tag stellar/stellar-core '^v[0-9]+\.[0-9]+\.[0-9]+$')
upstream_xdr=$(newest_tag stellar/stellar-xdr '^v[0-9]+\.[0-9]+$')
upstream_js=$(curl -fsS -A "$ua" https://registry.npmjs.org/@stellar%2fstellar-sdk/latest | json_field version)
upstream_go=$(curl -fsS -A "$ua" https://proxy.golang.org/github.com/stellar/go-stellar-sdk/@latest | json_field Version)
upstream_crate=$(curl -fsS -A "$ua" https://crates.io/api/v1/crates/stellar-xdr | json_field crate.max_stable_version)

# prints the five pinned versions, one per line, in the same order as above
read_pins() {
  python3 -I - "$pins" <<'PY'
import re, sys, tomllib

with open(sys.argv[1], "rb") as f:
    pins = tomllib.load(f)

def key(v):
    return tuple(int(n) for n in re.findall(r"\d+", v.split("+")[0]))

def git_tag(name):
    [entry] = [g for g in pins["git"] if g["name"] == name]
    return entry["ref"].removeprefix("refs/tags/")

# matrix packages pin several versions; the newest pinned one is what an upstream
# release is compared against
def newest(table, field, name):
    found = [e["version"] for e in pins[table] if e[field] == name]
    if not found:
        sys.exit(f"pins.toml has no {table} entry for {name}")
    return max(found, key=key)

print(git_tag("stellar-core"))
print(git_tag("stellar-xdr"))
print(newest("npm", "name", "@stellar/stellar-sdk"))
print(newest("gomod", "module", "github.com/stellar/go-stellar-sdk"))
print(newest("crate", "name", "stellar-xdr"))
PY
}

mapfile -t pinned < <(read_pins)
if [ "${#pinned[@]}" -ne 5 ]; then
  echo "could not read the five pins from $pins" >&2
  exit 1
fi

names=("stellar-core" "stellar-xdr (definitions)" "@stellar/stellar-sdk" "github.com/stellar/go-stellar-sdk" "stellar-xdr (crate)")
upstream=("$upstream_core" "$upstream_xdr" "$upstream_js" "$upstream_go" "$upstream_crate")

differ=0
report="| component | pinned | newest upstream | |
|---|---|---|---|"
for i in "${!names[@]}"; do
  mark=""
  if [ "${pinned[$i]}" != "${upstream[$i]}" ]; then
    mark="differs"
    differ=$((differ + 1))
  fi
  report+=$'\n'"| ${names[$i]} | ${pinned[$i]} | ${upstream[$i]} | $mark |"
done

echo "$report"
echo
if [ "$differ" -eq 0 ]; then
  echo "all pins match the newest upstream releases"
else
  echo "$differ of ${#names[@]} pins differ from the newest upstream release"
fi

if $dry_run; then
  echo "dry run, skipping the issue update"
  exit 0
fi

existing=$(gh issue list --state open --search "\"$issue_title\" in:title" --json number,title \
  --jq ".[] | select(.title == \"$issue_title\") | .number" | head -n 1)

if [ "$differ" -eq 0 ]; then
  if [ -n "$existing" ]; then
    gh issue close "$existing" --comment "all pins match the newest upstream releases again."
  fi
  exit 0
fi

body="Checked on $(date -u +%Y-%m-%d) by the upstream-drift workflow.

$report

A difference is not an error by itself. Moving a pin means updating pins.toml, running
\`shapekit pins verify\` and getting CI green on the new versions. This issue is closed by the
same workflow once every pin matches again."

if [ -n "$existing" ]; then
  gh issue edit "$existing" --body "$body"
  echo "updated issue #$existing"
else
  gh issue create --title "$issue_title" --body "$body"
fi
