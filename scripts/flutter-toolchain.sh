#!/usr/bin/env bash
# installs the pinned flutter sdk of the dart testee under target/toolchains/ when it is not
# there yet, checks the archive against its sha256 in pins.toml, and prints the directory to put
# in front of PATH.
#
# usage: export PATH="$(scripts/flutter-toolchain.sh):$PATH"
#
# needs curl, sha256sum, tar, xz and python3 >= 3.11 (for scripts/pin.sh).
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
dest=target/toolchains
mkdir -p "$dest"

read -r version artifact sha256 < <(scripts/pin.sh toolchain flutter)
# the stable channel's archive path, as flutter's release index (releases_linux.json) names it
url=https://storage.googleapis.com/flutter_infra_release/releases/stable/linux/$artifact
dir=$dest/flutter-$version
if [ ! -f "$dir/.sha256" ] || [ "$(cat "$dir/.sha256")" != "$sha256" ]; then
  rm -rf "$dir" "$dir.tar.xz"
  mkdir -p "$dir"
  curl -fsSL --retry 3 -o "$dir.tar.xz" "$url"
  echo "$sha256  $dir.tar.xz" | sha256sum --check --strict --quiet
  tar -xJf "$dir.tar.xz" -C "$dir" --strip-components=1
  rm "$dir.tar.xz"
  echo "$sha256" >"$dir/.sha256"
fi
echo "$PWD/$dir/bin"
