import { use, useMemo } from "react"
import { Link, useParams } from "react-router"
import { load, type Entry } from "@/lib/data"
import { Badge } from "@/components/ui/badge"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Fragments } from "@/components/fragments"
import { ExtLink, Id, useHashScroll, PageHeader, RepoLink, Section, Status, fmt, issueLabel } from "@/components/common"
import { NotFound } from "./not-found"

function Counts({ counts }: { counts: Record<string, number> }) {
  const e = Object.entries(counts)
  if (e.length === 0) return <span className="text-muted-foreground">none</span>
  return (
    <span className="num">
      {e.map(([k, v], i) => (
        <span key={k}>
          {k} {v}
          {i < e.length - 1 ? ", " : ""}
        </span>
      ))}
    </span>
  )
}

export function CapPage() {
  const { cap = "" } = useParams()
  const coverage = use(load("coverage"))
  const entries = use(load("entries"))
  const conformance = use(load("conformance"))
  const harness = use(load("harness"))

  const row = coverage.caps.find((c) => c.id === cap)
  const families = coverage.families.filter((f) => row?.families.includes(f.id))
  const capEntries = useMemo(() => entries.entries.filter((e) => e.caps.includes(cap)), [entries, cap])

  // per entry: failing sdk cells and failing harness replays, from the committed results
  const sdkFails = useMemo(() => {
    const m = new Map<string, string[]>()
    for (const s of conformance.sdks)
      for (const e of s.entries) for (const k of Object.keys(e.cells)) m.set(e.id, [...(m.get(e.id) ?? []), k])
    return m
  }, [conformance])
  const harnessFails = useMemo(() => {
    const m = new Map<string, string[]>()
    for (const r of harness.runs)
      for (const p of r.problems) m.set(p.entry, [...(m.get(p.entry) ?? []), `${p.verdict} on ${p.network}`])
    return m
  }, [harness])

  useHashScroll()

  if (!row) return <NotFound />

  const byFamily = new Map<string, Entry[]>()
  for (const e of capEntries) byFamily.set(e.family, [...(byFamily.get(e.family) ?? []), e])

  return (
    <div className="flex flex-col">
      <nav aria-label="Breadcrumb" className="pb-2 text-sm">
        <Link to="/dashboard/coverage" className="text-primary underline decoration-primary/40 hover:decoration-primary">
          CAP coverage
        </Link>
      </nav>
      <PageHeader title={`${row.id}: ${row.title}`}>
        <p>
          {row.status}, protocol {row.protocol ?? "not set"}.{" "}
          {row.indexer_visible
            ? "Indexer-visible: the CAP changes what an indexer reads."
            : `Not indexer-visible: ${row.not_visible_reason ?? "no reason recorded"}.`}{" "}
          <ExtLink href={row.text_url}>Read the CAP text</ExtLink>.
        </p>
      </PageHeader>

      <dl className="grid gap-x-8 gap-y-3 border-y py-4 text-sm sm:grid-cols-2">
        <div>
          <dt className="font-medium">Entries</dt>
          <dd className="num text-muted-foreground">
            {fmt.format(row.entries)}, of which {fmt.format(row.core_emitted_entries)} core-emitted
          </dd>
        </div>
        <div>
          <dt className="font-medium">By source</dt>
          <dd className="text-muted-foreground">
            <Counts counts={row.entries_by_source_class} />
          </dd>
        </div>
        <div>
          <dt className="font-medium">By network variant</dt>
          <dd className="text-muted-foreground">
            <Counts counts={row.entries_by_variant} />
          </dd>
        </div>
        <div>
          <dt className="font-medium">Protocols with core-emitted entries</dt>
          <dd className="num text-muted-foreground">{row.protocols_core_emitted.join(", ") || "none"}</dd>
        </div>
        {row.uncovered ? (
          <div className="sm:col-span-2">
            <dt className="font-medium">Why there is no core-emitted entry</dt>
            <dd className="text-muted-foreground">
              <Id>{row.uncovered.blocker}</Id>: {row.uncovered.reason}
            </dd>
          </div>
        ) : null}
        {row.xdr_additions.length > 0 ? (
          <div className="sm:col-span-2">
            <dt className="font-medium">XDR the CAP added</dt>
            <dd className="flex flex-wrap gap-1.5 pt-1">
              {row.xdr_additions.map((x) => (
                <Badge key={x} variant="outline" className="font-mono">
                  {x}
                </Badge>
              ))}
            </dd>
          </div>
        ) : null}
      </dl>

      {families.length === 0 ? (
        <p className="py-6 text-muted-foreground">No shape family names this CAP.</p>
      ) : null}

      {families.map((f) => {
        const list = byFamily.get(f.id) ?? []
        return (
          <Section
            key={f.id}
            id={f.id}
            title={f.id}
            description={`${f.title}. Shape introduced in protocol ${f.protocol_introduced}; valid from ${f.protocol_valid.min}${f.protocol_valid.max ? ` to ${f.protocol_valid.max}` : ""}.`}
          >
            <div className="flex flex-col gap-2 text-sm">
              <Fragments fragments={f.fragments} />
              {f.missing_source_classes.length > 0 ? (
                <p className="text-muted-foreground">
                  Source classes the registry expects but no entry has: {f.missing_source_classes.join(", ")}.
                </p>
              ) : null}
              {f.linked_issues.length > 0 ? (
                <p className="text-muted-foreground">
                  Upstream sightings:{" "}
                  {f.linked_issues.map((u, i) => (
                    <span key={u}>
                      <ExtLink href={u}>{issueLabel(u)}</ExtLink>
                      {i < f.linked_issues.length - 1 ? ", " : ""}
                    </span>
                  ))}
                </p>
              ) : null}
            </div>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Fragment</TableHead>
                  <TableHead>State</TableHead>
                  <TableHead className="text-right">Core-emitted</TableHead>
                  <TableHead className="text-right">Other</TableHead>
                  <TableHead className="text-right">Refused</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {f.fragments.map((x) => (
                  <TableRow key={x.fragment}>
                    <TableCell>
                      <Id>{x.fragment}</Id>
                    </TableCell>
                    <TableCell className="whitespace-normal">
                      {x.covered ? (
                        <Status tone="pass">covered</Status>
                      ) : (
                        <span className="flex flex-col gap-1">
                          <Status tone="diverge">uncovered</Status>
                          {x.uncovered ? (
                            <span className="text-xs text-muted-foreground">
                              {x.uncovered.blocker}: {x.uncovered.reason}
                            </span>
                          ) : null}
                        </span>
                      )}
                    </TableCell>
                    <TableCell className="num text-right">{x.core_emitted}</TableCell>
                    <TableCell className="num text-right">{x.non_core}</TableCell>
                    <TableCell className="num text-right">{x.refused_by_core}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <h3 className="pt-2 font-medium">
              Entries of this family tagged {row.id} <span className="num text-muted-foreground">({list.length})</span>
            </h3>
            {list.length === 0 ? (
              <p className="text-sm text-muted-foreground">None in this family carry the tag.</p>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Entry</TableHead>
                    <TableHead>Source</TableHead>
                    <TableHead className="text-right">Protocol</TableHead>
                    <TableHead>Expected</TableHead>
                    <TableHead>Files</TableHead>
                    <TableHead>Results</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {list.map((e) => {
                    const sdk = sdkFails.get(e.id) ?? []
                    const hx = harnessFails.get(e.id) ?? []
                    return (
                      <TableRow key={e.id}>
                        <TableCell className="max-w-96 align-top whitespace-normal">
                          <Id>{e.id}</Id>
                          {e.linked_issues.length > 0 ? (
                            <span className="block pt-1 text-xs">
                              {e.linked_issues.map((u, i) => (
                                <span key={u}>
                                  <ExtLink href={u}>{issueLabel(u)}</ExtLink>
                                  {i < e.linked_issues.length - 1 ? ", " : ""}
                                </span>
                              ))}
                            </span>
                          ) : null}
                        </TableCell>
                        <TableCell className="align-top">{e.source}</TableCell>
                        <TableCell className="num text-right align-top">
                          {e.protocol.generated_at ?? e.protocol.valid_min}
                        </TableCell>
                        <TableCell className="align-top text-xs">
                          {e.expected.decode}
                          {e.expected.validity !== "valid" ? <span className="block">{e.expected.validity}</span> : null}
                        </TableCell>
                        <TableCell className="align-top text-xs">
                          <span className="flex flex-col gap-0.5">
                            {e.files.map((file) => (
                              <RepoLink key={file.path} path={file.path}>
                                {file.variant}
                              </RepoLink>
                            ))}
                          </span>
                        </TableCell>
                        <TableCell className="align-top text-xs whitespace-normal">
                          {sdk.length === 0 && hx.length === 0 ? (
                            <span className="text-muted-foreground">no failure recorded</span>
                          ) : (
                            <span className="flex flex-col gap-0.5">
                              {sdk.length > 0 ? (
                                <Link
                                  to={`/dashboard/sdk?entry=${encodeURIComponent(e.id)}`}
                                  className="text-diverge underline decoration-diverge/40"
                                >
                                  SDK: {sdk.join(", ")}
                                </Link>
                              ) : null}
                              {hx.length > 0 ? (
                                <Link
                                  to={`/dashboard/indexer?q=${encodeURIComponent(e.id)}`}
                                  className="text-diverge underline decoration-diverge/40"
                                >
                                  Indexer: {hx.join(", ")}
                                </Link>
                              ) : null}
                            </span>
                          )}
                        </TableCell>
                      </TableRow>
                    )
                  })}
                </TableBody>
              </Table>
            )}
          </Section>
        )
      })}
    </div>
  )
}
