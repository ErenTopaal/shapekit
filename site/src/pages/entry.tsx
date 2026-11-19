import { use, useEffect, useMemo, useState } from "react"
import { Link, useParams } from "react-router"
import { load, type Entry } from "@/lib/data"
import { bySdkOrder, sdkName } from "@/lib/sdks"
import { docs, expectations, label, verdictMeaning } from "@/lib/terms"
import { findingRows } from "@/lib/metrics"
import { indexerVerdicts, sdkFailures, verdictText } from "@/lib/results"
import { capPath, entriesPath, findingPath, sdkPath } from "@/lib/paths"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import {
  Breadcrumb,
  Command,
  ExtLink,
  GlossaryLink,
  Id,
  IssueRef,
  NextSteps,
  PageHeader,
  RepoLink,
  Section,
  Status,
  TextLink,
  bytes,
  indexerHref,
  issueLabel,
  verdictTone,
} from "@/components/common"
import { NotFound } from "./not-found"

// the decoded XDR-JSON of a file larger than this is offered as a download, not shown inline
const INLINE_JSON_BYTES = 512 * 1024

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
  const hx = indexerVerdicts(harness.runs, e.id)
  const onEntry = findingRows(findings).filter((x) => x.entries.includes(e.id))
  const fieldRows = e.fields.map((id) => entries.guide.fields.find((r) => r.id === id) ?? { id, validated_by_core: "not in the field table", risk: "", handling: "" })
  const checklist = entries.guide.checklist.filter((c) => c.fields.some((x) => e.fields.includes(x)))
  const capRows = coverage.caps.filter((c) => e.caps.includes(c.id))
  const expectation = expectations.find((x) => x.id === e.expected.decode)

  return (
    <div className="flex flex-col">
      <Breadcrumb
        items={[
          { to: "/dashboard/corpus", label: "Corpus" },
          { to: "/dashboard/corpus/entries", label: "Entries" },
          { label: e.id, mono: true },
        ]}
      />
      <PageHeader title={e.id}>
        {e.notes ? <p>{e.notes}</p> : null}
      </PageHeader>

      <div className="pb-4">
        <NextSteps>
          <li>
            <a href="#reproduce" className="text-primary underline decoration-primary/40 hover:decoration-primary">
              Run this entry again
            </a>{" "}
            in your SDK or your indexer with the commands below.
          </li>
          {onEntry.length > 0 ? (
            <li>
              <a href="#results" className="text-primary underline decoration-primary/40 hover:decoration-primary">
                Read the findings on this entry
              </a>{" "}
              and open their upstream issues.
            </li>
          ) : null}
        </NextSteps>
      </div>

      <dl className="grid gap-x-8 gap-y-3 border-y py-4 text-sm sm:grid-cols-2 [&_dd]:[overflow-wrap:anywhere]">
        <div>
          <dt className="font-medium">
            <GlossaryLink id="family">Shape family</GlossaryLink>
          </dt>
          <dd className="text-muted-foreground">
            <TextLink to={entriesPath({ shape: e.family })}>
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
                    <TextLink to={capPath(c.id, e.family)}>{c.id}</TextLink> {c.title}
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
          <dt className="font-medium">Expected answer</dt>
          <dd className="text-muted-foreground">
            <TextLink to={`/dashboard/glossary#expected-${e.expected.decode}`}>
              <Id>{e.expected.decode}</Id>
            </TextLink>, {e.expected.validity}
            {e.expected.roundtrip ? ", round-trips" : ""}
            {expectation ? `: ${expectation.meaning}` : ""}
          </dd>
        </div>
        <div>
          <dt className="font-medium">Source</dt>
          <dd className="text-muted-foreground">
            <TextLink to={`/dashboard/glossary#source-${e.source}`}>
              <Id>{e.source}</Id>
            </TextLink>
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
            No SDK version in the committed matrices has a failing check on this entry. The matrices keep failing cells
            only, so this page cannot show a pass apart from an unsupported answer.
          </p>
        ) : (
          <Table>
            <caption className="sr-only">Failing SDK versions on this entry</caption>
            <TableHeader>
              <TableRow>
                <TableHead>SDK version</TableHead>
                <TableHead>Result</TableHead>
                <TableHead className="hidden sm:table-cell">Failing checks</TableHead>
                <TableHead>
                  <GlossaryLink id="known-failure">Known failure</GlossaryLink>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {f.map((x) => (
                <TableRow key={`${x.sdk}@${x.version}`}>
                  <TableCell className="align-top">
                    <Link to={sdkPath(x.sdk, e.id)} className="text-primary underline decoration-primary/40">
                      {sdkName(x.sdk)} {x.version}
                    </Link>
                  </TableCell>
                  <TableCell className="align-top">
                    <Status tone="diverge" wrap>
                      {label(x.result)}
                    </Status>
                  </TableCell>
                  <TableCell className="hidden align-top text-xs whitespace-normal sm:table-cell">
                    {x.checks.map((c) => (
                      <Id key={c} className="mr-2 inline-block">
                        {c}
                      </Id>
                    ))}
                  </TableCell>
                  <TableCell className="align-top text-xs whitespace-normal">
                    {x.known ? <IssueRef value={x.known} /> : <Status tone="diverge">not listed</Status>}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <h3 className="pt-2 font-medium">Indexer</h3>
        {hx.length === 0 ? (
          <p className="text-sm text-muted-foreground">No committed indexer run replayed this entry.</p>
        ) : (
          <ul className="flex flex-col gap-1 text-sm">
            {hx.map((r) => (
              <li key={`${r.target}/${r.network}`}>
                <Status tone={verdictTone(r.verdict)}>
                  {r.target} on {r.network}: {verdictText(r)}
                </Status>{" "}
                <span className="text-muted-foreground">{verdictMeaning(r.verdict)}.</span>{" "}
                {verdictTone(r.verdict) === "diverge" ? (
                  <TextLink to={indexerHref(r.target, e.id)}>The facts that differ</TextLink>
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
                  <TextLink to={findingPath(x.id)} className="shrink-0 font-mono sm:w-32">
                    {x.id}
                  </TextLink>
                  <span className="flex-1">{x.short ?? x.title}</span>
                  <span className="shrink-0 text-muted-foreground">
                    {label(x.severity)}, {label(x.status)}
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
        description="Run these commands in a shapekit checkout. The SDK command runs the current version when you do not give --version. The indexer command replays the whole chain of the entry into the indexer behind your adapter."
      >
        {bySdkOrder(conformance.sdks, (s) => s.sdk).map((s) => {
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
        description="The fields that this entry contains, their rows in the field table of the indexer-hardening guide, and the checklist items that name them."
      >
        {e.fields.length === 0 ? (
          <p className="text-sm text-muted-foreground">The entry names no field of the field table of the guide.</p>
        ) : (
          <>
            <Table>
              <caption className="sr-only">Fields that this entry contains</caption>
              <TableHeader>
                <TableRow>
                  <TableHead>Field</TableHead>
                  <TableHead>Checked by stellar-core</TableHead>
                  <TableHead className="hidden md:table-cell">Risk</TableHead>
                  <TableHead className="hidden md:table-cell">Handling</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {fieldRows.map((r) => (
                  <TableRow key={r.id}>
                    <TableCell className="align-top">
                      <Id>{r.id}</Id>
                    </TableCell>
                    <TableCell className="align-top">{r.validated_by_core}</TableCell>
                    <TableCell className="hidden max-w-[24rem] align-top text-xs whitespace-normal md:table-cell">{r.risk}</TableCell>
                    <TableCell className="hidden max-w-[24rem] align-top text-xs whitespace-normal md:table-cell">{r.handling}</TableCell>
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
