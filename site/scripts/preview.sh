#!/usr/bin/env sh
# serves site/dist/client with site/Caddyfile in the pinned caddy image, so a preview behaves
# like a production host serving that file: prerendered routes, the 404 fallback, cache
# headers and the content security policy. binds 0.0.0.0 so other machines on the network can open it.
# run it again after a build: the build replaces dist/, and the container would keep
# serving the directory it was started with.
#
#   site/scripts/preview.sh          start on port 5182 (PORT=... to change)
#   site/scripts/preview.sh stop

set -eu

here=$(cd "$(dirname "$0")" && pwd)
site=$(cd "$here/.." && pwd)
name=shapekit-site-preview
image=caddy:2.11.7@sha256:685ae3d7c34d9b5f8caeddc5d14051db7bbd88ff1f5a2fa0c16bd8ca328fb7af

docker rm -f "$name" >/dev/null 2>&1 || true
[ "${1:-}" = stop ] && exit 0

if [ ! -f "$site/dist/client/index.html" ]; then
  echo "preview: run npm run build in site/ first" >&2
  exit 1
fi

docker run -d --name "$name" \
  -p "0.0.0.0:${PORT:-5182}:80" \
  -v "$site/Caddyfile:/etc/caddy/Caddyfile:ro" \
  -v "$site/dist/client:/srv/current:ro" \
  "$image" >/dev/null
echo "preview: http://0.0.0.0:${PORT:-5182}/ (docker container $name)"
