"""looks request type names up in the generated type table (types_gen.py)."""

from typing import Any

from stellar_sdk import xdr as stellar_xdr

from . import types_gen
from .protocol import Internal, Unsupported


def table(version: str) -> dict[str, str]:
    if version not in types_gen.VERSIONS:
        raise Internal(
            f"types_gen.py was generated for stellar-sdk {', '.join(types_gen.VERSIONS)}, "
            f"not {version}; run testees/python/gentypes.py"
        )
    return types_gen.COMMON | types_gen.ONLY.get(version, {})


def xdr_class(version: str, name: str) -> Any:
    py_name = table(version).get(name)
    if py_name is None:
        raise Unsupported(f"stellar-sdk {version} has no xdr type {name}")
    cls = getattr(stellar_xdr, py_name, None)
    if cls is None:
        raise Internal(f"types_gen.py names stellar_sdk.xdr.{py_name}, which stellar-sdk {version} lacks")
    return cls
