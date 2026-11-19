#!/bin/sh
# checks the jars maven copied into a java or kmp testee build's lib/ against the version's
# committed lib.sha256: every listed file has its hash, and lib/ holds nothing the list does
# not name. sha256sum --check alone only reads the listed files, so it cannot see an extra jar.
#
#   scripts/check-jars.sh <lib dir> <lib.sha256>      (paths from the current directory)
set -eu
if [ "$#" -ne 2 ]; then
  echo "usage: $0 <lib dir> <lib.sha256>" >&2
  exit 2
fi
sums=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
cd "$1"
sha256sum --check --strict --quiet -- "$sums"
# names in lib/ itself; entries with a path (../jacocoagent.jar) sit outside it
listed=$(sed -n 's/^[0-9a-f]\{64\}  //p' "$sums" | grep -v / || true)
extra=0
for f in * .[!.]* ..?*; do
  if [ ! -e "$f" ] && [ ! -L "$f" ]; then
    continue
  fi
  if ! printf '%s\n' "$listed" | grep -qxF -- "$f"; then
    echo "$1/$f: not in $2" >&2
    extra=1
  fi
done
exit "$extra"
