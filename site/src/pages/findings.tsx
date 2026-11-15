import { use, useEffect, useMemo, useState } from "react"
import { useLocation, useSearchParams } from "react-router"
import { ChevronDownIcon } from "lucide-react"
import { load, type Entry, type Finding } from "@/lib/data"
import { severities, severityRank, sources, statuses } from "@/lib/terms"
import { cn } from "@/lib/utils"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import {
  ExtLink,
  Id,
  IssueRef,
  PageHeader,
  RepoLink,
  Status,
  Term,
  Terms,
  TextLink,
  fmt,
  issueLabel,
  type Tone,
} from "@/components/common"

const statusTone: Record<string, Tone> = {
  draft: "known",
  filed: "pass",
  "duplicate-of": "pass",
  "private-report": "pass",
  "wont-fix": "idle",
}

// an entry's row on the cap page of its first cap
function entryHref(e: Entry | undefined): string | null {
  const cap = e?.caps[0]
  return e && cap ? `/dashboard/coverage/${cap}#${e.id}` : null
}

function EntryLink({ id, entries }: { id: string; entries: Map<string, Entry> }) {
  const href = entryHref(entries.get(id))
  return href ? (
    <TextLink to={href} className="font-mono text-[0.85em] [overflow-wrap:anywhere]">
      {id}
    </TextLink>
  ) : (
    <Id>{id}</Id>
  )
}

function Detail({ f, entries }: { f: Finding; entries: Map<string, Entry> }) {
  return (
    <div className="flex flex-col gap-3 pt-2 pb-1 text-sm">
      <p className="max-w-[80ch] text-muted-foreground">{f.cause}</p>
      {f.reason ? <p className="max-w-[80ch] text-muted-foreground">Decision: {f.reason}</p> : null}
      <dl className="grid gap-x-6 gap-y-2 sm:grid-cols-[9rem_minmax(0,1fr)]">
        {f.minimal_entry ? (
          <>
            <dt className="font-medium">Smallest entry</dt>
            <dd>
              <EntryLink id={f.minimal_entry} entries={entries} />
            </dd>
          </>
        ) : null}
        {f.reproduce ? (
          <>
            <dt className="font-medium">Reproduce</dt>
            <dd className="min-w-0">
              <pre className="overflow-x-auto rounded-md bg-muted px-3 py-2 text-xs whitespace-pre-wrap [overflow-wrap:anywhere]">
                {f.reproduce}
              </pre>
            </dd>
          </>
        ) : null}
        <dt className="font-medium">Seen</dt>
        <dd className="num text-muted-foreground">
          {f.occurrences} failures (
          {Object.entries(f.occurrences_by_source)
            .map(([k, v]) => `${k} ${v}`)
            .join(", ")}
          ) on {f.entries.length} {f.entries.length === 1 ? "entry" : "entries"}
        </dd>
        {f.siblings.length > 0 ? (
          <>
            <dt className="font-medium">Siblings</dt>
            <dd className="flex flex-wrap gap-x-3 gap-y-1">
              {f.siblings.map((id) => (
                <TextLink key={id} to={`/dashboard/findings#${id}`} className="font-mono">
                  {id}
                </TextLink>
              ))}
            </dd>
          </>
        ) : null}
        {f.related.length > 0 ? (
          <>
            <dt className="font-medium">Related upstream</dt>
            <dd className="flex flex-wrap gap-x-3 gap-y-1">
              {f.related.map((u) => (
                <ExtLink key={u} href={u}>
                  {issueLabel(u)}
                </ExtLink>
              ))}
            </dd>
          </>
        ) : null}
        <dt className="font-medium">Entries</dt>
        <dd>
          <ul className="flex flex-col gap-0.5">
            {f.entries.map((e) => (
              <li key={e}>
                <EntryLink id={e} entries={entries} />
              </li>
            ))}
          </ul>
        </dd>
      </dl>
    </div>
  )
}

function FindingItem({ f, entries, open, onOpen }: { f: Finding; entries: Map<string, Entry>; open: boolean; onOpen: (o: boolean) => void }) {
  return (
    <li id={f.id} className="scroll-mt-20 py-3 target:bg-accent/40">
      <Collapsible open={open} onOpenChange={onOpen}>
        <div className="grid gap-x-4 gap-y-1.5 md:grid-cols-[7.5rem_minmax(0,1fr)_14rem]">
          <CollapsibleTrigger className="group flex h-fit items-center gap-1 rounded-sm text-left font-mono text-sm font-medium focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none">
            <ChevronDownIcon aria-hidden className="size-4 shrink-0 transition-transform group-data-[state=open]:rotate-180" />
            {f.id}
            <span className="sr-only">{open ? ", hide details" : ", show details"}</span>
          </CollapsibleTrigger>
          <div className="flex min-w-0 flex-col gap-1">
            <span>{f.title}</span>
            <span className="flex flex-wrap gap-x-2 gap-y-0.5 text-xs">
              {f.caps.map((c) => (
                <TextLink key={c} to={`/dashboard/coverage/${c}`} className="font-mono">
                  {c}
                </TextLink>
              ))}
            </span>
          </div>
          <div className="flex flex-col gap-1 text-sm">
            <span>
              <span className="font-medium">{f.severity}</span> <span className="text-muted-foreground">in</span>{" "}
              <span className="font-mono text-[0.85em]">{f.component}</span>
            </span>
            <span className="flex flex-wrap items-center gap-x-2">
              <Status tone={statusTone[f.status] ?? "idle"}>{f.status}</Status>
              {f.link ? <ExtLink href={f.link}>{issueLabel(f.link)}</ExtLink> : null}
            </span>
            {f.reference ? <span className="text-xs text-muted-foreground">{f.reference}</span> : null}
            {f.draft && !f.link ? <IssueRef value={f.draft} className="text-xs" /> : null}
          </div>
        </div>
        <CollapsibleContent className="md:pl-[8.5rem]">
          <Detail f={f} entries={entries} />
        </CollapsibleContent>
      </Collapsible>
    </li>
  )
}

export function FindingsPage() {
  const findings = use(load("findings"))
  const entriesFile = use(load("entries"))
  const entries = useMemo(() => new Map(entriesFile.entries.map((e) => [e.id, e])), [entriesFile])
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

  const options = {
    component: [...new Set(findings.findings.map((f) => f.component))].toSorted(),
    severity: [...new Set(findings.findings.map((f) => f.severity))].toSorted((a, b) => severityRank(a) - severityRank(b)),
    status: [...new Set(findings.findings.map((f) => f.status))].toSorted(),
  }
  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase()
    return findings.findings.filter(
      (f) =>
        (component === "all" || f.component === component) &&
        (severity === "all" || f.severity === severity) &&
        (status === "all" || f.status === status) &&
        (!needle ||
          f.id.toLowerCase().includes(needle) ||
          f.title.toLowerCase().includes(needle) ||
          f.caps.some((c) => c.toLowerCase().includes(needle)) ||
          f.entries.some((e) => e.includes(needle))),
    )
  }, [findings, q, component, severity, status])

  const filters = [
    { key: "component", label: "Component", value: component },
    { key: "severity", label: "Severity", value: severity },
    { key: "status", label: "Status", value: status },
  ] as const
  const s = findings.summary

  return (
    <div className="flex flex-col">
      <PageHeader title="Findings">
        <p>
          Every failure the indexer run, the SDK runs and the linter reported, traced to a root cause. {s.findings} findings
          cover {fmt.format(s.failures)} failures, and{" "}
          {s.unmapped === 0 ? "no failure is left without a finding" : `${s.unmapped} failures have no finding`}. Each
          finding names the smallest corpus entry that shows it and how to reproduce it. Source:{" "}
          <RepoLink path="findings/findings.json" />, readable as <RepoLink path={sources.findingsReport} />; the triage
          is in <RepoLink path="findings/triage.toml" />.
        </p>
      </PageHeader>

      <Terms>
        {severities.map((x) => (
          <Term key={x.id} name={x.id}>
            {x.meaning}.
          </Term>
        ))}
        {statuses
          .filter((x) => (s.by_status[x.id] ?? 0) > 0)
          .map((x) => (
            <Term key={x.id} name={<Status tone={statusTone[x.id] ?? "idle"}>{x.id}</Status>}>
              {x.meaning}.
            </Term>
          ))}
        <Term name="siblings, related">
          Siblings are other findings filed as the same defect elsewhere, usually in another SDK. Related links are
          upstream issues or pull requests that touch the same code but are not this finding. All of these terms are
          defined in <RepoLink path={sources.triage} />.
        </Term>
      </Terms>

      {findings.input_problems.length > 0 ? (
        <ul className="pt-4 text-sm text-diverge">
          {findings.input_problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      ) : null}

      <div className="flex flex-col gap-3 pt-6 lg:flex-row lg:items-center">
        <label className="sr-only" htmlFor="finding-filter">
          Filter findings
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
            <SelectTrigger className="w-full lg:w-48" aria-label={fl.label}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectItem value="all">Any {fl.label.toLowerCase()}</SelectItem>
                {options[fl.key].map((o) => (
                  <SelectItem key={o} value={o}>
                    {o}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        ))}
      </div>
      <p className="pt-3 text-sm text-muted-foreground" aria-live="polite">
        {rows.length} of {findings.findings.length} findings shown. Select an id for its cause, entries and how to
        reproduce it.
      </p>

      {rows.length === 0 ? (
        <Empty className="my-6 border">
          <EmptyHeader>
            <EmptyTitle>No finding matches</EmptyTitle>
            <EmptyDescription>Clear the filters to see all {findings.findings.length}.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <ol className={cn("mt-2 flex flex-col divide-y border-y")}>
          {rows.map((f) => (
            <FindingItem
              key={f.id}
              f={f}
              entries={entries}
              open={toggled.get(f.id) ?? f.id === target}
              onOpen={(o) => setToggled((prev) => new Map(prev).set(f.id, o))}
            />
          ))}
        </ol>
      )}

      <section className="flex flex-col gap-2 pt-8 text-sm text-muted-foreground" aria-labelledby="inputs-h">
        <h2 id="inputs-h" className="text-lg font-semibold tracking-tight text-foreground">
          Runs this record was collected from
        </h2>
        <ul className="flex flex-col gap-2">
          {findings.inputs.map((i) => (
            <li key={i.path} className="flex flex-col gap-0.5">
              <span>
                {i.kind}: <RepoLink path={i.path} />
              </span>
              <span className="[overflow-wrap:anywhere]">{i.detail}</span>
            </li>
          ))}
        </ul>
      </section>
    </div>
  )
}
