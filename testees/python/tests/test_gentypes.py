"""the type table a development build writes (gentypes.py --version, docs/sdk-ci.md) against the
table the testee loaded: for the sdk this venv holds, both map every type to the same class, and
every class is one stellar_sdk.xdr has. run from the repository root, as test_testee.py is.

the committed table fits the released versions. a development build of the default branch
reports the version of the last release but has other classes, so its tests run from the copy
of the testee its build writes a table into (docs/sdk-ci.md), with this directory copied next to
it:
    <venv>/bin/python -B -E -s -m unittest discover -s <copy>/tests -t <copy>
"""

import importlib.metadata
import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from stellar_sdk import xdr as stellar_xdr

from shapekit_testee import types_gen

VERSION = importlib.metadata.version("stellar-sdk")


class OneVersionTable(unittest.TestCase):
    def test_matches_the_loaded_table_for_this_version(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp, "types_gen.py")
            subprocess.run(
                [sys.executable, "-I", "testees/python/gentypes.py", "--version", VERSION,
                 "--venv", sys.prefix, "--out", str(out)],
                check=True,
                stdout=subprocess.DEVNULL,
            )
            spec = importlib.util.spec_from_file_location("one_version_types_gen", out)
            assert spec is not None and spec.loader is not None
            one = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(one)
        self.assertEqual(one.VERSIONS, [VERSION])
        self.assertEqual(one.ONLY, {})
        self.assertEqual(one.COMMON, types_gen.COMMON | types_gen.ONLY.get(VERSION, {}))
        for key, name in one.COMMON.items():
            cls = getattr(stellar_xdr, name, None)
            self.assertTrue(hasattr(cls, "from_xdr_bytes"), (key, name))

    def test_version_needs_venv(self) -> None:
        r = subprocess.run(
            [sys.executable, "-I", "testees/python/gentypes.py", "--version", VERSION],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("--version and --venv go together", r.stderr)


if __name__ == "__main__":
    unittest.main()
