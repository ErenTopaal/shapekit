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
  hasMatrix: boolean
}

export function sdkSummary(s: Sdk): SdkSummary {
  const cur = currentVersion(s)
  const diverging = divergingEntries(cur)
  const known = cur?.failing_cells_with_known_issue ?? 0
  return { sdk: s.sdk, version: s.current, diverging, known, notKnown: diverging - known, hasMatrix: s.matrix !== null }
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

export function coverageMetrics(s: CoverageSummary, rootType: string): CoverageMetric[] {
  return [
    { id: "core-emitted", label: "Entries emitted by stellar-core", value: s.core_emitted_entries, total: s.entries, better: "higher", section: "sources" },
    {
      id: "caps",
      label: "Indexer-visible CAPs with an entry from stellar-core",
      value: s.caps_indexer_visible_core_emitted,
      total: s.caps_indexer_visible,
      better: "higher",
      section: "caps",
    },
    { id: "arms", label: `Union arms of ${rootType} with an entry from stellar-core`, value: s.arms_covered, total: s.arms_reachable, better: "higher", section: "arms" },
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
