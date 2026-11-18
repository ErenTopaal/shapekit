#!/bin/sh
# builds the java testee of one java-stellar-sdk version and writes versions/<version>/lib.sha256,
# the hashes of every jar maven resolved into target/testees/java/<version>/lib. the build in
# conformance/versions.toml checks the jars against that file, so the resolved dependency tree
# is pinned, not only the sdk version in the pom.
#
#   testees/java/lock.sh <version>      (from any directory; needs java and mvn on PATH)
set -eu
if [ "$#" -ne 1 ]; then
  echo "usage: $0 <version>" >&2
  exit 2
fi
version=$1
root=$(cd "$(dirname "$0")/../.." && pwd)
pom="$root/testees/java/versions/$version/pom.xml"
if [ ! -f "$pom" ]; then
  echo "$pom does not exist" >&2
  exit 1
fi
rm -rf "$root/target/testees/java/$version"
mvn -B -q -f "$pom" -DskipTests package
cd "$root/target/testees/java/$version/lib"
# the jacoco agent jar the coverage build starts with sits next to lib/, so it is listed with
# its path from there and checked by the same sha256sum --check
LC_ALL=C sha256sum -- *.jar ../jacocoagent.jar > "$root/testees/java/versions/$version/lib.sha256"
echo "wrote testees/java/versions/$version/lib.sha256 ($(wc -l < "$root/testees/java/versions/$version/lib.sha256") jars)"
