"""coverage_snapshot and coverage_new through coverage.py, started before stellar_sdk is
imported. a unit is a line of a measured package's source file that has run: stellar_sdk, and
on the default branch's api also stellar_xdr and stellar_strkey, which hold the xdr and strkey
code there."""

import importlib.metadata
import re
from pathlib import Path
from typing import Any

# the packages the sdk does its xdr and strkey work in when it depends on them (the default
# branch); up to 17.x that code is in stellar_sdk itself
DELEGATES = {"stellar-xdr": "stellar_xdr", "stellar-strkey": "stellar_strkey"}


def measured_packages() -> list[str]:
    """stellar_sdk, and the delegate packages the installed sdk declares as dependencies. read
    from the distribution's metadata, because importing stellar_sdk to look at its api would run
    module bodies before coverage starts. Ops checks the choice against the api sdkapi detects
    once the sdk is imported."""
    names = set()
    for req in importlib.metadata.requires("stellar-sdk") or []:
        if ";" in req and "extra ==" in req.split(";", 1)[1]:
            continue
        name = re.split(r"[\s<>=!~\[;(]", req, maxsplit=1)[0]
        names.add(name.lower().replace("_", "-"))
    return ["stellar_sdk"] + [pkg for dist, pkg in DELEGATES.items() if dist in names]


class Coverage:
    def __init__(self, cov: Any, coverage_module: Any, packages: list[str]) -> None:
        self.cov = cov
        self.module = coverage_module
        self.packages = packages
        self.reported = 0

    @classmethod
    def start(cls) -> "Coverage":
        import coverage

        packages = measured_packages()
        # no data file: the counters are read in the process and never written to disk
        cov = coverage.Coverage(data_file=None, source_pkgs=packages, branch=False, config_file=False)
        cov.start()
        return cls(cov, coverage, packages)

    def lines(self) -> list[str]:
        data = self.cov.get_data()
        out = []
        for path in data.measured_files():
            rel = package_path(path, self.packages)
            for line in data.lines(path) or ():
                out.append(f"{rel}:{line}")
        return sorted(set(out))

    def snapshot(self) -> dict[str, Any]:
        tool = f"coverage.py {self.module.__version__} line coverage"
        return {"tool": tool, "ranges": self.lines()}

    def fresh(self) -> dict[str, Any]:
        total = len(self.lines())
        new, self.reported = total - self.reported, total
        return {"new": new, "total": total}


def package_path(path: str, packages: list[str]) -> str:
    """a file's path from its measured package's directory on: stellar_sdk/xdr/sc_val.py,
    stellar_xdr/_contract.py."""
    parts = Path(path).parts
    for i in range(len(parts) - 1, -1, -1):
        if parts[i] in packages:
            return "/".join(parts[i:])
    return path
