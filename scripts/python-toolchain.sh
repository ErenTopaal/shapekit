#!/usr/bin/env bash
# installs the pinned uv and cpython of the python testee under target/toolchains/ when they
# are not there yet, checks each download against its sha256 in pins.toml, and prints the
# directories to put in front of PATH, separated by ':'.
#
# usage: export PATH="$(scripts/python-toolchain.sh):$PATH"
#
# needs curl, sha256sum, tar and python3 >= 3.11 (for scripts/pin.sh).
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
dest=target/toolchains
mkdir -p "$dest"

dirs=()
for tool in uv python; do
  read -r tag _ url sha256 < <(scripts/pin.sh release "$tool")
  dir="$dest/$tool-$tag"
  if [ ! -f "$dir/.sha256" ] || [ "$(cat "$dir/.sha256")" != "$sha256" ]; then
    rm -rf "$dir" "$dir.tar.gz"
    mkdir -p "$dir"
    curl -fsSL --retry 3 -o "$dir.tar.gz" "$url"
    echo "$sha256  $dir.tar.gz" | sha256sum --check --strict --quiet
    tar -xzf "$dir.tar.gz" -C "$dir" --strip-components=1
    rm "$dir.tar.gz"
    echo "$sha256" >"$dir/.sha256"
  fi
  case "$tool" in
    uv) dirs+=("$PWD/$dir") ;;
    python) dirs+=("$PWD/$dir/bin") ;;
  esac
done
(IFS=:; echo "${dirs[*]}")
