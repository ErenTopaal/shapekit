import { use, useMemo } from "react"
import { Link, useParams } from "react-router"
import { load, type Entry } from "@/lib/data"
import { sdkName } from "@/lib/sdks"
import { entryPath } from "./corpus"
import { docs, expectations, sourceClasses, sources, verdictMeaning } from "@/lib/terms"
import { Badge } from "@/components/ui/badge"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Fragments } from "@/components/fragments"
import {
  ExtLink,
  Id,
  useHashScroll,
  PageHeader,
  RepoLink,
  Section,
  Status,
  Term,
  Terms,
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
  const capFindings = findings.findings.filter((f) => f.caps.includes(cap))

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
      <nav aria-label="Breadcrumb" className="pb-3 text-sm">
        <ol className="flex flex-wrap items-center gap-1.5 text-muted-foreground">
          <li>
            <TextLink to="/dashboard">Dashboard</TextLink>
          </li>
          <li aria-hidden>/</li>
          <li>
            <TextLink to="/dashboard/coverage">CAP coverage</TextLink>
          </li>
          <li aria-hidden>/</li>
          <li aria-current="page" className="font-mono">
            {row.id}
          </li>
        </ol>
      </nav>
      <PageHeader title={`${row.id}: ${row.title}`}>
        <p>
          {row.status}
          {row.protocol ? `, protocol ${row.protocol}` : ", no protocol"}.{" "}
          {row.indexer_visible
            ? "Indexer-visible: the CAP changes what an indexer reads."
            : `Not indexer-visible: ${row.not_visible_reason ?? "no reason recorded"}.`}{" "}
          <ExtLink href={row.text_url}>Read the CAP text</ExtLink>.
        </p>
      </PageHeader>

      <dl className="grid gap-x-8 gap-y-3 border-y py-4 text-sm sm:grid-cols-2 [&_dd]:[overflow-wrap:anywhere]">
        <div>
          <dt className="font-medium">Entries</dt>
          <dd className="num text-muted-foreground">
            {fmt.format(row.entries)}, of which {fmt.format(row.core_emitted_entries)} emitted by stellar-core
          </dd>
        </div>
        <div>
          <dt className="font-medium">By source</dt>
          <dd className="text-muted-foreground">
            <Counts counts={row.entries_by_source_class} />
          </dd>
        </div>
        <div>
          <dt className="font-medium">Files by network</dt>
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

      {families.length > 0 || capFindings.length > 0 ? (
        <div className="pt-4">
          <Terms>
            <Term name="source">
              Where an entry&apos;s bytes come from:{" "}
              {sourceClasses.map((c, i) => (
                <span key={c.id}>
                  <Id>{c.id}</Id> {c.meaning}
                  {i < sourceClasses.length - 1 ? "; " : "."}
                </span>
              ))}{" "}
              The first four are emitted by stellar-core and count as coverage.
            </Term>
            <Term name="fragment counts">
              Entries of any family that contain the fragment: emitted by stellar-core (only these cover it), not
              core-emitted (synthetic bytes or a refused envelope), and refusals whose result from core holds it.
            </Term>
            <Term name="fragment">
              A path into the XDR that a family&apos;s entries must contain, such as a union arm or a length (
              <Id>#one</Id>, <Id>#many</Id>, <Id>#max</Id>). Grammar: <RepoLink path={sources.signature} />.
            </Term>
            <Term name="expected">
              What an SDK must do with the entry:{" "}
              {expectations.map((x, i) => (
                <span key={x.id}>
                  <Id>{x.id}</Id> {x.meaning}
                  {i < expectations.length - 1 ? "; " : "."}
                </span>
              ))}{" "}
              <TextLink to={docs.checks}>How the runner checks it</TextLink>.
            </Term>
            <Term name="results">
              The committed SDK matrices list only versions that diverge on the entry; the indexer lines give the
              verdict of each committed indexer run, {harness.runs.map((r) => r.describe?.target ?? "").filter(Boolean).join(" and ")}{" "}
              (<TextLink to={docs.verdicts}>verdicts</TextLink>). Entries that are not on a ledger chain (synthetic,
              refused, vectors) are not replayed into an indexer.
            </Term>
          </Terms>
        </div>
      ) : null}

      {capFindings.length > 0 ? (
        <Section
          title="Findings on this CAP"
          id="findings"
          description={`${capFindings.length} of the ${findings.findings.length} findings name ${row.id}.`}
        >
          <ul className="flex flex-col divide-y border-y text-sm">
            {capFindings.map((f) => (
              <li key={f.id} className="flex flex-col gap-1 py-2.5 sm:flex-row sm:items-baseline sm:gap-4">
                <TextLink to={`/dashboard/findings#${f.id}`} className="shrink-0 font-mono sm:w-24">
                  {f.id}
                </TextLink>
                <span className="flex-1">{f.title}</span>
                <span className="shrink-0 text-muted-foreground">
                  {f.component}, {f.severity}, {f.status}
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
                {f.title}. Shape introduced in protocol {f.protocol_introduced}; valid from protocol {f.protocol_valid.min}
                {f.protocol_valid.max ? ` to ${f.protocol_valid.max}` : " on"}.
                {f.caps.length > 1 ? (
                  <>
                    {" "}
                    The family also carries{" "}
                    {f.caps
                      .filter((c) => c !== row.id)
                      .map((c, i, all) => (
                        <span key={c}>
                          <TextLink to={`/dashboard/coverage/${c}#${f.id}`}>{c}</TextLink>
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
                  {covered} of {f.fragments.length} fragments covered
                </span>
              </div>
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
              <caption className="sr-only">Fragments of {f.id} and the entries that contain each</caption>
              <TableHeader>
                <TableRow>
                  <TableHead>Fragment</TableHead>
                  <TableHead>State</TableHead>
                  <TableHead className="text-right">Core-emitted entries</TableHead>
                  <TableHead className="text-right">Not core-emitted</TableHead>
                  <TableHead className="text-right">In a core refusal</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {f.fragments.map((x) => (
                  <TableRow key={x.fragment}>
                    <TableCell className="max-w-[28rem] align-top whitespace-normal">
                      <Id>{x.fragment}</Id>
                    </TableCell>
                    <TableCell className="min-w-28 align-top whitespace-normal">
                      {x.covered ? (
                        <Status tone="pass">covered</Status>
                      ) : (
                        <span className="flex flex-col gap-1">
                          <Status tone="diverge">uncovered</Status>
                          {x.uncovered ? (
                            <span className="text-xs text-muted-foreground">
                              <Id>{x.uncovered.blocker}</Id>: {x.uncovered.reason}
                            </span>
                          ) : null}
                        </span>
                      )}
                    </TableCell>
                    <TableCell className="num text-right align-top">{x.core_emitted}</TableCell>
                    <TableCell className="num text-right align-top">{x.non_core}</TableCell>
                    <TableCell className="num text-right align-top">{x.refused_by_core}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <h3 className="pt-2 font-medium">
              Entries of this family tagged {row.id} <span className="num text-muted-foreground">({list.length})</span>
            </h3>
            {list.length === 0 ? (
              <p className="text-sm text-muted-foreground">No entry of this family carries the tag.</p>
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
                          {e.protocol.generated_at ?? e.protocol.valid_min}; {e.expected.decode}
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
                              to={`/dashboard/sdk?sdk=${s}&entry=${encodeURIComponent(e.id)}`}
                              className="text-diverge underline decoration-diverge/40"
                            >
                              {sdkName(s)} diverges at {vs.length} of {versionsOf.get(s)} versions ({vs.join(", ")})
                            </Link>
                          ))
                        ) : (
                          <span className="text-muted-foreground">No SDK version diverges</span>
                        )}
                        {hx.length > 0 ? (
                          hx.map((r) =>
                            r.tone === "pass" || r.tone === "idle" ? (
                              <span key={r.target} className="text-muted-foreground" title={verdictMeaning(r.verdict)}>
                                {r.target}: {r.text}
                              </span>
                            ) : (
                              <Link
                                key={r.target}
                                to={indexerHref(r.target, e.id)}
                                className="text-diverge underline decoration-diverge/40"
                              >
                                {r.target}: {r.text}
                              </Link>
                            ),
                          )
                        ) : (
                          <span className="text-muted-foreground">Indexers: not replayed</span>
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
