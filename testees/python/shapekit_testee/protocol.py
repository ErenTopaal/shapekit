"""the testee protocol's message shapes (docs/testee-protocol.md) and the errors ops raise to
pick a response status."""

import binascii
import decimal
import json
import struct
from collections.abc import Callable
from typing import Any, TypeVar

import xdrlib3

PROTOCOL_VERSION = 2

T = TypeVar("T")


class Unsupported(Exception):
    """the sdk version under test has no api for this op or this input."""


class BadRequest(Exception):
    """the request does not follow the protocol."""


class Internal(Exception):
    """the testee itself failed for a reason unrelated to the input."""


class SdkFailure(Exception):
    """an exception raised inside an sdk call, already classified as rejected or panic."""

    def __init__(self, error_class: str, message: str) -> None:
        super().__init__(message)
        self.error_class = error_class


# the exceptions stellar-sdk raises on purpose when it refuses an input. its docstrings name
# ValueError (stellar_sdk.exceptions.SdkError's subclasses are ValueErrors too); the generated
# xdr classes let xdrlib3 refuse short or malformed input with EOFError, xdrlib3.Error or the
# struct.error its packer raises; OverflowError is python's refusal of an int that does not fit
# a fixed width. anything else escaping an sdk call (RecursionError, TypeError, KeyError,
# AttributeError, AssertionError, ...) is not a documented refusal and counts as a crash.
REJECTIONS: tuple[type[BaseException], ...] = (
    ValueError,
    EOFError,
    OverflowError,
    struct.error,
    binascii.Error,
    xdrlib3.Error,
)


def describe(err: BaseException) -> str:
    return f"{type(err).__name__}: {err}"


def sdk(call: Callable[[], T], documented: tuple[type[BaseException], ...] = ()) -> T:
    """runs one sdk call. an exception in REJECTIONS, or one the called function documents as
    its refusal (`documented`), is the sdk refusing the input; any other is a crash."""
    try:
        return call()
    except (Unsupported, BadRequest, SdkFailure):
        raise
    except REJECTIONS + documented as err:
        raise SdkFailure("rejected", describe(err)) from err
    except BaseException as err:  # noqa: BLE001 - every escape from the sdk is classified
        if isinstance(err, (KeyboardInterrupt, SystemExit)):
            raise
        raise SdkFailure("panic", describe(err)) from err


def respond(request_id: str, run: Callable[[], dict[str, Any]]) -> dict[str, Any]:
    try:
        return {"id": request_id, "status": "ok", "result": run()}
    except Unsupported as err:
        return {"id": request_id, "status": "unsupported", "reason": str(err)}
    except BadRequest as err:
        return error(request_id, "bad-request", str(err))
    except SdkFailure as err:
        return error(request_id, err.error_class, str(err))
    except Internal as err:
        return error(request_id, "internal", str(err))
    except RecursionError as err:
        # the testee's own recursion (rendering a native value) ran out of stack
        return error(request_id, "internal", describe(err))
    except Exception as err:  # noqa: BLE001 - a testee bug is reported, never raised
        return error(request_id, "internal", describe(err))


def error(request_id: str, error_class: str, message: str) -> dict[str, Any]:
    return {"id": request_id, "status": "error", "error": {"class": error_class, "message": message}}


class Request:
    """one request; numbers that are not integers keep their exact text as decimal.Decimal, so
    id_parse sees the digits the runner wrote."""

    def __init__(self, op: str, members: dict[str, Any]) -> None:
        self.op = op
        self.members = members

    def has(self, name: str) -> bool:
        return name in self.members

    def raw(self, name: str) -> Any:
        if name not in self.members:
            raise BadRequest(f"{self.op} needs member {name}")
        return self.members[name]

    def str(self, name: str) -> str:
        value = self.raw(name)
        if not isinstance(value, str):
            raise BadRequest(f"{name} must be a string")
        return value

    def bytes(self, name: str) -> bytes:
        return base64_member(self.str(name), name)

    def int(self, name: str) -> int:
        value = self.raw(name)
        if isinstance(value, bool) or not isinstance(value, int):
            raise BadRequest(f"{name} must be an integer")
        return value

    def bool(self, name: str) -> bool:
        value = self.raw(name)
        if not isinstance(value, bool):
            raise BadRequest(f"{name} must be a boolean")
        return value


def base64_member(text: str, what: str) -> bytes:
    try:
        return binascii.a2b_base64(text.encode("ascii"), strict_mode=True)
    except (binascii.Error, UnicodeEncodeError) as err:
        raise BadRequest(f"{what} is not standard base64: {err}") from err


def parse_line(line: bytes) -> tuple[str, Request | BadRequest]:
    try:
        value = json.loads(line, parse_float=decimal.Decimal, parse_constant=reject_constant)
    except (ValueError, UnicodeDecodeError) as err:
        return "", BadRequest(f"request is not json: {err}")
    if not isinstance(value, dict):
        return "", BadRequest("request is not a json object")
    request_id = value.get("id")
    if not isinstance(request_id, str):
        return "", BadRequest("request has no string id")
    op = value.get("op")
    if not isinstance(op, str):
        return request_id, BadRequest("request has no string op")
    return request_id, Request(op, value)


def reject_constant(name: str) -> Any:
    raise ValueError(f"{name} is not json")


def plain_json(value: Any) -> Any:
    """the request value with decimals turned back into the floats json.loads gives a caller,
    for handing json to the sdk's own reader."""
    if isinstance(value, decimal.Decimal):
        return float(value)
    if isinstance(value, list):
        return [plain_json(v) for v in value]
    if isinstance(value, dict):
        return {k: plain_json(v) for k, v in value.items()}
    return value
