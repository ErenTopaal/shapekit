"""coverage mode: the packages it measures follow the sdk's api. stellar_sdk alone up to 17.x,
and stellar_sdk, stellar_xdr and stellar_strkey on the default branch, where the xdr and strkey
code lives in those packages. skipped in a venv without coverage.py (the build of a development
checkout in docs/sdk-ci.md installs none)."""

import importlib.util
import json
import os
import subprocess
import sys
import unittest
from pathlib import Path

from shapekit_testee import sdkapi
from shapekit_testee.coverage_ops import measured_packages

MAIN = Path(__file__).resolve().parents[1] / "main.py"


@unittest.skipUnless(importlib.util.find_spec("coverage"), "coverage.py is not installed in this venv")
class CoverageUnit(unittest.TestCase):
    def test_measured_packages_follow_the_api(self) -> None:
        want = ["stellar_sdk"]
        if sdkapi.STELLAR_XDR:
            want.append("stellar_xdr")
        if sdkapi.STRKEY_CLASSES:
            want.append("stellar_strkey")
        self.assertEqual(measured_packages(), want)

    def test_snapshot_counts_lines_of_the_measured_packages(self) -> None:
        requests = [
            {"id": "1", "op": "hello"},
            {"id": "2", "op": "decode_encode", "type": "ScVal", "xdr": "AAAAAwAAAAc=", "depth_limit": 1000, "len_limit": 8},
            {"id": "3", "op": "strkey_decode", "text": "GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ"},
            {"id": "4", "op": "coverage_snapshot"},
        ]
        r = subprocess.run(
            [sys.executable, "-B", "-E", "-s", str(MAIN)],
            input="".join(json.dumps(q) + "\n" for q in requests),
            capture_output=True,
            text=True,
            env={**os.environ, "SHAPEKIT_COVERAGE": "1"},
            check=True,
        )
        responses = [json.loads(line) for line in r.stdout.splitlines()]
        self.assertEqual([x["status"] for x in responses], ["ok"] * 4, responses)
        self.assertIn("coverage", responses[0]["result"]["components"])
        packages = {unit.split("/", 1)[0] for unit in responses[3]["result"]["ranges"]}
        self.assertEqual(packages, set(measured_packages()))


if __name__ == "__main__":
    unittest.main()
