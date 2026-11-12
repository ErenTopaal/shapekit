#!/usr/bin/env bash
# prints the release notes for a tag: the newest dated section of STATUS.md, then the
# checksums of the release assets when a SHA256SUMS file is given.
#
# usage: scripts/release-notes.sh <tag> [SHA256SUMS]
set -euo pipefail

tag=${1:?usage: scripts/release-notes.sh <tag> [SHA256SUMS]}
sums=${2:-}

cd "$(git rev-parse --show-toplevel)"

# the newest entry is the first "## " heading; it runs until the next one
section=$(awk '/^## /{n++} n==1' STATUS.md)
if [ -z "$section" ]; then
  echo "STATUS.md has no dated section" >&2
  exit 1
fi

echo "# shapekit $tag"
echo
echo "$section" | sed '1s/^## /## status: /'
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
