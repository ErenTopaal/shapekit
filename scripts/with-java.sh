#!/usr/bin/env bash
# runs a command with the java testee's pinned jdk as JAVA_HOME and the pinned jdk and maven in
# front of PATH, fetching them first as scripts/java-toolchain.sh does. maven starts on
# JAVA_HOME before PATH and forks its test jvm from that jdk, so an inherited JAVA_HOME naming
# another jdk (github's runners set jdk 17) would otherwise compile and test the testees with
# that one.
#
# a mvn command that fails because maven central refused a download (its cdn has answered 404 to
# some ci runners for files it serves everywhere else) runs once more with
# scripts/maven-mirror-settings.xml, which reads central from google's mirror of it, and with
# strict checksums. any other failure is final. the retry replaces the user settings file
# (~/.m2/settings.xml) for that run.
#
# usage: scripts/with-java.sh mvn -B -f testees/java/versions/<version>/pom.xml package
#        scripts/with-java.sh printenv JAVA_HOME
set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo "usage: scripts/with-java.sh <command> [<arg>...]" >&2
  exit 2
fi
root=$(cd "$(dirname "$0")/.." && pwd)
# java-toolchain.sh finds the repository from its working directory, which here may be any
path=$(cd "$root" && scripts/java-toolchain.sh)
# the first directory java-toolchain.sh prints is the jdk's bin/
JAVA_HOME=$(dirname "${path%%:*}")
export JAVA_HOME PATH="$path:$PATH"
if [ "$1" != mvn ]; then
  exec "$@"
fi

# maven prints its errors on stdout, so stdout is copied to a log the failure is read from
log=$(mktemp)
trap 'rm -f "$log"' EXIT
set +e
"$@" | tee "$log"
status=${PIPESTATUS[0]}
set -e
if [ "$status" -eq 0 ]; then
  exit 0
fi
# a failed download names the repository it asked; a compile or test failure does not
if ! grep -qF '(https://repo.maven.apache.org/maven2' "$log"; then
  exit "$status"
fi
echo "with-java: a download from maven central failed; retrying with central read from scripts/maven-mirror-settings.xml" >&2
shift
mvn -s "$root/scripts/maven-mirror-settings.xml" --strict-checksums "$@"
