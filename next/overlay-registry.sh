#!/usr/bin/env bash
# builds the next-tier registry: a copy of registry/ with the records under
# next/registry/ merged in, so `shapekit registry check --dir <out>` can check
# the protocol 31 drafts against everything they reference. the release
# registry is only read. records in next/registry/caps.toml replace the cap of
# the same id; families and fields are appended (their ids must be new).
#
# usage: next/overlay-registry.sh <empty or missing output dir>
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:?usage: next/overlay-registry.sh <output dir>}"
if [ -e "$out" ] && [ -n "$(ls -A "$out")" ]; then
    echo "$out is not empty" >&2
    exit 1
fi
mkdir -p "$out"
cp "$root"/registry/*.toml "$out"/

# ids of the caps the next tier redefines
ids="$(sed -n 's/^id = "\(CAP-[0-9-]*\)"$/\1/p' "$root/next/registry/caps.toml")"

# drop those caps' [[cap]] blocks from the release copy. a block runs from its
# [[cap]] line to the line before the next [[...]] header
awk -v ids="$ids" '
    BEGIN { n = split(ids, a, "\n"); for (i = 1; i <= n; i++) drop[a[i]] = 1 }
    function flush() { if (buf != "" && !skip) printf "%s", buf; buf = ""; skip = 0 }
    /^\[\[/ { flush(); inblock = 1 }
    inblock && /^id = "/ { id = $0; sub(/^id = "/, "", id); sub(/"$/, "", id); if (id in drop) skip = 1 }
    { if (inblock) buf = buf $0 "\n"; else print }
    END { flush() }
' "$root/registry/caps.toml" > "$out/caps.toml"

for f in caps families fields; do
    {
        printf '\n# ---- next tier, from next/registry/%s.toml\n' "$f"
        grep -v '^schema_version' "$root/next/registry/$f.toml"
    } >> "$out/$f.toml"
done
echo "next-tier registry written to $out"
