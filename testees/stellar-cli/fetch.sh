#!/usr/bin/env bash
# installs the stellar binary of one stellar-cli release as target/testees/stellar-cli/<version>/stellar,
# from the release's linux x86-64 asset, checked against the sha256 in
# testees/stellar-cli/versions/<version>/asset.sha256 and against the version and commit the
# binary prints. the verified tarball is kept next to the binary, so a later run installs from
# it without the network.
#
#   testees/stellar-cli/fetch.sh <version>          (from any directory)
#   testees/stellar-cli/fetch.sh --lock <version>   writes asset.sha256 from the digest github
#                                                    records for the asset, then installs
#
# needs: curl, sha256sum, tar, python3.
set -euo pipefail
lock=0
if [ "${1:-}" = --lock ]; then
  lock=1
  shift
fi
if [ "$#" -ne 1 ]; then
  echo "usage: $0 [--lock] <version>" >&2
  exit 2
fi
version=$1
root=$(cd "$(dirname "$0")/../.." && pwd)
asset="stellar-cli-$version-x86_64-unknown-linux-gnu.tar.gz"
url="https://github.com/stellar/stellar-cli/releases/download/v$version/$asset"
pin="$root/testees/stellar-cli/versions/$version/asset.sha256"
dest="$root/target/testees/stellar-cli/$version"
tarball="$dest/$asset"

if [ "$lock" = 1 ]; then
  # the digest github computed when the release workflow uploaded the asset
  api="https://api.github.com/repos/stellar/stellar-cli/releases/tags/v$version"
  auth=()
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    auth=(-H "Authorization: Bearer $GITHUB_TOKEN")
  fi
  digest=$(curl -fsSL --retry 3 "${auth[@]}" "$api" | python3 -I -c '
import json, sys
name = sys.argv[1]
for a in json.load(sys.stdin)["assets"]:
    if a["name"] == name:
        d = a.get("digest") or ""
        if not d.startswith("sha256:"):
            sys.exit(f"github records no sha256 digest for {name}")
        print(d[len("sha256:"):])
        break
else:
    sys.exit(f"the release has no asset {name}")
' "$asset")
  mkdir -p "$(dirname "$pin")"
  echo "$digest  $asset" >"$pin"
  echo "wrote ${pin#"$root/"}" >&2
fi

if [ ! -f "$pin" ]; then
  echo "${pin#"$root/"} does not exist; run $0 --lock $version" >&2
  exit 1
fi
read -r sha256 name <"$pin"
if [ "$name" != "$asset" ]; then
  echo "${pin#"$root/"} names $name, not $asset" >&2
  exit 1
fi

mkdir -p "$dest"
if [ -f "$tarball" ] && echo "$sha256  $tarball" | sha256sum --check --status; then
  :
else
  rm -f "$tarball.part"
  curl -fsSL --retry 3 -o "$tarball.part" "$url"
  if ! echo "$sha256  $tarball.part" | sha256sum --check --status; then
    got=$(sha256sum "$tarball.part" | cut -d' ' -f1)
    rm -f "$tarball.part"
    echo "$asset has sha256 $got, ${pin#"$root/"} pins $sha256" >&2
    exit 1
  fi
  mv "$tarball.part" "$tarball"
fi

# always install from the verified tarball, so a binary changed in place is replaced
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tar -xzf "$tarball" -C "$work" stellar
got=$("$work/stellar" --version | head -n 1)
case "$got" in
  "stellar $version ("*")") ;;
  *)
    echo "the binary in $asset reports '$got', expected stellar $version" >&2
    exit 1
    ;;
esac
install -m 0755 "$work/stellar" "$dest/stellar"
