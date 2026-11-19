#!/bin/sh
# builds the kmp testee of one stellar-sdk-jvm version and writes versions/<version>/lib.sha256,
# the hashes of every jar maven resolved into target/testees/kmp/<version>/lib. the build in
# conformance/versions.toml checks the jars against that file, so the resolved dependency tree
# is pinned, not only the sdk version in the pom.
#
#   testees/kmp/lock.sh <version>      (from any directory; builds with the pinned jdk and maven)
set -eu
if [ "$#" -ne 1 ]; then
  echo "usage: $0 <version>" >&2
  exit 2
fi
version=$1
root=$(cd "$(dirname "$0")/../.." && pwd)
pom="$root/testees/kmp/versions/$version/pom.xml"
if [ ! -f "$pom" ]; then
  echo "$pom does not exist" >&2
  exit 1
fi
rm -rf "$root/target/testees/kmp/$version"
"$root/scripts/with-java.sh" mvn -B -q -f "$pom" -DskipTests package
cd "$root/target/testees/kmp/$version/lib"
# the jacoco agent jar the coverage build starts with sits next to lib/, so it is listed with
# its path from there and checked by the same sha256sum --check
LC_ALL=C sha256sum -- *.jar ../jacocoagent.jar > "$root/testees/kmp/versions/$version/lib.sha256"
echo "wrote testees/kmp/versions/$version/lib.sha256 ($(wc -l < "$root/testees/kmp/versions/$version/lib.sha256") jars)"
