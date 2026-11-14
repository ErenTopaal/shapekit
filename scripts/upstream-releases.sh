#!/usr/bin/env bash
# compares the newest upstream releases and core's golden index with what pins.toml and the
# committed golden snapshot hold, and opens one tracking issue, carrying the protocol upgrade
# runbook's checklist, for each new upstream item.
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
upstream_cli=$(newest_tag stellar/stellar-cli '^v[0-9]+\.[0-9]+\.[0-9]+$')

# prints the six pinned versions, one per line, in the same order as above
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
[cli] = [r for r in pins["release"] if r["name"] == "stellar-cli"]
print(cli["tag"])
PY
}

mapfile -t pinned < <(read_pins)
if [ "${#pinned[@]}" -ne 6 ]; then
  echo "could not read the six pins from $pins" >&2
  exit 1
fi

names=("stellar-core" "stellar-xdr (definitions)" "@stellar/stellar-sdk" "github.com/stellar/go-stellar-sdk" "stellar-xdr (crate)" "stellar-cli")
upstream=("$upstream_core" "$upstream_xdr" "$upstream_js" "$upstream_go" "$upstream_crate" "$upstream_cli")

# one tracking issue per new upstream item; the title names the item, so a later run finds
# the issue (open or closed) instead of opening it again
titles=()
details=()

differ=0
report="| component | pinned | newest upstream | |
|---|---|---|---|"
for i in "${!names[@]}"; do
  mark=""
  if [ "${pinned[$i]}" != "${upstream[$i]}" ]; then
    mark="differs"
    differ=$((differ + 1))
    titles+=("upstream: ${names[$i]} ${upstream[$i]}")
    details+=("${names[$i]} ${upstream[$i]} is out; pins.toml pins ${pinned[$i]}.")
  fi
  report+=$'\n'"| ${names[$i]} | ${pinned[$i]} | ${upstream[$i]} | $mark |"
done

# the golden index: at the newest core release when it is not the pinned one (the pinned one
# is checked every night by nightly.yml), and the stamps on core's default branch, where a
# protocol bump of the goldens shows up before any release
golden_lines=""
if [ "$upstream_core" != "${pinned[0]}" ]; then
  golden_lines+=$("$root/scripts/golden-drift.sh" "$upstream_core")
fi
master_lines=$("$root/scripts/golden-drift.sh" master stamps)
if [ -n "$master_lines" ]; then
  golden_lines+="${golden_lines:+$'\n'}$master_lines"
fi
if [ -n "$golden_lines" ]; then
  differ=$((differ + 1))
  digest=$(printf '%s\n' "$golden_lines" | sha256sum | cut -c1-12)
  titles+=("upstream: core golden index changed ($digest)")
  details+=("The core golden index differs from corpus/goldens:"$'\n\n'"$(printf '%s\n' "$golden_lines" | sed 's/^/- /')")
fi

echo "$report"
echo
if [ -n "$golden_lines" ]; then
  echo "golden index differences:"
  echo "$golden_lines"
else
  echo "golden index: no stamp or index change upstream"
fi
echo
if [ "$differ" -eq 0 ]; then
  echo "all pins match the newest upstream releases"
else
  echo "$differ upstream changes need a tracking issue"
fi

# the checklist is kept in one place, the runbook, between two marker comments
checklist=$(awk '/<!-- checklist:end -->/{p=0} p; /<!-- checklist:start -->/{p=1}' \
  "$root/docs/protocol-upgrade-runbook.md")
if [ -z "$checklist" ]; then
  echo "no checklist between the markers in docs/protocol-upgrade-runbook.md" >&2
  exit 1
fi

if $dry_run; then
  for t in "${titles[@]}"; do echo "would open: $t"; done
  echo "dry run, skipping the issue updates"
  exit 0
fi

for i in "${!titles[@]}"; do
  title=${titles[$i]}
  existing=$(gh issue list --state all --search "\"$title\" in:title" --json number,title \
    --jq ".[] | select(.title == \"$title\") | .number" | head -n 1)
  if [ -n "$existing" ]; then
    echo "#$existing already tracks: $title"
    continue
  fi
  body="Found on $(date -u +%Y-%m-%d) by the upstream-drift workflow.

${details[$i]}

$report

Not every upstream change needs a pin move. If this one does not, close this issue with the
reason. Otherwise work through docs/protocol-upgrade-runbook.md (for a release without a
protocol change, steps 2, 3 and 8 to 11 usually suffice):

$checklist"
  gh issue create --title "$title" --body "$body"
done
