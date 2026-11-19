import { use, useMemo, useState } from "react"
import { useSearchParams } from "react-router"
import { load, type Cell, type FamilyRow, type Observation, type Reproduction, type Sdk } from "@/lib/data"
import { checkResults, docs, label as termLabel } from "@/lib/terms"
import { bySdkOrder, sdkComponent, sdkLabel, sdkName } from "@/lib/sdks"
import { findingRows, sdkSummary } from "@/lib/metrics"
import { capPath, entryPath, findingsPath } from "@/lib/paths"
import { cn } from "@/lib/utils"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import {
  Command,
  ExtLink,
  GlossaryLink,
  Id,
  IssueRef,
  More,
  NextSteps,
  PageHeader,
  PageTerms,
  RepoLink,
  Section,
  Status,
  TextLink,
  issueLabel,
  type Tone,
} from "@/components/common"

const failing = new Set(checkResults.filter((c) => c.id !== "unsupported").map((c) => c.id))
const failingChecks = (cell: Cell) => cell.checks.filter((k) => failing.has(k.result))

// the family an entry belongs to: entry ids are the family id, a dot, and the entry's own name
function familyOf(id: string, families: FamilyRow[]): FamilyRow | undefined {
  return families.filter((f) => id.startsWith(`${f.id}.`)).toSorted((a, b) => b.id.length - a.id.length)[0]
}

function MatrixCell({ cell, onOpen, label }: { cell: Cell | undefined; onOpen: () => void; label: string }) {
  if (!cell) {
    return (
      <span className="block h-7 w-full rounded-sm bg-muted/70">
        <span className="sr-only">{label}: no failing check</span>
      </span>
    )
  }
  const known = cell.known_issue !== null
  const n = failingChecks(cell).length
  return (
    <button
      type="button"
      onClick={onOpen}
      aria-label={`${label}: ${cell.result}, ${n} failing ${n === 1 ? "check" : "checks"}${known ? ", known failure" : ", not a known failure"}. Open the checks.`}
      className={cn(
        "num flex h-7 w-full items-center justify-center rounded-sm border text-xs font-semibold focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none",
        known ? "hatch border-known/70 text-known" : "border-diverge bg-diverge/15 text-diverge",
      )}
    >
      {n}
      {known ? null : <span aria-hidden className="ml-0.5">!</span>}
    </button>
  )
}

function Legend() {
  return (
    <ul className="flex flex-col gap-1.5 text-sm text-muted-foreground sm:flex-row sm:flex-wrap sm:gap-x-5">
      <li className="flex items-center gap-2">
        <span aria-hidden className="hatch inline-block h-4 w-6 shrink-0 rounded-sm border border-known/70 text-known" />
        diverges, <GlossaryLink id="known-failure">known failure</GlossaryLink>
      </li>
      <li className="flex items-center gap-2">
        <span aria-hidden className="inline-block h-4 w-6 shrink-0 rounded-sm border border-diverge bg-diverge/15" />
        ! diverges, not a <GlossaryLink id="known-failure">known failure</GlossaryLink> (fails CI)
      </li>
      <li className="flex items-center gap-2">
        <span aria-hidden className="inline-block h-4 w-6 shrink-0 rounded-sm bg-muted/70" />
        no failing <GlossaryLink id="check">check</GlossaryLink>
      </li>
      <li>The number in a cell counts its failing checks. Select a cell to see them.</li>
    </ul>
  )
}

function count(x: number | undefined) {
  return x ? x : <span className="text-muted-foreground">0</span>
}

function Totals({ sdk }: { sdk: Sdk }) {
  const ops = [...new Set(sdk.versions.flatMap((v) => Object.keys(v.failing_checks_by_op)))].toSorted()
  const results = [...new Set(sdk.versions.flatMap((v) => Object.keys(v.failing_cells_by_result)))].toSorted()
  const sticky = "sticky left-0 z-10 bg-background whitespace-normal min-w-40"
  return (
    <Table>
      <caption className="sr-only">{sdk.package}: counts per version</caption>
      <TableHeader>
        <TableRow>
          <TableHead className={sticky}>Count</TableHead>
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
            <TableCell className={sticky}>
              Entries with the result <GlossaryLink id={`check-${r}`}>{termLabel(r)}</GlossaryLink> <span className="text-xs text-muted-foreground">(lower is better)</span>
            </TableCell>
            {sdk.versions.map((v) => (
              <TableCell key={v.version} className="text-right">
                {count(v.failing_cells_by_result[r])}
              </TableCell>
            ))}
          </TableRow>
        ))}
        <TableRow>
          <TableCell className={sticky}>
            Of those, <GlossaryLink id="known-failure">known failures</GlossaryLink>
          </TableCell>
          {sdk.versions.map((v) => (
            <TableCell key={v.version} className="text-right">
              {count(v.failing_cells_with_known_issue)}
            </TableCell>
          ))}
        </TableRow>
        <TableRow>
          <TableCell className={sticky}>Lines in the known-failures file for the version</TableCell>
          {sdk.versions.map((v) => (
            <TableCell key={v.version} className="text-right">
              {count(v.known_failure_lines)}
            </TableCell>
          ))}
        </TableRow>
        {ops.map((op) => (
          <TableRow key={op}>
            <TableCell className={sticky}>
              Failing <TextLink to={`/docs/sdk/testee-protocol#${op}`}><Id>{op}</Id></TextLink> checks <span className="text-xs text-muted-foreground">(lower is better)</span>
            </TableCell>
            {sdk.versions.map((v) => (
              <TableCell key={v.version} className="text-right">
                {count(v.failing_checks_by_op[op])}
              </TableCell>
            ))}
          </TableRow>
        ))}
        <TableRow>
          <TableCell className={sticky}>Unsupported checks in the diverging entries</TableCell>
          {sdk.versions.map((v) => (
            <TableCell key={v.version} className="text-right">
              {count(v.checks_by_result.unsupported)}
            </TableCell>
          ))}
        </TableRow>
        <TableRow>
          <TableCell className={sticky}>Newest protocol the version reads</TableCell>
          {sdk.versions.map((v) => (
            <TableCell key={v.version} className="text-right">
              {v.max_protocol ?? <span className="text-muted-foreground">any</span>}
            </TableCell>
          ))}
        </TableRow>
      </TableBody>
    </Table>
  )
}

function Matrix({ sdk, families, filter, op }: { sdk: Sdk; families: FamilyRow[]; filter: string; op: string }) {
  const [open, setOpen] = useState<{ entry: string; version: string; cell: Cell } | null>(null)
  const groups = useMemo(() => {
    const needle = filter.trim().toLowerCase()
    const rows = sdk.entries.filter(
      (e) =>
        (!needle || e.id.toLowerCase().includes(needle)) &&
        (op === "all" || Object.values(e.cells).some((c) => c.checks.some((k) => k.op === op && failing.has(k.result)))),
    )
    const by = new Map<string, { family: FamilyRow | undefined; rows: typeof rows }>()
    for (const e of rows) {
      const f = familyOf(e.id, families)
      const key = f?.id ?? ""
      const g = by.get(key) ?? { family: f, rows: [] }
      g.rows.push(e)
      by.set(key, g)
    }
    return [...by.values()]
  }, [sdk, families, filter, op])

  if (groups.length === 0) {
    return (
      <Empty className="border">
        <EmptyHeader>
          <EmptyTitle>No failing entry matches</EmptyTitle>
          <EmptyDescription>
            {sdk.entries.length === 0
              ? `The committed ${sdkLabel(sdk.sdk)} matrix has no failing cells.`
              : "Clear the filter to see every failing entry."}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  }

  const cols = sdk.versions.length + 1
  return (
    <>
      <div className="rounded-md border">
        <Table className="border-collapse">
          <caption className="sr-only">
            Failing cells of the {sdkLabel(sdk.sdk)} matrix: corpus entries by {sdk.package} version, grouped by shape
            family
          </caption>
          <thead>
            <tr className="border-b">
              <th scope="col" className="sticky left-0 z-10 min-w-36 bg-background px-2 py-2 text-left font-medium sm:min-w-64 sm:px-3">
                Entry
              </th>
              {sdk.versions.map((v) => (
                <th key={v.version} scope="col" className="min-w-14 px-1 py-2 text-center font-mono text-xs font-medium">
                  {v.version}
                  {v.current ? <span className="block font-sans font-normal text-muted-foreground">current</span> : null}
                </th>
              ))}
            </tr>
          </thead>
          {groups.map((g) => (
            <tbody key={g.family?.id ?? "other"}>
              <tr className="border-b bg-muted/40">
                <th scope="colgroup" colSpan={cols} className="px-2 py-1.5 text-left text-xs font-medium sm:px-3">
                  <span className="sticky left-2 inline-block sm:left-3">
                    {g.family ? (
                      <TextLink to={capPath(g.family.caps[0] ?? "", g.family.id)} className="font-mono">
                        {g.family.id}
                      </TextLink>
                    ) : (
                      "other entries"
                    )}
                  </span>
                </th>
              </tr>
              {g.rows.map((e) => {
                const name = g.family ? e.id.slice(g.family.id.length + 1) : e.id
                return (
                  <tr key={e.id} className="border-b last:border-0">
                    <th
                      scope="row"
                      className="sticky left-0 z-10 max-w-40 min-w-36 bg-background px-2 py-1 text-left font-normal sm:max-w-[26rem] sm:min-w-64 sm:px-3"
                    >
                      <TextLink to={entryPath(e.id)} aria-label={e.id} className="block font-mono text-xs leading-snug [overflow-wrap:anywhere]">
                        {name}
                      </TextLink>
                    </th>
                    {sdk.versions.map((v) => {
                      const cell = e.cells[v.key]
                      return (
                        <td key={v.version} className="min-w-13 px-1 py-1">
                          <MatrixCell
                            cell={cell}
                            label={`${e.id} at ${v.version}`}
                            onOpen={() => cell && setOpen({ entry: e.id, version: v.version, cell })}
                          />
                        </td>
                      )
                    })}
                  </tr>
                )
              })}
            </tbody>
          ))}
        </Table>
      </div>
      <Sheet open={open !== null} onOpenChange={(o) => !o && setOpen(null)}>
        <SheetContent className="w-full overflow-y-auto sm:max-w-xl">
          {open ? (
            <>
              <SheetHeader>
                <SheetTitle className="font-mono text-sm break-all">{open.entry}</SheetTitle>
                <SheetDescription>
                  {sdk.package} <Id>{open.version}</Id>:{" "}
                  <GlossaryLink id={`check-${open.cell.result}`}>{termLabel(open.cell.result)}</GlossaryLink>, {failingChecks(open.cell).length} failing of{" "}
                  {open.cell.checks.length} listed checks
                </SheetDescription>
              </SheetHeader>
              <div className="flex flex-col gap-4 px-4 pb-6 text-sm">
                {open.cell.known_issue ? (
                  <p>
                    A known failure, listed in{" "}
                    {sdk.known_failures_file ? <RepoLink path={sdk.known_failures_file} /> : "the known-failures file"}{" "}
                    with <IssueRef value={open.cell.known_issue} />.
                  </p>
                ) : (
                  <p className="text-diverge">Not listed in the known-failures file.</p>
                )}
                <ol className="flex flex-col divide-y border-y">
                  {open.cell.checks.map((k) => (
                    <li key={k.check} className="flex flex-col gap-1 py-2">
                      <span className="flex flex-wrap items-baseline justify-between gap-2">
                        <Id>{k.check}</Id>
                        <span className={cn("text-xs", failing.has(k.result) ? "text-diverge" : "text-muted-foreground")}>
                          <GlossaryLink id={`check-${k.result}`} className="decoration-current/40">
                            {termLabel(k.result)}
                          </GlossaryLink>
                        </span>
                      </span>
                      {k.detail ? <span className="break-words text-muted-foreground">{k.detail}</span> : null}
                    </li>
                  ))}
                </ol>
                <span className="font-medium">Run this entry again</span>
                <Command>{`shapekit check sdk --sdk ${sdk.sdk} --version ${open.version} --entry ${open.entry}`}</Command>
                <TextLink to={entryPath(open.entry)}>The entry page</TextLink>
                <TextLink to={findingsPath({ q: open.entry })}>Findings on this entry</TextLink>
              </div>
            </>
          ) : null}
        </SheetContent>
      </Sheet>
    </>
  )
}

function observation(o: Observation, side: "fail" | "pass", r: Reproduction): { tone: Tone; text: string } {
  if (o === "not-in-matrix")
    return {
      tone: "idle",
      text: r.probes.length > 0 ? "runs as a probe, not in the matrix" : "version not in the committed matrix",
    }
  if (side === "fail") {
    return o === "named-checks-diverge"
      ? { tone: "pass", text: "reproduced: the named checks fail" }
      : { tone: "diverge", text: o === "diverges-elsewhere" ? "not reproduced: fails on other checks" : "not reproduced: passes" }
  }
  return o === "no-counted-check-fails"
    ? { tone: "pass", text: "fixed: no counted check fails" }
    : { tone: "diverge", text: o === "named-checks-diverge" ? "still fails" : "fails on other checks" }
}

function Reproductions() {
  const conformance = use(load("conformance"))
  return (
    <Section
      title="Upstream SDK bugs that shapekit reproduces"
      id="reproductions"
      description={
        <>
          Each row of <RepoLink path="conformance/reproductions.toml" /> names a bug that somebody reported upstream, the
          corpus entries that show it, and SDK versions before and after its fix. The list shows the results of the
          committed matrices for those entries and versions. <Id>shapekit conformance reproduce</Id> runs the same rows, probes included, in the
          nightly workflow.
        </>
      }
    >
      <Command>shapekit conformance reproduce</Command>
      <ul className="flex flex-col divide-y border-y">
        {conformance.reproductions.map((r) => (
          <li key={r.id} className="grid gap-x-6 gap-y-2 py-3 text-sm md:grid-cols-[minmax(0,1.5fr)_minmax(0,1fr)_minmax(0,1fr)]">
            <span className="flex flex-col gap-1">
              <ExtLink href={r.link}>{issueLabel(r.link)}</ExtLink>
              <span className="text-muted-foreground">{r.sighting.replace(/^[^\s:]+#\d+:\s*/, "")}</span>
              <span className="text-xs text-muted-foreground">
                {r.entries.length > 0 ? `${r.entries.length} ${r.entries.length === 1 ? "entry" : "entries"}` : null}
                {r.probes.length > 0 ? `${r.entries.length > 0 ? ", " : ""}probes: ${r.probes.join(", ")}` : null}
                {r.ops.length > 0 ? `; ops ${r.ops.join(", ")}` : null}
              </span>
            </span>
            {(["fail", "pass"] as const).map((side) => (
              <div key={side} className="flex flex-col gap-1">
                <span className="text-xs font-medium text-muted-foreground">{side === "fail" ? "Before the fix" : "After the fix"}</span>
                <ul className="flex flex-col gap-1">
                  {Object.entries(r[side]).map(([v, o]) => {
                    const ob = observation(o, side, r)
                    return (
                      <li key={v} className="flex flex-col">
                        <span className="font-mono text-xs">
                          {sdkName(r.sdk)} {v}
                        </span>
                        <Status tone={ob.tone}>{ob.text}</Status>
                      </li>
                    )
                  })}
                </ul>
              </div>
            ))}
          </li>
        ))}
      </ul>
    </Section>
  )
}

// the summary of every sdk: the one home of the per-sdk numbers the start page also shows
function Summary({ sdks, findingsBy, select }: { sdks: Sdk[]; findingsBy: Map<string, number>; select: (sdk: string) => void }) {
  return (
    <Table>
      <caption className="sr-only">Each SDK at its current version: diverging entries, entries that fail CI, and reported findings</caption>
      <TableHeader>
        <TableRow>
          <TableHead>SDK</TableHead>
          <TableHead className="text-right">
            <GlossaryLink id="diverging-entry">Diverging entries</GlossaryLink>
            <span className="block text-xs font-normal text-muted-foreground">current version, lower is better</span>
          </TableHead>
          <TableHead className="hidden text-right sm:table-cell">
            Not a <GlossaryLink id="known-failure">known failure</GlossaryLink>
            <span className="block text-xs font-normal text-muted-foreground">fails CI, lower is better</span>
          </TableHead>
          <TableHead className="text-right">
            <GlossaryLink id="reported-finding">Findings</GlossaryLink>
            <span className="block text-xs font-normal text-muted-foreground">lower is better</span>
          </TableHead>
        </TableRow>
      </TableHeader>
      <TableBody className="num">
        {sdks.map((sdk) => {
          const x = sdkSummary(sdk)
          const nf = findingsBy.get(sdk.sdk) ?? 0
          return (
            <TableRow key={sdk.sdk}>
              <TableCell className="font-sans whitespace-normal">
                <button
                  type="button"
                  onClick={() => select(sdk.sdk)}
                  className="text-left font-medium text-primary underline decoration-primary/40 hover:decoration-primary focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
                >
                  {sdkName(sdk.sdk)}
                </button>
                <span className="block text-xs text-muted-foreground">
                  <Id>{sdk.package}</Id> <Id>{x.version}</Id>
                </span>
              </TableCell>
              <TableCell className="text-right">
                {x.diverging > 0 ? <span className="font-semibold">{x.diverging}</span> : <span className="text-muted-foreground">0</span>}
              </TableCell>
              <TableCell className="hidden text-right sm:table-cell">
                {x.notKnown > 0 ? <Status tone="diverge">{x.notKnown}</Status> : <span className="text-muted-foreground">0</span>}
              </TableCell>
              <TableCell className="text-right">
                {nf > 0 ? (
                  <TextLink to={findingsPath({ component: sdkComponent(sdk.sdk) })} aria-label={`${nf} reported findings in ${sdk.package}`}>
                    {nf}
                  </TextLink>
                ) : (
                  <span className="text-muted-foreground">0</span>
                )}
              </TableCell>
            </TableRow>
          )
        })}
      </TableBody>
    </Table>
  )
}

export function SdkPage() {
  const loaded = use(load("conformance"))
  const conformance = useMemo(() => ({ ...loaded, sdks: bySdkOrder(loaded.sdks, (s) => s.sdk) }), [loaded])
  const coverage = use(load("coverage"))
  const findings = use(load("findings"))
  const [params, setParams] = useSearchParams()
  const first = conformance.sdks.find((s) => s.sdk === "js")?.sdk ?? conformance.sdks[0]?.sdk ?? ""
  const entryParam = params.get("entry") ?? ""
  // a link from an entry opens the first sdk that fails on it
  const entrySdk = entryParam ? conformance.sdks.find((s) => s.entries.some((e) => e.id === entryParam))?.sdk : undefined
  const tab = params.get("sdk") ?? entrySdk ?? first
  const s = conformance.sdks.find((x) => x.sdk === tab) ?? conformance.sdks[0]
  const op = params.get("op") ?? "all"
  const set = (k: string, v: string, dflt: string) => {
    const next = new URLSearchParams(params)
    if (v === dflt) next.delete(k)
    else next.set(k, v)
    setParams(next, { replace: true, preventScrollReset: true })
  }
  // reported findings per sdk, counted from the same rows as the findings page
  const findingsBy = useMemo(() => {
    const rows = findingRows(findings).filter((f) => f.reported)
    return new Map(conformance.sdks.map((x) => [x.sdk, rows.filter((f) => f.component === sdkComponent(x.sdk)).length]))
  }, [findings, conformance])
  const select = (sdk: string) => {
    const next = new URLSearchParams(params)
    next.set("sdk", sdk)
    next.delete("entry")
    next.delete("op")
    setParams(next, { replace: true, preventScrollReset: true })
    requestAnimationFrame(() => document.getElementById("detail")?.scrollIntoView())
  }

  return (
    <div className="flex flex-col gap-6">
      <PageHeader title="SDK results">
        <p>
          The conformance runner gives every corpus entry to each SDK version through a{" "}
          <GlossaryLink id="testee">testee</GlossaryLink>. It compares each answer with the answer of the network. An
          entry diverges when one <GlossaryLink id="check">check</GlossaryLink> of the SDK gives another answer.{" "}
          <TextLink to={docs.testee}>The testee protocol</TextLink>.
        </p>
      </PageHeader>

      <NextSteps>
        <li>
          <TextLink to={docs.sdkCi}>Add the corpus check to your SDK CI</TextLink>. CI fails on a divergence that is not a
          known failure.
        </li>
        <li>Select a diverging cell in the matrix to see the failing checks and the command that runs the entry again.</li>
        <li>Select the findings count of your SDK to see its defects and their upstream issues.</li>
      </NextSteps>

      <Section
        title="Summary"
        id="summary"
        description="Select an SDK to see its matrix. The matrix columns count the runs of the first record. The second record has its own runs of the newer SDKs, and their defects show in the findings column."
      >
        <Summary sdks={conformance.sdks} findingsBy={findingsBy} select={select} />
      </Section>

      <PageTerms ids={["testee", "check", "diverging-entry", "cell", "known-failure", "sdk-terms"]} />

      {s ? (
        <Section title={`${sdkLabel(s.sdk)} in detail`} id="detail">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
            <label htmlFor="sdk-select" className="text-sm font-medium">
              SDK
            </label>
            <Select value={s.sdk} onValueChange={(v) => select(v)}>
              <SelectTrigger id="sdk-select" className="w-full sm:w-64">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {conformance.sdks.map((x) => (
                    <SelectItem key={x.sdk} value={x.sdk}>
                      {sdkName(x.sdk)}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </div>
          <p className="text-sm text-muted-foreground">
            <Id className="text-foreground">{s.package}</Id>, {s.versions.length} {s.versions.length === 1 ? "version" : "versions"}, current{" "}
            <Id>{s.current}</Id>. Issues go to <ExtLink href={`https://github.com/${s.issues_repo}`}>{s.issues_repo}</ExtLink>. Matrix:{" "}
            {s.matrix ? <RepoLink path={s.matrix} /> : "none committed"}
            {s.known_failures_file ? (
              <>
                . Known failures: <RepoLink path={s.known_failures_file} />
              </>
            ) : null}
            .
          </p>
          {s.known_failure_problems && s.known_failure_problems.length > 0 ? (
            <ul className="text-sm text-diverge">
              {s.known_failure_problems.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          ) : null}
          <h3 className="pt-2 font-medium">Diverging entries by version</h3>
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
            <label className="sr-only" htmlFor={`entry-filter-${s.sdk}`}>
              Filter the entries
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
          <Matrix key={s.sdk} sdk={s} families={coverage.families} filter={entryParam} op={op} />
          <More label="Show the counts for each version">
            <div className="overflow-x-auto">
              <Totals sdk={s} />
            </div>
          </More>
        </Section>
      ) : null}
      <Reproductions />
    </div>
  )
}
