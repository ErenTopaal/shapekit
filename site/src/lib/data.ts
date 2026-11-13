// types of the json files `shapekit site data` writes (crates/shapekit-cli/src/site_cmd.rs).
// the site never computes a number the files do not hold except sums and filters over them.

export type Counts = Record<string, number>

export interface Meta {
  schema: number
  commit: string
  repo: string
  repo_url: string
  shapekit_version: string
  corpus: {
    version: string
    entries: number
    protocol_tiers: number[]
    generated_with: {
      stellar_xdr: string
      soroban_env_host: string
      xdr_definitions: { tag: string; commit: string }
      core_image: { repository: string; tag: string; digest: string }
    }
  }
  pins: {
    toolchains: { name: string; version: string }[]
    images: { name: string; repository: string; tag: string; digest: string }[]
    git: { name: string; url: string; ref: string | null; sha: string | null; purpose: string }[]
    npm: { name: string; version: string; purpose: string }[]
    gomod: { module: string; version: string; purpose: string }[]
    crates: { name: string; version: string; purpose: string }[]
  }
  sources: { path: string; sha256: string; bytes: number }[]
  outputs: { file: string; sha256: string }[]
}

export interface Reason {
  blocker: string
  reason: string
}

export interface CapRow {
  id: string
  title: string
  protocol: number | null
  status: string
  indexer_visible: boolean
  not_visible_reason: string | null
  families: string[]
  entries: number
  core_emitted_entries: number
  entries_by_source_class: Counts
  entries_by_variant: Counts
  protocols_core_emitted: number[]
  protocols_all: number[]
  uncovered: Reason | null
  xdr_additions: string[]
  text_url: string
}

export interface Fragment {
  fragment: string
  covered: boolean
  core_emitted: number
  non_core: number
  refused_by_core: number
  uncovered: Reason | null
}

export interface FamilyRow {
  id: string
  title: string
  caps: string[]
  priority: number
  protocol_introduced: number
  protocol_valid: { min: number; max: number | null }
  linked_issues: string[]
  entries_by_source_class: Counts
  missing_source_classes: string[]
  fragments: Fragment[]
  planned: number
}

export interface CoverageSummary {
  entries: number
  core_emitted_entries: number
  entries_by_source_class: Counts
  caps: number
  caps_indexer_visible: number
  caps_indexer_visible_core_emitted: number
  caps_indexer_visible_with_reason: number
  arms_reachable: number
  arms_covered: number
  arms_uncovered: number
  arms_uncovered_by_blocker: Counts
  families: number
  fragments: number
  fragments_covered: number
  fragments_uncovered: number
  fields_not_fully_validated: number
  fields_not_fully_validated_exercised: number
}

export interface Coverage {
  corpus_version: string
  stellar_xdr: string
  root_type: string
  summary: CoverageSummary
  caps: CapRow[]
  families: FamilyRow[]
  arms: {
    reachable: number
    covered: number
    uncovered: { arm: string; non_core: number; refused_by_core: number; uncovered: Reason | null }[]
  }
  fields: {
    id: string
    xdr_path: string
    validated_by_core: string
    entries: number
    core_emitted_entries: number
  }[]
}

export interface Entry {
  id: string
  family: string
  kind: string
  caps: string[]
  protocol: { introduced: number; generated_at?: number; valid_min: number; valid_max?: number }
  source: string
  synthetic: boolean
  xdr_type: string | null
  expected: { decode: string; validity: string; roundtrip: boolean }
  flags: string[]
  linked_issues: string[]
  files: { variant: string; path: string }[]
}

export interface Entries {
  corpus_version: string
  entries: Entry[]
}

export interface Check {
  check: string
  op: string
  result: string
  detail: string | null
}

export interface Cell {
  result: string
  known_issue: string | null
  checks: Check[]
}

export interface SdkVersion {
  version: string
  key: string
  current: boolean
  max_protocol: number | null
  in_matrix: boolean
  failing_cells_by_result: Counts
  failing_cells_with_known_issue: number
  failing_checks_by_op: Counts
  failing_checks_by_result: Counts
  known_failure_lines: number
}

export interface Sdk {
  sdk: string
  package: string
  current: string
  issues_repo: string
  matrix: string | null
  corpus_version: string | null
  known_failure_problems: string[] | null
  known_failures_file: string | null
  known_failure_lines: number
  versions: SdkVersion[]
  entries: { id: string; cells: Record<string, Cell> }[]
}

export type Observation =
  | "named-checks-diverge"
  | "no-counted-check-fails"
  | "diverges-elsewhere"
  | "not-in-matrix"

export interface Reproduction {
  id: string
  sighting: string
  link: string
  sdk: string
  entries: string[]
  ops: string[]
  probes: string[]
  must_diverge: { entry: string | null; probe: string | null; checks: string[] }[]
  fail: Record<string, Observation>
  pass: Record<string, Observation>
}

export interface Conformance {
  sdks: Sdk[]
  reproductions: Reproduction[]
}

export interface HarnessProblemRow {
  entry: string
  family: string
  chain: string
  network: string
  ledger: number
  verdict: string
  reason: string | null
  detail: string
  problems: { kind: string; fact: string; detail: string }[]
}

export interface HarnessRun {
  report: string
  ok: boolean
  shapekit_version: string
  manifest_sha256: string
  describe: {
    adapter: string
    adapter_version: string
    target: string
    target_version: string
    default_delivery: string
    protocols: { min: number; max: number }
  } | null
  components: Record<string, string>
  totals: {
    entries: number
    by_verdict: Counts
    by_reason: Counts
    without_chain: number
    without_oracle: number
  }
  chains: number
  chain_errors: { chain: string; network: string; error: string }[]
  rounds: { round: number; peak_rss_bytes: number; seconds: number; same_verdicts_as_first: boolean }[]
  memory: { slack_bytes: number; budget_bytes?: number; flat: boolean; within_budget: boolean; detail: string }
  target_memory: {
    limit_bytes: number
    peak_bytes?: number
    within_limit: boolean
    flat: boolean
    detail: string
  } | null
  by_network: Record<string, Counts>
  families: { family: string; network: string; verdicts: Counts }[]
  problems: HarnessProblemRow[]
}

export interface Harness {
  runs: HarnessRun[]
  lint: {
    target: string
    version: string
    result: string
    command: string
    findings: {
      rule: string
      package: string
      function: string
      file: string
      line: number
      column: number
      message: string
    }[]
    suppressed: number
    baselined: number
  }[]
}

export interface Finding {
  id: string
  title: string
  component: string
  severity: string
  status: string
  caps: string[]
  minimal_entry: string | null
  reproduce: string | null
  link: string | null
  reference: string | null
  reason: string | null
  draft: string | null
  cause: string
  siblings: string[]
  related: string[]
  entries: string[]
  occurrences: number
  occurrences_by_source: Counts
}

export interface Findings {
  summary: {
    findings: number
    failures: number
    unmapped: number
    by_component: Record<string, Counts>
    by_severity: Counts
    by_status: Counts
  }
  inputs: { kind: string; path: string; sha256: string; detail: string }[]
  unmapped: number
  input_problems: string[]
  findings: Finding[]
}

interface Files {
  meta: Meta
  coverage: Coverage
  entries: Entries
  conformance: Conformance
  harness: Harness
  findings: Findings
}

const cache = new Map<keyof Files, Promise<unknown>>()

// one fetch per file per page load; the promise is shared so suspense can re-read it
export function load<K extends keyof Files>(name: K): Promise<Files[K]> {
  let p = cache.get(name)
  if (!p) {
    p = fetch(`/data/${name}.json`)
      .then((r) => {
        if (!r.ok) throw new Error(`data/${name}.json: HTTP ${r.status}`)
        return r.json()
      })
      .then((v: unknown) => {
        // marks the promise settled the way react's use() reads it, so a page rendered after
        // its route's clientLoader awaited the file does not suspend again
        Object.assign(p as object, { status: "fulfilled", value: v })
        return v
      })
    // a failed fetch is not cached, so a retry fetches again
    p.catch(() => cache.delete(name))
    cache.set(name, p)
  }
  return p as Promise<Files[K]>
}

export function sum(c: Counts | undefined): number {
  if (!c) return 0
  return Object.values(c).reduce((a, b) => a + b, 0)
}
