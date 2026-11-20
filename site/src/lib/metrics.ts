// the metrics the landing page and the dashboard both show, each defined once here over the
// json files `shapekit site data` writes. a page that shows one of these numbers calls the
// function, so the landing page and the dashboard cannot count the same thing two ways.
import { sum, type CoverageSummary, type Finding, type Findings, type HarnessRun, type Sdk, type SdkVersion } from "./data"

// whether a higher or a lower value of a metric is better
export type Better = "higher" | "lower"

export const betterText: Record<Better, string> = {
  higher: "higher is better",
  lower: "lower is better",
}

// ---- findings

export interface FindingRow extends Finding {
  // the record the finding is in: the first record, findings/findings.json, or the second,
  // the candidate lane's record
  record: "first" | "second"
  recordPath: string
  reported: boolean
}

// every finding of both records, the first record first
export function findingRows(f: Findings): FindingRow[] {
  const reported = new Set(f.reported.statuses)
  const tag = (record: FindingRow["record"], recordPath: string) => (x: Finding): FindingRow => ({
    ...x,
    record,
    recordPath,
    reported: reported.has(x.status),
  })
  return [
    ...f.findings.map(tag("first", "findings/findings.json")),
    ...(f.candidates?.findings ?? []).map(tag("second", f.candidates?.record ?? "")),
  ]
}

// the reported findings: the findings total of every page. the count must be the count
// `shapekit site data` wrote, or the build fails
export function reportedFindings(f: Findings): FindingRow[] {
  const rows = findingRows(f).filter((x) => x.reported)
  if (rows.length !== f.reported.findings) {
    throw new Error(`findings.json: ${rows.length} rows have a reported status, but reported.findings is ${f.reported.findings}`)
  }
  return rows
}

// whether shapekit filed the upstream issue that a finding links. shapekit's issues are the links
// of the filed findings of both records, so a finding that links one of them is shapekit's,
// whatever its status, and it is never counted as reported before
export function filedByShapekit(f: Findings): (x: Finding) => boolean {
  const own = new Set(findingRows(f).flatMap((x) => (x.status === "filed" && x.link ? [x.link] : [])))
  return (x) => x.link !== null && own.has(x.link)
}

export interface IssueSplit {
  // the reported findings: the findings total
  findings: number
  // distinct upstream issues that shapekit filed
  filed: number
  // distinct upstream issues that others reported before shapekit, now reproduced
  reportedBefore: number
  // findings that shapekit reported through a private channel; each is one report
  privateReports: number
}

// the split of the reported findings into upstream issues, the split that the landing page and
// every dashboard page show. findings that link the same upstream issue count as one issue, so
// the issues can be fewer than the findings
export function issueSplit(f: Findings): IssueSplit {
  const rows = reportedFindings(f)
  const own = filedByShapekit(f)
  const issues = (keep: (x: FindingRow) => boolean) => new Set(rows.filter(keep).map((x) => x.link ?? x.id)).size
  const isPrivate = (x: FindingRow) => !own(x) && x.status === "private-report"
  return {
    findings: rows.length,
    filed: issues(own),
    reportedBefore: issues((x) => !own(x) && !isPrivate(x)),
    privateReports: rows.filter(isPrivate).length,
  }
}

// the split as the pages write it: "42 issues filed upstream by shapekit"
export function issueCount(n: number): string {
  return `${n.toLocaleString("en-US")} ${n === 1 ? "issue" : "issues"}`
}

export const issueSplitText = {
  filed: "filed upstream by shapekit",
  reportedBefore: "reported before by others",
  privateReports: "reported privately",
} as const

// ---- sdks

export function currentVersion(s: Sdk): SdkVersion | undefined {
  return s.versions.find((v) => v.current)
}

// entries on which at least one check of the version fails; lower is better
export function divergingEntries(v: SdkVersion | undefined): number {
  return sum(v?.failing_cells_by_result)
}

export interface SdkSummary {
  sdk: string
  version: string
  // diverging entries at the current version
  diverging: number
  // of those, the entries the known-failures file lists
  known: number
  // the rest: these fail the sdk's CI
  notKnown: number
}

export function sdkSummary(s: Sdk): SdkSummary {
  const cur = currentVersion(s)
  const diverging = divergingEntries(cur)
  const known = cur?.failing_cells_with_known_issue ?? 0
  return { sdk: s.sdk, version: s.current, diverging, known, notKnown: diverging - known }
}

// ---- indexers

export interface RunSummary {
  target: string
  version: string
  // one replay is one entry on one network
  replays: number
  entries: number
  networks: string[]
  // verdicts over every replay, the counts of the harness report's totals
  byVerdict: Record<string, number>
  pass: number
  // replays the indexer got wrong: mishandled and dropped; lower is better
  wrong: number
  // whether every network got the same verdicts, so one network's counts stand for all
  sameOnEveryNetwork: boolean
}

// what the replay harness found in one indexer run: the replays with the verdict mishandled or
// dropped. the landing page, the start page and the indexer page show this text
export function foundText(r: Pick<RunSummary, "wrong">): string {
  return r.wrong === 0 ? "Found no wrong or missing rows" : `Found wrong or missing rows in ${r.wrong.toLocaleString("en-US")} replays`
}

// the reported findings of one component, for the link from an indexer's result to its findings
export function componentFindings(f: Findings, component: string): number {
  return reportedFindings(f).filter((x) => x.component === component).length
}

export function runSummary(run: HarnessRun, i = 0): RunSummary {
  const v = run.totals.by_verdict
  const networks = Object.keys(run.by_network).toSorted()
  const first = JSON.stringify(run.by_network[networks[0] ?? ""])
  return {
    target: run.describe?.target ?? `run ${i + 1}`,
    version: run.describe?.target_version ?? "",
    replays: run.totals.entries,
    entries: run.distinct_entries,
    networks,
    byVerdict: v,
    pass: v["pass"] ?? 0,
    wrong: (v["mishandled"] ?? 0) + (v["dropped"] ?? 0),
    sameOnEveryNetwork: networks.every((n) => JSON.stringify(run.by_network[n]) === first),
  }
}

// ---- corpus coverage

export interface CoverageMetric {
  id: string
  label: string
  value: number
  total: number
  better: Better
  // the section of the corpus page that explains the metric
  section: string
}

export interface LedgerArms {
  // every union arm that the root type can reach
  all: number
  // the arms that a ledger can hold: all arms minus the arms of the no-ledger blockers
  total: number
  // the arms with an entry from stellar-core
  covered: number
  // the arms that cannot occur in any ledger, by blocker
  refused: number
  never: number
  // the other uncovered arms, each with a written reason
  other: number
  // covered of total, in percent with one decimal
  percent: string
}

// the arm coverage that the landing page and the dashboard show: covered arms of the arms that
// a ledger can hold. no ledger can hold the arms of two blockers: refused-before-ledger, where
// stellar-core refuses the transaction before a ledger, and never-emitted, where the pinned
// stellar-core has no code path that emits the arm
export function ledgerArms(s: CoverageSummary): LedgerArms {
  const by = s.arms_uncovered_by_blocker
  const refused = by["refused-before-ledger"] ?? 0
  const never = by["never-emitted"] ?? 0
  const total = s.arms_reachable - refused - never
  return {
    all: s.arms_reachable,
    total,
    covered: s.arms_covered,
    refused,
    never,
    other: s.arms_uncovered - refused - never,
    percent: total > 0 ? ((s.arms_covered / total) * 100).toFixed(1) : "0.0",
  }
}

export interface CapCoverage {
  // indexer-visible CAPs with an entry from stellar-core
  covered: number
  // indexer-visible CAPs with a written blocker; they are in no total
  blocked: number
  // indexer-visible CAPs without a blocker
  total: number
}

export function capCoverage(s: CoverageSummary): CapCoverage {
  const blocked = s.caps_indexer_visible_with_reason
  return { covered: s.caps_indexer_visible_core_emitted, blocked, total: s.caps_indexer_visible - blocked }
}

export interface SourcePart {
  id: string
  label: string
  // the source classes of the part, from the manifest
  classes: string[]
  entries: number
}

const sourceParts: Omit<SourcePart, "entries">[] = [
  { id: "local-core", label: "recorded from a local stellar-core", classes: ["local-core"] },
  { id: "core-golden", label: "from the stellar-core test goldens", classes: ["core-golden"] },
  { id: "history", label: "from network history", classes: ["mainnet-history", "testnet-history"] },
  { id: "synthetic", label: "synthetic inputs", classes: ["synthetic"] },
  { id: "refused", label: "inputs that stellar-core refuses", classes: ["local-core-refused"] },
]

// the corpus entries by source: each source is a part of its own, and the parts add up to every
// entry, or the build fails
export function entryComposition(s: CoverageSummary): SourcePart[] {
  const by = s.entries_by_source_class
  for (const c of Object.keys(by)) {
    if (!sourceParts.some((p) => p.classes.includes(c))) throw new Error(`coverage.json: source class ${c} is in no part of the corpus`)
  }
  const parts = sourceParts.map((p) => ({ ...p, entries: p.classes.reduce((a, c) => a + (by[c] ?? 0), 0) })).filter((p) => p.entries > 0)
  const total = parts.reduce((a, p) => a + p.entries, 0)
  if (total !== s.entries) throw new Error(`coverage.json: the parts of the corpus hold ${total} entries, but it has ${s.entries}`)
  return parts
}

export function coverageMetrics(s: CoverageSummary, rootType: string): CoverageMetric[] {
  const arms = ledgerArms(s)
  const caps = capCoverage(s)
  return [
    {
      id: "caps",
      label: "Indexer-visible CAPs with an entry from stellar-core",
      value: caps.covered,
      total: caps.total,
      better: "higher",
      section: "caps",
    },
    {
      id: "arms",
      label: `Union arms of ${rootType} that a ledger can hold with an entry from stellar-core`,
      value: arms.covered,
      total: arms.total,
      better: "higher",
      section: "arms",
    },
    { id: "fragments", label: "Family fragments with an entry from stellar-core", value: s.fragments_covered, total: s.fragments, better: "higher", section: "families" },
    {
      id: "fields",
      label: "Unvalidated fields that an entry contains",
      value: s.fields_not_fully_validated_exercised,
      total: s.fields_not_fully_validated,
      better: "higher",
      section: "fields",
    },
  ]
}
