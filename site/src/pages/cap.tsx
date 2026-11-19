import { use, useMemo } from "react"
import { Link, useParams } from "react-router"
import { load, type Entry } from "@/lib/data"
import { sdkName } from "@/lib/sdks"
import { findingRows } from "@/lib/metrics"
import { capPath, entryPath, findingPath, sdkPath } from "@/lib/paths"
import { label, verdictMeaning } from "@/lib/terms"
import { Badge } from "@/components/ui/badge"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Fragments } from "@/components/fragments"
import {
  Breadcrumb,
  ExtLink,
  GlossaryLink,
  Id,
  useHashScroll,
  PageHeader,
  PageTerms,
  RepoLink,
  Section,
  Status,
  TextLink,
  fmt,
  indexerHref,
  indexerResults,
  issueLabel,
} from "@/components/common"
import { NotFound } from "./not-found"

function Counts({ counts }: { counts: Record<string, number> }) {
  const e = Object.entries(counts)
  if (e.length === 0) return <span className="text-muted-foreground">none</span>
  return <span className="num">{e.map(([k, v]) => `${k} ${v}`).join(", ")}</span>
}

export function CapPage() {
  const { cap = "" } = useParams()
  const coverage = use(load("coverage"))
  const entries = use(load("entries"))
  const conformance = use(load("conformance"))
  const harness = use(load("harness"))
  const findings = use(load("findings"))

  const row = coverage.caps.find((c) => c.id === cap)
  const families = coverage.families.filter((f) => row?.families.includes(f.id))
  const capEntries = useMemo(() => entries.entries.filter((e) => e.caps.includes(cap)), [entries, cap])
  const allFindings = useMemo(() => findingRows(findings), [findings])
  const capFindings = allFindings.filter((f) => f.reported && f.caps.includes(cap))

  // per entry and sdk, the versions with a failing cell in the committed matrix
  const sdkFails = useMemo(() => {
    const m = new Map<string, Map<string, string[]>>()
    for (const s of conformance.sdks)
      for (const e of s.entries) {
        const per = m.get(e.id) ?? new Map<string, string[]>()
        per.set(
          s.sdk,
          s.versions.filter((v) => e.cells[v.key]).map((v) => v.version),
        )
        m.set(e.id, per)
      }
    return m
  }, [conformance])
  const versionsOf = new Map(conformance.sdks.map((s) => [s.sdk, s.versions.length]))

  useHashScroll()

  if (!row) return <NotFound />

  const byFamily = new Map<string, Entry[]>()
  for (const e of capEntries) byFamily.set(e.family, [...(byFamily.get(e.family) ?? []), e])

  return (
    <div className="flex flex-col">
      <Breadcrumb items={[{ to: "/dashboard/corpus", label: "Corpus" }, { to: "/dashboard/corpus#caps", label: "CAPs" }, { label: row.id, mono: true }]} />
      <PageHeader title={`${row.id}: ${row.title}`}>
        <p>
          {row.status}
          {row.protocol ? `, protocol ${row.protocol}` : ", no protocol"}.{" "}
          {row.indexer_visible ? (
            <>
              <GlossaryLink id="indexer-visible">Indexer-visible</GlossaryLink>: the CAP changes what an indexer reads.
            </>
          ) : (
            `Not indexer-visible: ${row.not_visible_reason ?? "no reason recorded"}.`
          )}{" "}
          <ExtLink href={row.text_url}>Read the CAP text</ExtLink>.
        </p>
      </PageHeader>

      <dl className="grid gap-x-8 gap-y-3 border-y py-4 text-sm sm:grid-cols-2 [&_dd]:[overflow-wrap:anywhere]">
        <div>
          <dt className="font-medium">Entries</dt>
          <dd className="num text-muted-foreground">
            {fmt.format(row.entries)}. stellar-core emitted {fmt.format(row.core_emitted_entries)} of them (higher is better).
          </dd>
        </div>
        <div>
          <dt className="font-medium">By source</dt>
          <dd className="text-muted-foreground">
            <Counts counts={row.entries_by_source_class} />
          </dd>
        </div>
        <div>
          <dt className="font-medium">Files for each network</dt>
          <dd className="text-muted-foreground">
            <Counts counts={row.entries_by_variant} />
          </dd>
        </div>
        <div>
          <dt className="font-medium">Protocols with an entry from stellar-core</dt>
          <dd className="num text-muted-foreground">{row.protocols_core_emitted.join(", ") || "none"}</dd>
        </div>
        {row.uncovered ? (
          <div className="sm:col-span-2">
            <dt className="font-medium">Why no entry from stellar-core</dt>
            <dd className="text-muted-foreground">
              <Id>{row.uncovered.blocker}</Id> ({coverage.blockers.find((b) => b.blocker === row.uncovered?.blocker)?.meaning}
              ): {row.uncovered.reason}
            </dd>
          </div>
        ) : null}
        {row.xdr_additions.length > 0 ? (
          <div className="sm:col-span-2">
            <dt className="font-medium">XDR the CAP added</dt>
            <dd className="flex flex-wrap gap-1.5 pt-1">
              {row.xdr_additions.map((x) => (
                <Badge key={x} variant="outline" className="h-auto max-w-full font-mono whitespace-normal [overflow-wrap:anywhere]">
                  {x}
                </Badge>
              ))}
            </dd>
          </div>
        ) : null}
      </dl>

      <div className="pt-4">
        <PageTerms ids={["family", "fragment", "core-emitted", "corpus-terms", "diverging-entry", "verdict"]} />
      </div>

      {capFindings.length > 0 ? (
        <Section
          title="Findings on this CAP"
          id="findings"
          description={`${capFindings.length} of the ${findings.reported.findings} reported findings name ${row.id}.`}
        >
          <ul className="flex flex-col divide-y border-y text-sm">
            {capFindings.map((f) => (
              <li key={f.id} className="flex flex-col gap-1 py-2.5 sm:flex-row sm:items-baseline sm:gap-4">
                <TextLink to={findingPath(f.id)} className="shrink-0 font-mono sm:w-32">
                  {f.id}
                </TextLink>
                <span className="flex-1">{f.short ?? f.title}</span>
                <span className="shrink-0 text-muted-foreground">
                  {f.component}, {label(f.severity)}, {label(f.status)}
                </span>
              </li>
            ))}
          </ul>
        </Section>
      ) : null}

      {families.length === 0 ? (
        <p className="py-6 text-muted-foreground">
          No shape family names this CAP{row.indexer_visible ? "" : ", since it does not change what an indexer reads"}.
        </p>
      ) : null}

      {families.map((f) => {
        const list = byFamily.get(f.id) ?? []
        const covered = f.fragments.filter((x) => x.covered).length
        return (
          <Section
            key={f.id}
            id={f.id}
            title={f.id}
            description={
              <>
                {f.title}. The shape starts in protocol {f.protocol_introduced}. It is valid from protocol {f.protocol_valid.min}
                {f.protocol_valid.max ? ` to ${f.protocol_valid.max}` : " on"}.
                {f.caps.length > 1 ? (
                  <>
                    {" "}
                    The family also carries{" "}
                    {f.caps
                      .filter((c) => c !== row.id)
                      .map((c, i, all) => (
                        <span key={c}>
                          <TextLink to={capPath(c, f.id)}>{c}</TextLink>
                          {i < all.length - 1 ? ", " : ""}
                        </span>
                      ))}
                    .
                  </>
                ) : null}
              </>
            }
          >
            <div className="flex flex-col gap-2 text-sm">
              <div className="flex flex-wrap items-center gap-3">
                <Fragments fragments={f.fragments} />
                <span className="num text-muted-foreground">
                  {covered} of {f.fragments.length} fragments have an entry from stellar-core (higher is better)
                </span>
              </div>
              {f.missing_source_classes.length > 0 ? (
                <p className="text-muted-foreground">
                  The registry expects these sources, but no entry has them: {f.missing_source_classes.join(", ")}.
                </p>
              ) : null}
              {f.linked_issues.length > 0 ? (
                <p className="text-muted-foreground">
                  Upstream issues about this shape:{" "}
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
              <caption className="sr-only">Fragments of {f.id} and the entries that contain each</caption>
              <TableHeader>
                <TableRow>
                  <TableHead>
                    <GlossaryLink id="fragment">Fragment</GlossaryLink>
                  </TableHead>
                  <TableHead className="text-right">
                    From stellar-core
                    <span className="block text-xs font-normal text-muted-foreground">higher is better</span>
                  </TableHead>
                  <TableHead className="hidden text-right sm:table-cell">Other sources</TableHead>
                  <TableHead className="hidden text-right sm:table-cell">In a refusal of stellar-core</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {f.fragments.map((x) => (
                  <TableRow key={x.fragment}>
                    <TableCell className="max-w-[28rem] align-top whitespace-normal">
                      <Id>{x.fragment}</Id>
                      <span className="flex flex-col gap-1 pt-1">
                        {x.covered ? (
                          <Status tone="pass">covered</Status>
                        ) : (
                          <>
                            <Status tone="diverge">no entry from stellar-core</Status>
                            {x.uncovered ? (
                              <span className="text-xs text-muted-foreground">
                                <Id>{x.uncovered.blocker}</Id>: {x.uncovered.reason}
                              </span>
                            ) : null}
                          </>
                        )}
                      </span>
                    </TableCell>
                    <TableCell className="num text-right align-top">{x.core_emitted}</TableCell>
                    <TableCell className="num hidden text-right align-top sm:table-cell">{x.non_core}</TableCell>
                    <TableCell className="num hidden text-right align-top sm:table-cell">{x.refused_by_core}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <h3 className="pt-2 font-medium">
              Entries of this family with the tag {row.id} <span className="num text-muted-foreground">({list.length})</span>
            </h3>
            {list.length === 0 ? (
              <p className="text-sm text-muted-foreground">No entry of this family has the tag.</p>
            ) : (
              <ol className="flex flex-col divide-y border-y text-sm">
                {list.map((e) => {
                  const sdk = sdkFails.get(e.id)
                  const hx = indexerResults(harness.runs, e.id)
                  const short = e.id.startsWith(`${e.family}.`) ? e.id.slice(e.family.length + 1) : e.id
                  return (
                    <li
                      key={e.id}
                      id={e.id}
                      className="grid scroll-mt-20 gap-x-6 gap-y-1.5 py-3 target:bg-accent/60 md:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]"
                    >
                      <div className="flex min-w-0 flex-col gap-1">
                        <Link to={entryPath(e.id)} className="text-primary underline decoration-primary/40 hover:decoration-primary">
                          <Id className="font-medium" title={e.id}>
                            <span className="text-muted-foreground">{e.family}.</span>
                            {short}
                          </Id>
                        </Link>
                        <span className="text-xs text-muted-foreground">
                          {e.source}
                          {e.synthetic && e.source !== "synthetic" ? ", synthetic" : ""}, protocol{" "}
                          {e.protocol.generated_at ?? e.protocol.valid_min}, expected {e.expected.decode}
                          {e.expected.validity !== "valid" ? `, ${e.expected.validity}` : ""}
                          {e.expected.roundtrip ? ", round-trips" : ""}
                        </span>
                        <span className="flex flex-wrap gap-x-3 text-xs">
                          {e.files.map((file) => (
                            <RepoLink key={file.path} path={file.path}>
                              {file.variant} file
                            </RepoLink>
                          ))}
                          {e.linked_issues.map((u) => (
                            <ExtLink key={u} href={u}>
                              {issueLabel(u)}
                            </ExtLink>
                          ))}
                        </span>
                      </div>
                      <div className="flex flex-col gap-1 text-xs">
                        {sdk && sdk.size > 0 ? (
                          [...sdk].map(([s, vs]) => (
                            <Link
                              key={s}
                              to={sdkPath(s, e.id)}
                              className="text-diverge underline decoration-diverge/40"
                            >
                              <Status tone="diverge" wrap>
                                {sdkName(s)} diverges at {vs.length} of {versionsOf.get(s)} versions ({vs.join(", ")})
                              </Status>
                            </Link>
                          ))
                        ) : (
                          <span className="text-muted-foreground">No SDK version diverges.</span>
                        )}
                        {hx.length > 0 ? (
                          hx.map((r) =>
                            r.tone === "pass" || r.tone === "idle" ? (
                              <span key={r.target} title={verdictMeaning(r.verdict)}>
                                <Status tone={r.tone} wrap>
                                  {r.target}: {r.text}
                                </Status>
                              </span>
                            ) : (
                              <Link
                                key={r.target}
                                to={indexerHref(r.target, e.id)}
                                className="text-diverge underline decoration-diverge/40"
                              >
                                <Status tone={r.tone} wrap>
                                  {r.target}: {r.text}
                                </Status>
                              </Link>
                            ),
                          )
                        ) : (
                          <span className="text-muted-foreground">The indexers did not replay this entry.</span>
                        )}
                      </div>
                    </li>
                  )
                })}
              </ol>
            )}
          </Section>
        )
      })}
    </div>
  )
}
