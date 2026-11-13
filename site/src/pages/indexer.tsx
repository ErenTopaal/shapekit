import { use, useMemo } from "react"
import { Link, useSearchParams } from "react-router"
import { load, sum, type Counts, type HarnessRun } from "@/lib/data"
import { Input } from "@/components/ui/input"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Id, PageHeader, RepoLink, Section, Status, bytes, fmt, short, verdictTone } from "@/components/common"

const order = ["pass", "mishandled", "dropped", "not-evaluated", "out-of-scope"]

// one bar per family and network: the share of each verdict, widths from the counts
function VerdictBar({ counts }: { counts: Counts }) {
  const total = sum(counts)
  const parts = order.filter((v) => counts[v])
  const label = parts.map((v) => `${counts[v]} ${v}`).join(", ")
  return (
    <span className="flex h-2.5 w-full min-w-24 overflow-hidden rounded-full bg-muted" aria-hidden title={label}>
      {parts.map((v) => (
        <span
          key={v}
          className={verdictTone(v) === "pass" ? "bg-pass" : verdictTone(v) === "idle" ? "bg-idle/50" : "bg-diverge"}
          style={{ width: `${((counts[v] ?? 0) / total) * 100}%` }}
        />
      ))}
    </span>
  )
}

function CountsText({ counts }: { counts: Counts }) {
  return (
    <span className="num text-xs text-muted-foreground">
      {order
        .filter((v) => counts[v])
        .map((v) => `${counts[v]} ${v}`)
        .join(", ")}
    </span>
  )
}

function Families({ run }: { run: HarnessRun }) {
  const networks = [...new Set(run.families.map((f) => f.network))].toSorted()
  const fams = [...new Set(run.families.map((f) => f.family))].toSorted()
  const get = (fam: string, net: string) => run.families.find((f) => f.family === fam && f.network === net)?.verdicts
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Family</TableHead>
          {networks.map((n) => (
            <TableHead key={n} className="min-w-48">
              {n}
            </TableHead>
          ))}
        </TableRow>
      </TableHeader>
      <TableBody>
        {fams.map((fam) => (
          <TableRow key={fam}>
            <TableCell className="align-top">
              <Link
                to={`/dashboard/indexer?q=${encodeURIComponent(fam)}&network=both`}
                className="font-mono text-xs text-primary underline decoration-primary/40"
              >
                {fam}
              </Link>
            </TableCell>
            {networks.map((n) => {
              const c = get(fam, n)
              return (
                <TableCell key={n} className="align-top">
                  {c ? (
                    <span className="flex flex-col gap-1">
                      <VerdictBar counts={c} />
                      <CountsText counts={c} />
                    </span>
                  ) : (
                    <span className="text-xs text-muted-foreground">not run</span>
                  )}
                </TableCell>
              )
            })}
          </TableRow>
        ))}
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
    setParams(next, { replace: true })
  }
  const verdicts = [...new Set(run.problems.map((p) => p.verdict))].toSorted()
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
      title="Problems"
      id="problems"
      description="Every replayed entry whose verdict is not pass or out of scope, with each fact the indexer stored differently from what the ledger meta shows."
    >
      <div className="flex flex-col gap-3 lg:flex-row lg:items-center">
        <label className="sr-only" htmlFor="problem-filter">
          Filter problems
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
          {verdicts.map((v) => (
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
        {rows.length} of {run.problems.length} entry replays shown.
      </p>
      {rows.length === 0 ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyTitle>No problem matches</EmptyTitle>
            <EmptyDescription>Clear the filter or pick another verdict or network.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <ol className="flex flex-col divide-y border-y">
          {rows.map((p) => (
            <li key={`${p.entry}/${p.network}`} className="flex flex-col gap-2 py-3">
              <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                <Id className="text-sm">{p.entry}</Id>
                <Status tone={verdictTone(p.verdict)}>{p.verdict}</Status>
                {p.reason ? <span className="text-xs text-muted-foreground">{p.reason}</span> : null}
                <span className="text-xs text-muted-foreground">
                  {p.network}, chain <Id>{p.chain}</Id>, ledger {p.ledger}
                </span>
              </div>
              <p className="text-sm text-muted-foreground [overflow-wrap:anywhere]">{p.detail}</p>
              {p.problems.length > 0 ? (
                <ul className="flex flex-col gap-1 text-sm">
                  {p.problems.map((x, i) => (
                    <li key={i} className="grid grid-cols-[minmax(0,1fr)] gap-x-3 sm:grid-cols-[7rem_10rem_minmax(0,1fr)]">
                      <span className="font-medium">{x.kind}</span>
                      <Id className="text-muted-foreground">{x.fact}</Id>
                      <span className="text-muted-foreground [overflow-wrap:anywhere]">{x.detail}</span>
                    </li>
                  ))}
                </ul>
              ) : null}
            </li>
          ))}
        </ol>
      )}
    </Section>
  )
}

export function IndexerPage() {
  const harness = use(load("harness"))
  const run = harness.runs[0]
  if (!run) {
    return (
      <Empty>
        <EmptyHeader>
          <EmptyTitle>No harness report is committed</EmptyTitle>
          <EmptyDescription>findings/triage.toml names no harness input.</EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  }
  const d = run.describe
  const networks = Object.keys(run.by_network).toSorted()
  return (
    <div className="flex flex-col">
      <PageHeader title={`Indexer run: ${d?.target ?? "target"}`}>
        <p>
          The replay harness fed the corpus chains to {d?.target ?? "the target"} at commit{" "}
          <Id>{short(d?.target_version ?? "", 10)}</Id> through {d?.adapter ?? "its adapter"} {d?.adapter_version},
          over {d?.default_delivery ?? "its default delivery"}, and compared what it stored with what the ledger
          meta shows. Report: <RepoLink path={run.report} />.
        </p>
      </PageHeader>

      {!run.ok ? (
        <Alert variant="destructive">
          <AlertTitle>The run itself did not complete cleanly</AlertTitle>
          <AlertDescription>
            A chain failed to run, the memory checks did not hold, or rounds disagreed. See the report.
          </AlertDescription>
        </Alert>
      ) : null}

      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Network</TableHead>
            {order.map((v) => (
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
              {order.map((v) => (
                <TableCell key={v} className="text-right">
                  {run.by_network[n]?.[v] ?? 0}
                </TableCell>
              ))}
              <TableCell className="text-right">{sum(run.by_network[n])}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      <p className="pt-3 text-sm text-muted-foreground">
        {fmt.format(run.totals.entries)} entry replays over {run.chains} chain runs.{" "}
        {run.totals.without_chain} selected entries have no chain and {run.totals.without_oracle} no oracle file, so
        they were not replayed. Reasons for mishandled:{" "}
        {Object.entries(run.totals.by_reason)
          .map(([k, v]) => `${k} ${v}`)
          .join(", ") || "none"}
        .
      </p>

      <Section title="Run" id="run">
        <dl className="grid gap-x-8 gap-y-3 text-sm sm:grid-cols-2">
          <div>
            <dt className="font-medium">Rounds</dt>
            <dd className="num text-muted-foreground">
              {run.rounds.length}, verdicts{" "}
              {run.rounds.every((r) => r.same_verdicts_as_first) ? "identical in every round" : "differ between rounds"}
              ; {run.rounds.map((r) => `${Math.round(r.seconds / 60)} min`).join(" and ")}
            </dd>
          </div>
          <div>
            <dt className="font-medium">Harness memory</dt>
            <dd className="num text-muted-foreground">
              peak {bytes(Math.max(...run.rounds.map((r) => r.peak_rss_bytes)))},{" "}
              {run.memory.flat ? "flat across rounds" : "grew across rounds"}
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
            <dt className="font-medium">Ingestible protocols</dt>
            <dd className="num text-muted-foreground">
              {d ? `${d.protocols.min} to ${d.protocols.max}` : "not reported"}
            </dd>
          </div>
          <div className="sm:col-span-2">
            <dt className="font-medium">Components</dt>
            <dd className="flex flex-col gap-0.5 text-muted-foreground">
              {Object.entries(run.components).map(([k, v]) => (
                <span key={k} className="break-all">
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

      <Section
        title="Verdicts per family"
        id="families"
        description="Green is pass, red is mishandled or dropped, grey is out of scope (the adapter does not claim the fact)."
      >
        <Families run={run} />
      </Section>

      <Problems run={run} />

      {harness.lint.map((l) => (
        <Section
          key={l.result}
          title={`Linter on ${l.target}`}
          id="lint"
          description={
            <>
              <Id>{l.command}</Id> at <Id>{short(l.version, 10)}</Id>: {l.findings.length} findings, {l.suppressed}{" "}
              suppressed, {l.baselined} baselined. Result: <RepoLink path={l.result} />.
            </>
          }
        >
          <ul className="flex flex-col divide-y border-y text-sm">
            {l.findings.map((f) => (
              <li key={`${f.file}:${f.line}:${f.column}`} className="flex flex-col gap-1 py-3">
                <span className="flex flex-wrap gap-x-3">
                  <Id className="font-medium">{f.rule}</Id>
                  <Id className="text-muted-foreground">
                    {f.file}:{f.line}
                  </Id>
                </span>
                <span className="text-muted-foreground">{f.message}</span>
              </li>
            ))}
          </ul>
        </Section>
      ))}
    </div>
  )
}
