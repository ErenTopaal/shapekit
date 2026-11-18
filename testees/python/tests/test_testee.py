"""the python testee against the sdk version its venv holds. run from the repository root:
    target/testees/python/<version>/venv/bin/python -B -E -s -m unittest discover -s testees/python/tests -t testees/python
"""

import base64
import importlib.metadata
import io
import json
import unittest

from shapekit_testee.serve import serve
from shapekit_testee.types import table

VERSION = importlib.metadata.version("stellar-sdk")


def run(*requests: str) -> list[dict]:
    out = io.StringIO()
    serve(io.BytesIO("".join(r + "\n" for r in requests).encode()), out)
    return [json.loads(line) for line in out.getvalue().splitlines()]


def one(request: dict) -> dict:
    (response,) = run(json.dumps(request))
    return response


U32_7 = "AAAAAwAAAAc="


class Hello(unittest.TestCase):
    def test_reports_the_installed_sdk(self) -> None:
        r = one({"id": "0", "op": "hello"})
        self.assertEqual(r["status"], "ok")
        self.assertEqual(r["result"]["protocol"], 2)
        self.assertEqual(r["result"]["sdk"], "stellar-sdk")
        self.assertEqual(r["result"]["sdk_version"], VERSION)
        self.assertEqual(r["result"]["decode_limits"], "ignored")
        self.assertNotIn("coverage_snapshot", r["result"]["ops"])
        self.assertIn("xdrlib3", r["result"]["components"])


class RequestErrors(unittest.TestCase):
    def test_malformed_requests_are_bad_requests(self) -> None:
        got = run(
            "not json",
            '{"op":"hello"}',
            '{"id":7,"op":"hello"}',
            '{"id":"a"}',
            '{"id":"b","op":"frobnicate"}',
            '{"id":"c","op":"strkey_decode"}',
            '{"id":"d","op":"decode_encode","type":"ScVal","xdr":"!!","depth_limit":1000,"len_limit":4}',
            '{"id":"e","op":"tx_hash","xdr":"AAAA","network_passphrase":"x","inner":1}',
        )
        self.assertEqual([r["id"] for r in got], ["", "", "", "a", "b", "c", "d", "e"])
        for r in got:
            self.assertEqual((r["status"], r["error"]["class"]), ("error", "bad-request"), r)

    def test_one_response_per_line_and_ids_echo(self) -> None:
        got = run('{"id":"x","op":"hello"}', "", '{"id":"y","op":"hello"}')
        self.assertEqual([r["id"] for r in got], ["x", "y"])


class Xdr(unittest.TestCase):
    def test_round_trip_and_json(self) -> None:
        base = {"type": "ScVal", "xdr": U32_7, "depth_limit": 1000, "len_limit": 8}
        self.assertEqual(one({"id": "1", "op": "decode_encode", **base})["result"], {"xdr": U32_7})
        self.assertEqual(one({"id": "2", "op": "decode_json", **base})["result"], {"json": {"u32": 7}})
        r = one({"id": "3", "op": "encode_json", "type": "ScVal", "json": {"u32": 7}})
        self.assertEqual(r["result"], {"xdr": U32_7})

    def test_trailing_and_short_input_are_rejected(self) -> None:
        for data in ("AAAAAwAAAAcA", "AAAAAwAA"):
            r = one({"id": "1", "op": "decode_encode", "type": "ScVal", "xdr": data, "depth_limit": 1000, "len_limit": 9})
            self.assertEqual(r["error"]["class"], "rejected", r)

    def test_unknown_type_is_unsupported(self) -> None:
        r = one({"id": "1", "op": "decode_encode", "type": "NoSuchType", "xdr": U32_7, "depth_limit": 1, "len_limit": 8})
        self.assertEqual(r["status"], "unsupported")

    def test_table_holds_the_manifest_types(self) -> None:
        t = table(VERSION)
        for name in ("TransactionEnvelope", "LedgerCloseMeta", "ScVal", "TransactionMeta", "SError"):
            self.assertIn(name, t)


class Strkey(unittest.TestCase):
    ACCOUNT = "GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ"

    def test_decode_and_encode(self) -> None:
        r = one({"id": "1", "op": "strkey_decode", "text": self.ACCOUNT})
        self.assertEqual(r["result"], {"kind": "account", "payload": base64.b64encode(b":" * 32).decode()})
        r = one({"id": "2", "op": "strkey_encode", "kind": "account", "payload": base64.b64encode(b":" * 32).decode()})
        self.assertEqual(r["result"], {"text": self.ACCOUNT})

    def test_refusals(self) -> None:
        for text in ("", "Q" + self.ACCOUNT[1:], self.ACCOUNT[:-1] + "A", self.ACCOUNT + "\n"):
            r = one({"id": "1", "op": "strkey_decode", "text": text})
            self.assertEqual(r["error"]["class"], "rejected", (text, r))
        r = one({"id": "2", "op": "strkey_decode", "text_base64": base64.b64encode(b"G\xff").decode()})
        self.assertEqual(r["status"], "unsupported")


class Ids(unittest.TestCase):
    def test_numbers_reach_the_sdk_and_strings_have_no_api(self) -> None:
        self.assertEqual(one({"id": "1", "op": "id_parse", "kind": "offer", "input": 16})["result"], {"id": "16"})
        big = '{"id":"2","op":"id_parse","kind":"muxed","input":18446744073709551615}'
        self.assertEqual(run(big)[0]["result"], {"id": "18446744073709551615"})
        negative = '{"id":"3","op":"id_parse","kind":"muxed","input":-1}'
        self.assertEqual(run(negative)[0]["error"]["class"], "rejected")
        for kind in ("offer", "muxed"):
            self.assertEqual(one({"id": "4", "op": "id_parse", "kind": kind, "input": "16"})["status"], "unsupported")


class Native(unittest.TestCase):
    def test_integer_and_map(self) -> None:
        self.assertEqual(one({"id": "1", "op": "to_native", "xdr": U32_7})["result"], {"native": {"type": "integer", "value": "7"}})
        # {u32 1: u32 7}
        m = "AAAAEQAAAAEAAAABAAAAAwAAAAEAAAADAAAABw=="
        native = one({"id": "2", "op": "to_native", "xdr": m})["result"]["native"]
        self.assertEqual(native["type"], "map")
        self.assertEqual(native["entries"], [{"key": {"type": "integer", "value": "1"}, "value": {"type": "integer", "value": "7"}}])


class Address(unittest.TestCase):
    # SCV_ADDRESS of the account with key 32 bytes of 0x3a
    ACCOUNT = "AAAAEgAAAAAAAAAAOjo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo="

    def test_address_from_scval(self) -> None:
        r = one({"id": "1", "op": "address_from_scval", "xdr": self.ACCOUNT})
        self.assertEqual(r["result"], {"text": "GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ"})
        r = one({"id": "2", "op": "address_from_scval", "xdr": U32_7})
        self.assertEqual(r["error"]["class"], "rejected")


class Ordering(unittest.TestCase):
    NATIVE = "AAAAAA=="

    def test_native_against_itself(self) -> None:
        r = one({"id": "1", "op": "asset_order", "a": self.NATIVE, "b": self.NATIVE})
        self.assertEqual(r["result"], {"order": 0})
        r = one({"id": "2", "op": "pool_id", "a": self.NATIVE, "b": self.NATIVE, "fee": 30})
        self.assertEqual(r["error"]["class"], "rejected")

    def test_map_sort_and_validate(self) -> None:
        one_ = "AAAAAwAAAAE="
        sym = "AAAADwAAAAFhAAAA"
        r = one({"id": "1", "op": "map_sort", "entries": [{"key": sym, "val": U32_7}, {"key": one_, "val": U32_7}]})
        self.assertEqual(r["status"], "ok")
        r = one({"id": "2", "op": "map_sort", "entries": [{"key": one_, "val": U32_7}, {"key": one_, "val": U32_7}]})
        self.assertEqual(r["status"], "unsupported")
        self.assertEqual(one({"id": "3", "op": "map_validate", "xdr": U32_7})["status"], "unsupported")


if __name__ == "__main__":
    unittest.main()
