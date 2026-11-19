// the results of the committed runs for one corpus entry, read by the entry list, the entry
// page and the cap pages
import type { Conformance, HarnessRun } from "./data"
import { checkResults, label } from "./terms"

export const failingResults = new Set(checkResults.filter((c) => c.id !== "unsupported").map((c) => c.id))

export interface SdkFailure {
  sdk: string
  version: string
  result: string
  known: string | null
  checks: string[]
}

// per entry, sdk and version: the failing checks of the committed matrices
export function sdkFailures(conformance: Conformance): Map<string, SdkFailure[]> {
  const m = new Map<string, SdkFailure[]>()
  for (const s of conformance.sdks)
    for (const e of s.entries)
      for (const v of s.versions) {
        const cell = e.cells[v.key]
        if (!cell) continue
        const list = m.get(e.id) ?? []
        list.push({
          sdk: s.sdk,
          version: v.version,
          result: cell.result,
          known: cell.known_issue,
          checks: cell.checks.filter((k) => failingResults.has(k.result)).map((k) => k.check),
        })
        m.set(e.id, list)
      }
  return m
}

// every committed indexer run's verdict on the entry, per network
export function indexerVerdicts(runs: HarnessRun[], id: string) {
  return runs.flatMap((run, i) =>
    Object.entries(run.verdicts[id] ?? {})
      .toSorted(([a], [b]) => a.localeCompare(b))
      .map(([network, r]) => ({ target: run.describe?.target ?? `run ${i + 1}`, network, verdict: r.verdict, reason: r.reason })),
  )
}

export const verdictText = (r: { verdict: string; reason: string | null }) => (r.reason ? `${label(r.verdict)} (${r.reason})` : label(r.verdict))
