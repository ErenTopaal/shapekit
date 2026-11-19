"""the parts of stellar-sdk's api that differ between releases, chosen by what the installed sdk
has rather than by its version string: a development build of the default branch reports the
version of the last release while its api already differs.

up to 17.x the sdk has StrKey, one encoder and decoder per kind, and generated xdr classes on
xdrlib3 that write and read SEP-51 json as text. the default branch replaces StrKey with the
classes of stellar-strkey, which stellar_sdk.strkey re-exports, and the xdr classes with
stellar-xdr, which stellar_sdk.xdr re-exports and whose values convert to and from json values.
the two changes came in separate pull requests, so each is detected on its own."""

import importlib
import importlib.metadata
from typing import Any

import stellar_sdk
from stellar_sdk import xdr as stellar_xdr

# stellar-xdr's errors all extend XdrError; the generated classes of 17.x have no such name
STELLAR_XDR = hasattr(stellar_xdr, "XdrError")

STRKEY_CLASSES = not hasattr(stellar_sdk, "StrKey")
strkey: Any = importlib.import_module("stellar_sdk.strkey") if STRKEY_CLASSES else None

# the protocol's strkey kind of each stellar-strkey class
STRKEY_CLASS_KINDS = {
    "Ed25519PublicKey": "account",
    "MuxedEd25519Account": "muxed_account",
    "Ed25519PrivateKey": "seed",
    "PreAuthTx": "pre_auth_tx",
    "HashX": "hash_x",
    "Ed25519SignedPayload": "signed_payload",
    "Contract": "contract",
    "LiquidityPool": "liquidity_pool",
    "ClaimableBalance": "claimable_balance",
    "MuxedContract": "muxed_contract",
}


# a constructor of stellar-xdr or stellar-strkey given an argument of the wrong type refuses it
# with TypeError ("constructing a value with a wrong argument raises a plain TypeError or
# ValueError", stellar_xdr's error docs). up to 17.x a TypeError escaping an sdk call is no
# documented refusal
XDR_TYPE_REFUSALS: tuple[type[BaseException], ...] = (TypeError,) if STELLAR_XDR else ()
STRKEY_TYPE_REFUSALS: tuple[type[BaseException], ...] = (TypeError,) if STRKEY_CLASSES else ()


def components() -> dict[str, str]:
    """the packages the sdk does its xdr and strkey work with, and their versions."""
    dists = ["stellar-xdr" if STELLAR_XDR else "xdrlib3"]
    if STRKEY_CLASSES:
        dists.append("stellar-strkey")
    dists.append("pynacl")
    return {d: importlib.metadata.version(d) for d in dists}


def codec_rejections() -> tuple[type[BaseException], ...]:
    """the codec's own refusal exceptions that are not ValueErrors: xdrlib3.Error up to 17.x.
    every stellar-xdr refusal is an XdrError, which is a ValueError."""
    if STELLAR_XDR:
        return ()
    import xdrlib3

    return (xdrlib3.Error,)


def to_json(value: Any) -> Any:
    """the SEP-51 json of a decoded value: stellar-xdr's json value, or the json text of 17.x
    still to be parsed (the caller parses it under the testee's recursion limit)."""
    if STELLAR_XDR:
        return value.to_json_value()
    return value.to_json()


def from_json(cls: Any, plain: Any, text: str) -> Any:
    """reads a SEP-51 value of cls: stellar-xdr reads the json value, 17.x the json text."""
    if STELLAR_XDR:
        return cls.from_json_value(plain)
    return cls.from_json(text)


def contract_id_preimage(network_id: bytes, preimage: Any) -> Any:
    """the HashIDPreimage of a contract id, built the way the sdk's Asset.contract_id builds
    it: a union arm on stellar-xdr, the union class with its discriminant on 17.x."""
    contract_id = stellar_xdr.HashIDPreimageContractID(
        network_id=network_id if STELLAR_XDR else stellar_xdr.Hash(network_id),
        contract_id_preimage=preimage,
    )
    if STELLAR_XDR:
        return stellar_xdr.HashIDPreimage.ContractID(contract_id)
    return stellar_xdr.HashIDPreimage(stellar_xdr.EnvelopeType.ENVELOPE_TYPE_CONTRACT_ID, contract_id=contract_id)


def decode_contract(text: str) -> bytes:
    if STRKEY_CLASSES:
        return strkey.Contract.from_string(text).hash
    return stellar_sdk.StrKey.decode_contract(text)


def muxed_account_id(text: str) -> int:
    """the id inside an M address."""
    if STRKEY_CLASSES:
        return strkey.MuxedEd25519Account.from_string(text).id
    raw = stellar_sdk.StrKey.decode_med25519_public_key(text)
    return int.from_bytes(raw[32:40], "big")
