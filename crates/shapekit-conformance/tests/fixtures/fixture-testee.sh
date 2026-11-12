#!/bin/sh
# a protocol-speaking stand-in used only by the runner's own tests, to exercise timeouts,
# crashes, bad responses and the memory limit. FIXTURE_MODE picks the behaviour after hello.
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"op":"hello"'*)
      printf '{"id":"%s","status":"ok","result":{"protocol":1,"sdk":"fixture","sdk_version":"1.0.0","components":{},"ops":["asset_order"],"decode_limits":"ignored"}}\n' "$id"
      continue
      ;;
  esac
  case "$FIXTURE_MODE" in
    slow) sleep 5 ;;
    die) exit 3 ;;
    badid) printf '{"id":"x%s","status":"ok","result":{}}\n' "$id" ;;
    limits) printf '{"id":"%s","status":"error","error":{"class":"internal","message":"as=%s data=%s"}}\n' "$id" "$(ulimit -v)" "$(ulimit -d)" ;;
    *) printf '{"id":"%s","status":"ok","result":{"order":-1}}\n' "$id" ;;
  esac
done
