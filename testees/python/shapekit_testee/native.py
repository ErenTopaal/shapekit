"""the lossless description of a native python value that to_native returns
(docs/testee-protocol.md, to_native)."""

import base64
from typing import Any


def describe_native(value: Any) -> Any:
    if value is None:
        return {"type": "null"}
    # bool is a subclass of int, so it is checked first
    if isinstance(value, bool):
        return {"type": "bool", "value": value}
    if isinstance(value, int):
        return {"type": "integer", "value": str(value)}
    if isinstance(value, str):
        return {"type": "string", "value": value}
    if isinstance(value, (bytes, bytearray)):
        return {"type": "bytes", "value": base64.b64encode(bytes(value)).decode("ascii")}
    if isinstance(value, list):
        return {"type": "array", "items": [describe_native(v) for v in value]}
    if isinstance(value, dict):
        return {
            "type": "map",
            "entries": [{"key": describe_native(k), "value": describe_native(v)} for k, v in value.items()],
        }
    return {"type": "object", "class": type(value).__name__, "text": str(value)}
