"""upstream drift of the indexers and sdks the corpus runs against: which ones have a newer
upstream version than the pin, and, after the drift workflow has run that version over the
corpus, the text of its tracking issue. nothing here changes a pin.

usage (from the repository root, python3 >= 3.11, stdlib only):

  python3 -I scripts/upstream-drift.py indexers
      json list of the indexers whose newest upstream version differs from the pin, each with
      the adapter command that runs that version and the committed run it is compared with
  python3 -I scripts/upstream-drift.py sdks --versions <versions.toml> --summary <file>
      json list of the sdks whose current version in the registries, as the summary
      `shapekit conformance sync-versions --check` prints names it, differs from versions.toml
  python3 -I scripts/upstream-drift.py sdk-report --sdk <key> --pinned <v> --upstream <v>
      --matrix <divergence-matrix.json>
      markdown for the tracking issue of a new sdk version, from an --all-versions run
  python3 -I scripts/upstream-drift.py indexer-report --name <indexer> --upstream <version>
      --base <committed report or expectations file> --run <harness-report.json>
      [--command <the harness command that ran>]
      markdown for the tracking issue of a new indexer version

which version is "newest upstream":
  stellar-etl and stellar-rpc  the newest vX.Y.Z tag: both release regularly
  wallet-backend               the newest commit of main: the repository has no release tags
                               and deploys from main, so main is what an operator runs
"""

import argparse
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(".")
FAILING_RESULTS = {"diverges-from-network", "diverges-from-sdks", "error"}
# harness verdicts that are not a failure of the target
NOT_FAILING = {"pass", "out-of-scope", "not-evaluated"}


def pins():
    with open(ROOT / "pins.toml", "rb") as f:
        return tomllib.load(f)


def ls_remote(url, *refs):
    out = subprocess.run(["git", "ls-remote", url, *refs], capture_output=True, text=True, check=True).stdout
    return [line.split("\t") for line in out.splitlines() if line]


def version_key(tag):
    return tuple(int(n) for n in re.findall(r"\d+", tag))


def newest_tag(url):
    """the newest vX.Y.Z tag and the commit it names (a peeled annotated tag's commit)."""
    rows = ls_remote(url, "refs/tags/v*")
    shas = {}
    for sha, ref in rows:
        name = ref.removeprefix("refs/tags/")
        peeled = name.endswith("^{}")
        name = name.removesuffix("^{}")
        if not re.fullmatch(r"v\d+\.\d+\.\d+", name):
            continue
        if peeled or name not in shas:
            shas[name] = sha
    if not shas:
        sys.exit(f"{url} has no vX.Y.Z tag")
    tag = max(shas, key=version_key)
    return tag, shas[tag]


def git_pin(p, name):
    [e] = [g for g in p["git"] if g["name"] == name]
    return e


def indexers():
    p = pins()
    found = []

    wb = git_pin(p, "wallet-backend")
    [(head, _)] = ls_remote(wb["url"], "refs/heads/main")
    if head != wb["sha"]:
        found.append({
            "name": "wallet-backend",
            "pinned": wb["sha"],
            "upstream": head,
            "adapter": f"adapters/bin/wb-adapter --commit {head} --cpus 2",
            "base": f"findings/inputs/harness-wallet-backend-{wb['sha'][:8]}.json",
        })

    etl = git_pin(p, "stellar-etl")
    tag, sha = newest_tag(etl["url"])
    if sha != etl["sha"]:
        found.append({
            "name": "stellar-etl",
            "pinned": etl["ref"].removeprefix("refs/tags/"),
            "upstream": tag,
            "adapter": f"adapters/bin/etl-adapter --commit {sha} --cpus 2",
            "base": f"findings/inputs/harness-stellar-etl-{etl['sha'][:8]}.json",
        })

    [rpc] = [i for i in p["image"] if i["name"] == "stellar-rpc"]
    tag, sha = newest_tag("https://github.com/stellar/stellar-rpc")
    version = tag.removeprefix("v")
    if version != rpc["tag"]:
        found.append({
            "name": "stellar-rpc",
            "pinned": rpc["tag"],
            "upstream": version,
            # the release image carries the release number; getVersionInfo answers
            # <release>-<commit>, which the adapter checks
            "adapter": (f"adapters/bin/rpc-adapter --cpus 2 --events-from events "
                        f"--image {rpc['repository']}:{version} --version {version}-{sha} --commit {sha}"),
            "base": "crates/shapekit-harness/expect/stellar-rpc.toml",
        })
    return found


def sdks(versions, summary):
    with open(versions, "rb") as f:
        pinned = {k: v["current"] for k, v in tomllib.load(f)["sdk"].items()}
    # sync-versions prints "<sdk>: <n> versions, current <version>" per sdk, the version it
    # derived from the registry, before it compares or writes anything
    upstream = dict(re.findall(r"^(\S+): \d+ versions?, current (\S+)$", Path(summary).read_text(), re.M))
    missing = sorted(set(pinned) - set(upstream))
    if missing:
        sys.exit(f"the sync-versions summary names no current version for {', '.join(missing)}")
    return [{"sdk": k, "pinned": v, "upstream": upstream[k]} for k, v in pinned.items() if upstream[k] != v]


def load_findings():
    out = []
    for path in ("findings/findings.json", "candidate/findings/findings.json"):
        p = ROOT / path
        if p.exists():
            for f in json.loads(p.read_text())["findings"]:
                out.append((path, f))
    return out


def finding_line(f):
    where = f.get("link") or f.get("draft") or "not filed"
    return f"- {f['id']} ({f.get('status', 'unknown')}): {f['title']} [{where}]"


def failing_checks(cell):
    return [f"{c['check']}: {c['result']}, {c.get('detail', '')}".rstrip(", ")
            for c in cell.get("checks", []) if c["result"] in FAILING_RESULTS]


def sdk_report(sdk, pinned, upstream, matrix):
    m = json.loads(Path(matrix).read_text())
    old_col, new_col = f"{sdk}@{pinned}", f"{sdk}@{upstream}"
    newly_failing, now_passing, missing = [], [], 0
    new_result = {}
    for e in m["entries"]:
        cells = e["cells"]
        if new_col not in cells or old_col not in cells:
            missing += 1
            continue
        a, b = cells[old_col]["result"], cells[new_col]["result"]
        new_result[e["id"]] = b
        if a not in FAILING_RESULTS and b in FAILING_RESULTS:
            newly_failing.append((e["id"], a, b, failing_checks(cells[new_col])))
        elif a in FAILING_RESULTS and b not in FAILING_RESULTS:
            now_passing.append((e["id"], a, b))

    fixed = []
    for _, f in load_findings():
        entries = {o["entry"] for o in f["occurrences"]
                   if o["source"] == "conformance" and o["on"] == old_col}
        if entries and all(new_result.get(x) not in FAILING_RESULTS and x in new_result for x in entries):
            fixed.append(f)

    lines = [f"### {sdk} {upstream} over the corpus", "",
             f"`shapekit conformance run --sdk {sdk} --all-versions`, with conformance/versions.toml as "
             f"`shapekit conformance sync-versions` writes it today. Compared cell by cell with the pinned "
             f"current version {pinned}; {len(m['entries'])} entries, {missing} without both cells.", ""]
    lines += [f"Findings whose every failing entry at {pinned} passes at {upstream}, so their upstream "
              f"issue may be fixed ({len(fixed)}):", ""]
    lines += [finding_line(f) for f in fixed] or ["- none"]
    lines += ["", f"Entries that fail at {upstream} and did not at {pinned} ({len(newly_failing)}):", ""]
    for entry, a, b, checks in newly_failing:
        lines.append(f"- `{entry}`: {a} at {pinned}, {b} at {upstream}")
        lines += [f"  - {c}" for c in checks[:8]]
        if len(checks) > 8:
            lines.append(f"  - and {len(checks) - 8} more checks")
    if not newly_failing:
        lines.append("- none")
    lines += ["", f"Entries that failed at {pinned} and do not at {upstream} ({len(now_passing)}):", ""]
    lines += [f"- `{entry}`: {a} at {pinned}, {b} at {upstream}" for entry, a, b in now_passing] or ["- none"]
    lines += ["", "Moving the pin (docs/maintenance.md, \"Moving an sdk to a new version\"):", "", "```sh",
              "shapekit conformance sync-versions"]
    if sdk in ("rust", "go", "js"):
        lines.append("# and the version in pins.toml ([[crate]], [[gomod]] or [[npm]]), then")
        lines.append("shapekit pins verify")
    lines += [f"shapekit conformance run --sdk {sdk} --all-versions",
              "shapekit conformance reproduce --no-build",
              "```"]
    return "\n".join(lines) + "\n"


def verdicts_of(path):
    """(entry, network) -> verdict from a harness-report.json or an expectations file."""
    p = Path(path)
    if p.suffix == ".toml":
        with open(p, "rb") as f:
            rows = tomllib.load(f).get("entry", [])
    else:
        rows = json.loads(p.read_text())["entries"]
    return {(r["entry"], r["network"]): r["verdict"] for r in rows}


MOVE = {
    "wallet-backend": [
        "# pins.toml: [[git]] wallet-backend sha; findings/triage.toml: the import paths and command",
        "shapekit harness run --network both --repeat 2 --adapter \"adapters/bin/wb-adapter --commit <sha>\" "
        "--out target/findings/wallet-backend-<sha8>",
        "shapekit harness reproduce --table adapters/walletbackend/reproductions.toml",
        "shapekit findings import-inputs && shapekit findings collect && shapekit findings check",
    ],
    "stellar-etl": [
        "# pins.toml: [[git]] stellar-etl ref and sha; adapters/stellaretl/build.go: sdkProtocols when its",
        "# go-stellar-sdk changed; findings/triage.toml and candidate/findings/triage.toml: import paths",
        "shapekit harness run --network both --adapter \"adapters/bin/etl-adapter --commit <sha> --cpus 2\" "
        "--out target/findings/stellar-etl-<sha8>",
        "shapekit findings import-inputs && shapekit findings collect && shapekit findings check",
    ],
    "stellar-rpc": [
        "# pins.toml: [[image]] stellar-rpc tag and digest; adapters/stellarrpc/adapter.go: Image, Version30, Commit",
        "for src in events transactions ledgers; do shapekit harness run --network both "
        "--adapter \"adapters/bin/rpc-adapter --cpus 2 --events-from $src\" --out target/rpc-$src; done",
        "shapekit harness expect --report target/rpc-events/harness-report.json "
        "--write crates/shapekit-harness/expect/stellar-rpc.toml",
    ],
}


def indexer_report(name, upstream, base, run, command):
    old = verdicts_of(base)
    report = json.loads(Path(run).read_text())
    new = {(r["entry"], r["network"]): r for r in report["entries"]}
    newly_failing, now_passing, other = [], [], []
    for key, r in sorted(new.items()):
        was = old.get(key)
        if was is None or was == r["verdict"]:
            continue
        if was in NOT_FAILING - {"not-evaluated"} and r["verdict"] not in NOT_FAILING - {"not-evaluated"}:
            newly_failing.append((key, was, r))
        elif was not in NOT_FAILING and r["verdict"] == "pass":
            now_passing.append((key, was, r))
        else:
            other.append((key, was, r))
    gone = sorted(set(old) - set(new))

    fixed = []
    for _, f in load_findings():
        keys = {(o["entry"], o["at"]) for o in f["occurrences"]
                if o["source"] == "harness" and o["on"].split("@")[0] == name}
        if keys and all(new.get(k, {}).get("verdict") == "pass" for k in keys):
            fixed.append(f)

    totals = report.get("totals", {}).get("by_verdict", {})
    lines = [f"### {name} {upstream} over the corpus", ""]
    if command:
        lines += [f"`{command}`", ""]
    lines += [f"Verdicts: {', '.join(f'{v} {n}' for v, n in sorted(totals.items()))}. Compared entry by entry "
              f"with `{base}`, the committed run of the pinned version.", ""]
    lines += [f"Findings whose every failing entry now passes, so their upstream issue may be fixed "
              f"({len(fixed)}):", ""]
    lines += [finding_line(f) for f in fixed] or ["- none"]
    lines += ["", f"Entries that newly fail ({len(newly_failing)}):", ""]
    for (entry, net), was, r in newly_failing:
        lines.append(f"- `{entry}` ({net}): {was} before, {r['verdict']} now: {r.get('detail', '')}")
        lines += [f"  - {p}" for p in r.get("problems", [])[:6]]
    if not newly_failing:
        lines.append("- none")
    lines += ["", f"Entries that failed before and pass now ({len(now_passing)}):", ""]
    lines += [f"- `{e}` ({n}): {was} before" for (e, n), was, _ in now_passing] or ["- none"]
    if other:
        lines += ["", f"Other verdict changes ({len(other)}):", ""]
        lines += [f"- `{e}` ({n}): {was} before, {r['verdict']} now" for (e, n), was, r in other[:60]]
    if gone:
        lines += ["", f"Entries of the committed run this run did not reach ({len(gone)}): "
                  + ", ".join(f"`{e}` ({n})" for e, n in gone[:20])]
    lines += ["", "Moving the pin (docs/maintenance.md, \"Moving an indexer pin\"):", "", "```sh",
              *MOVE[name], "```"]
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("indexers")
    s = sub.add_parser("sdks")
    s.add_argument("--versions", required=True)
    s.add_argument("--summary", required=True)
    s = sub.add_parser("sdk-report")
    for a in ("--sdk", "--pinned", "--upstream", "--matrix"):
        s.add_argument(a, required=True)
    s = sub.add_parser("indexer-report")
    for a in ("--name", "--upstream", "--base", "--run"):
        s.add_argument(a, required=True)
    s.add_argument("--command", default="")
    a = ap.parse_args()
    if a.cmd == "indexers":
        print(json.dumps(indexers()))
    elif a.cmd == "sdks":
        print(json.dumps(sdks(a.versions, a.summary)))
    elif a.cmd == "sdk-report":
        sys.stdout.write(sdk_report(a.sdk, a.pinned, a.upstream, a.matrix))
    else:
        sys.stdout.write(indexer_report(a.name, a.upstream, a.base, a.run, a.command))


if __name__ == "__main__":
    main()
