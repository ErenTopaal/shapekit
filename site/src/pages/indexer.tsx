import { use, useMemo } from "react"
import { Link, useSearchParams } from "react-router"
import { load, type FamilyRow, type HarnessRun, type HarnessReproduction } from "@/lib/data"
import { docs, label, lintRules } from "@/lib/terms"
import { runSummary } from "@/lib/metrics"
import { capPath, entryPath, findingsPath } from "@/lib/paths"
import { useMeta } from "@/lib/meta"
import { Input } from "@/components/ui/input"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { VerdictBar, VerdictLegend, verdictOrder } from "@/components/charts"
import {
  Command,
  ExtLink,
  GlossaryLink,
  Id,
  More,
  NextSteps,
  PageHeader,
  PageTerms,
  RepoLink,
  Section,
  Status,
  TextLink,
  bytes,
  fmt,
  indexerHref,
  issueLabel,
  short,
  useHashScroll,
  verdictTone,
} from "@/components/common"

// an entry's own page
function entryHref(entry: string): string {
  return entryPath(entry)
}

// whether every family got the same verdicts on every network, so one column can stand for all
function networksAgree(run: HarnessRun, networks: string[]): boolean {
  const key = (n: string) =>
    JSON.stringify(
      run.families
        .filter((f) => f.network === n)
        .map((f) => [f.family, f.verdicts])
        .toSorted(),
    )
  return networks.every((n) => key(n) === key(networks[0] ?? ""))
}

function Families({ run, networks, agree, families }: { run: HarnessRun; networks: string[]; agree: boolean; families: FamilyRow[] }) {
  const shown = agree ? networks.slice(0, 1) : networks
  const fams = [...new Set(run.families.map((f) => f.family))].toSorted()
  const get = (fam: string, net: string) => run.families.find((f) => f.family === fam && f.network === net)?.verdicts
  return (
    <Table>
      <caption className="sr-only">Verdicts per shape family{agree ? ", the same on every network" : ", per network"}</caption>
      <TableHeader>
        <TableRow>
          <TableHead>
            <GlossaryLink id="family">Shape family</GlossaryLink>
          </TableHead>
          {shown.map((n) => (
            <TableHead key={n} className="min-w-52">
              <GlossaryLink id="verdict">Verdicts</GlossaryLink> on {agree ? "each network" : n}
            </TableHead>
          ))}
        </TableRow>
      </TableHeader>
      <TableBody>
        {fams.map((fam) => {
          const cap = families.find((f) => f.id === fam)?.caps[0]
          return (
            <TableRow key={fam}>
              <TableCell className="align-top">
                {cap ? (
                  <TextLink to={capPath(cap, fam)} className="font-mono text-xs">
                    {fam}
                  </TextLink>
                ) : (
                  <Id>{fam}</Id>
                )}
              </TableCell>
              {shown.map((n) => {
                const c = get(fam, n)
                return (
                  <TableCell key={n} className="align-top">
                    {c ? (
                      <span className="flex flex-col gap-1.5">
                        <VerdictBar counts={c} className="h-2.5" />
                        <VerdictLegend counts={c} className="gap-x-3 text-xs" />
                      </span>
                    ) : (
                      <span className="text-xs text-muted-foreground">not run</span>
                    )}
                  </TableCell>
                )
              })}
            </TableRow>
          )
        })}
      </TableBody>
    </Table>
  )
}

function Problems({ run }: { run: HarnessRun }) {
  const [params, setParams] = useSearchParams()
  const q = params.get("q") ?? ""
  const verdict = params.get("verdict") ?? "all"
  const network = params.get("network") ?? "testnet"
  const set = (k: string, v: string, dflt: string) => {
    const next = new URLSearchParams(params)
    if (v === dflt) next.delete(k)
    else next.set(k, v)
    setParams(next, { replace: true, preventScrollReset: true })
  }
  const present = [...new Set(run.problems.map((p) => p.verdict))]
  const verdictList = verdictOrder.filter((v) => present.includes(v))
  const networks = [...new Set(run.problems.map((p) => p.network))].toSorted()
  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase()
    return run.problems.filter(
      (p) =>
        (verdict === "all" || p.verdict === verdict) &&
        (network === "both" || p.network === network) &&
        (!needle ||
          p.entry.toLowerCase().includes(needle) ||
          p.family.toLowerCase().includes(needle) ||
          p.problems.some((x) => x.fact.toLowerCase().includes(needle) || x.detail.toLowerCase().includes(needle))),
    )
  }, [run.problems, q, verdict, network])

  return (
    <Section
      title="Failing replays"
      id="problems"
      level={3}
      description="This list shows each replay with a verdict other than pass or out of scope. Each line gives a fact that the indexer stored in another way than the ledger meta shows. The kind dropped means that the indexer did not store the fact. The kind extra means that the indexer stored a fact that the meta does not show. The kind wrong-value means that the indexer stored the fact with another value."
    >
      <div className="flex flex-col gap-3 lg:flex-row lg:flex-wrap lg:items-center">
        <label className="sr-only" htmlFor="problem-filter">
          Filter failing entries
        </label>
        <Input
          id="problem-filter"
          type="search"
          placeholder="Filter by entry, family or fact"
          value={q}
          onChange={(e) => set("q", e.target.value, "")}
          className="lg:max-w-sm"
        />
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          className="flex-wrap"
          value={verdict}
          onValueChange={(v) => v && set("verdict", v, "all")}
          aria-label="Verdict"
        >
          <ToggleGroupItem value="all">All verdicts</ToggleGroupItem>
          {verdictList.map((v) => (
            <ToggleGroupItem key={v} value={v}>
              {label(v)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          value={network}
          onValueChange={(v) => v && set("network", v, "testnet")}
          aria-label="Network"
        >
          {networks.map((n) => (
            <ToggleGroupItem key={n} value={n}>
              {n}
            </ToggleGroupItem>
          ))}
          <ToggleGroupItem value="both">both</ToggleGroupItem>
        </ToggleGroup>
      </div>
      <p className="text-sm text-muted-foreground" aria-live="polite">
        {rows.length} of {run.problems.length} failing replays shown.
      </p>
      {rows.length === 0 ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyTitle>No failing replay matches</EmptyTitle>
            <EmptyDescription>Clear the filter or pick another verdict or network.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <ol className="flex flex-col divide-y border-y">
          {rows.map((p) => {
            const href = entryHref(p.entry)
            return (
              <li key={`${p.entry}/${p.network}`} className="flex flex-col gap-2 py-3">
                <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                  <TextLink to={href} className="font-mono text-sm [overflow-wrap:anywhere]">
                    {p.entry}
                  </TextLink>
                  <Status tone={verdictTone(p.verdict)}>
                    <GlossaryLink id={`verdict-${p.verdict}`}>{label(p.verdict)}</GlossaryLink>
                  </Status>
                  {p.reason ? <span className="text-xs text-muted-foreground">{p.reason}</span> : null}
                  <span className="text-xs text-muted-foreground">
                    {p.network}, chain <Id>{p.chain}</Id>, ledger {p.ledger}
                  </span>
                  <TextLink to={findingsPath({ q: p.entry })} className="text-xs">
                    findings on this entry
                  </TextLink>
                </div>
                <p className="text-sm text-muted-foreground [overflow-wrap:anywhere]">{p.detail}</p>
                {p.problems.length > 0 ? (
                  <ul className="flex flex-col gap-1.5 text-sm">
                    {p.problems.map((x, i) => (
                      <li key={i} className="grid grid-cols-[minmax(0,1fr)] gap-x-3 sm:grid-cols-[5rem_11rem_minmax(0,1fr)]">
                        <span className="font-medium">{x.kind}</span>
                        <Id className="text-muted-foreground">{x.fact}</Id>
                        <span className="text-muted-foreground [overflow-wrap:anywhere]">{x.detail}</span>
                      </li>
                    ))}
                  </ul>
                ) : null}
              </li>
            )
          })}
        </ol>
      )}
    </Section>
  )
}

function verdictText(v: string, reason: string | null) {
  return (
    <span>
      <GlossaryLink id={`verdict-${v}`}>{label(v)}</GlossaryLink>
      {reason ? ` (${reason})` : null}
    </span>
  )
}

function Reproductions({ rows, table, repo }: { rows: HarnessReproduction[]; table: string; repo: string }) {
  return (
    <Section
      level={3}
      title="Upstream wallet-backend bugs that shapekit reproduces"
      id="reproductions"
      description={
        <>
          Each row of <RepoLink path={table} /> names a wallet-backend bug that somebody reported upstream, the corpus
          chain that shows it, and the verdicts that its entries must get before and after the fix. The nightly workflow
          runs the rows and commits no result. The last column gives the verdict of the committed run at the pinned
          commit. <TextLink to={docs.harnessReproductions}>How reproductions work</TextLink>. Run them yourself:
        </>
      }
    >
      <Command>shapekit harness reproduce --table adapters/walletbackend/reproductions.toml</Command>
      <ul className="flex flex-col divide-y border-y">
        {rows.map((r) => (
          <li key={r.id} className="flex flex-col gap-2 py-3 text-sm">
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
              <ExtLink href={r.issue} className="font-medium">
                {issueLabel(r.issue)}
              </ExtLink>
              <span className="text-xs text-muted-foreground">
                chain <Id>{r.chain}</Id> on {r.network}
              </span>
            </div>
            {r.notes ? <p className="max-w-[80ch] text-muted-foreground">{r.notes}</p> : null}
            <Table>
              <caption className="sr-only">Expected verdicts of {r.id}</caption>
              <TableHeader>
                <TableRow>
                  <TableHead>Entry</TableHead>
                  {r.cases.map((c) => (
                    <TableHead key={c.label}>
                      {c.label} <ExtLink href={`${repo}/commit/${c.commit}`} className="font-mono text-xs font-normal">{short(c.commit, 8)}</ExtLink>
                    </TableHead>
                  ))}
                  <TableHead>Committed run at the pin</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {[...new Set(r.cases.flatMap((c) => c.expect.map((x) => x.entry)))].map((entry) => {
                  const href = entryHref(entry)
                  const atPin = r.cases.flatMap((c) => c.expect).find((x) => x.entry === entry)?.at_pin ?? []
                  return (
                    <TableRow key={entry}>
                      <TableCell className="max-w-80 align-top whitespace-normal">
                        <TextLink to={href} className="font-mono text-xs [overflow-wrap:anywhere]">
                          {entry}
                        </TextLink>
                      </TableCell>
                      {r.cases.map((c) => {
                        const x = c.expect.find((y) => y.entry === entry)
                        return (
                          <TableCell key={c.label} className="align-top text-xs">
                            {x ? verdictText(x.verdict, x.reason) : <span className="text-muted-foreground">not named</span>}
                          </TableCell>
                        )
                      })}
                      <TableCell className="align-top">
                        {atPin.length > 0 ? (
                          atPin.map((a) => (
                            <Status key={a.report} tone={verdictTone(a.verdict)}>
                              {verdictText(a.verdict, a.reason)}
                            </Status>
                          ))
                        ) : (
                          <span className="text-xs text-muted-foreground">not in the committed run</span>
                        )}
                      </TableCell>
                    </TableRow>
                  )
                })}
              </TableBody>
            </Table>
          </li>
        ))}
      </ul>
    </Section>
  )
}

const targetOf = (r: HarnessRun, i: number) => r.describe?.target ?? `run ${i + 1}`

// the summary of every indexer run: the one home of the per-run numbers the start page and the
// landing page also show
function RunTable({ runs, index }: { runs: HarnessRun[]; index: number }) {
  return (
    <Table>
      <caption className="sr-only">Each indexer run: the verdicts of its replays</caption>
      <TableHeader>
        <TableRow>
          <TableHead>Indexer</TableHead>
          <TableHead className="text-right">
            <GlossaryLink id="verdict-pass">Pass</GlossaryLink>
            <span className="block text-xs font-normal text-muted-foreground">higher is better</span>
          </TableHead>
          <TableHead className="text-right">
            <GlossaryLink id="verdict-mishandled">Mishandled</GlossaryLink> or <GlossaryLink id="verdict-dropped">dropped</GlossaryLink>
            <span className="block text-xs font-normal text-muted-foreground">lower is better</span>
          </TableHead>
          <TableHead className="hidden text-right sm:table-cell">
            <GlossaryLink id="verdict-not-evaluated">Not evaluated</GlossaryLink>
            <span className="block text-xs font-normal text-muted-foreground">lower is better</span>
          </TableHead>
          <TableHead className="hidden text-right sm:table-cell">
            <GlossaryLink id="verdict-out-of-scope">Out of scope</GlossaryLink>
            <span className="block text-xs font-normal text-muted-foreground">not a failure</span>
          </TableHead>
        </TableRow>
      </TableHeader>
      <TableBody className="num">
        {runs.map((run, i) => {
          const x = runSummary(run, i)
          return (
            <TableRow key={run.report} aria-current={i === index ? "true" : undefined} className={i === index ? "bg-accent/50" : undefined}>
              <TableCell className="font-sans whitespace-normal">
                <Link
                  to={indexerHref(x.target)}
                  preventScrollReset
                  className="font-medium text-primary underline decoration-primary/40 hover:decoration-primary"
                >
                  {x.target}
                </Link>
                <span className="block text-xs text-muted-foreground">
                  {fmt.format(x.replays)} replays
                </span>
              </TableCell>
              <TableCell className="text-right font-semibold">{fmt.format(x.pass)}</TableCell>
              <TableCell className="text-right">{x.wrong > 0 ? <Status tone="diverge">{fmt.format(x.wrong)}</Status> : <span className="text-muted-foreground">0</span>}</TableCell>
              <TableCell className="hidden text-right sm:table-cell">{fmt.format(x.byVerdict["not-evaluated"] ?? 0)}</TableCell>
              <TableCell className="hidden text-right sm:table-cell">{fmt.format(x.byVerdict["out-of-scope"] ?? 0)}</TableCell>
            </TableRow>
          )
        })}
      </TableBody>
    </Table>
  )
}

export function IndexerPage() {
  const meta = useMeta()
  // the target's repository, from its pin in pins.toml
  const repoOf = (name: string) => meta.pins.git.find((g) => g.name === name)?.url.replace(/\.git$/, "") ?? `https://github.com/stellar/${name}`
  const harness = use(load("harness"))
  const coverage = use(load("coverage"))
  const [params] = useSearchParams()
  useHashScroll()
  const wanted = params.get("target")
  const index = Math.max(
    0,
    harness.runs.findIndex((r, i) => targetOf(r, i) === wanted),
  )
  const run = harness.runs[index]
  if (!run) {
    return (
      <>
        <PageHeader title="Indexer results" />
        <Empty>
          <EmptyHeader>
            <EmptyTitle>No harness report is committed</EmptyTitle>
            <EmptyDescription>findings/triage.toml names no harness input.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      </>
    )
  }
  const d = run.describe
  const sum = runSummary(run, index)
  const networks = sum.networks
  const agree = networks.length > 1 && networksAgree(run, networks)
  const families = coverage.families
  return (
    <div className="flex flex-col gap-6">
      <PageHeader title="Indexer results">
        <p>
          The replay harness gives the ledgers of the corpus to each indexer through an{" "}
          <GlossaryLink id="adapter">adapter</GlossaryLink>. Then it compares each fact that the indexer stored with the
          ledger meta. Each <GlossaryLink id="replay">replay</GlossaryLink> is one entry on one network and gets one{" "}
          <GlossaryLink id="verdict">verdict</GlossaryLink>.
        </p>
      </PageHeader>

      <NextSteps>
        <li>
          <TextLink to={docs.indexerCi}>Add the replay to your indexer CI</TextLink>. CI fails when a verdict gets worse.
        </li>
        <li>
          <TextLink to={docs.indexerCiLocal}>Reproduce a result on your computer</TextLink> with the command on the entry
          page.
        </li>
        <li>Read the failing replays below to see the facts that the indexer stored in another way.</li>
      </NextSteps>

      <Section title="Summary" id="summary" description="Each indexer run replays the same entries. Select an indexer to see its run in detail.">
        <RunTable runs={harness.runs} index={index} />
      </Section>

      <PageTerms ids={["replay", "verdict", "indexer-terms", "claim", "adapter"]} />

      <section aria-labelledby="run-h" className="flex flex-col gap-3" id="run-detail">
        <h2 id="run-h" className="text-xl font-semibold tracking-tight">
          {d?.target ?? "The run"} in detail
        </h2>
        <p className="max-w-[70ch] text-sm text-muted-foreground">
          The harness ran {d?.target ?? "the target"} at commit{" "}
          <ExtLink href={`${repoOf(d?.target ?? "")}/commit/${d?.target_version ?? ""}`}>
            <Id>{short(d?.target_version ?? "", 10)}</Id>
          </ExtLink>{" "}
          through the adapter <Id>{d?.adapter ?? "the adapter"}</Id> {d?.adapter_version}, over {d?.default_delivery ?? "its default delivery"}.
          The report is <RepoLink path={run.report} />.{" "}
          {d?.target === "wallet-backend" ? (
            <TextLink to={docs.walletBackendAdapter}>The wallet-backend adapter</TextLink>
          ) : (
            <TextLink to={docs.adapter}>The adapter interface</TextLink>
          )}
          .
        </p>

        {!run.ok ? (
          <Alert variant="destructive">
            <AlertTitle>The run did not complete correctly</AlertTitle>
            <AlertDescription>
              A chain did not run, the memory checks did not hold, or two rounds gave different verdicts. Read the report.
            </AlertDescription>
          </Alert>
        ) : null}

        <Section
          title="Verdicts"
          id="verdicts"
          level={3}
          description={
            <>
              {fmt.format(sum.replays)} replays: {fmt.format(sum.entries)} entries on {networks.join(" and ")}, from {run.chains}{" "}
              chain runs.{" "}
              {agree ? "Each network gave the same verdicts." : "The networks gave different verdicts, so each network has its own bar."}
            </>
          }
        >
          {agree ? (
            <div className="flex flex-col gap-2">
              <VerdictBar counts={sum.byVerdict} className="h-4" />
              <VerdictLegend counts={sum.byVerdict} />
            </div>
          ) : (
            networks.map((n) => (
              <div key={n} className="flex flex-col gap-2">
                <span className="text-sm font-medium">{n}</span>
                <VerdictBar counts={run.by_network[n] ?? {}} className="h-4" />
                <VerdictLegend counts={run.by_network[n] ?? {}} />
              </div>
            ))
          )}
          <p className="text-sm text-muted-foreground">
            Reasons of the <GlossaryLink id="verdict-mishandled">mishandled</GlossaryLink> replays:{" "}
            {Object.entries(run.totals.by_reason)
              .map(([k, v]) => `${k} ${v}`)
              .join(", ") || "none"}
            . The harness did not replay {fmt.format(run.totals.without_chain)} entries, because they are on no recorded
            ledger chain (core goldens, history captures, synthetic bytes, refused envelopes and vectors). It also did not
            replay {run.totals.without_oracle} pairs of entry and network that have no{" "}
            <GlossaryLink id="oracle">oracle file</GlossaryLink>.
          </p>
        </Section>

        <Problems key={run.report} run={run} />

        <More label="Show the verdicts for each shape family">
          <div className="overflow-x-auto">
            <Families run={run} networks={networks} agree={agree} families={families} />
          </div>
        </More>
      {/* the reproduction table and the linter result are wallet-backend's */}
      {d?.target === "wallet-backend" ? (
        <Reproductions
          rows={harness.reproductions.rows}
          table={harness.reproductions.table}
          repo={repoOf("wallet-backend")}
        />
      ) : null}

      <More label="Show how the run went: rounds and memory">
        <dl className="grid gap-x-8 gap-y-3 text-sm sm:grid-cols-2">
          <div>
            <dt className="font-medium">Rounds</dt>
            <dd className="num text-muted-foreground">
              {run.rounds.length}, verdicts{" "}
              {run.rounds.every((r) => r.same_verdicts_as_first) ? "identical in every round" : "differ between rounds"};{" "}
              {run.rounds.map((r) => `${Math.round(r.seconds / 60)} min`).join(" and ")}
            </dd>
          </div>
          <div>
            <dt className="font-medium">Harness memory</dt>
            <dd className="num text-muted-foreground">
              peak {bytes(Math.max(...run.rounds.map((r) => r.peak_rss_bytes)))},{" "}
              {run.memory.flat ? "within the allowed growth across rounds" : "grew past the allowed growth"} (
              {bytes(run.memory.slack_bytes)})
            </dd>
          </div>
          {run.target_memory ? (
            <div>
              <dt className="font-medium">Indexer memory</dt>
              <dd className="num text-muted-foreground">
                peak {run.target_memory.peak_bytes ? bytes(run.target_memory.peak_bytes) : "not reported"} of a{" "}
                {bytes(run.target_memory.limit_bytes)} limit, {run.target_memory.flat ? "flat" : "grew"} across rounds
              </dd>
            </div>
          ) : null}
          <div>
            <dt className="font-medium">Protocols the indexer ingests</dt>
            <dd className="num text-muted-foreground">{d ? `${d.protocols.min} to ${d.protocols.max}` : "not reported"}</dd>
          </div>
          <div className="sm:col-span-2">
            <dt className="font-medium">Components</dt>
            <dd className="flex flex-col gap-0.5 text-muted-foreground">
              {Object.entries(run.components).map(([k, v]) => (
                <span key={k} className="[overflow-wrap:anywhere]">
                  {k}: <Id>{v}</Id>
                </span>
              ))}
            </dd>
          </div>
        </dl>
        {run.chain_errors.length > 0 ? (
          <ul className="text-sm text-diverge">
            {run.chain_errors.map((c) => (
              <li key={`${c.chain}/${c.network}`}>
                {c.chain} ({c.network}): {c.error}
              </li>
            ))}
          </ul>
        ) : null}
      </More>

      {harness.lint.filter((l) => l.target === d?.target).map((l) => (
        <Section
          key={l.result}
          title={`Linter results for ${l.target}`}
          id="lint"
          level={3}
          description={
            <>
              <TextLink to={docs.linter}>indexerlint</TextLink> checks the source code of a Go indexer for three
              ingestion mistakes. IDX001: the code trusts a field that stellar-core does not fully check. IDX002: a
              recursion has no depth or size limit. IDX003: the code reads the participants from the envelope, not from
              the meta. The command <Id>{l.command}</Id> at <Id>{short(l.version, 10)}</Id> gave {l.findings.length}{" "}
              {l.findings.length === 1 ? "result" : "results"}. It suppressed {l.suppressed} and the baseline holds{" "}
              {l.baselined}. The result file is <RepoLink path={l.result} />.
            </>
          }
        >
          {l.findings.length === 0 ? (
            <p className="text-sm text-muted-foreground">The linter reported no result.</p>
          ) : (
            <ul className="flex flex-col divide-y border-y text-sm">
              {l.findings.map((f) => (
                <li key={`${f.file}:${f.line}:${f.column}`} className="flex flex-col gap-1 py-3">
                  <span className="flex flex-wrap gap-x-3">
                    <TextLink to={lintRules[f.rule] ? `${docs.linter}#${lintRules[f.rule]}` : docs.linter} className="font-mono font-medium">
                      {f.rule}
                    </TextLink>
                    <ExtLink
                      href={`${repoOf(l.target)}/blob/${l.version}/${f.file}#L${f.line}`}
                      className="font-mono text-[0.85em] [overflow-wrap:anywhere]"
                    >
                      {f.file}:{f.line}
                    </ExtLink>
                  </span>
                  <span className="text-muted-foreground">{f.message}</span>
                </li>
              ))}
            </ul>
          )}
        </Section>
      ))}
      </section>
    </div>
  )
}
