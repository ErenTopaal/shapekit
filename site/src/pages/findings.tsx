import { use, useMemo, useState } from "react"
import { Link, useSearchParams } from "react-router"
import { ChevronDownIcon } from "lucide-react"
import { load, type Finding } from "@/lib/data"
import { cn } from "@/lib/utils"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { ExtLink, Id, PageHeader, RepoLink, Status, fmt, issueLabel } from "@/components/common"

function StatusCell({ f }: { f: Finding }) {
  return (
    <span className="flex flex-col gap-1">
      <Status tone={f.status === "draft" ? "known" : "pass"}>{f.status}</Status>
      {f.link ? <ExtLink href={f.link}>{issueLabel(f.link)}</ExtLink> : null}
      {f.reference ? <span className="text-xs text-muted-foreground">{f.reference}</span> : null}
      {f.draft ? (
        <RepoLink path={f.draft} className="text-xs">
          issue draft
        </RepoLink>
      ) : null}
    </span>
  )
}

function Detail({ f }: { f: Finding }) {
  return (
    <div className="flex flex-col gap-3 py-2 text-sm">
      <p className="max-w-[80ch] text-muted-foreground">{f.cause}</p>
      {f.reason ? <p className="text-muted-foreground">Not filed: {f.reason}</p> : null}
      <dl className="grid gap-x-6 gap-y-2 sm:grid-cols-[10rem_1fr]">
        {f.minimal_entry ? (
          <>
            <dt className="font-medium">Smallest entry</dt>
            <dd>
              <Id>{f.minimal_entry}</Id>
            </dd>
          </>
        ) : null}
        {f.reproduce ? (
          <>
            <dt className="font-medium">Reproduce</dt>
            <dd>
              <pre className="overflow-x-auto rounded-md bg-muted px-3 py-2 text-xs whitespace-pre-wrap break-all">
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
          ) on {f.entries.length} entries
        </dd>
        {f.siblings.length + f.related.length > 0 ? (
          <>
            <dt className="font-medium">Siblings, related</dt>
            <dd className="flex flex-wrap gap-1.5">
              {[...f.siblings, ...f.related].map((s) => (
                <Badge key={s} variant="outline" className="font-mono">
                  {s}
                </Badge>
              ))}
            </dd>
          </>
        ) : null}
        <dt className="font-medium">Entries</dt>
        <dd>
          <ul className="flex flex-col gap-0.5">
            {f.entries.map((e) => (
              <li key={e}>
                <Id className="text-muted-foreground">{e}</Id>
              </li>
            ))}
          </ul>
        </dd>
      </dl>
    </div>
  )
}

function Row({ f }: { f: Finding }) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <TableRow className={cn(open && "border-b-0")}>
        <TableCell className="align-top">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setOpen(!open)}
            aria-expanded={open}
            aria-label={`${open ? "Hide" : "Show"} details of ${f.id}`}
            className="font-mono"
          >
            <ChevronDownIcon data-icon="inline-start" className={cn("transition-transform", open && "rotate-180")} />
            {f.id}
          </Button>
        </TableCell>
        <TableCell className="max-w-md align-top whitespace-normal">{f.title}</TableCell>
        <TableCell className="align-top">{f.component}</TableCell>
        <TableCell className="align-top">{f.severity}</TableCell>
        <TableCell className="align-top">
          <span className="flex flex-col">
            {f.caps.map((c) => (
              <Link key={c} to={`/dashboard/coverage/${c}`} className="font-mono text-xs text-primary underline decoration-primary/40">
                {c}
              </Link>
            ))}
          </span>
        </TableCell>
        <TableCell className="align-top">
          <StatusCell f={f} />
        </TableCell>
      </TableRow>
      {open ? (
        <TableRow>
          <TableCell colSpan={6} className="whitespace-normal">
            <Detail f={f} />
          </TableCell>
        </TableRow>
      ) : null}
    </>
  )
}

export function FindingsPage() {
  const findings = use(load("findings"))
  const [params, setParams] = useSearchParams()
  const q = params.get("q") ?? ""
  const component = params.get("component") ?? "all"
  const severity = params.get("severity") ?? "all"
  const status = params.get("status") ?? "all"
  const set = (k: string, v: string, dflt: string) => {
    const next = new URLSearchParams(params)
    if (v === dflt) next.delete(k)
    else next.set(k, v)
    setParams(next, { replace: true })
  }
  const options = (k: "component" | "severity" | "status") => [...new Set(findings.findings.map((f) => f[k]))].toSorted()
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

  return (
    <div className="flex flex-col">
      <PageHeader title="Findings">
        <p>
          Every failure the harness, the conformance runner and the linter reported, grouped into triaged root
          causes. {findings.summary.findings} findings cover {fmt.format(findings.summary.failures)} failures;{" "}
          {findings.summary.unmapped} failures match no finding. A draft has an issue body written but not yet
          filed; a duplicate points at the upstream issue or fix that already covers it. Source:{" "}
          <RepoLink path="findings/findings.json" />, triage in <RepoLink path="findings/triage.toml" />.
        </p>
      </PageHeader>

      {findings.input_problems.length > 0 ? (
        <ul className="pb-4 text-sm text-diverge">
          {findings.input_problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      ) : null}

      <div className="flex flex-col gap-3 lg:flex-row lg:items-center">
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
                {options(fl.key).map((o) => (
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
        {rows.length} of {findings.findings.length} findings shown.
      </p>

      {rows.length === 0 ? (
        <Empty className="my-6 border">
          <EmptyHeader>
            <EmptyTitle>No finding matches</EmptyTitle>
            <EmptyDescription>Clear the filters to see all {findings.findings.length}.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <Table className="mt-2">
          <TableHeader>
            <TableRow>
              <TableHead>Id</TableHead>
              <TableHead className="min-w-64">Title</TableHead>
              <TableHead>Component</TableHead>
              <TableHead>Severity</TableHead>
              <TableHead>CAP</TableHead>
              <TableHead>Status</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((f) => (
              <Row key={f.id} f={f} />
            ))}
          </TableBody>
        </Table>
      )}

      <section className="flex flex-col gap-2 pt-8 text-sm text-muted-foreground" aria-labelledby="inputs-h">
        <h2 id="inputs-h" className="text-lg font-semibold tracking-tight text-foreground">
          Inputs of this record
        </h2>
        <ul className="flex flex-col gap-1">
          {findings.inputs.map((i) => (
            <li key={i.path} className="flex flex-col sm:flex-row sm:gap-2">
              <span>{i.kind}:</span>
              <RepoLink path={i.path} />
              <span className="[overflow-wrap:anywhere]">{i.detail}</span>
            </li>
          ))}
        </ul>
      </section>
    </div>
  )
}
