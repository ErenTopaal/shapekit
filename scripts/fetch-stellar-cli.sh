#!/usr/bin/env bash
# fetches the stellar-cli release asset pinned in pins.toml ([[release]] stellar-cli), checks it
# against the pinned sha256 and installs its `stellar` binary as
# target/tools/stellar-cli-<version>/stellar, where contracts/build.sh and
# contracts/build-legacy.sh run it. prints that path on stdout, everything else goes to stderr.
#
# usage: scripts/fetch-stellar-cli.sh
#
# the asset is the linux x86-64 build, the platform the contracts and the corpus are built on.
# the verified tarball is kept next to the binary, so a later run re-checks it and installs from
# it without the network. github also publishes a build attestation for the asset, which can be
# checked on top of the sha256 with
#   gh attestation verify target/tools/stellar-cli-<version>/<asset> --repo stellar/stellar-cli
#
# needs: curl, sha256sum, tar, python3 >= 3.11 (scripts/pin.sh).
set -euo pipefail

root=$(git rev-parse --show-toplevel)
read -r version commit url sha256 < <("$root/scripts/pin.sh" release stellar-cli)
asset=${url##*/}
dest="$root/target/tools/stellar-cli-$version"
tarball="$dest/$asset"
# what the pinned binary prints first for --version: the version and the commit it was built from
want="stellar $version ($commit)"

mkdir -p "$dest"
if [ -f "$tarball" ] && echo "$sha256  $tarball" | sha256sum --check --status; then
  echo "using the verified $asset in $dest" >&2
else
  echo "fetching $url" >&2
  rm -f "$tarball.part"
  curl -fsSL --retry 3 -o "$tarball.part" "$url"
  if ! echo "$sha256  $tarball.part" | sha256sum --check --status; then
    got=$(sha256sum "$tarball.part" | cut -d' ' -f1)
    rm -f "$tarball.part"
    echo "$asset has sha256 $got, pins.toml pins $sha256" >&2
    exit 1
  fi
  mv "$tarball.part" "$tarball"
fi

# always install from the verified tarball, so a binary changed in place is replaced
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tar -xzf "$tarball" -C "$work" stellar
got=$("$work/stellar" --version | head -n 1)
if [ "$got" != "$want" ]; then
  echo "the binary in $asset reports '$got', expected '$want'" >&2
  exit 1
fi
install -m 0755 "$work/stellar" "$dest/stellar"
echo "$got" >&2
echo "$dest/stellar"
