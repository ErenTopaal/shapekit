"""unit checks of the .net testee: runs the built testee as the conformance runner does, one
request per line, and checks its answers.

    python3 -I testees/dotnet/tests/check.py target/testees/dotnet/<version>
"""

import json
import subprocess
import sys

ACCOUNT = "AAAAEgAAAAAAAAAAOjo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo="
W = "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAAWWC"


def decode(id_, type_, xdr, length):
    return {"id": id_, "op": "decode_encode", "type": type_, "xdr": xdr, "depth_limit": 1000, "len_limit": length}


def main() -> int:
    build = sys.argv[1]
    requests = [
        {"id": "hello", "op": "hello"},
        decode("roundtrip", "ScVal", "AAAAAwAAAAc=", 8),
        decode("trailing", "ScVal", "AAAAAwAAAAcA", 9),
        decode("nested", "TransactionResultResult", "AAAAAAAAAAA=", 8),
        decode("unpadded", "ScVal", "AAAAAwAAAAc", 8),
        decode("unknown", "NoSuchType", "AAAAAA==", 4),
        {"id": "json", "op": "decode_json", "type": "ScVal", "xdr": "AAAAAwAAAAc=", "depth_limit": 1000, "len_limit": 8},
        {"id": "address", "op": "address_from_scval", "xdr": ACCOUNT},
        {"id": "not-address", "op": "address_from_scval", "xdr": "AAAAAwAAAAc="},
        {"id": "fee-text", "op": "pool_id", "a": "AAAAAA==", "b": "AAAAAA==", "fee": "30"},
        {"id": "w", "op": "strkey_decode", "text": W},
        {"id": "nope", "op": "nope"},
    ]
    text = "".join(json.dumps(r) + "\n" for r in requests) + "not json\n"
    out = subprocess.run(
        ["dotnet", f"{build}/ShapekitTestee.dll"], input=text, capture_output=True, text=True, timeout=120, check=False
    )
    if out.returncode != 0:
        print(f"FAIL the testee exited with {out.returncode}: {out.stderr}")
        return 1
    answers = [json.loads(line) for line in out.stdout.splitlines()]
    by_id = {a["id"]: a for a in answers if a["id"]}
    hello = by_id["hello"]["result"]
    w_class = "ok" if hello["sdk_version"] != "16.0.0" else "rejected"

    def cls(a):
        return a["error"]["class"] if a["status"] == "error" else a["status"]

    checks = [
        ("hello names the sdk", hello["sdk"] == "stellar-dotnet-sdk" and "address_from_scval" in hello["ops"]),
        ("an ScVal round trips", by_id["roundtrip"].get("result") == {"xdr": "AAAAAwAAAAc="}),
        ("trailing bytes are a refusal", cls(by_id["trailing"]) == "rejected"),
        ("a nested class is found", cls(by_id["nested"]) == "ok"),
        ("base64 without padding is a bad request", cls(by_id["unpadded"]) == "bad-request"),
        ("a type without a class is unsupported", cls(by_id["unknown"]) == "unsupported"),
        ("decode_json is unsupported", cls(by_id["json"]) == "unsupported"),
        ("an address converts", by_id["address"].get("result", {}).get("text") == "GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ"),
        ("a u32 is no address", cls(by_id["not-address"]) == "rejected"),
        ("a fee given as text is a bad request", cls(by_id["fee-text"]) == "bad-request"),
        ("a W strkey: decoded where the sdk has the decoder, refused where not", cls(by_id["w"]) == w_class),
        ("an unknown op is a bad request", cls(by_id["nope"]) == "bad-request"),
        ("a line that is no json is a bad request with no id", answers[-1]["id"] == "" and cls(answers[-1]) == "bad-request"),
    ]
    failed = 0
    for name, ok in checks:
        print(("ok   " if ok else "FAIL ") + name)
        failed += not ok
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
