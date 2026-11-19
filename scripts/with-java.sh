#!/usr/bin/env bash
# runs a command with the java testee's pinned jdk as JAVA_HOME and the pinned jdk and maven in
# front of PATH, fetching them first as scripts/java-toolchain.sh does. maven starts on
# JAVA_HOME before PATH and forks its test jvm from that jdk, so an inherited JAVA_HOME naming
# another jdk (github's runners set jdk 17) would otherwise compile and test the testees with
# that one.
#
# usage: scripts/with-java.sh mvn -B -f testees/java/versions/<version>/pom.xml package
#        scripts/with-java.sh printenv JAVA_HOME
set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo "usage: scripts/with-java.sh <command> [<arg>...]" >&2
  exit 2
fi
# java-toolchain.sh finds the repository from its working directory, which here may be any
path=$(cd "$(dirname "$0")/.." && scripts/java-toolchain.sh)
# the first directory java-toolchain.sh prints is the jdk's bin/
JAVA_HOME=$(dirname "${path%%:*}")
export JAVA_HOME PATH="$path:$PATH"
exec "$@"
