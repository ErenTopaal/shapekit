import { use, useMemo } from "react"
import { useSearchParams } from "react-router"
import { load, sum, type FamilyRow, type HarnessRun, type HarnessReproduction } from "@/lib/data"
import { docs, lintRules, verdicts } from "@/lib/terms"
import { useMeta } from "@/lib/meta"
import { Input } from "@/components/ui/input"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { VerdictBar, VerdictLegend, verdictOrder } from "@/components/charts"
import {
  ExtLink,
  Id,
  PageHeader,
  RepoLink,
  Section,
  Status,
  Term,
  Terms,
  TextLink,
  bytes,
  fmt,
  issueLabel,
  short,
  useHashScroll,
  verdictTone,
} from "@/components/common"

// where an entry's own page is: the cap page of its family, at the entry's row
function entryHref(entry: string, family: string, families: FamilyRow[]): string | null {
  const cap = families.find((f) => f.id === family)?.caps[0]
  return cap ? `/dashboard/coverage/${cap}#${entry}` : null
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
          <TableHead>Family</TableHead>
          {shown.map((n) => (
            <TableHead key={n} className="min-w-52">
              {agree ? `Verdicts (each network)` : n}
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
                  <TextLink to={`/dashboard/coverage/${cap}#${fam}`} className="font-mono text-xs">
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

function Problems({ run, families }: { run: HarnessRun; families: FamilyRow[] }) {
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
      title="Failing entries"
      id="problems"
      description="Every replayed entry whose verdict is not pass or out of scope, with each fact the indexer stored differently from what the ledger meta shows. In the first column, dropped means the meta shows the fact and the indexer did not store it, extra means the indexer stored a fact the meta does not show, and wrong-value means it stored the fact with another value."
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
          value={verdict}
          onValueChange={(v) => v && set("verdict", v, "all")}
          aria-label="Verdict"
        >
          <ToggleGroupItem value="all">All verdicts</ToggleGroupItem>
          {verdictList.map((v) => (
            <ToggleGroupItem key={v} value={v}>
              {v}
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
            const href = entryHref(p.entry, p.family, families)
            return (
              <li key={`${p.entry}/${p.network}`} className="flex flex-col gap-2 py-3">
                <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                  {href ? (
                    <TextLink to={href} className="font-mono text-sm [overflow-wrap:anywhere]">
                      {p.entry}
                    </TextLink>
                  ) : (
                    <Id className="text-sm">{p.entry}</Id>
                  )}
                  <Status tone={verdictTone(p.verdict)}>{p.verdict}</Status>
                  {p.reason ? <span className="text-xs text-muted-foreground">{p.reason}</span> : null}
                  <span className="text-xs text-muted-foreground">
                    {p.network}, chain <Id>{p.chain}</Id>, ledger {p.ledger}
                  </span>
                  <TextLink to={`/dashboard/findings?q=${encodeURIComponent(p.entry)}`} className="text-xs">
                    findings
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
  return reason ? `${v} (${reason})` : v
}

function Reproductions({
  rows,
  table,
  families,
  repo,
}: {
  rows: HarnessReproduction[]
  table: string
  families: FamilyRow[]
  repo: string
}) {
  return (
    <Section
      title="Reproductions of upstream wallet-backend bugs"
      id="reproductions"
      description={
        <>
          Each row of <RepoLink path={table} /> names a wallet-backend bug reported upstream, the corpus chain that carries
          it, and the verdicts its entries must get on a commit before the fix and on the fix.{" "}
          <Id>shapekit harness reproduce</Id> runs them in the nightly workflow, which commits no result; the last column
          is the verdict the committed run gave at the pinned commit. <TextLink to={docs.harnessReproductions}>How reproductions work</TextLink>.
        </>
      }
    >
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
                  const fam = families.find((f) => entry.startsWith(`${f.id}.`))
                  const href = fam ? entryHref(entry, fam.id, families) : null
                  const atPin = r.cases.flatMap((c) => c.expect).find((x) => x.entry === entry)?.at_pin ?? []
                  return (
                    <TableRow key={entry}>
                      <TableCell className="max-w-80 align-top whitespace-normal">
                        {href ? (
                          <TextLink to={href} className="font-mono text-xs [overflow-wrap:anywhere]">
                            {entry}
                          </TextLink>
                        ) : (
                          <Id>{entry}</Id>
                        )}
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

export function IndexerPage() {
  const meta = useMeta()
  // the target's repository, from its pin in pins.toml
  const repoOf = (name: string) => meta.pins.git.find((g) => g.name === name)?.url.replace(/\.git$/, "") ?? `https://github.com/stellar/${name}`
  const harness = use(load("harness"))
  const coverage = use(load("coverage"))
  useHashScroll()
  const run = harness.runs[0]
  if (!run) {
    return (
      <>
        <PageHeader title="Indexer run" />
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
  const networks = Object.keys(run.by_network).toSorted()
  const agree = networks.length > 1 && networksAgree(run, networks)
  const families = coverage.families
  return (
    <div className="flex flex-col">
      <PageHeader title={`Indexer run: ${d?.target ?? "target"}`}>
        <p>
          The replay harness served the corpus&apos;s ledger chains to {d?.target ?? "the target"} at commit{" "}
          <ExtLink href={`${repoOf(d?.target ?? "")}/commit/${d?.target_version ?? ""}`}>
            <Id>{short(d?.target_version ?? "", 10)}</Id>
          </ExtLink>{" "}
          through its adapter, <Id>{d?.adapter ?? "the adapter"}</Id> {d?.adapter_version}, over {d?.default_delivery ?? "its default delivery"}
          , then compared every fact the indexer stored with what the ledger meta shows. Report:{" "}
          <RepoLink path={run.report} />. <TextLink to={docs.walletBackendAdapter}>The wallet-backend adapter</TextLink>.
        </p>
      </PageHeader>

      {!run.ok ? (
        <Alert variant="destructive" className="mb-4">
          <AlertTitle>The run itself did not complete cleanly</AlertTitle>
          <AlertDescription>
            A chain failed to run, the memory checks did not hold, or rounds disagreed. See the report.
          </AlertDescription>
        </Alert>
      ) : null}

      <Terms label="What the verdicts mean">
        {verdicts.map((v) => (
          <Term key={v.id} name={<Status tone={verdictTone(v.id)}>{v.id}</Status>}>
            {v.meaning}.
          </Term>
        ))}
        <Term name="claims">
          The facts the adapter says the indexer records (participants, balance changes, events and so on). A fact the
          indexer does not claim is never a failure. <TextLink to={docs.claims}>Claims</TextLink>.
        </Term>
      </Terms>

      <Section
        title="Verdicts"
        id="verdicts"
        description={
          <>
            {fmt.format(run.distinct_entries)} entries replayed on {networks.join(" and ")}, {fmt.format(run.totals.entries)}{" "}
            replays over {run.chains} chain runs (
            {Object.entries(run.chains_by_network)
              .map(([n, c]) => `${c} on ${n}`)
              .join(", ")}
            ).{agree ? " Every family got the same verdicts on both networks." : ""}
          </>
        }
      >
        {(agree ? networks.slice(0, 1) : networks).map((n) => (
          <div key={n} className="flex flex-col gap-2">
            {!agree ? <span className="text-sm font-medium">{n}</span> : null}
            <VerdictBar counts={run.by_network[n] ?? {}} className="h-4" />
            <VerdictLegend counts={run.by_network[n] ?? {}} />
          </div>
        ))}
        <Table>
          <caption className="sr-only">Verdicts per network</caption>
          <TableHeader>
            <TableRow>
              <TableHead>Network</TableHead>
              {verdictOrder.map((v) => (
                <TableHead key={v} className="text-right">
                  {v}
                </TableHead>
              ))}
              <TableHead className="text-right">Total</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody className="num">
            {networks.map((n) => (
              <TableRow key={n}>
                <TableCell className="font-medium">{n}</TableCell>
                {verdictOrder.map((v) => (
                  <TableCell key={v} className="text-right">
                    {run.by_network[n]?.[v] ?? 0}
                  </TableCell>
                ))}
                <TableCell className="text-right">{sum(run.by_network[n])}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <p className="text-sm text-muted-foreground">
          Mishandled, by reason:{" "}
          {Object.entries(run.totals.by_reason)
            .map(([k, v]) => `${k} ${v}`)
            .join(", ") || "none"}
          . Not replayed: {fmt.format(run.totals.without_chain)} entries that are on no recorded ledger chain (core goldens,
          history captures, synthetic bytes, refused envelopes, vectors), and {run.totals.without_oracle} entry and network
          pairs with no oracle file, the expected facts the comparison needs.
        </p>
      </Section>

      <Section title="Verdicts per family" id="families" description="Select a family for its CAP page and entries.">
        <Families run={run} networks={networks} agree={agree} families={families} />
      </Section>

      <Problems run={run} families={families} />

      <Reproductions
        rows={harness.reproductions.rows}
        table={harness.reproductions.table}
        families={families}
        repo={repoOf(d?.target ?? "wallet-backend")}
      />

      <Section title="How the run went" id="run">
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
      </Section>

      {harness.lint.map((l) => (
        <Section
          key={l.result}
          title={`Linter on ${l.target}`}
          id="lint"
          description={
            <>
              <TextLink to={docs.linter}>indexerlint</TextLink> checks Go indexer source for the three ingestion mistakes:
              trusting unvalidated fields (IDX001), recursion with no depth or size cap (IDX002), and participants taken
              from the envelope instead of the meta (IDX003). <Id>{l.command}</Id> at <Id>{short(l.version, 10)}</Id>:{" "}
              {l.findings.length} {l.findings.length === 1 ? "finding" : "findings"}, {l.suppressed} suppressed,{" "}
              {l.baselined} baselined. Result: <RepoLink path={l.result} />.
            </>
          }
        >
          {l.findings.length === 0 ? (
            <p className="text-sm text-muted-foreground">No findings.</p>
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
    </div>
  )
}
