// every number the landing page shows, read at build time from the json files `shapekit site data`
// writes. the page computes only counts, sums and filters over these files.
import { readFile } from "node:fs/promises"
import type { Conformance, Coverage, Entries, Finding, Findings, Harness, Meta } from "@/lib/data"
import { proofText } from "./content"

async function json<T>(name: string): Promise<T> {
  return JSON.parse(await readFile(`public/data/${name}.json`, "utf8")) as T
}

// sources grouped as the page draws them: written by a local stellar-core, stellar-core test goldens,
// public network history, refused by stellar-core
const sourceGroup: Record<string, number> = {
  "local-core": 0,
  "core-golden": 1,
  "mainnet-history": 2,
  "testnet-history": 2,
  "local-core-refused": 3,
}

export interface LandingFinding {
  id: string
  title: string
  component: string
  severity: string
  filed: boolean
  link: string | null
  ref: string | null
}

export interface Bug {
  id: string
  component: string
  title: string
  link: string
  observed: string
  before: { label: string; commit: string | null }
  after: { label: string; commit: string | null }
  verdict: string
  why: string
  passWhy: string
  entry: string
  command: string
  harness: boolean
}

export interface IndexerRun {
  target: string
  note: string
  entries: number
  pass: number
  mishandled: number
  dropped: number
  outOfScope: number
  notEvaluated: number
}

export interface Sdk {
  key: string
  package: string
  current: string
}

export interface LandingStats {
  corpusVersion: string
  protocol: number
  coreVersion: string
  xdrTag: string
  entries: number
  coreEmitted: number
  synthetic: number
  refused: number
  sources: { local: number; golden: number; history: number; refused: number }
  decode: { accept: number; reject: number; defined: number }
  arms: { covered: number; reachable: number; refused: number; never: number; other: number }
  caps: { visible: number; covered: number }
  fields: { notValidated: number; exercised: number }
  findings: LandingFinding[]
  findingsIntended: string[]
  /** upstream issues shapekit filed that the findings link, each counted once */
  issuesFiled: number
  /** upstream issues the findings link that shapekit did not file, each counted once */
  issuesReported: number
  sdks: Sdk[]
  runs: IndexerRun[]
  bugs: Bug[]
  guide: { id: string; validated: string }[]
  tiers: Record<number, [number, number, number, number]>
}

function issueRef(link: string | null): string | null {
  const m = link?.match(/github\.com\/[^/]+\/([^/]+)\/(?:issues|pull)\/(\d+)/)
  return m ? `${m[1]}#${m[2]}` : null
}

const sdkOrder = ["rust", "go", "js", "python", "java", "dart", "php", "dotnet", "kmp", "soroban", "stellar-cli"]
const runNotes: Record<string, string> = { "wallet-backend": "reference adapter", "stellar-rpc": "release image" }

export async function loadLandingStats(): Promise<LandingStats> {
  const [meta, coverage, entries, findings, harness, conformance] = await Promise.all([
    json<Meta>("meta"),
    json<Coverage>("coverage"),
    json<Entries>("entries"),
    json<Findings>("findings"),
    json<Harness>("harness"),
    json<Conformance>("conformance"),
  ])
  const sum = coverage.summary
  const byClass = sum.entries_by_source_class
  const n = (k: string) => byClass[k] ?? 0

  const decode = { accept: 0, reject: 0, defined: 0 }
  const tiers: Record<number, [number, number, number, number]> = {}
  for (const e of entries.entries) {
    if (e.expected.decode === "must-accept") decode.accept++
    else if (e.expected.decode === "must-reject") decode.reject++
    else decode.defined++
    const g = sourceGroup[e.source]
    const p = e.protocol.generated_at
    if (g === undefined || p === undefined) continue
    const row = (tiers[p] ??= [0, 0, 0, 0])
    row[g] = (row[g] ?? 0) + 1
  }

  const blockers = sum.arms_uncovered_by_blocker
  const refusedArms = blockers["refused-before-ledger"] ?? 0
  const neverArms = blockers["never-emitted"] ?? 0

  const all: Finding[] = findings.findings
  // the candidate lane's findings join once they have an upstream decision: filed, or a
  // duplicate of an issue reported before; drafts and won't-fix records stay off the page
  const candidates = (findings.candidates?.findings ?? []).filter((f) => f.status === "filed" || f.status === "duplicate-of")
  const bugs = [...all.filter((f) => f.status !== "wont-fix"), ...candidates]
  // shapekit's own issues are the links of the filed findings of both lanes; a finding that
  // links one of them is shapekit's whatever its status, so it is never counted as reported before
  const ownLinks = new Set([...all, ...(findings.candidates?.findings ?? [])].flatMap((f) => (f.status === "filed" && f.link ? [f.link] : [])))
  const own = (f: Finding) => f.link !== null && ownLinks.has(f.link)
  // findings that share one upstream link count once
  const issues = (keep: (f: Finding) => boolean) => new Set(bugs.filter(keep).map((f) => f.link ?? f.id)).size
  const landingFindings: LandingFinding[] = bugs.map((f) => ({
    id: f.id,
    title: f.short ?? f.title,
    component: f.component,
    severity: f.severity,
    filed: own(f),
    link: f.link,
    ref: issueRef(f.link),
  }))

  const sdks: Sdk[] = conformance.sdks
    .map((s) => ({ key: s.sdk, package: s.package, current: s.current }))
    .toSorted((a, b) => sdkOrder.indexOf(a.key) - sdkOrder.indexOf(b.key))

  const etlRef = meta.pins.git.find((g) => g.name === "stellar-etl")?.ref?.replace("refs/tags/", "")
  const runs: IndexerRun[] = harness.runs
    .filter((r) => r.describe)
    .map((r) => {
      const v = r.totals.by_verdict
      const target = r.describe?.target ?? ""
      return {
        target,
        note: runNotes[target] ?? (target === "stellar-etl" && etlRef ? etlRef : ""),
        entries: r.totals.entries,
        pass: v["pass"] ?? 0,
        mishandled: v["mishandled"] ?? 0,
        dropped: v["dropped"] ?? 0,
        outOfScope: v["out-of-scope"] ?? 0,
        notEvaluated: v["not-evaluated"] ?? 0,
      }
    })
    .toSorted((a, b) => ["wallet-backend", "stellar-rpc", "stellar-etl"].indexOf(a.target) - ["wallet-backend", "stellar-rpc", "stellar-etl"].indexOf(b.target))

  // the known bugs: text per id from content.ts, everything else from the reproduction tables
  const wb = "https://github.com/stellar/wallet-backend/commit/"
  const bugsOut: Bug[] = []
  const seen = new Set<string>()
  for (const r of harness.reproductions.rows) {
    const t = proofText[r.id]
    if (!t) throw new Error(`landing: no card text for harness reproduction ${r.id}`)
    const pre = r.cases.find((c) => c.label === "pre-fix")
    const fixed = r.cases.find((c) => c.label === "fixed")
    const first = pre?.expect[0]
    if (!pre || !fixed || !first) throw new Error(`landing: reproduction ${r.id} lacks a pre-fix or fixed case`)
    seen.add(r.id)
    bugsOut.push({
      id: r.id,
      component: "wallet-backend",
      title: t.title,
      link: r.issue,
      observed: t.observed,
      before: { label: `commit ${pre.commit.slice(0, 7)}`, commit: wb + pre.commit },
      after: { label: `commit ${fixed.commit.slice(0, 7)}`, commit: wb + fixed.commit },
      verdict: first.reason ? `${first.verdict}: ${first.reason.replace(/-/g, " ")}` : first.verdict,
      why: t.why,
      passWhy: "Every fact of the entry is stored with the same value.",
      entry: first.entry,
      command: "shapekit harness reproduce",
      harness: true,
    })
  }
  const sdkComponent: Record<string, string> = { js: "js-stellar-sdk", go: "go-stellar-sdk", rust: "rs-stellar-xdr" }
  for (const r of conformance.reproductions) {
    const t = proofText[r.id]
    if (!t) throw new Error(`landing: no card text for conformance reproduction ${r.id}`)
    const entry = r.entries[0] ?? r.must_diverge[0]?.entry
    if (!entry) throw new Error(`landing: reproduction ${r.id} names no entry`)
    seen.add(r.id)
    bugsOut.push({
      id: r.id,
      component: sdkComponent[r.sdk] ?? r.sdk,
      title: t.title,
      link: r.link,
      observed: t.observed,
      before: { label: Object.keys(r.fail).join(" and "), commit: null },
      after: { label: Object.keys(r.pass).join(" and "), commit: null },
      verdict: "diverges",
      why: t.why,
      passWhy: "The answer is equal to the expected answer.",
      entry,
      command: "shapekit conformance reproduce",
      harness: false,
    })
  }
  for (const id of Object.keys(proofText)) {
    if (!seen.has(id)) throw new Error(`landing: card text for ${id}, which no reproduction table holds`)
  }

  const known = new Set(entries.entries.map((e) => e.id))
  for (const id of requiredEntries) {
    if (!known.has(id)) throw new Error(`landing: the page names corpus entry ${id}, which the corpus does not hold`)
  }

  return {
    corpusVersion: meta.corpus.version,
    protocol: Math.max(...meta.corpus.protocol_tiers),
    coreVersion: meta.corpus.generated_with.core_image.tag.split("-")[0] ?? "",
    xdrTag: meta.corpus.generated_with.xdr_definitions.tag,
    entries: sum.entries,
    coreEmitted: sum.core_emitted_entries,
    synthetic: n("synthetic"),
    refused: n("local-core-refused"),
    sources: { local: n("local-core"), golden: n("core-golden"), history: n("mainnet-history") + n("testnet-history"), refused: n("local-core-refused") },
    decode,
    arms: { covered: sum.arms_covered, reachable: sum.arms_reachable, refused: refusedArms, never: neverArms, other: sum.arms_uncovered - refusedArms - neverArms },
    caps: { visible: sum.caps_indexer_visible, covered: sum.caps_indexer_visible_core_emitted },
    fields: { notValidated: sum.fields_not_fully_validated, exercised: sum.fields_not_fully_validated_exercised },
    findings: landingFindings,
    findingsIntended: all.filter((f) => f.status === "wont-fix").map((f) => f.id),
    issuesFiled: issues(own),
    issuesReported: issues((f) => !own(f)),
    sdks,
    runs,
    bugs: bugsOut,
    guide: entries.guide.fields.map((f) => ({ id: f.id, validated: f.validated_by_core })),
    tiers,
  }
}

// corpus entries the page text names; the build fails if one leaves the corpus
const requiredEntries = [
  "cap40.signed-payload-signer.p28-account-four-payload-signers",
  "cap23.claimable-balances.synth-strkey-b-invalid",
  "soroban.scval-shapes.synth-bool-value-2",
]
