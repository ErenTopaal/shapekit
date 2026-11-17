import { use, useEffect, useMemo, useState } from "react"
import { Link, useParams, useSearchParams } from "react-router"
import { load, type Conformance, type Entry, type HarnessRun } from "@/lib/data"
import { checkResults, docs, expectations, sourceClasses, verdictMeaning } from "@/lib/terms"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import {
  ExtLink,
  Id,
  IssueRef,
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
  verdictTone,
} from "@/components/common"
import { NotFound } from "./not-found"

const sdkName: Record<string, string> = { js: "JavaScript", go: "Go", rust: "Rust" }
const failing = new Set(checkResults.filter((c) => c.id !== "unsupported").map((c) => c.id))
// the decoded XDR-JSON of a file larger than this is offered as a download, not shown inline
const INLINE_JSON_BYTES = 512 * 1024

export const entryPath = (id: string) => `/dashboard/corpus/${encodeURIComponent(id)}`

// per entry, sdk and version: the failing checks of the committed matrices
function sdkFailures(conformance: Conformance) {
  const m = new Map<string, { sdk: string; version: string; result: string; known: string | null; checks: string[] }[]>()
  for (const s of conformance.sdks)
    for (const e of s.entries)
      for (const v of s.versions) {
        const cell = e.cells[v.key]
        if (!cell) continue
        const list = m.get(e.id) ?? []
        list.push({
          sdk: s.sdk,
          version: v.version,
          result: cell.result,
          known: cell.known_issue,
          checks: cell.checks.filter((k) => failing.has(k.result)).map((k) => k.check),
        })
        m.set(e.id, list)
      }
  return m
}

function indexerVerdicts(run: HarnessRun | undefined, id: string) {
  const v = run?.verdicts[id]
  if (!v) return []
  return Object.entries(v)
    .toSorted(([a], [b]) => a.localeCompare(b))
    .map(([network, r]) => ({ network, verdict: r.verdict, reason: r.reason }))
}

const verdictText = (r: { verdict: string; reason: string | null }) => (r.reason ? `${r.verdict} (${r.reason})` : r.verdict)

function FilterSelect({
  label,
  value,
  any,
  options,
  onChange,
}: {
  label: string
  value: string
  any: string
  options: { value: string; label: string }[]
  onChange: (v: string) => void
}) {
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger className="w-full sm:w-52" aria-label={label}>
        <SelectValue placeholder={any} />
      </SelectTrigger>
      <SelectContent>
        <SelectGroup>
          <SelectItem value="all">{any}</SelectItem>
          {options.map((o) => (
            <SelectItem key={o.value} value={o.value}>
              {o.label}
            </SelectItem>
          ))}
        </SelectGroup>
      </SelectContent>
    </Select>
  )
}

function CorpusTerms() {
  return (
    <Terms>
      <Term name="shape">
        The family an entry belongs to: one shape of XDR, such as a muxed payment destination, described in the
        registry. <TextLink to={docs.addingShape}>Adding a shape</TextLink>.
      </Term>
      <Term name="source">
        Where an entry&apos;s bytes come from:{" "}
        {sourceClasses.map((c, i) => (
          <span key={c.id}>
            <Id>{c.id}</Id> {c.meaning}
            {i < sourceClasses.length - 1 ? "; " : "."}
          </span>
        ))}
      </Term>
      <Term name="failing version">
        An SDK version whose committed matrix holds a failing check for the entry (
        {checkResults
          .filter((c) => c.id !== "unsupported")
          .map((c) => c.id)
          .join(", ")}
        ). The committed matrices keep failing cells only, so an entry with no failing version passed or could not be
        answered at every version. <TextLink to={docs.checks}>How the runner checks an entry</TextLink>.
      </Term>
      <Term name="indexer">
        The verdict of the committed wallet-backend run, per network (<TextLink to={docs.verdicts}>verdicts</TextLink>).
        Entries that are not on a ledger chain (synthetic, refused, vectors) are not replayed.
      </Term>
    </Terms>
  )
}

export function CorpusPage() {
  const entries = use(load("entries"))
  const conformance = use(load("conformance"))
  const harness = use(load("harness"))
  const [params, setParams] = useSearchParams()
  const q = params.get("q") ?? ""
  const cap = params.get("cap") ?? "all"
  const family = params.get("shape") ?? "all"
  const sdk = params.get("sdk") ?? "all"
  const version = params.get("version") ?? "all"
  const set = (k: string, v: string, dflt: string) => {
    const next = new URLSearchParams(params)
    if (v === dflt) next.delete(k)
    else next.set(k, v)
    // a version belongs to one sdk
    if (k === "sdk") next.delete("version")
    setParams(next, { replace: true })
  }

  const fails = useMemo(() => sdkFailures(conformance), [conformance])
  const run = harness.runs[0]
  const caps = useMemo(() => [...new Set(entries.entries.flatMap((e) => e.caps))].toSorted(), [entries])
  const families = useMemo(() => [...new Set(entries.entries.map((e) => e.family))].toSorted(), [entries])
  const versions = conformance.sdks.find((s) => s.sdk === sdk)?.versions.map((v) => v.version) ?? []

  const shown = useMemo(() => {
    const needle = q.trim().toLowerCase()
    return entries.entries.filter((e) => {
      if (needle && !e.id.toLowerCase().includes(needle)) return false
      if (cap !== "all" && !e.caps.includes(cap)) return false
      if (family !== "all" && e.family !== family) return false
      if (sdk !== "all") {
        const f = (fails.get(e.id) ?? []).filter((x) => x.sdk === sdk)
        if (f.length === 0) return false
        if (version !== "all" && !f.some((x) => x.version === version)) return false
      }
      return true
    })
  }, [entries, fails, q, cap, family, sdk, version])

  return (
    <div className="flex flex-col">
      <PageHeader title="Corpus explorer">
        <p>
          Every corpus entry, with its decoded XDR-JSON, where its bytes come from, what each SDK version and the
          reference indexer did with it, and how to run it again. Filter by CAP, by shape, or by an SDK and the version
          that fails on it; the filters are kept in the address, so a filtered list can be linked.
        </p>
      </PageHeader>
      <CorpusTerms />
      <Section
        title="Entries"
        id="entries"
        description={`${fmt.format(shown.length)} of ${fmt.format(entries.entries.length)} entries of corpus ${entries.corpus_version}.`}
      >
        <div className="flex flex-col flex-wrap gap-3 sm:flex-row sm:items-center">
          <label className="sr-only" htmlFor="corpus-q">
            Filter by entry id
          </label>
          <Input
            id="corpus-q"
            type="search"
            placeholder="Filter by entry id"
            value={q}
            onChange={(e) => set("q", e.target.value, "")}
            className="sm:max-w-xs"
          />
          <FilterSelect
            label="Filter by CAP"
            value={cap}
            any="Any CAP"
            options={caps.map((c) => ({ value: c, label: c }))}
            onChange={(v) => set("cap", v, "all")}
          />
          <FilterSelect
            label="Filter by shape"
            value={family}
            any="Any shape"
            options={families.map((f) => ({ value: f, label: f }))}
            onChange={(v) => set("shape", v, "all")}
          />
          <FilterSelect
            label="Filter by an SDK that fails"
            value={sdk}
            any="Any SDK result"
            options={conformance.sdks.map((s) => ({ value: s.sdk, label: `${sdkName[s.sdk] ?? s.sdk} fails` }))}
            onChange={(v) => set("sdk", v, "all")}
          />
          {sdk !== "all" ? (
            <FilterSelect
              label="Filter by failing version"
              value={version}
              any="At any version"
              options={versions.map((v) => ({ value: v, label: `at ${v}` }))}
              onChange={(v) => set("version", v, "all")}
            />
          ) : null}
        </div>
        {shown.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyTitle>No entry matches</EmptyTitle>
              <EmptyDescription>Clear a filter to see more entries.</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <Table>
            <caption className="sr-only">Corpus entries with their CAPs, source and results</caption>
            <TableHeader>
              <TableRow>
                <TableHead>Entry</TableHead>
                <TableHead>CAPs</TableHead>
                <TableHead>Source</TableHead>
                <TableHead>Failing SDK versions</TableHead>
                <TableHead>Indexer</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {shown.map((e) => {
                const f = fails.get(e.id) ?? []
                const hx = indexerVerdicts(run, e.id)
                const short = e.id.startsWith(`${e.family}.`) ? e.id.slice(e.family.length + 1) : e.id
                return (
                  <TableRow key={e.id}>
                    <TableCell className="max-w-[26rem] align-top whitespace-normal">
                      <Link to={entryPath(e.id)} className="text-primary underline decoration-primary/40 hover:decoration-primary">
                        <Id title={e.id}>
                          <span className="text-muted-foreground">{e.family}.</span>
                          {short}
                        </Id>
                      </Link>
                    </TableCell>
                    <TableCell className="align-top text-xs whitespace-normal">{e.caps.join(", ") || "none"}</TableCell>
                    <TableCell className="align-top text-xs">{e.source}</TableCell>
                    <TableCell className="align-top text-xs whitespace-normal">
                      {f.length === 0 ? (
                        <span className="text-muted-foreground">none</span>
                      ) : (
                        <span className="text-diverge">{f.map((x) => `${x.sdk}@${x.version}`).join(", ")}</span>
                      )}
                    </TableCell>
                    <TableCell className="align-top text-xs whitespace-normal">
                      {hx.length === 0 ? (
                        <span className="text-muted-foreground">not replayed</span>
                      ) : (
                        hx.map((r) => (
                          <span key={r.network} className="block" title={verdictMeaning(r.verdict)}>
                            <Status tone={verdictTone(r.verdict)}>
                              {r.network}: {verdictText(r)}
                            </Status>
                          </span>
                        ))
                      )}
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        )}
      </Section>
    </div>
  )
}

// fetches one file's decoded XDR-JSON from the site's own copy of the corpus
function DecodedJson({ path, size }: { path: string; size: number }) {
  const [text, setText] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const inline = size <= INLINE_JSON_BYTES
  useEffect(() => {
    if (!inline) return
    let live = true
    fetch(`/${path}`)
      .then((r) => {
        if (!r.ok) throw new Error(`${path}: HTTP ${r.status}`)
        return r.text()
      })
      .then((t) => live && setText(t))
      .catch((e: unknown) => live && setError(e instanceof Error ? e.message : String(e)))
    return () => {
      live = false
    }
  }, [path, inline])
  if (!inline)
    return (
      <p className="text-sm text-muted-foreground">
        The XDR-JSON is {bytes(size)}, too large to show here.{" "}
        <a href={`/${path}`} download className="text-primary underline decoration-primary/40">
          Download it
        </a>
        .
      </p>
    )
  if (error) return <p className="text-sm text-diverge">{error}</p>
  if (text === null) return <p className="text-sm text-muted-foreground">Loading the XDR-JSON.</p>
  return (
    <pre className="max-h-[32rem] overflow-auto rounded-md border bg-muted/40 p-3 font-mono text-xs leading-relaxed">
      {text}
    </pre>
  )
}

function Command({ children }: { children: string }) {
  return <pre className="overflow-x-auto rounded-md border bg-muted/40 px-3 py-2 font-mono text-xs whitespace-pre-wrap [overflow-wrap:anywhere]">{children}</pre>
}

const quote = (s: string) => (/^[A-Za-z0-9_./=:@,+%-]+$/.test(s) ? s : `'${s.replaceAll("'", "'\\''")}'`)

export function EntryPage() {
  const { entry = "" } = useParams()
  const entries = use(load("entries"))
  const conformance = use(load("conformance"))
  const harness = use(load("harness"))
  const findings = use(load("findings"))
  const coverage = use(load("coverage"))
  const fails = useMemo(() => sdkFailures(conformance), [conformance])

  const e: Entry | undefined = entries.entries.find((x) => x.id === entry)
  if (!e) return <NotFound />

  const f = fails.get(e.id) ?? []
  const hx = indexerVerdicts(harness.runs[0], e.id)
  const onEntry = findings.findings.filter((x) => x.entries.includes(e.id))
  const fieldRows = e.fields.map((id) => entries.guide.fields.find((r) => r.id === id) ?? { id, validated_by_core: "not in the field table", risk: "", handling: "" })
  const checklist = entries.guide.checklist.filter((c) => c.fields.some((x) => e.fields.includes(x)))
  const capRows = coverage.caps.filter((c) => e.caps.includes(c.id))
  const expectation = expectations.find((x) => x.id === e.expected.decode)

  return (
    <div className="flex flex-col">
      <nav aria-label="Breadcrumb" className="pb-3 text-sm">
        <ol className="flex flex-wrap items-center gap-1.5 text-muted-foreground">
          <li>
            <TextLink to="/dashboard">Dashboard</TextLink>
          </li>
          <li aria-hidden>/</li>
          <li>
            <TextLink to="/dashboard/corpus">Corpus explorer</TextLink>
          </li>
          <li aria-hidden>/</li>
          <li aria-current="page" className="font-mono [overflow-wrap:anywhere]">
            {e.id}
          </li>
        </ol>
      </nav>
      <PageHeader title={e.id}>
        {e.notes ? <p>{e.notes}</p> : null}
      </PageHeader>

      <dl className="grid gap-x-8 gap-y-3 border-y py-4 text-sm sm:grid-cols-2 [&_dd]:[overflow-wrap:anywhere]">
        <div>
          <dt className="font-medium">Shape</dt>
          <dd className="text-muted-foreground">
            <TextLink to={`/dashboard/corpus?shape=${encodeURIComponent(e.family)}`}>
              <Id>{e.family}</Id>
            </TextLink>
            , {e.kind}
            {e.xdr_type ? (
              <>
                , <Id>{e.xdr_type}</Id>
              </>
            ) : null}
          </dd>
        </div>
        <div>
          <dt className="font-medium">CAPs</dt>
          <dd className="text-muted-foreground">
            {capRows.length === 0
              ? "none"
              : capRows.map((c, i) => (
                  <span key={c.id}>
                    <TextLink to={`/dashboard/coverage/${c.id}#${e.family}`}>{c.id}</TextLink> {c.title}
                    {i < capRows.length - 1 ? "; " : ""}
                  </span>
                ))}
          </dd>
        </div>
        <div>
          <dt className="font-medium">Protocol</dt>
          <dd className="num text-muted-foreground">
            introduced {e.protocol.introduced}, valid from {e.protocol.valid_min}
            {e.protocol.valid_max ? ` to ${e.protocol.valid_max}` : ""}
            {e.protocol.generated_at ? `, recorded at ${e.protocol.generated_at}` : ""}
          </dd>
        </div>
        <div>
          <dt className="font-medium">Expected</dt>
          <dd className="text-muted-foreground">
            <Id>{e.expected.decode}</Id>, {e.expected.validity}
            {e.expected.roundtrip ? ", round-trips" : ""}
            {expectation ? `: ${expectation.meaning}` : ""}
          </dd>
        </div>
        <div>
          <dt className="font-medium">Source</dt>
          <dd className="text-muted-foreground">
            <Id>{e.source}</Id>
            {e.synthetic && e.source !== "synthetic" ? ", synthetic" : ""}
            {e.ledger_seq !== null ? `, ledger ${e.ledger_seq}` : ""}
            {e.provenance_url ? (
              <>
                , <ExtLink href={e.provenance_url}>provenance</ExtLink>
              </>
            ) : null}
          </dd>
        </div>
        <div>
          <dt className="font-medium">Recorded by</dt>
          <dd className="text-muted-foreground">
            {e.recipe_ir ? (
              <>
                recipe <RepoLink path={e.recipe_ir} />
                {e.chain ? (
                  <>
                    {" "}
                    on chain <Id>{e.chain}</Id>
                  </>
                ) : null}
              </>
            ) : e.chain ? (
              <>
                chain <Id>{e.chain}</Id> (<Id>shapekit gen chains --family {e.family}</Id>)
              </>
            ) : (
              "not recorded on a local core"
            )}
          </dd>
        </div>
        {e.flags.length > 0 ? (
          <div>
            <dt className="font-medium">Flags</dt>
            <dd className="text-muted-foreground">{e.flags.join(", ")}</dd>
          </div>
        ) : null}
        {e.linked_issues.length > 0 ? (
          <div>
            <dt className="font-medium">Upstream sightings</dt>
            <dd className="text-muted-foreground">
              {e.linked_issues.map((u, i) => (
                <span key={u}>
                  <ExtLink href={u}>{issueLabel(u)}</ExtLink>
                  {i < e.linked_issues.length - 1 ? ", " : ""}
                </span>
              ))}
            </dd>
          </div>
        ) : null}
      </dl>

      <Section title="Results" id="results">
        <h3 className="font-medium">SDKs</h3>
        {f.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No SDK version in the committed matrices has a failing check on this entry.
          </p>
        ) : (
          <Table>
            <caption className="sr-only">Failing SDK versions on this entry</caption>
            <TableHeader>
              <TableRow>
                <TableHead>SDK version</TableHead>
                <TableHead>Result</TableHead>
                <TableHead>Failing checks</TableHead>
                <TableHead>Known failure</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {f.map((x) => (
                <TableRow key={`${x.sdk}@${x.version}`}>
                  <TableCell className="align-top">
                    <Link
                      to={`/dashboard/sdk?sdk=${x.sdk}&entry=${encodeURIComponent(e.id)}`}
                      className="text-primary underline decoration-primary/40"
                    >
                      {sdkName[x.sdk] ?? x.sdk} {x.version}
                    </Link>
                  </TableCell>
                  <TableCell className="align-top">
                    <Status tone="diverge">{x.result}</Status>
                  </TableCell>
                  <TableCell className="align-top text-xs whitespace-normal">
                    {x.checks.map((c) => (
                      <Id key={c} className="mr-2 inline-block">
                        {c}
                      </Id>
                    ))}
                  </TableCell>
                  <TableCell className="align-top text-xs whitespace-normal">
                    {x.known ? <IssueRef value={x.known} /> : <span className="text-diverge">not listed</span>}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <h3 className="pt-2 font-medium">Indexer</h3>
        {hx.length === 0 ? (
          <p className="text-sm text-muted-foreground">The committed indexer run did not replay this entry.</p>
        ) : (
          <ul className="flex flex-col gap-1 text-sm">
            {hx.map((r) => (
              <li key={r.network}>
                <Status tone={verdictTone(r.verdict)}>
                  {harness.runs[0]?.describe?.target ?? "indexer"} on {r.network}: {verdictText(r)}
                </Status>{" "}
                <span className="text-muted-foreground">{verdictMeaning(r.verdict)}.</span>{" "}
                {verdictTone(r.verdict) === "diverge" ? (
                  <TextLink to={`/dashboard/indexer?q=${encodeURIComponent(e.id)}&network=both#problems`}>
                    The problems
                  </TextLink>
                ) : null}
              </li>
            ))}
          </ul>
        )}
        {onEntry.length > 0 ? (
          <>
            <h3 className="pt-2 font-medium">Findings</h3>
            <ul className="flex flex-col divide-y border-y text-sm">
              {onEntry.map((x) => (
                <li key={x.id} className="flex flex-col gap-1 py-2 sm:flex-row sm:items-baseline sm:gap-4">
                  <TextLink to={`/dashboard/findings#${x.id}`} className="shrink-0 font-mono sm:w-24">
                    {x.id}
                  </TextLink>
                  <span className="flex-1">{x.title}</span>
                  <span className="shrink-0 text-muted-foreground">
                    {x.severity}, {x.status}
                    {x.link ? (
                      <>
                        , <ExtLink href={x.link}>{issueLabel(x.link)}</ExtLink>
                      </>
                    ) : null}
                  </span>
                </li>
              ))}
            </ul>
          </>
        ) : null}
      </Section>

      <Section
        title="Run it again"
        id="reproduce"
        description="From a shapekit checkout. The SDK command runs the current version unless you add --version; the harness command replays the entry's whole chain into the indexer behind your adapter."
      >
        {conformance.sdks.map((s) => {
          const failingVersions = f.filter((x) => x.sdk === s.sdk).map((x) => x.version)
          const v = failingVersions.at(-1)
          return (
            <Command key={s.sdk}>
              {`shapekit check sdk --sdk ${s.sdk}${v ? ` --version ${quote(v)}` : ""} --entry ${quote(e.id)}`}
            </Command>
          )
        })}
        {e.chain ? (
          <Command>{`shapekit check indexer --chain ${quote(e.chain)} --network ${e.files.some((x) => x.variant === "testnet") ? "testnet" : "mainnet"} --adapter "<your adapter command>"`}</Command>
        ) : (
          <p className="text-sm text-muted-foreground">The entry is not on a ledger chain, so the harness cannot replay it.</p>
        )}
      </Section>

      <Section title="Files and decoded XDR-JSON" id="files">
        {e.files.map((x) => (
          <div key={x.path} className="flex flex-col gap-2">
            <h3 className="font-medium">
              {x.variant === "any" ? "Network-independent" : x.variant === "vector" ? "Vector" : x.variant}{" "}
              <span className="text-sm font-normal text-muted-foreground">
                <a href={`/${x.path}`} download className="text-primary underline decoration-primary/40">
                  download {x.path.split("/").at(-1)}
                </a>{" "}
                ({bytes(x.bytes)}),{" "}
                <RepoLink path={x.path}>on GitHub</RepoLink>
                {x.expected ? (
                  <>
                    ,{" "}
                    <a href={`/${x.expected}`} download className="text-primary underline decoration-primary/40">
                      download the XDR-JSON
                    </a>
                  </>
                ) : null}
              </span>
            </h3>
            {x.expected ? (
              <DecodedJson path={x.expected} size={x.expected_bytes ?? 0} />
            ) : x.variant === "vector" ? (
              <DecodedJson path={x.path} size={x.bytes} />
            ) : (
              <p className="text-sm text-muted-foreground">This file has no XDR-JSON rendering.</p>
            )}
          </div>
        ))}
      </Section>

      <Section
        title="In the guide"
        id="guide"
        description="The fields this entry exercises, their rows in the indexer-hardening guide's field table, and the checklist items that name them."
      >
        {e.fields.length === 0 ? (
          <p className="text-sm text-muted-foreground">The entry names no field of the guide&apos;s field table.</p>
        ) : (
          <>
            <Table>
              <caption className="sr-only">Fields this entry exercises</caption>
              <TableHeader>
                <TableRow>
                  <TableHead>Field</TableHead>
                  <TableHead>Validated by core</TableHead>
                  <TableHead>Risk</TableHead>
                  <TableHead>Handling</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {fieldRows.map((r) => (
                  <TableRow key={r.id}>
                    <TableCell className="align-top">
                      <Id>{r.id}</Id>
                    </TableCell>
                    <TableCell className="align-top">{r.validated_by_core}</TableCell>
                    <TableCell className="max-w-[24rem] align-top text-xs whitespace-normal">{r.risk}</TableCell>
                    <TableCell className="max-w-[24rem] align-top text-xs whitespace-normal">{r.handling}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <ul className="flex list-disc flex-col gap-1 pl-5 text-sm">
              <li>
                <TextLink to={`${docs.guide}#validated-and-unvalidated-fields`}>Validated and unvalidated fields</TextLink>
                : what core checks for each field.
              </li>
              <li>
                <TextLink to={`${docs.guide}#corpus-entries-by-field`}>Corpus entries by field</TextLink>: the entries
                listed for these fields.
              </li>
              {checklist.map((c) => (
                <li key={c.id}>
                  <TextLink to={`${docs.guide}#self-audit-checklist`}>Self-audit checklist</TextLink>: {c.title}
                </li>
              ))}
            </ul>
          </>
        )}
      </Section>
    </div>
  )
}
