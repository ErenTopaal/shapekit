"""coverage_snapshot and coverage_new through coverage.py, started before stellar_sdk is
imported. a unit is a line of a stellar_sdk source file that has run."""

from pathlib import Path
from typing import Any


class Coverage:
    def __init__(self, cov: Any, coverage_module: Any) -> None:
        self.cov = cov
        self.module = coverage_module
        self.reported = 0

    @classmethod
    def start(cls) -> "Coverage":
        import coverage

        # no data file: the counters are read in the process and never written to disk
        cov = coverage.Coverage(data_file=None, source_pkgs=["stellar_sdk"], branch=False, config_file=False)
        cov.start()
        return cls(cov, coverage)

    def lines(self) -> list[str]:
        data = self.cov.get_data()
        out = []
        for path in data.measured_files():
            rel = package_path(path)
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


def package_path(path: str) -> str:
    """a file's path from the stellar_sdk package directory on: stellar_sdk/xdr/sc_val.py."""
    parts = Path(path).parts
    if "stellar_sdk" not in parts:
        return path
    i = len(parts) - 1 - parts[::-1].index("stellar_sdk")
    return "/".join(parts[i:])
