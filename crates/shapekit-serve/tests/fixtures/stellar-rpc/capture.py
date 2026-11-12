"""captures getLedgers responses from a live stellar-rpc for shapekit-serve's conformance test.

run once with: python3 -I capture.py <rpc-url> <out-dir>

every request is written as a template whose ledger numbers are relative to the
server's oldest and latest ledger at capture time, so the same shapes can be
replayed against a chain with a different range. responses that hold many
ledgers are summarised (key sets, value types, counts, cursor relation, outer
structure of xdr-json bodies); every error response is kept whole.
"""

import datetime
import hashlib
import json
import re
import sys
import urllib.request

UA = "shapekit-fixture-capture"


def call(url, method, params):
    body = {"jsonrpc": "2.0", "id": 1, "method": method}
    if params is not None:
        body["params"] = params
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode(),
        headers={"content-type": "application/json", "user-agent": UA},
    )
    with urllib.request.urlopen(req, timeout=120) as r:
        raw = r.read()
    return raw, json.loads(raw)


def jtype(v):
    if isinstance(v, bool):
        return "boolean"
    if isinstance(v, (int, float)):
        return "number"
    if isinstance(v, str):
        return "string"
    if isinstance(v, list):
        return "array"
    if isinstance(v, dict):
        return "object"
    return "null"


def template(message):
    # numbers in messages are ledger sequences that depend on the server's range
    return re.sub(r"\d+", "<n>", message)


def summarise(resp):
    if "error" in resp:
        e = resp["error"]
        return {
            "kind": "error",
            "code": e["code"],
            "message": e["message"],
            "message_template": template(e["message"]),
        }
    res = resp["result"]
    ledgers = res.get("ledgers", [])
    key_sets = {tuple(sorted((k, jtype(v)) for k, v in l.items())) for l in ledgers}
    if len(key_sets) > 1:
        raise SystemExit("ledgers in one response have different keys")
    seqs = [l["sequence"] for l in ledgers]
    return {
        "kind": "result",
        "top_keys": {k: jtype(v) for k, v in sorted(res.items())},
        "ledger_keys": dict(next(iter(key_sets))) if key_sets else {},
        "ledger_count": len(ledgers),
        "first_sequence": seqs[0] if seqs else None,
        "last_sequence": seqs[-1] if seqs else None,
        "contiguous": seqs == list(range(seqs[0], seqs[0] + len(seqs))) if seqs else True,
        "cursor": res["cursor"],
        "latestLedger": res["latestLedger"],
        "oldestLedger": res["oldestLedger"],
        "json_shapes": json_shapes(ledgers),
    }


def json_shapes(ledgers):
    # the xdr-json bodies are large, so only their outer structure is kept
    out = {}
    for l in ledgers[:1]:
        if "headerJson" in l:
            out["headerJson"] = sorted(l["headerJson"].keys())
        if "metadataJson" in l:
            arm, body = next(iter(l["metadataJson"].items()))
            out["metadataJson"] = {arm: sorted(body.keys())}
    return out


def resolve(v, oldest, latest):
    if isinstance(v, dict):
        return {k: resolve(x, oldest, latest) for k, x in v.items()}
    if isinstance(v, str) and v.startswith("$"):
        base, _, off = v[1:].partition("+") if "+" in v else v[1:].partition("-")
        n = {"oldest": oldest, "latest": latest}[base]
        if off:
            n = n + int(off) if "+" in v else n - int(off)
        return n
    if isinstance(v, str) and v.startswith("#"):
        return str(resolve("$" + v[1:], oldest, latest))
    return v


# "$oldest+N" / "$latest-N" become numbers, "#..." the same as a string (cursors)
CASES = [
    ("start-default-limit", {"startLedger": "$oldest+1000"}),
    ("start-near-latest", {"startLedger": "$latest-4"}),
    ("start-at-latest", {"startLedger": "$latest"}),
    ("limit-1", {"startLedger": "$oldest+1000", "pagination": {"limit": 1}}),
    ("limit-0-means-default", {"startLedger": "$oldest+1000", "pagination": {"limit": 0}}),
    ("limit-50", {"startLedger": "$oldest+1000", "pagination": {"limit": 50}}),
    ("limit-200", {"startLedger": "$oldest+1000", "pagination": {"limit": 200}}),
    ("limit-201", {"startLedger": "$oldest+1000", "pagination": {"limit": 201}}),
    ("cursor-page", {"pagination": {"cursor": "#oldest+1000", "limit": 3}}),
    ("cursor-at-latest", {"pagination": {"cursor": "#latest"}}),
    ("cursor-before-oldest", {"pagination": {"cursor": "#oldest-2"}}),
    ("cursor-not-a-number", {"pagination": {"cursor": "abc"}}),
    ("cursor-and-start", {"startLedger": "$oldest+1000", "pagination": {"cursor": "#oldest+1000"}}),
    ("start-before-oldest", {"startLedger": "$oldest-1"}),
    ("start-after-latest", {"startLedger": "$latest+100"}),
    ("start-missing", {}),
    ("format-json-limit-1", {"startLedger": "$oldest+1000", "pagination": {"limit": 1}, "xdrFormat": "json"}),
    ("format-base64-limit-1", {"startLedger": "$oldest+1000", "pagination": {"limit": 1}, "xdrFormat": "base64"}),
    ("format-unknown", {"startLedger": "$oldest+1000", "xdrFormat": "yaml"}),
    ("start-wrong-type", {"startLedger": "ten"}),
]


def main():
    url, out = sys.argv[1], sys.argv[2]
    _, version = call(url, "getVersionInfo", None)
    _, health = call(url, "getHealth", None)
    oldest = health["result"]["oldestLedger"]
    latest = health["result"]["latestLedger"]
    started = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    cases = []
    follow = None
    for name, tmpl in CASES:
        params = resolve(tmpl, oldest, latest)
        raw, resp = call(url, "getLedgers", params)
        s = summarise(resp)
        case = {
            "name": name,
            "template": tmpl,
            "params": params,
            "response_sha256": hashlib.sha256(raw).hexdigest(),
            "summary": s,
        }
        if s["kind"] == "error":
            case["response"] = resp
        cases.append(case)
        if name == "cursor-page":
            follow = s["cursor"]
    # the next page, from the cursor the server just returned
    raw, resp = call(url, "getLedgers", {"pagination": {"cursor": follow, "limit": 3}})
    cases.append(
        {
            "name": "cursor-page-next",
            "template": {"pagination": {"cursor": "<cursor of cursor-page>", "limit": 3}},
            "params": {"pagination": {"cursor": follow, "limit": 3}},
            "response_sha256": hashlib.sha256(raw).hexdigest(),
            "summary": summarise(resp),
        }
    )
    raw, resp = call(url, "noSuchMethod", None)
    other = [{"name": "unknown-method", "method": "noSuchMethod", "response": resp}]
    for m in ["getHealth", "getNetwork", "getVersionInfo", "getLatestLedger"]:
        raw, resp = call(url, m, None)
        res = resp["result"]
        other.append(
            {
                "name": m,
                "method": m,
                "response_sha256": hashlib.sha256(raw).hexdigest(),
                "top_keys": {k: jtype(v) for k, v in sorted(res.items())},
            }
        )
    doc = {
        "source": url,
        "captured_at": started,
        "server_version": version["result"],
        "range_at_start": {"oldestLedger": oldest, "latestLedger": latest},
        "capture_tool": "crates/shapekit-serve/tests/fixtures/stellar-rpc/capture.py",
        "getLedgers": cases,
        "other_methods": other,
    }
    with open(f"{out}/getledgers.json", "w") as f:
        json.dump(doc, f, indent=2, sort_keys=True)
        f.write("\n")


main()
