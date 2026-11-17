#!/usr/bin/env bash
# installs the pinned jdk and maven of the java testee under target/toolchains/ when they are
# not there yet, checks each download against its sha256 in pins.toml, and prints the
# directories to put in front of PATH, separated by ':'.
#
# usage: export PATH="$(scripts/java-toolchain.sh):$PATH"
#
# needs curl, sha256sum, tar and python3 >= 3.11 (for scripts/pin.sh).
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
dest=target/toolchains
mkdir -p "$dest"

# fetch <dir> <url> <sha256>
fetch() {
  local dir=$1 url=$2 sha256=$3
  if [ -f "$dir/.sha256" ] && [ "$(cat "$dir/.sha256")" = "$sha256" ]; then
    return
  fi
  rm -rf "$dir" "$dir.tar.gz"
  mkdir -p "$dir"
  curl -fsSL --retry 3 -o "$dir.tar.gz" "$url"
  echo "$sha256  $dir.tar.gz" | sha256sum --check --strict --quiet
  tar -xzf "$dir.tar.gz" -C "$dir" --strip-components=1
  rm "$dir.tar.gz"
  echo "$sha256" >"$dir/.sha256"
}

read -r tag _ url sha256 < <(scripts/pin.sh release jdk)
fetch "$dest/$tag" "$url" "$sha256"
read -r version url sha256 < <(scripts/pin.sh maven apache-maven)
fetch "$dest/maven-$version" "$url" "$sha256"
echo "$PWD/$dest/$tag/bin:$PWD/$dest/maven-$version/bin"
