"""the type table a development build writes (gentypes.py --version, docs/sdk-ci.md) against the
committed TypesGen.kt: for the sdk's current version, both map every type to the same class.
needs that version's jar in the local maven repository, which the testee's build puts there. run
from the repository root:
    python3 -I -m unittest discover -s testees/kmp/tests
"""

import os
import re
import subprocess
import sys
import tempfile
import tomllib
import unittest

ENTRY = re.compile(r'"([^"]*)" to "([^"]*)"')
JAR = "~/.m2/repository/com/soneso/stellar/stellar-sdk-jvm/{v}/stellar-sdk-jvm-{v}.jar"


def parse(path):
    """COMMON and ONLY of a generated TypesGen.kt"""
    with open(path, encoding="utf-8") as f:
        text = f.read()
    common_part, rest = text.split("val ONLY: Map<String, Map<String, String>> = mapOf(")
    common = dict(ENTRY.findall(common_part.split("val COMMON: Map<String, String> = mapOf(")[1]))
    only = {}
    for line in rest.split("val ABSENT")[0].splitlines():
        m = re.match(r'\s*"([^"]*)" to mapOf\((.*)\),?$', line)
        if m:
            only[m.group(1)] = dict(ENTRY.findall(m.group(2)))
    return common, only


class OneVersionTable(unittest.TestCase):
    def test_matches_the_committed_table_for_the_current_version(self):
        with open("conformance/versions.toml", "rb") as f:
            v = tomllib.load(f)["sdk"]["kmp"]["current"]
        common, only = parse("testees/kmp/src/main/kotlin/org/shapekit/testee/TypesGen.kt")
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "TypesGen.kt")
            subprocess.run(
                [sys.executable, "-I", "gentypes.py", "--version", v, "--jar", os.path.expanduser(JAR.format(v=v)),
                 "--out", out],
                cwd="testees/kmp",
                check=True,
                stdout=subprocess.DEVNULL,
            )
            one_common, one_only = parse(out)
        self.assertEqual(one_only, {v: {}})
        self.assertEqual(one_common, common | only[v])

    def test_version_needs_jar(self):
        r = subprocess.run(
            [sys.executable, "-I", "gentypes.py", "--version", "0.0.0"],
            cwd="testees/kmp",
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("--version and --jar go together", r.stderr)


if __name__ == "__main__":
    unittest.main()
