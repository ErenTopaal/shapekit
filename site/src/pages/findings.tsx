import { use, useEffect, useMemo, useState } from "react"
import { useLocation, useSearchParams } from "react-router"
import { ChevronDownIcon } from "lucide-react"
import { load, sum } from "@/lib/data"
import { findingRows, reportedFindings, type FindingRow } from "@/lib/metrics"
import { docs, label, severities, severityRank, sources, statuses } from "@/lib/terms"
import { entryPath, findingPath, findingsPath, capPath } from "@/lib/paths"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import {
  Command,
  ExtLink,
  GlossaryLink,
  IssueRef,
  Metric,
  More,
  NextSteps,
  PageHeader,
  PageTerms,
  RepoLink,
  Section,
  Status,
  TextLink,
  fmt,
  issueLabel,
  type Tone,
} from "@/components/common"

export const statusTone: Record<string, Tone> = {
  filed: "pass",
  "duplicate-of": "pass",
  "private-report": "pass",
  draft: "known",
  "wont-fix": "idle",
}

// the severities of some findings as one phrase, worst first: "3 data loss, 1 robustness"
export function severityPhrase(counts: Record<string, number>): string {
  return severities
    .filter((s) => (counts[s.id] ?? 0) > 0)
    .map((s) => `${counts[s.id]} ${s.label}`)
    .join(", ")
}

// answers recorded on implementation-defined entries are evidence, not failures
function seen(f: FindingRow) {
  const answers = f.occurrences_by_source.answer ?? 0
  const failures = Object.entries(f.occurrences_by_source).filter(([k]) => k !== "answer")
  const parts = []
  if (failures.length > 0) {
    parts.push(`${f.occurrences - answers} failures (${failures.map(([k, v]) => `${k} ${v}`).join(", ")})`)
  }
  if (answers > 0) parts.push(`${answers} implementation-defined ${answers === 1 ? "answer" : "answers"}`)
  return parts.join(" and ")
}

function EntryLink({ id }: { id: string }) {
  return (
    <TextLink to={entryPath(id)} className="font-mono text-[0.85em] [overflow-wrap:anywhere]">
      {id}
    </TextLink>
  )
}

function Detail({ f }: { f: FindingRow }) {
  const shown = 5
  return (
    <div className="flex flex-col gap-3 pt-2 pb-1 text-sm">
      {f.short && f.short !== f.title ? <p className="max-w-[80ch]">{f.title}</p> : null}
      <p className="max-w-[80ch] text-muted-foreground">{f.cause}</p>
      {f.reason ? <p className="max-w-[80ch] text-muted-foreground">Reason for the status: {f.reason}</p> : null}
      {f.reproduce ? (
        <div className="flex flex-col gap-1">
          <span className="font-medium">Reproduce this finding</span>
          <Command>{f.reproduce}</Command>
        </div>
      ) : null}
      <dl className="grid gap-x-6 gap-y-2 sm:grid-cols-[10rem_minmax(0,1fr)]">
        {f.minimal_entry ? (
          <>
            <dt className="font-medium">Smallest entry</dt>
            <dd>
              <EntryLink id={f.minimal_entry} />
            </dd>
          </>
        ) : null}
        <dt className="font-medium">Seen</dt>
        <dd className="num text-muted-foreground">
          {seen(f)} on {f.entries.length} {f.entries.length === 1 ? "entry" : "entries"}
        </dd>
        {f.caps.length > 0 ? (
          <>
            <dt className="font-medium">CAPs</dt>
            <dd className="flex flex-wrap gap-x-3 gap-y-1">
              {f.caps.map((c) => (
                <TextLink key={c} to={capPath(c)} className="font-mono">
                  {c}
                </TextLink>
              ))}
            </dd>
          </>
        ) : null}
        {f.siblings.length > 0 ? (
          <>
            <dt className="font-medium">
              <GlossaryLink id="siblings">Sibling findings</GlossaryLink>
            </dt>
            <dd className="flex flex-wrap gap-x-3 gap-y-1">
              {f.siblings.map((id) => (
                <TextLink key={id} to={findingPath(id)} className="font-mono">
                  {id}
                </TextLink>
              ))}
            </dd>
          </>
        ) : null}
        {f.related.length > 0 ? (
          <>
            <dt className="font-medium">
              <GlossaryLink id="related">Related issues</GlossaryLink>
            </dt>
            <dd className="flex flex-wrap gap-x-3 gap-y-1">
              {f.related.map((u) => (
                <ExtLink key={u} href={u}>
                  {issueLabel(u)}
                </ExtLink>
              ))}
            </dd>
          </>
        ) : null}
        {f.reference ? (
          <>
            <dt className="font-medium">Report reference</dt>
            <dd className="text-muted-foreground">{f.reference}</dd>
          </>
        ) : null}
        {f.draft && !f.link ? (
          <>
            <dt className="font-medium">Issue text</dt>
            <dd>
              <IssueRef value={f.draft} />
            </dd>
          </>
        ) : null}
        <dt className="font-medium">
          <GlossaryLink id="record">Record</GlossaryLink>
        </dt>
        <dd>
          {f.record === "first" ? "first record" : "second record"}, <RepoLink path={f.recordPath} />
        </dd>
        <dt className="font-medium">Entries</dt>
        <dd className="flex min-w-0 flex-col gap-1">
          <ul className="flex flex-col gap-0.5">
            {f.entries.slice(0, shown).map((e) => (
              <li key={e}>
                <EntryLink id={e} />
              </li>
            ))}
          </ul>
          {f.entries.length > shown ? (
            <More label={`Show the other ${f.entries.length - shown} entries`} className="border-dashed">
              <ul className="flex flex-col gap-0.5">
                {f.entries.slice(shown).map((e) => (
                  <li key={e}>
                    <EntryLink id={e} />
                  </li>
                ))}
              </ul>
            </More>
          ) : null}
        </dd>
      </dl>
    </div>
  )
}

function FindingItem({ f, open, onOpen }: { f: FindingRow; open: boolean; onOpen: (o: boolean) => void }) {
  return (
    <li id={f.id} className="scroll-mt-20 py-3 target:bg-accent/40">
      <Collapsible open={open} onOpenChange={onOpen}>
        <div className="grid gap-x-4 gap-y-1.5 md:grid-cols-[8rem_minmax(0,1fr)_15rem]">
          <CollapsibleTrigger className="group flex h-fit items-center gap-1 rounded-sm text-left font-mono text-sm font-medium focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none">
            <ChevronDownIcon aria-hidden className="size-4 shrink-0 transition-transform group-data-[state=open]:rotate-180" />
            {f.id}
            <span className="sr-only">{open ? ", hide the details" : ", show the details"}</span>
          </CollapsibleTrigger>
          <span className="min-w-0">{f.short ?? f.title}</span>
          <div className="flex flex-col gap-1 text-sm">
            <span>
              <span className="text-muted-foreground">Severity:</span> <span className="font-medium">{label(f.severity)}</span>
            </span>
            <span className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
              <Status tone={statusTone[f.status] ?? "idle"}>{label(f.status)}</Status>
              {f.link ? (
                <ExtLink href={f.link} className="text-sm">
                  <span className="sr-only">Open the upstream issue </span>
                  {issueLabel(f.link)}
                </ExtLink>
              ) : null}
            </span>
          </div>
        </div>
        <CollapsibleContent className="md:pl-[9rem]">
          <Detail f={f} />
        </CollapsibleContent>
      </Collapsible>
    </li>
  )
}

// the table of reported findings by component: the one home of these counts
function ByComponent({ byComponent }: { byComponent: Record<string, Record<string, number>> }) {
  const rows = Object.entries(byComponent).toSorted(
    ([a, x], [b, y]) =>
      Math.min(...Object.keys(x).map(severityRank)) - Math.min(...Object.keys(y).map(severityRank)) || sum(y) - sum(x) || a.localeCompare(b),
  )
  return (
    <Table>
      <caption className="sr-only">Reported findings by component. Fewer findings is better for a component.</caption>
      <TableHeader>
        <TableRow>
          <TableHead>
            <GlossaryLink id="component">Component</GlossaryLink>
          </TableHead>
          <TableHead className="text-right">
            Findings <span className="block text-xs font-normal text-muted-foreground">lower is better</span>
          </TableHead>
          <TableHead className="hidden sm:table-cell">By severity</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map(([c, counts]) => (
          <TableRow key={c}>
            <TableCell className="whitespace-normal">
              <TextLink to={findingsPath({ component: c })} className="font-mono text-[0.85em] [overflow-wrap:anywhere]">
                {c}
              </TextLink>
              <span className="block text-xs text-muted-foreground sm:hidden">{severityPhrase(counts)}</span>
            </TableCell>
            <TableCell className="num text-right font-semibold">{sum(counts)}</TableCell>
            <TableCell className="hidden text-sm whitespace-normal text-muted-foreground sm:table-cell">{severityPhrase(counts)}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  )
}

export function FindingsPage() {
  const findings = use(load("findings"))
  const [params, setParams] = useSearchParams()
  const { hash } = useLocation()
  // findings the reader opened or closed; the one the address names starts open
  const [toggled, setToggled] = useState<Map<string, boolean>>(() => new Map())
  const q = params.get("q") ?? ""
  const component = params.get("component") ?? "all"
  const severity = params.get("severity") ?? "all"
  const status = params.get("status") ?? "all"
  const set = (k: string, v: string, dflt: string) => {
    const next = new URLSearchParams(params)
    if (v === dflt) next.delete(k)
    else next.set(k, v)
    setParams(next, { replace: true, preventScrollReset: true })
  }
  const target = hash ? decodeURIComponent(hash.slice(1)) : ""
  // a link to one finding (#WB-W3) brings it into view once it has rendered open
  useEffect(() => {
    if (target) requestAnimationFrame(() => document.getElementById(target)?.scrollIntoView())
  }, [target])

  const all = useMemo(() => findingRows(findings), [findings])
  const reported = useMemo(() => reportedFindings(findings), [findings])
  const r = findings.reported
  const notReported = all.length - reported.length
  const options = {
    component: [...new Set(all.map((f) => f.component))].toSorted(),
    severity: severities.filter((s) => all.some((f) => f.severity === s.id)).map((s) => s.id),
    status: statuses.filter((s) => all.some((f) => f.status === s.id)).map((s) => s.id),
  }
  // a finding that is not reported shows only when the status filter or the address names it
  const groups = useMemo(() => {
    const needle = q.trim().toLowerCase()
    const keep = (f: FindingRow) =>
      f.id === target ||
      ((status === "all" ? f.reported : f.status === status) &&
        (component === "all" || f.component === component) &&
        (severity === "all" || f.severity === severity) &&
        (!needle ||
          f.id.toLowerCase().includes(needle) ||
          f.title.toLowerCase().includes(needle) ||
          (f.short ?? "").toLowerCase().includes(needle) ||
          f.caps.some((c) => c.toLowerCase().includes(needle)) ||
          f.entries.some((e) => e.includes(needle))))
    const by = new Map<string, FindingRow[]>()
    for (const f of all.filter(keep)) by.set(f.component, [...(by.get(f.component) ?? []), f])
    return [...by.entries()]
      .map(
        ([c, rows]) =>
          [c, rows.toSorted((a, b) => severityRank(a.severity) - severityRank(b.severity) || a.id.localeCompare(b.id, "en", { numeric: true }))] as const,
      )
      .toSorted(([a, x], [b, y]) => y.length - x.length || a.localeCompare(b))
  }, [all, q, component, severity, status, target])
  const shown = groups.reduce((n, [, rows]) => n + rows.length, 0)

  const filters = [
    { key: "component", label: "Component", value: component },
    { key: "severity", label: "Severity", value: severity },
    { key: "status", label: "Status", value: status },
  ] as const
  const recordRows = [
    {
      name: "First record",
      path: "findings/findings.json",
      triage: "findings/triage.toml",
      summary: findings.summary,
      unmapped: findings.unmapped,
      problems: findings.input_problems,
    },
    ...(findings.candidates
      ? [
          {
            name: "Second record",
            path: findings.candidates.record,
            triage: "candidate/findings/triage.toml",
            summary: findings.candidates.summary,
            unmapped: findings.candidates.unmapped,
            problems: findings.candidates.input_problems,
          },
        ]
      : []),
  ]
  const problems = recordRows.flatMap((x) => x.problems)

  return (
    <div className="flex flex-col gap-6">
      <PageHeader title="Findings">
        <p>
          A <GlossaryLink id="finding">finding</GlossaryLink> is one defect in an SDK, an indexer or a tool. One finding
          can cause many failures. This page lists every finding with its upstream issue and the command that reproduces
          it.
        </p>
      </PageHeader>

      <NextSteps>
        <li>Open a finding to see its cause and the command that reproduces it.</li>
        <li>Select the issue link of a finding to follow the upstream fix.</li>
        <li>
          Stop a defect from coming back: add the corpus to <TextLink to={docs.sdkCi}>your SDK CI</TextLink> or{" "}
          <TextLink to={docs.indexerCi}>your indexer CI</TextLink>.
        </li>
      </NextSteps>

      <Section title="Summary" id="summary">
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
          <Metric value={fmt.format(r.findings)} label={<GlossaryLink id="reported-finding">Reported findings</GlossaryLink>}>
            in {r.components} components
          </Metric>
          {statuses
            .filter((s) => (r.by_status[s.id] ?? 0) > 0)
            .map((s) => (
              <Metric key={s.id} value={fmt.format(r.by_status[s.id] ?? 0)} label={<GlossaryLink id={`status-${s.id}`}>{s.label}</GlossaryLink>} />
            ))}
        </div>
        <p className="text-sm text-muted-foreground">
          Each finding is a defect, so fewer findings is better for a component.
          {notReported > 0 ? (
            <>
              {" "}
              The total does not include{" "}
              {Object.entries(r.not_reported)
                .map(([k, n]) => `${n} ${n === 1 ? "finding" : "findings"} with the status ${label(k)}`)
                .join(" and ")}
              . Use the status filter below to see them.
            </>
          ) : null}
        </p>
        <ByComponent byComponent={r.by_component} />
      </Section>

      <PageTerms ids={["finding", "reported-finding", "severity-terms", "status-terms", "record"]} />

      {problems.length > 0 ? (
        <ul className="text-sm text-diverge">
          {problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      ) : null}

      <Section
        title="All findings"
        id="list"
        description="The list shows the reported findings, grouped by component. Use the status filter to see drafts and intended behavior."
      >
        <div className="flex flex-col gap-3 lg:flex-row lg:items-center">
          <label className="sr-only" htmlFor="finding-filter">
            Filter the findings
          </label>
          <Input
            id="finding-filter"
            type="search"
            placeholder="Filter by id, title, CAP or entry"
            value={q}
            onChange={(e) => set("q", e.target.value, "")}
            className="lg:max-w-xs"
          />
          {filters.map((fl) => (
            <Select key={fl.key} value={fl.value} onValueChange={(v) => set(fl.key, v, "all")}>
              <SelectTrigger className="w-full lg:w-52" aria-label={fl.label}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="all">{fl.key === "status" ? "Any reported status" : `Any ${fl.label.toLowerCase()}`}</SelectItem>
                  {options[fl.key].map((o) => (
                    <SelectItem key={o} value={o}>
                      {label(o)}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          ))}
        </div>
        <p className="text-sm text-muted-foreground" aria-live="polite">
          {shown} of {all.length} findings shown ({r.findings} reported, {notReported} not reported).
        </p>
        {groups.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyTitle>No finding matches</EmptyTitle>
              <EmptyDescription>Clear the filters to see all {r.findings} reported findings.</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          groups.map(([c, rows]) => (
            <section key={c} aria-labelledby={`c-${c}`} className="flex flex-col">
              <h3 id={`c-${c}`} className="pt-3 font-mono text-sm font-semibold [overflow-wrap:anywhere]">
                {c} <span className="num font-sans font-normal text-muted-foreground">({rows.length})</span>
              </h3>
              <ol className="mt-1 flex flex-col divide-y border-y">
                {rows.map((f) => (
                  <FindingItem
                    key={f.id}
                    f={f}
                    open={toggled.get(f.id) ?? f.id === target}
                    onOpen={(o) => setToggled((prev) => new Map(prev).set(f.id, o))}
                  />
                ))}
              </ol>
            </section>
          ))
        )}
      </Section>

      <Section
        title="Records and inputs"
        id="records"
        description={
          <>
            Two committed <GlossaryLink id="record">records</GlossaryLink> hold the findings. Each record maps every{" "}
            <GlossaryLink id="failure">failure</GlossaryLink> of its runs to one finding. The readable report of the first
            record is <RepoLink path={sources.findingsReport} />.
          </>
        }
      >
        <Table>
          <caption className="sr-only">The two findings records</caption>
          <TableHeader>
            <TableRow>
              <TableHead>Record</TableHead>
              <TableHead className="text-right">Findings, all statuses</TableHead>
              <TableHead className="text-right">Failures</TableHead>
              <TableHead className="text-right">
                <GlossaryLink id="unmapped">Unmapped</GlossaryLink>
                <span className="block text-xs font-normal text-muted-foreground">lower is better</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody className="num">
            {recordRows.map((x) => (
              <TableRow key={x.path}>
                <TableCell className="font-sans whitespace-normal">
                  {x.name}
                  <span className="block text-xs">
                    <RepoLink path={x.path} />, triage <RepoLink path={x.triage} />
                  </span>
                </TableCell>
                <TableCell className="text-right">{x.summary.findings}</TableCell>
                <TableCell className="text-right">{fmt.format(x.summary.failures)}</TableCell>
                <TableCell className="text-right">{x.unmapped}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <p className="text-sm text-muted-foreground">
          The two records can read the same run, so you cannot add the failures of the two records.
        </p>
        <More label={`Show the ${findings.inputs.length} runs of the first record`}>
          <ul className="flex flex-col gap-2 text-sm text-muted-foreground">
            {findings.inputs.map((i) => (
              <li key={i.path} className="flex flex-col gap-0.5">
                <span>
                  {i.kind}: <RepoLink path={i.path} />
                </span>
                <span className="[overflow-wrap:anywhere]">{i.detail}</span>
              </li>
            ))}
          </ul>
        </More>
      </Section>
    </div>
  )
}
