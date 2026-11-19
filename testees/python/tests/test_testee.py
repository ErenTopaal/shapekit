"""the python testee against the sdk version its venv holds. run from the repository root:
    target/testees/python/<version>/venv/bin/python -B -E -s -m unittest discover -s testees/python/tests -t testees/python
"""

import base64
import hashlib
import importlib.metadata
import io
import json
import unittest

import stellar_sdk

from shapekit_testee import sdkapi
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
        components = r["result"]["components"]
        self.assertIn("stellar-xdr" if sdkapi.STELLAR_XDR else "xdrlib3", components)
        self.assertEqual("stellar-strkey" in components, sdkapi.STRKEY_CLASSES)


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

    def test_every_kind_round_trips(self) -> None:
        key = b":" * 32
        payloads = {
            "account": key,
            "muxed_account": key + (7).to_bytes(8, "big"),
            "seed": key,
            "pre_auth_tx": key,
            "hash_x": key,
            "signed_payload": key + (5).to_bytes(4, "big") + b"hello" + bytes(3),
            "contract": key,
            "liquidity_pool": key,
            "claimable_balance": b"\x00" + key,
            "muxed_contract": key + (7).to_bytes(8, "big"),
        }
        for kind, payload in payloads.items():
            b64 = base64.b64encode(payload).decode()
            r = one({"id": "1", "op": "strkey_encode", "kind": kind, "payload": b64})
            if kind == "muxed_contract" and not sdkapi.STRKEY_CLASSES and not hasattr(stellar_sdk.StrKey, "encode_muxed_contract"):
                # W addresses came with 17.0.0
                self.assertEqual(r["status"], "unsupported", r)
                continue
            self.assertEqual(r["status"], "ok", (kind, r))
            r = one({"id": "2", "op": "strkey_decode", "text": r["result"]["text"]})
            self.assertEqual(r["result"], {"kind": kind, "payload": b64})

    def test_encode_of_a_short_payload(self) -> None:
        r = one({"id": "1", "op": "strkey_encode", "kind": "account", "payload": base64.b64encode(b":" * 31).decode()})
        if sdkapi.STRKEY_CLASSES:
            self.assertEqual(r["error"]["class"], "rejected", r)
        else:
            # StrKey.encode_ed25519_public_key writes a payload of any length
            self.assertEqual(r["status"], "ok", r)


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


class ContractId(unittest.TestCase):
    PASSPHRASE = "Test SDF Network ; September 2015"

    def expected(self, preimage: bytes) -> str:
        # HashIDPreimage: ENVELOPE_TYPE_CONTRACT_ID (8), the network id, the ContractIDPreimage
        network_id = hashlib.sha256(self.PASSPHRASE.encode()).digest()
        return hashlib.sha256((8).to_bytes(4, "big") + network_id + preimage).hexdigest()

    def test_address_and_asset_preimages(self) -> None:
        # CONTRACT_ID_PREIMAGE_FROM_ADDRESS of the account with key 32 bytes of 0x3a, salt 32 x 0x01
        address = bytes(4) + bytes(4) + bytes(4) + b":" * 32 + b"\x01" * 32
        # CONTRACT_ID_PREIMAGE_FROM_ASSET of the native asset
        asset = (1).to_bytes(4, "big") + bytes(4)
        for preimage in (address, asset):
            r = one({"id": "1", "op": "contract_id", "preimage": base64.b64encode(preimage).decode(),
                     "network_passphrase": self.PASSPHRASE, "depth_limit": 1000, "len_limit": len(preimage)})
            self.assertEqual(r["result"], {"contract_id": self.expected(preimage)})


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
