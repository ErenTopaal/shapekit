"""the testee protocol operations on stellar-sdk's public api. one source runs against every
release in the matrix: where releases differ an op checks for the api it needs and answers
unsupported when the release has none."""

import base64
import decimal
import importlib.metadata
import json
import platform
from typing import Any

import stellar_sdk
from stellar_sdk import Asset, MuxedAccount, Network, TransactionBuilder, scval
from stellar_sdk import xdr as stellar_xdr
from stellar_sdk.fee_bump_transaction_envelope import FeeBumpTransactionEnvelope
from stellar_sdk.liquidity_pool_asset import LiquidityPoolAsset
from stellar_sdk.operation import ManageSellOffer
from stellar_sdk.utils import sha256

from . import sdkapi
from .native import describe_native
from .protocol import (
    PROTOCOL_VERSION,
    BadRequest,
    Internal,
    Request,
    SdkFailure,
    Unsupported,
    base64_member,
    plain_json,
    sdk,
    testee_recursion,
)
from .types import xdr_class

SDK_PACKAGE = "stellar-sdk"

# the base account of muxed ids: 32 bytes of 0x3a, the key the textual-id vectors use
MUXED_BASE = "GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ"

# StrKey (up to 17.x) has one decoder and encoder per kind; the protocol's kind, its first
# character and the StrKey method suffix
STRKEY_KINDS = {
    "account": ("G", "ed25519_public_key"),
    "muxed_account": ("M", "med25519_public_key"),
    "seed": ("S", "ed25519_secret_seed"),
    "pre_auth_tx": ("T", "pre_auth_tx"),
    "hash_x": ("X", "sha256_hash"),
    "signed_payload": ("P", "ed25519_signed_payload"),
    "contract": ("C", "contract"),
    "liquidity_pool": ("L", "liquidity_pool"),
    "claimable_balance": ("B", "claimable_balance"),
    "muxed_contract": ("W", "muxed_contract"),
}

UNSUPPORTED_OPS = {
    "map_validate": "stellar-sdk has no ScMap validation: SCMap.from_xdr_bytes and scval.from_map "
    "read entries in any key order, and the key comparison scval.to_map sorts with "
    "(scval._compare_sc_val) is private",
}


class Ops:
    def __init__(self, coverage: Any = None) -> None:
        self.coverage = coverage
        if coverage is not None:
            # coverage picked its packages from the sdk's declared dependencies before the
            # import; the api the import shows has to be the same one
            want = ["stellar_sdk"] + (["stellar_xdr"] if sdkapi.STELLAR_XDR else []) + (
                ["stellar_strkey"] if sdkapi.STRKEY_CLASSES else []
            )
            if coverage.packages != want:
                raise Internal(f"coverage measures {coverage.packages}, but the sdk's api uses {want}")
        self.version = importlib.metadata.version(SDK_PACKAGE)
        if stellar_sdk.__version__ != self.version:
            raise Internal(
                f"stellar_sdk.__version__ is {stellar_sdk.__version__} but the installed "
                f"distribution is {self.version}"
            )

    def supported(self) -> list[str]:
        ops = [
            "decode_encode",
            "decode_json",
            "encode_json",
            "strkey_decode",
            "strkey_encode",
            "asset_order",
            "pool_id",
            "map_sort",
            "id_parse",
            "to_native",
            "address_from_scval",
            "tx_hash",
            "balance_id",
            "contract_id",
        ]
        if self.coverage is not None:
            ops += ["coverage_snapshot", "coverage_new"]
        return ops

    def run(self, req: Request) -> dict[str, Any]:
        handler = {
            "hello": self.hello,
            "decode_encode": self.decode_encode,
            "decode_json": self.decode_json,
            "encode_json": self.encode_json,
            "strkey_decode": self.strkey_decode,
            "strkey_encode": self.strkey_encode,
            "asset_order": self.asset_order,
            "pool_id": self.pool_id,
            "map_sort": self.map_sort,
            "id_parse": self.id_parse,
            "to_native": self.to_native,
            "address_from_scval": self.address_from_scval,
            "tx_hash": self.tx_hash,
            "balance_id": self.balance_id,
            "contract_id": self.contract_id,
            "coverage_snapshot": self.coverage_snapshot,
            "coverage_new": self.coverage_new,
        }.get(req.op)
        if handler is not None:
            return handler(req)
        if req.op in UNSUPPORTED_OPS:
            raise Unsupported(UNSUPPORTED_OPS[req.op])
        raise BadRequest(f"unknown op {req.op}")

    def hello(self, req: Request) -> dict[str, Any]:
        components = {
            "python": f"{platform.python_implementation()} {platform.python_version()}",
            **sdkapi.components(),
        }
        if self.coverage is not None:
            components["coverage"] = importlib.metadata.version("coverage")
        return {
            "protocol": PROTOCOL_VERSION,
            "sdk": SDK_PACKAGE,
            "sdk_version": self.version,
            "components": components,
            "ops": self.supported(),
            # nothing in the sdk takes a length limit, so the sdk decodes with its defaults. up
            # to 17.x from_xdr_bytes takes no limits (the generated unpack takes a depth): depth
            # 512, any length. stellar-xdr's from_xdr_bytes takes max_depth only: depth 500,
            # any length
            "decode_limits": "ignored",
        }

    # xdr

    def decode(self, req: Request) -> Any:
        cls = xdr_class(self.version, req.str("type"))
        data = req.bytes("xdr")
        # from_xdr_bytes refuses trailing bytes itself ("Unexpected trailing ... bytes", or
        # stellar-xdr's "... unexpected bytes after the value")
        return sdk(lambda: cls.from_xdr_bytes(data))

    def decode_encode(self, req: Request) -> dict[str, Any]:
        value = self.decode(req)
        out = sdk(value.to_xdr_bytes)
        return {"xdr": base64.b64encode(out).decode("ascii")}

    def decode_json(self, req: Request) -> dict[str, Any]:
        value = self.decode(req)
        out = sdk(lambda: sdkapi.to_json(value))
        try:
            with testee_recursion():
                if isinstance(out, str):
                    return {"json": json.loads(out)}
                # a json value goes through json text, which checks that it holds only json
                return {"json": json.loads(json.dumps(out, allow_nan=False))}
        except (TypeError, ValueError) as err:
            raise Internal(f"the sdk wrote json that is not json: {err}") from err

    def encode_json(self, req: Request) -> dict[str, Any]:
        cls = xdr_class(self.version, req.str("type"))
        plain = plain_json(req.raw("json"))
        with testee_recursion():
            text = json.dumps(plain)
        value = sdk(lambda: sdkapi.from_json(cls, plain, text))
        out = sdk(value.to_xdr_bytes)
        return {"xdr": base64.b64encode(out).decode("ascii")}

    # strkey

    def strkey_decode(self, req: Request) -> dict[str, Any]:
        has_text, has_bytes = req.has("text"), req.has("text_base64")
        if has_text == has_bytes:
            raise BadRequest("strkey_decode needs exactly one of text and text_base64")
        if has_text:
            text = req.str("text")
        else:
            raw = req.bytes("text_base64")
            try:
                text = raw.decode("utf-8")
            except UnicodeDecodeError as err:
                raise Unsupported(
                    "the sdk's strkey decoders take python str, which cannot hold bytes that are not utf-8"
                ) from err
        if sdkapi.STRKEY_CLASSES:
            return self.strkey_decode_classes(text)
        for kind, (prefix, suffix) in STRKEY_KINDS.items():
            if text.startswith(prefix):
                decode = getattr(stellar_sdk.StrKey, f"decode_{suffix}", None)
                if decode is None:
                    # the protocol counts a missing decoder for a prefix as the sdk refusing it
                    raise_rejected(f"StrKey.decode_{suffix} does not exist in stellar-sdk {self.version}")
                arg: Any = text
                payload = sdk(lambda: decode(arg))
                return {"kind": kind, "payload": base64.b64encode(payload).decode("ascii")}
        raise_rejected(f"no StrKey decoder for a string starting with {text[:1]!r}")

    def strkey_encode(self, req: Request) -> dict[str, Any]:
        kind = req.str("kind")
        payload = req.bytes("payload")
        if kind not in STRKEY_KINDS:
            raise BadRequest(f"unknown strkey kind {kind}")
        if sdkapi.STRKEY_CLASSES:
            return self.strkey_encode_classes(kind, payload)
        _, suffix = STRKEY_KINDS[kind]
        encode = getattr(stellar_sdk.StrKey, f"encode_{suffix}", None)
        if encode is None:
            raise Unsupported(f"stellar-sdk {self.version} has no StrKey.encode_{suffix}")
        return {"text": sdk(lambda: encode(payload))}

    def strkey_decode_classes(self, text: str) -> dict[str, Any]:
        """stellar-strkey's generic decoder, parse, which refuses a seed by design and points
        at Ed25519PrivateKey.from_string for it."""
        strkey = sdkapi.strkey

        def decode() -> Any:
            try:
                return strkey.parse(text)
            except strkey.WrongStrkeyTypeError as err:
                if err.expected is None and err.found is strkey.Ed25519PrivateKey:
                    return strkey.Ed25519PrivateKey.from_string(text)
                raise

        key = sdk(decode)
        kind = sdkapi.STRKEY_CLASS_KINDS.get(type(key).__name__)
        if kind is None:
            raise Internal(f"stellar-strkey decoded a {type(key).__name__}, which has no protocol kind")
        # to_bytes is the binary form between the version byte and the checksum
        return {"kind": kind, "payload": base64.b64encode(sdk(key.to_bytes)).decode("ascii")}

    def strkey_encode_classes(self, kind: str, payload: bytes) -> dict[str, Any]:
        """the class of the kind built from_bytes, the binary form, and written as text: str,
        or reveal for a seed, whose str hides it."""
        name = next(n for n, k in sdkapi.STRKEY_CLASS_KINDS.items() if k == kind)
        cls = getattr(sdkapi.strkey, name, None)
        if cls is None:
            raise Unsupported(f"stellar-sdk {self.version} has no stellar_sdk.strkey.{name}")
        key = sdk(lambda: cls.from_bytes(payload))
        return {"text": sdk(key.reveal if kind == "seed" else lambda: str(key))}

    # ordering

    def asset(self, req: Request, name: str) -> Asset:
        data = req.bytes(name)
        xdr_asset = sdk(lambda: stellar_xdr.Asset.from_xdr_bytes(data))
        return sdk(lambda: Asset.from_xdr_object(xdr_asset))

    def asset_order(self, req: Request) -> dict[str, Any]:
        a, b = self.asset(req, "a"), self.asset(req, "b")
        # the sdk's only asset ordering is "a < b", which LiquidityPoolAsset checks its pair with
        lt = LiquidityPoolAsset.is_valid_lexicographic_order
        less = sdk(lambda: lt(a, b))
        greater = sdk(lambda: lt(b, a))
        return {"order": -1 if less else 1 if greater else 0}

    def pool_id(self, req: Request) -> dict[str, Any]:
        fee = req.int("fee")
        # the pair goes in as given: LiquidityPoolAsset refuses pairs it does not see in order
        a, b = self.asset(req, "a"), self.asset(req, "b")
        pool = sdk(lambda: LiquidityPoolAsset(a, b, fee))
        return {"pool_id": sdk(lambda: pool.liquidity_pool_id)}

    def map_sort(self, req: Request) -> dict[str, Any]:
        entries = req.raw("entries")
        if not isinstance(entries, list):
            raise BadRequest("map_sort needs array member entries")
        items = []
        for i, entry in enumerate(entries):
            if not isinstance(entry, dict) or not isinstance(entry.get("key"), str) or not isinstance(entry.get("val"), str):
                raise BadRequest(f"entries[{i}] needs string members key and val")
            key_bytes = base64_member(entry["key"], f"entries[{i}].key")
            val_bytes = base64_member(entry["val"], f"entries[{i}].val")
            key = sdk(lambda: stellar_xdr.SCVal.from_xdr_bytes(key_bytes))
            val = sdk(lambda: stellar_xdr.SCVal.from_xdr_bytes(val_bytes))
            items.append((key, val))
        # a dict key is hashed by the sdk's own SCVal.__hash__
        data = sdk(lambda: dict(items))
        if len(data) != len(items):
            raise Unsupported("scval.to_map takes a dict, which holds one entry per key, and the input repeats a key")
        out = sdk(lambda: scval.to_map(data).to_xdr_bytes())
        return {"xdr": base64.b64encode(out).decode("ascii")}

    # ids

    def id_parse(self, req: Request) -> dict[str, Any]:
        kind = req.str("kind")
        value = req.raw("input")
        if isinstance(value, str):
            reason = {
                "offer": "ManageSellOffer and the other offer operations take offer_id as an int, and "
                "the sdk reads no textual offer id (horizon responses are plain dicts)",
                "muxed": "MuxedAccount takes account_muxed_id as an int, and the sdk reads no textual "
                "muxed id (horizon responses are plain dicts)",
            }.get(kind)
            if reason is None:
                raise BadRequest(f"id_parse kind must be muxed or offer, not {kind!r}")
            raise Unsupported(reason)
        if isinstance(value, bool) or not isinstance(value, (int, decimal.Decimal)):
            raise BadRequest("id_parse input must be a string or a number")
        # a caller's json.loads gives an int for an integer literal and a float for any other
        # number, so that is what reaches the sdk
        number: int | float = value if isinstance(value, int) else float(value)
        if kind == "offer":
            return {"id": self.offer_id(number)}
        if kind == "muxed":
            return {"id": self.muxed_id(number)}
        raise BadRequest(f"id_parse kind must be muxed or offer, not {kind!r}")

    def offer_id(self, number: int | float) -> str:
        op = sdk(
            lambda: ManageSellOffer(
                selling=Asset.native(),
                buying=Asset("USD", MUXED_BASE),
                amount="1",
                price="1",
                offer_id=number,  # type: ignore[arg-type]
            )
        )
        # stellar-xdr's constructors refuse a field of the wrong type, a float here, with
        # TypeError, as its documentation and the sdk's changelog say
        raw = sdk(lambda: op.to_xdr_object().to_xdr_bytes(), documented=sdkapi.XDR_TYPE_REFUSALS)
        # ManageSellOfferOp ends with the int64 offer id, so the operation's xdr does too
        return str(int.from_bytes(raw[-8:], "big", signed=True))

    def muxed_id(self, number: int | float) -> str:
        muxed = sdk(lambda: MuxedAccount(MUXED_BASE, number))  # type: ignore[arg-type]
        # the id the network sees is the one in the M address
        # stellar-strkey's MuxedEd25519Account refuses an id that is not an int with TypeError
        address = sdk(lambda: muxed.account_muxed, documented=sdkapi.STRKEY_TYPE_REFUSALS)
        return str(sdk(lambda: sdkapi.muxed_account_id(address)))

    def to_native(self, req: Request) -> dict[str, Any]:
        data = req.bytes("xdr")
        value = sdk(lambda: stellar_xdr.SCVal.from_xdr_bytes(data))
        native = sdk(lambda: scval.to_native(value))
        return {"native": describe_native(native)}

    def address_from_scval(self, req: Request) -> dict[str, Any]:
        """scval.from_address, which refuses an ScVal that is not an address with ValueError,
        and the address's strkey."""
        data = req.bytes("xdr")
        value = sdk(lambda: stellar_xdr.SCVal.from_xdr_bytes(data))
        address = sdk(lambda: scval.from_address(value))
        return {"text": sdk(lambda: address.address)}

    # hashes and ids the network writes into a ledger

    def envelope(self, req: Request) -> Any:
        passphrase = req.str("network_passphrase")
        text = base64.b64encode(req.bytes("xdr")).decode("ascii")
        return sdk(lambda: TransactionBuilder.from_xdr(text, passphrase))

    def tx_hash(self, req: Request) -> dict[str, Any]:
        inner = req.bool("inner") if req.has("inner") else False
        env = self.envelope(req)
        if inner:
            if not isinstance(env, FeeBumpTransactionEnvelope):
                raise BadRequest("inner is only defined for a fee bump envelope")
            env = env.transaction.inner_transaction_envelope
        return {"hash": sdk(env.hash_hex)}

    def balance_id(self, req: Request) -> dict[str, Any]:
        index = req.int("op_index")
        if index < 0:
            raise BadRequest("op_index must not be negative")
        env = self.envelope(req)
        # the id comes from the transaction that holds the operation, the inner one of a fee bump
        if isinstance(env, FeeBumpTransactionEnvelope):
            env = env.transaction.inner_transaction_envelope
        tx = env.transaction
        # get_claimable_balance_id documents IndexError and TypeError as its refusals
        balance = sdk(lambda: tx.get_claimable_balance_id(index), documented=(IndexError, TypeError))
        return {"balance_id": balance.lower()}

    def contract_id(self, req: Request) -> dict[str, Any]:
        """Asset.contract_id is the sdk's api for an asset's contract; a preimage from an address
        has no api of its own, so its id is the sdk's sha256 of the sdk's encoding of the
        HashIDPreimage, the way Asset.contract_id builds it."""
        passphrase = req.str("network_passphrase")
        data = req.bytes("preimage")
        preimage_cls = xdr_class(self.version, "ContractIdPreimage")
        preimage = sdk(lambda: preimage_cls.from_xdr_bytes(data))
        if preimage.type == stellar_xdr.ContractIDPreimageType.CONTRACT_ID_PREIMAGE_FROM_ASSET:
            asset = sdk(lambda: Asset.from_xdr_object(preimage.from_asset))
            text = sdk(lambda: asset.contract_id(passphrase))
            raw = sdk(lambda: sdkapi.decode_contract(text))
            return {"contract_id": raw.hex()}

        def address_id() -> bytes:
            full = sdkapi.contract_id_preimage(Network(passphrase).network_id(), preimage)
            return sha256(full.to_xdr_bytes())

        return {"contract_id": sdk(address_id).hex()}

    # coverage

    def coverage_snapshot(self, req: Request) -> dict[str, Any]:
        if self.coverage is None:
            raise Unsupported("coverage is collected only when the testee runs with SHAPEKIT_COVERAGE=1")
        return self.coverage.snapshot()

    def coverage_new(self, req: Request) -> dict[str, Any]:
        if self.coverage is None:
            raise Unsupported("coverage is collected only when the testee runs with SHAPEKIT_COVERAGE=1")
        return self.coverage.fresh()


def raise_rejected(message: str) -> Any:
    raise SdkFailure("rejected", message)
