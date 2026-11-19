#!/usr/bin/env bash
# installs the pinned jdk and maven of the java testee under target/toolchains/ when they are
# not there yet, checks each download against its sha256 in pins.toml, and prints the
# directories to put in front of PATH, separated by ':'.
#
# with --gradle-jdk it installs the pinned jdk 21 instead and prints its home directory: the
# jdk the sdk-conformance workflow sets as JAVA_HOME for the gradle build of a java-stellar-sdk
# or kmp-stellar-sdk checkout (docs/sdk-ci.md).
#
# usage: export PATH="$(scripts/java-toolchain.sh):$PATH"
#        export JAVA_HOME="$(scripts/java-toolchain.sh --gradle-jdk)"
#
# needs curl, sha256sum, tar and python3 >= 3.11 (for scripts/pin.sh).
set -euo pipefail

mode=testee
case "${1:-}" in
  "") ;;
  --gradle-jdk) mode=gradle ;;
  *)
    echo "usage: scripts/java-toolchain.sh [--gradle-jdk]" >&2
    exit 2
    ;;
esac

cd "$(git rev-parse --show-toplevel)"
dest=target/toolchains
mkdir -p "$dest"

# fetch <dir> <sha256> <url>...
# tries the urls in order until one serves the file. a source that refuses or cannot be reached
# passes to the next; bytes that do not match the sha256 stop the script, whichever source
# served them. the source that served the file is logged on stderr (stdout is the PATH line).
fetch() {
  local dir=$1 sha256=$2 url
  shift 2
  if [ -f "$dir/.sha256" ] && [ "$(cat "$dir/.sha256")" = "$sha256" ]; then
    return
  fi
  rm -rf "$dir" "$dir.tar.gz"
  mkdir -p "$dir"
  for url in "$@"; do
    if curl -fsSL --retry 3 -o "$dir.tar.gz" "$url"; then
      echo "java-toolchain: $(basename "$dir") from $url" >&2
      echo "$sha256  $dir.tar.gz" | sha256sum --check --strict --quiet
      tar -xzf "$dir.tar.gz" -C "$dir" --strip-components=1
      rm "$dir.tar.gz"
      echo "$sha256" >"$dir/.sha256"
      return
    fi
    echo "java-toolchain: $url failed, trying the next source" >&2
    rm -f "$dir.tar.gz"
  done
  echo "java-toolchain: no source served $(basename "$dir")" >&2
  return 1
}

if [ "$mode" = gradle ]; then
  read -r tag _ url sha256 < <(scripts/pin.sh release jdk-gradle)
  fetch "$dest/$tag" "$sha256" "$url"
  echo "$PWD/$dest/$tag"
  exit 0
fi

read -r tag _ url sha256 < <(scripts/pin.sh release jdk)
fetch "$dest/$tag" "$sha256" "$url"
# maven central, then its official mirrors and the apache archive (scripts/pin.sh maven-urls)
read -r version _ sha256 < <(scripts/pin.sh maven apache-maven)
urls=$(scripts/pin.sh maven-urls apache-maven)
mapfile -t urls <<<"$urls"
fetch "$dest/maven-$version" "$sha256" "${urls[@]}"
echo "$PWD/$dest/$tag/bin:$PWD/$dest/maven-$version/bin"
