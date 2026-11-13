"""captures getTransactions paging and json-rpc envelope answers from a live stellar-rpc.

run once with: python3 -I capture_transactions.py <rpc-url> <out-dir>

the paging walk follows the cursor each page returns, so the fixture records how a
real server moves its cursor: which transactions each page holds, the cursor it
hands back, and what a cursor past the tip gets. transactions are summarised to
their position (ledger, applicationOrder), hash and key set, since their xdr is
large and says nothing about paging. error answers and envelope answers (batches,
scalars, empty batches) are kept whole.
"""

import datetime
import hashlib
import json
import re
import sys
import urllib.request

UA = "shapekit-fixture-capture"

# a full page is small enough to keep the capture light and large enough to span ledgers
PAGE = 3
PAGES = 12


def post(url, body):
    req = urllib.request.Request(
        url,
        data=body,
        headers={"content-type": "application/json", "user-agent": UA},
    )
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.status, r.read()


def call(url, method, params):
    body = {"jsonrpc": "2.0", "id": 1, "method": method}
    if params is not None:
        body["params"] = params
    _, raw = post(url, json.dumps(body).encode())
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


def toid(ledger, tx, op):
    return (ledger << 32) | (tx << 12) | op


def parse_toid(c):
    c = int(c)
    return {"ledger": c >> 32, "tx": (c >> 12) & 0xFFFFF, "op": c & 0xFFF}


def template(message):
    # numbers in messages are ledger sequences that depend on the server's range
    return re.sub(r"\d+", "<n>", message)


def summarise(params, resp):
    if "error" in resp:
        e = resp["error"]
        return {
            "kind": "error",
            "code": e["code"],
            "message": e["message"],
            "message_template": template(e["message"]),
        }
    res = resp["result"]
    txs = res.get("transactions", [])
    key_sets = sorted({tuple(sorted((k, jtype(v)) for k, v in t.items())) for t in txs})
    req_cursor = (params.get("pagination") or {}).get("cursor")
    return {
        "kind": "result",
        "top_keys": {k: jtype(v) for k, v in sorted(res.items())},
        "tx_key_sets": [dict(k) for k in key_sets],
        "events_keys": sorted({k for t in txs for k in t.get("events", {})}),
        "txs": [
            {"ledger": t["ledger"], "applicationOrder": t["applicationOrder"], "txHash": t["txHash"]}
            for t in txs
        ],
        "cursor": res["cursor"],
        "cursor_toid": parse_toid(res["cursor"]),
        "request_cursor": req_cursor,
        "latestLedger": res["latestLedger"],
        "oldestLedger": res["oldestLedger"],
    }


def case(url, name, params):
    raw, resp = call(url, "getTransactions", params)
    c = {
        "name": name,
        "params": params,
        "response_sha256": hashlib.sha256(raw).hexdigest(),
        "summary": summarise(params, resp),
    }
    if "error" in resp:
        c["response"] = resp
    return c


# raw bodies, sent as written: what the server does with the json-rpc envelope itself
ENVELOPES = [
    ("scalar", b"5"),
    ("string", b'"getHealth"'),
    ("null", b"null"),
    ("empty-batch", b"[]"),
    ("scalar-members", b'[1, {"jsonrpc": "2.0", "id": 1, "method": "getNetwork"}, "x"]'),
    ("no-method", b'{"jsonrpc": "2.0", "id": 3}'),
    ("wrong-version", b'{"jsonrpc": "1.0", "id": 4, "method": "getNetwork"}'),
    ("extra-field", b'{"jsonrpc": "2.0", "id": 5, "method": "getNetwork", "x": 1}'),
    ("params-scalar", b'{"jsonrpc": "2.0", "id": 6, "method": "getNetwork", "params": 1}'),
    ("id-object", b'{"jsonrpc": "2.0", "id": {"a": 1}, "method": "getNetwork"}'),
    ("null-id-notification", b'{"jsonrpc": "2.0", "id": null, "method": "getNetwork"}'),
    ("invalid-notification", b'{"jsonrpc": "1.0", "method": "getNetwork"}'),
    ("only-notifications", b'[{"jsonrpc": "2.0", "method": "getNetwork"}]'),
    (
        "batch-of-10",
        ("[" + ",".join('{"jsonrpc":"2.0","id":%d,"method":"getNetwork"}' % i for i in range(10)) + "]").encode(),
    ),
    (
        "batch-of-11",
        ("[" + ",".join('{"jsonrpc":"2.0","id":%d,"method":"getNetwork"}' % i for i in range(11)) + "]").encode(),
    ),
]


def envelope_summary(raw):
    # a getNetwork result is the same on every call; only its keys matter here
    if not raw:
        return None
    v = json.loads(raw)

    def one(r):
        if "result" in r:
            return {"id": r["id"], "result_keys": sorted(r["result"].keys())}
        return r

    return [one(r) for r in v] if isinstance(v, list) else one(v)


def main():
    url, out = sys.argv[1], sys.argv[2]
    _, version = call(url, "getVersionInfo", None)
    _, health = call(url, "getHealth", None)
    oldest = health["result"]["oldestLedger"]
    latest = health["result"]["latestLedger"]
    started = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    walk = []
    params = {"startLedger": latest - 40, "pagination": {"limit": PAGE}}
    for i in range(PAGES):
        c = case(url, f"walk-{i}", params)
        walk.append(c)
        if c["summary"]["kind"] != "result":
            break
        params = {"pagination": {"cursor": c["summary"]["cursor"], "limit": PAGE}}

    cases = [
        case(url, "json-limit-1", {"startLedger": latest - 40, "pagination": {"limit": 1}, "xdrFormat": "json"}),
        case(url, "cursor-past-tip", {"pagination": {"cursor": str(toid(latest + 1000, 0, 0))}}),
        case(url, "cursor-past-tip-mid-ledger", {"pagination": {"cursor": str(toid(latest + 1000, 7, 1))}}),
        case(url, "cursor-negative-ledger", {"pagination": {"cursor": str(-(1 << 32))}}),
        case(url, "cursor-not-a-number", {"pagination": {"cursor": "abc"}}),
        case(url, "cursor-and-start", {"startLedger": latest - 40, "pagination": {"cursor": "1"}}),
        case(url, "limit-201", {"startLedger": latest - 40, "pagination": {"limit": 201}}),
        case(url, "start-after-latest", {"startLedger": latest + 100}),
        case(url, "format-unknown", {"startLedger": latest - 40, "xdrFormat": "yaml"}),
    ]

    envelopes = []
    for name, body in ENVELOPES:
        status, raw = post(url, body)
        envelopes.append(
            {
                "name": name,
                "body": body.decode(),
                "http_status": status,
                "response_sha256": hashlib.sha256(raw).hexdigest(),
                "response": envelope_summary(raw),
            }
        )

    doc = {
        "source": url,
        "captured_at": started,
        "server_version": version["result"],
        "range_at_start": {"oldestLedger": oldest, "latestLedger": latest},
        "capture_tool": "crates/shapekit-serve/tests/fixtures/stellar-rpc/capture_transactions.py",
        "page_limit": PAGE,
        "walk": walk,
        "getTransactions": cases,
        "envelopes": envelopes,
    }
    with open(f"{out}/gettransactions.json", "w") as f:
        json.dump(doc, f, indent=2, sort_keys=True)
        f.write("\n")


main()
