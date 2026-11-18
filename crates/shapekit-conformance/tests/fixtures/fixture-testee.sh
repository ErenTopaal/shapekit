#!/bin/sh
# a protocol-speaking stand-in used only by the runner's own tests, to exercise timeouts,
# crashes, bad responses, the response line limit, writes to a testee that stops reading, the
# memory limit and expected_json cross-checks. FIXTURE_MODE picks the behaviour after hello.
ops='["asset_order"]'
case "$FIXTURE_MODE" in
  json*) ops='["asset_order","decode_json"]' ;;
esac
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"op":"hello"'*)
      printf '{"id":"%s","status":"ok","result":{"protocol":1,"sdk":"fixture","sdk_version":"1.0.0","components":{},"ops":%s,"decode_limits":"ignored"}}\n' "$id" "$ops"
      # stops reading, so a request larger than the pipe buffer blocks the runner's write
      [ "$FIXTURE_MODE" = noread ] && sleep 5
      continue
      ;;
  esac
  case "$FIXTURE_MODE" in
    slow) sleep 5 ;;
    die) exit 3 ;;
    badid) printf '{"id":"x%s","status":"ok","result":{}}\n' "$id" ;;
    huge) printf '{"id":"%s","status":"ok","result":{"order":-1,"pad":"%0200d"}}\n' "$id" 0 ;;
    json7) printf '{"id":"%s","status":"ok","result":{"json":{"u32":7}}}\n' "$id" ;;
    json8) printf '{"id":"%s","status":"ok","result":{"json":{"u32":8}}}\n' "$id" ;;
    json9) printf '{"id":"%s","status":"ok","result":{"json":{"u32":9}}}\n' "$id" ;;
    limits) printf '{"id":"%s","status":"error","error":{"class":"internal","message":"as=%s data=%s"}}\n' "$id" "$(ulimit -v)" "$(ulimit -d)" ;;
    # grows the testee itself, or a process it starts, past 300 MiB
    hog) x=a; while :; do x=$x$x; done ;;
    hogchild) perl -e '$x = "a" x 400e6; sleep 5'; printf '{"id":"%s","status":"error","error":{"class":"internal","message":"child=%s"}}\n' "$id" "$?" ;;
    *) printf '{"id":"%s","status":"ok","result":{"order":-1}}\n' "$id" ;;
  esac
done
