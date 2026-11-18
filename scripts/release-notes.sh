#!/usr/bin/env bash
# prints the release notes for a tag: the STATUS.md section of that version's release
# candidate ("## <date>: <version> release candidate", the version being the tag without its
# leading v) and every dated section above it, newest first, then the checksums of the release
# assets when a SHA256SUMS file is given. a tag whose release candidate section is missing is
# refused, so the notes never describe another version.
#
# usage: scripts/release-notes.sh <tag> [SHA256SUMS]
set -euo pipefail

tag=${1:?usage: scripts/release-notes.sh <tag> [SHA256SUMS]}
sums=${2:-}
version=${tag#v}

cd "$(git rev-parse --show-toplevel)"

# from the first "## " heading down to the end of the release candidate's section
notes=$(awk -v want="$version release candidate" '
  /^## / {
    if (found) { done = 1; exit }
    n++
    heading = $0
    sub(/^## [^:]*: /, "", heading)
    if (heading == want || index(heading, want ",") == 1) found = 1
  }
  n > 0 { print }
  END { if (!found) exit 1 }
' STATUS.md) || {
  echo "STATUS.md has no section headed \"## <date>: $version release candidate\"" >&2
  exit 1
}

echo "# shapekit $tag"
echo
echo "$notes" | sed 's/^## /## status: /'
echo
echo "The full dated log is in STATUS.md at this tag."
if [ -n "$sums" ]; then
  echo
  echo "## checksums (sha256)"
  echo
  echo '```'
  cat "$sums"
  echo '```'
  echo
  if [ -n "${GITHUB_REPOSITORY:-}" ]; then
    echo "Every asset also carries a build provenance attestation, checked with"
    echo "\`gh attestation verify <asset file> --repo $GITHUB_REPOSITORY\`."
  fi
fi
