import { use, useMemo, useState } from "react"
import { Link, useSearchParams } from "react-router"
import { load, type Cell, type Observation, type Sdk } from "@/lib/data"
import { cn } from "@/lib/utils"
import { Input } from "@/components/ui/input"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { ExtLink, Id, PageHeader, RepoLink, Section, Status, issueLabel, type Tone } from "@/components/common"

const sdkName: Record<string, string> = { js: "JavaScript", go: "Go", rust: "Rust" }

function MatrixCell({ cell, onOpen, label }: { cell: Cell | undefined; onOpen: () => void; label: string }) {
  if (!cell) {
    return (
      <span className="block h-7 w-full rounded-sm bg-muted/60">
        <span className="sr-only">{label}: no failing check</span>
      </span>
    )
  }
  const known = cell.known_issue !== null
  return (
    <button
      type="button"
      onClick={onOpen}
      aria-label={`${label}: ${cell.result}, ${cell.checks.length} failing checks${known ? ", known failure" : ""}`}
      className={cn(
        "num flex h-7 w-full items-center justify-center rounded-sm border text-xs font-medium focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none",
        known ? "hatch border-known/70 text-known" : "border-diverge bg-diverge/15 text-diverge",
      )}
    >
      {cell.checks.length}
    </button>
  )
}

function Legend() {
  return (
    <ul className="flex flex-wrap gap-x-5 gap-y-2 text-sm text-muted-foreground">
      <li className="flex items-center gap-2">
        <span aria-hidden className="hatch inline-block h-4 w-6 rounded-sm border border-known/70 text-known" />
        diverges, listed in the known-failures file with an issue
      </li>
      <li className="flex items-center gap-2">
        <span aria-hidden className="inline-block h-4 w-6 rounded-sm border border-diverge bg-diverge/15" />
        diverges, not a known failure
      </li>
      <li className="flex items-center gap-2">
        <span aria-hidden className="inline-block h-4 w-6 rounded-sm bg-muted/60" />
        no failing check: passed, matched the reference, or the version predates the entry
      </li>
      <li>The number in a cell is its failing checks.</li>
    </ul>
  )
}

function n(x: number | undefined) {
  return x ? x : <span className="text-muted-foreground">0</span>
}

function Totals({ sdk }: { sdk: Sdk }) {
  const ops = [...new Set(sdk.versions.flatMap((v) => Object.keys(v.failing_checks_by_op)))].toSorted()
  const results = [...new Set(sdk.versions.flatMap((v) => Object.keys(v.failing_cells_by_result)))].toSorted()
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="sticky left-0 z-10 bg-background">Count</TableHead>
          {sdk.versions.map((v) => (
            <TableHead key={v.version} className="text-right font-mono">
              {v.version}
              {v.current ? <span className="block font-sans text-xs font-normal">current</span> : null}
            </TableHead>
          ))}
        </TableRow>
      </TableHeader>
      <TableBody className="num">
        {results.map((r) => (
          <TableRow key={r}>
            <TableCell className="sticky left-0 z-10 bg-background">Entries: {r}</TableCell>
            {sdk.versions.map((v) => (
              <TableCell key={v.version} className="text-right">
                {n(v.failing_cells_by_result[r])}
              </TableCell>
            ))}
          </TableRow>
        ))}
        <TableRow>
          <TableCell className="sticky left-0 z-10 bg-background">Entries with a known issue</TableCell>
          {sdk.versions.map((v) => (
            <TableCell key={v.version} className="text-right">
              {n(v.failing_cells_with_known_issue)}
            </TableCell>
          ))}
        </TableRow>
        <TableRow>
          <TableCell className="sticky left-0 z-10 bg-background">Known-failure lines that apply</TableCell>
          {sdk.versions.map((v) => (
            <TableCell key={v.version} className="text-right">
              {n(v.known_failure_lines)}
            </TableCell>
          ))}
        </TableRow>
        {ops.map((op) => (
          <TableRow key={op}>
            <TableCell className="sticky left-0 z-10 bg-background">
              Failing checks: <Id>{op}</Id>
            </TableCell>
            {sdk.versions.map((v) => (
              <TableCell key={v.version} className="text-right">
                {n(v.failing_checks_by_op[op])}
              </TableCell>
            ))}
          </TableRow>
        ))}
        <TableRow>
          <TableCell className="sticky left-0 z-10 bg-background">Newest protocol known</TableCell>
          {sdk.versions.map((v) => (
            <TableCell key={v.version} className="text-right">
              {v.max_protocol ?? ""}
            </TableCell>
          ))}
        </TableRow>
      </TableBody>
    </Table>
  )
}

function Matrix({ sdk, filter, op }: { sdk: Sdk; filter: string; op: string }) {
  const [open, setOpen] = useState<{ entry: string; version: string; cell: Cell } | null>(null)
  const rows = useMemo(() => {
    const needle = filter.trim().toLowerCase()
    return sdk.entries.filter(
      (e) =>
        (!needle || e.id.toLowerCase().includes(needle)) &&
        (op === "all" || Object.values(e.cells).some((c) => c.checks.some((k) => k.op === op))),
    )
  }, [sdk, filter, op])

  if (rows.length === 0) {
    return (
      <Empty className="border">
        <EmptyHeader>
          <EmptyTitle>No failing entry matches</EmptyTitle>
          <EmptyDescription>
            {sdk.entries.length === 0
              ? `The committed ${sdkName[sdk.sdk] ?? sdk.sdk} matrix has no failing cells.`
              : "Clear the filter to see every failing entry."}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  }

  return (
    <>
      <div className="relative overflow-x-auto rounded-md border">
        <table className="w-full border-collapse text-sm">
          <caption className="sr-only">
            Failing cells of the {sdkName[sdk.sdk]} matrix: corpus entries by SDK version
          </caption>
          <thead>
            <tr className="border-b">
              <th scope="col" className="sticky left-0 z-10 bg-background px-3 py-2 text-left font-medium">
                Entry
              </th>
              {sdk.versions.map((v) => (
                <th key={v.version} scope="col" className="min-w-16 px-1 py-2 text-center font-mono text-xs font-medium">
                  {v.version}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((e) => (
              <tr key={e.id} className="border-b last:border-0">
                <th
                  scope="row"
                  className="sticky left-0 z-10 max-w-[11rem] bg-background px-3 py-1 text-left font-normal sm:max-w-[26rem]"
                >
                  <span className="block truncate font-mono text-xs" title={e.id}>
                    {e.id}
                  </span>
                </th>
                {sdk.versions.map((v) => {
                  const cell = e.cells[v.key]
                  return (
                    <td key={v.version} className="px-1 py-1">
                      <MatrixCell
                        cell={cell}
                        label={`${e.id} at ${v.version}`}
                        onOpen={() => cell && setOpen({ entry: e.id, version: v.version, cell })}
                      />
                    </td>
                  )
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <Sheet open={open !== null} onOpenChange={(o) => !o && setOpen(null)}>
        <SheetContent className="w-full overflow-y-auto sm:max-w-xl">
          {open ? (
            <>
              <SheetHeader>
                <SheetTitle className="font-mono text-sm break-all">{open.entry}</SheetTitle>
                <SheetDescription>
                  {sdk.package} <Id>{open.version}</Id>: {open.cell.result}, {open.cell.checks.length} failing checks
                </SheetDescription>
              </SheetHeader>
              <div className="flex flex-col gap-4 px-4 pb-6 text-sm">
                {open.cell.known_issue ? (
                  <p>
                    Known failure. Issue draft: <RepoLink path={open.cell.known_issue} />
                  </p>
                ) : (
                  <p className="text-diverge">Not listed in the known-failures file.</p>
                )}
                <ol className="flex flex-col divide-y border-y">
                  {open.cell.checks.map((k) => (
                    <li key={k.check} className="flex flex-col gap-1 py-2">
                      <span className="flex flex-wrap items-baseline justify-between gap-2">
                        <Id>{k.check}</Id>
                        <span className="text-xs text-muted-foreground">{k.result}</span>
                      </span>
                      {k.detail ? <span className="text-muted-foreground break-words">{k.detail}</span> : null}
                    </li>
                  ))}
                </ol>
                <Link
                  to={`/dashboard/findings?q=${encodeURIComponent(open.entry)}`}
                  className="text-primary underline decoration-primary/40"
                >
                  Findings on this entry
                </Link>
              </div>
            </>
          ) : null}
        </SheetContent>
      </Sheet>
    </>
  )
}

function observation(o: Observation, side: "fail" | "pass"): { tone: Tone; text: string } {
  if (o === "not-in-matrix") return { tone: "idle", text: "not in the committed matrix" }
  if (side === "fail") {
    return o === "named-checks-diverge"
      ? { tone: "pass", text: "fails as the sighting says" }
      : { tone: "diverge", text: o === "diverges-elsewhere" ? "diverges, not on the named checks" : "does not fail" }
  }
  return o === "no-counted-check-fails"
    ? { tone: "pass", text: "passes" }
    : { tone: "diverge", text: o === "named-checks-diverge" ? "still fails" : "diverges on other checks" }
}

function Reproductions() {
  const conformance = use(load("conformance"))
  return (
    <Section
      title="Reproductions of upstream sightings"
      id="reproductions"
      description={
        <>
          Each row of <RepoLink path="conformance/reproductions.toml" /> names a reported SDK bug, the entries
          that carry it, and the versions before and after the fix. Shown here is what the committed matrix holds
          for those entries; <Id>shapekit conformance reproduce</Id> is the check that runs them.
        </>
      }
    >
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Sighting</TableHead>
            <TableHead>Before the fix</TableHead>
            <TableHead>After the fix</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {conformance.reproductions.map((r) => (
            <TableRow key={r.id}>
              <TableCell className="max-w-md align-top whitespace-normal">
                <span className="flex flex-col gap-1">
                  <ExtLink href={r.link}>{issueLabel(r.link)}</ExtLink>
                  <span className="text-muted-foreground">{r.sighting.replace(/^[^:]+:\s*/, "")}</span>
                  <span className="text-xs text-muted-foreground">
                    {r.entries.length > 0 ? `${r.entries.length} ${r.entries.length === 1 ? "entry" : "entries"}` : null}
                    {r.probes.length > 0 ? `${r.entries.length > 0 ? ", " : ""}probes: ${r.probes.join(", ")}` : null}
                    {r.ops.length > 0 ? `; ops ${r.ops.join(", ")}` : null}
                  </span>
                </span>
              </TableCell>
              {(["fail", "pass"] as const).map((side) => (
                <TableCell key={side} className="align-top">
                  <ul className="flex flex-col gap-1">
                    {Object.entries(r[side]).map(([v, o]) => {
                      const ob = observation(o, side)
                      return (
                        <li key={v} className="flex flex-col">
                          <span className="font-mono text-xs">
                            {r.sdk} {v}
                          </span>
                          <Status tone={ob.tone}>{ob.text}</Status>
                        </li>
                      )
                    })}
                  </ul>
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </Section>
  )
}

export function SdkPage() {
  const conformance = use(load("conformance"))
  const [params, setParams] = useSearchParams()
  const first = conformance.sdks.find((s) => s.sdk === "js")?.sdk ?? conformance.sdks[0]?.sdk ?? ""
  const entryParam = params.get("entry") ?? ""
  // a link from an entry opens the first sdk that fails on it
  const entrySdk = entryParam
    ? conformance.sdks.find((s) => s.entries.some((e) => e.id === entryParam))?.sdk
    : undefined
  const tab = params.get("sdk") ?? entrySdk ?? first
  const op = params.get("op") ?? "all"
  const set = (k: string, v: string, dflt: string) => {
    const next = new URLSearchParams(params)
    if (v === dflt) next.delete(k)
    else next.set(k, v)
    setParams(next, { replace: true })
  }

  return (
    <div className="flex flex-col">
      <PageHeader title="SDK matrix">
        <p>
          Corpus entries run through each SDK version by the conformance runner. A check diverges from the network
          when the SDK accepts what stellar-core refuses, refuses what it emits, or reads other values. The
          committed matrices keep failing cells only, so an entry missing here passed, or was unsupported by
          that version, everywhere.
        </p>
      </PageHeader>
      <Tabs value={tab} onValueChange={(v) => set("sdk", v, first)}>
        <TabsList>
          {conformance.sdks.map((s) => (
            <TabsTrigger key={s.sdk} value={s.sdk}>
              {sdkName[s.sdk] ?? s.sdk}
            </TabsTrigger>
          ))}
        </TabsList>
        {conformance.sdks.map((s) => (
          <TabsContent key={s.sdk} value={s.sdk} className="flex flex-col">
            <p className="pt-4 text-sm text-muted-foreground">
              <Id className="text-foreground">{s.package}</Id>, current <Id>{s.current}</Id>, issues go to{" "}
              <ExtLink href={`https://github.com/${s.issues_repo}`}>{s.issues_repo}</ExtLink>. Matrix:{" "}
              {s.matrix ? <RepoLink path={s.matrix} /> : "none committed"}
              {s.known_failures_file ? (
                <>
                  . Known failures: <RepoLink path={s.known_failures_file} />
                </>
              ) : null}
              .
            </p>
            {s.known_failure_problems && s.known_failure_problems.length > 0 ? (
              <ul className="pt-2 text-sm text-diverge">
                {s.known_failure_problems.map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            ) : null}
            <Section title="Failing entries by version" id={`matrix-${s.sdk}`}>
              <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
                <label className="sr-only" htmlFor={`entry-filter-${s.sdk}`}>
                  Filter entries
                </label>
                <Input
                  id={`entry-filter-${s.sdk}`}
                  type="search"
                  placeholder="Filter by entry id"
                  value={entryParam}
                  onChange={(e) => set("entry", e.target.value, "")}
                  className="sm:max-w-sm"
                />
                <Select value={op} onValueChange={(v) => set("op", v, "all")}>
                  <SelectTrigger className="w-full sm:w-56" aria-label="Filter by operation">
                    <SelectValue placeholder="Any operation" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectItem value="all">Any operation</SelectItem>
                      {[...new Set(s.versions.flatMap((v) => Object.keys(v.failing_checks_by_op)))].toSorted().map((o) => (
                        <SelectItem key={o} value={o}>
                          {o}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>
              <Legend />
              <Matrix sdk={s} filter={entryParam} op={op} />
            </Section>
            <Section title="Totals per version" id={`totals-${s.sdk}`}>
              <Totals sdk={s} />
            </Section>
          </TabsContent>
        ))}
      </Tabs>
      <Reproductions />
    </div>
  )
}
