# Reading a run: shapekit report

`shapekit report` turns one harness run or one conformance run into a single HTML page you can
open from disk, attach to an issue or keep as a CI artifact. The page groups the run's
failures by root cause. For each group it names the smallest corpus entry, the finding with
its upstream issue, the guide sections that cover the fields involved and the command that
reruns the entry. With a baseline it also says which failures are new, which changed, which
are fixed and how many are unchanged, and it can write the same numbers as a short markdown
summary for a pull request comment.

```sh
shapekit check sdk --sdk go --out target/conformance/go
shapekit report target/conformance/go --known-failures conformance/known-failures

shapekit check indexer --adapter "adapters/bin/wb-adapter --commit <commit>"
shapekit report harness-out --baseline previous-run/harness-report.json
```

The first argument is a run directory (what `--out` of either command names, also of
`shapekit conformance run` and `shapekit harness run`) or the
`harness-report.json` or `divergence-matrix.json` in one. Without `--out` the page is written
as `report.html` next to that file. Run the command from a shapekit checkout, or point
`--root` at one: the report reads the findings triage and the registry from it.

| Option | What it does |
|---|---|
| `--out <file>` | Where the HTML page goes. |
| `--summary <file>` | Also writes a markdown summary for a pull request comment. |
| `--json <file>` | Also writes everything the page shows as JSON. |
| `--baseline <run or file>` | Compares with an earlier run of the same kind, or a harness run with an expectations file (`shapekit harness expect --write`). |
| `--known-failures <dir>` | For a conformance run: compares with the known-failures directory the run used. |
| `--manifest <file>` | The corpus manifest the run used, when it is not the checkout's `corpus/manifest.json`. |
| `--root <dir>`, `--triage <file>` | The shapekit checkout, and the triage in it (default `findings/triage.toml`). |
| `--site <url>` | Where findings and guide sections are linked (default `https://shapekit.org`). |

The command exits 0 when it wrote the page, whatever the run found. The run's own command is
the one that fails on its results.

## How failures are grouped

The report does not invent root causes. It hands the run to the same matching that writes
the findings record ([`findings/FINDINGS.md`](../findings/FINDINGS.md)), with the run as the
only input, so every failure lands where the committed triage puts it:

- **Known finding.** A finding of `findings/triage.toml`. The group shows its title, cause,
  severity, upstream status and issue, and links the finding on the site.
- **Candidate finding.** A finding of the candidate triage, triaged but not yet in the
  record.
- **Untriaged.** Failures no finding matches. They are grouped by corpus family and by what
  failed: the problem and fact of a harness problem (`dropped token-event`), the verdict of a
  whole harness entry, or the result of a conformance cell. Such a group is a starting point
  for triage, not a proven root cause, and the page says so.

What counts as a failure is what the findings record counts: a harness entry with verdict
`dropped`, `mishandled` or `not-evaluated` (one failure per problem it lists), and a
conformance cell whose result is neither `pass` nor `unsupported`.

## What each group links

- **Smallest entry.** The finding's own minimal entry when the triage names one and this run
  shows it, otherwise the group's entry with the smallest file. Every entry the page names
  links its page in the site's corpus explorer, which shows the decoded XDR-JSON, where the
  bytes come from and the committed results for it.
- **Reproduce.** For a harness run, `shapekit check indexer --chain <chain> --network
  <network> --adapter <the run's adapter command>` for that entry's chain (testnet when the
  entry ran on both). For a conformance run, `shapekit check sdk --sdk <sdk> --version
  <newest failing version> --entry <entry>`.
- **Recorded by.** The recipe file or chain that produced the entry on a local core, when
  there is one.
- **Guide.** Every corpus entry lists the fields it exercises (`fields_exercised` in the
  manifest). The group links the guide's field table for those fields, its list of corpus
  entries by field, and each item of the guide's self-audit checklist that names one of
  them (`registry/guide-checklist.toml`). A collapsed table repeats each field's row: whether
  core validates it, the risk and the handling the guide recommends. A group whose entries
  name no field links no guide section and says so.

## Baselines

A harness failure is one entry on one network, and it is the same failure in both runs when
its verdict and reason are the same. A conformance failure is one check of one entry on one
testee, and it is the same when its result is. Against a baseline each failure is:

- **new** when the baseline does not fail it;
- **changed** when both fail it with a different verdict, reason or result;
- **unchanged** when both fail it the same way;
- **fixed** when the baseline fails it and this run covers the same entry and network or
  testee without failing it.

A baseline failure on an entry this run did not cover is counted as not rerun, never as
fixed.

Conformance testees are matched by `<sdk>@<version>`. When an SDK has a single version on
both sides, it is matched by the SDK alone, so a pull request build compares with the run
before it although the two report different version strings.

With `--known-failures`, a conformance run is compared the way `shapekit conformance run`
checks it: a failing check without a line is new, a check listed with another result is
changed, and a listed check that no longer fails is fixed. A line that names a
`diverges-from-peers` check is not counted as fixed in a run of one SDK, since that run had
no peers to diverge from. See [testee-protocol.md](testee-protocol.md#known-failures) for the
format of the lists.

## The pull request summary

`--summary` writes a markdown text of a few kilobytes: the new, changed, fixed and unchanged
counts, the first rows of each list, and one line per group with its finding and smallest
entry. Its first line is the HTML comment `<!-- shapekit-report harness -->` or
`<!-- shapekit-report conformance -->`, so a workflow can find its earlier comment and update
it instead of adding one per push.

## The page

The page is one file with its styles inline. It has no scripts and loads no fonts, images or
styles from anywhere, and its content security policy forbids it to. It follows the light or
dark setting of your system. The failures of each group, the field rows and long lists sit in
collapsed sections; each group has an anchor (`report.html#WB-W4`), so you can link one.
