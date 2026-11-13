import { use, type ReactNode } from "react"
import { Link } from "react-router"
import { ChevronDownIcon } from "lucide-react"
import { load, sum } from "@/lib/data"
import { useMeta } from "@/lib/meta"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Button } from "@/components/ui/button"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { FamilyStrip } from "@/components/fragments"
import { Id, RepoLink, Section, fmt, short } from "@/components/common"

// pages of this site's docs first, then committed files that only exist in the repository
const docs: { to?: string; path?: string; label: string }[] = [
  { to: "/docs/install", label: "Install" },
  { to: "/docs/quick-start", label: "Quick start" },
  { to: "/docs/corpus/adding-a-shape", label: "Adding a shape" },
  { to: "/docs/sdk/testee-protocol", label: "Testee protocol for SDKs" },
  { to: "/docs/sdk/sdk-ci", label: "Running the corpus in an SDK's CI" },
  { to: "/docs/harness/adapter-interface", label: "Indexer adapter interface" },
  { to: "/docs/linter/indexerlint", label: "indexerlint" },
  { to: "/docs/guide/indexer-hardening", label: "Indexer-hardening guide (draft)" },
  { to: "/docs/operations/infrastructure", label: "CI and infrastructure" },
  { to: "/docs/operations/site", label: "How this site's data is made" },
  { path: "STATUS.md", label: "Status log, in the repository" },
  { path: "corpus/coverage/COVERAGE.md", label: "Coverage per CAP, as committed" },
  { path: "findings/FINDINGS.md", label: "Findings, as committed" },
]

function Line({ label, to, link, children }: { label: string; to: string; link: string; children: ReactNode }) {
  return (
    <div className="grid gap-1 border-t py-3 sm:grid-cols-[11rem_1fr_auto] sm:items-baseline sm:gap-4">
      <dt className="font-medium">{label}</dt>
      <dd className="num text-muted-foreground [&_b]:font-semibold [&_b]:text-foreground">{children}</dd>
      <dd>
        <Link to={to} className="text-sm text-primary underline decoration-primary/40 hover:decoration-primary">
          {link}
        </Link>
      </dd>
    </div>
  )
}

export function Overview() {
  const meta = useMeta()
  const coverage = use(load("coverage"))
  const conformance = use(load("conformance"))
  const harness = use(load("harness"))
  const findings = use(load("findings"))
  const s = coverage.summary
  const run = harness.runs[0]
  const versions = sum(Object.fromEntries(conformance.sdks.map((x) => [x.sdk, x.versions.length])))
  const current = conformance.sdks.map((x) => {
    const v = x.versions.find((y) => y.current)
    return { sdk: x.sdk, version: x.current, failing: sum(v?.failing_cells_by_result) }
  })
  const gw = meta.corpus.generated_with
  const families = coverage.families.toSorted((a, b) => a.priority - b.priority || a.id.localeCompare(b.id))

  return (
    <div className="flex flex-col">
      <header className="flex flex-col gap-4 pb-8">
        <h1 className="max-w-[24ch] text-3xl font-semibold tracking-tight text-balance sm:text-4xl">
          Rare but valid Stellar XDR, and how SDKs and indexers read it
        </h1>
        <p className="max-w-[68ch] text-muted-foreground">
          shapekit is a test kit for software that reads the Stellar ledger. It holds a corpus of XDR shapes
          produced by a real stellar-core, a conformance runner that feeds them to the JavaScript, Go and Rust SDKs,
          and a replay harness that feeds them to an indexer. This site shows what the committed results say. It
          is built against stellar-core <Id>{gw.core_image.tag.split("-")[0]}</Id> and stellar-xdr{" "}
          <Id>{gw.xdr_definitions.tag}</Id>.
        </p>
      </header>

      <dl className="border-b">
        <Line label="Corpus" to="/dashboard/coverage" link="Coverage by CAP">
          <b>{fmt.format(s.entries)}</b> entries in <b>{s.families}</b> shape families;{" "}
          <b>{fmt.format(s.core_emitted_entries)}</b> of them emitted by stellar-core, the rest synthetic or
          refused by core. Protocols {meta.corpus.protocol_tiers[0]} to{" "}
          {meta.corpus.protocol_tiers[meta.corpus.protocol_tiers.length - 1]}.
        </Line>
        <Line label="CAP coverage" to="/dashboard/coverage#arms" link="Union arms and fields">
          <b>{s.caps_indexer_visible_core_emitted}</b> of <b>{s.caps_indexer_visible}</b> indexer-visible CAPs
          have core-emitted entries
          {s.caps_indexer_visible_with_reason > 0
            ? `, ${s.caps_indexer_visible_with_reason} ${s.caps_indexer_visible_with_reason === 1 ? "has" : "have"} a recorded reason instead`
            : ""}
          .{" "}
          <b>{s.arms_covered}</b> of <b>{s.arms_reachable}</b> reachable union arms of {coverage.root_type} are
          covered; <b>{s.fragments_covered}</b> of <b>{s.fragments}</b> family fragments.
        </Line>
        <Line label="SDK matrix" to="/dashboard/sdk" link="Open the matrix">
          {conformance.sdks.length} SDKs, {versions} versions. Failing cells at the current version:{" "}
          {current.map((c, i) => (
            <span key={c.sdk}>
              {c.sdk} <Id>{c.version}</Id> <b>{c.failing}</b>
              {i < current.length - 1 ? ", " : "."}
            </span>
          ))}{" "}
          {conformance.reproductions.length} upstream sightings tracked as reproduction rows.
        </Line>
        {run ? (
          <Line label="Indexer run" to="/dashboard/indexer" link="Open the run">
            {run.describe?.target ?? "target"} <Id>{short(run.describe?.target_version ?? "", 8)}</Id>:{" "}
            <b>{fmt.format(run.totals.entries)}</b> entry replays over {run.chains} chains,{" "}
            <b>{run.totals.by_verdict.pass ?? 0}</b> pass, <b>{run.totals.by_verdict.mishandled ?? 0}</b>{" "}
            mishandled, <b>{run.totals.by_verdict.dropped ?? 0}</b> dropped,{" "}
            {run.totals.by_verdict["out-of-scope"] ?? 0} out of scope.
          </Line>
        ) : null}
        <Line label="Findings" to="/dashboard/findings" link="Open the findings">
          <b>{findings.summary.findings}</b> triaged root causes for <b>{fmt.format(findings.summary.failures)}</b>{" "}
          failures, {findings.summary.unmapped} unmapped.{" "}
          {Object.entries(findings.summary.by_status)
            .map(([k, v]) => `${v} ${k}`)
            .join(", ")}
          .
        </Line>
      </dl>

      <Section
        title="Shape families"
        id="families"
        description="Each square is one fragment of a family's signature: filled when a core-emitted entry matches it, outlined when none does. Ordered by the registry's priority, highest demonstrated damage first."
      >
        <FamilyStrip families={families} />
      </Section>

      <div className="grid gap-x-10 md:grid-cols-2">
        <Section title="Documentation" id="docs">
          <ul className="flex flex-col gap-1.5 text-sm">
            {docs.map((d) => (
              <li key={d.to ?? d.path}>
                {d.to ? (
                  <Link to={d.to} className="text-primary underline decoration-primary/40 hover:decoration-primary">
                    {d.label}
                  </Link>
                ) : (
                  <RepoLink path={d.path ?? ""}>{d.label}</RepoLink>
                )}
              </li>
            ))}
          </ul>
        </Section>
        <Section title="Pinned versions" id="pins" description="From pins.toml and conformance/versions.toml.">
          <Table>
            <TableBody>
              {meta.pins.git
                .filter((g) => ["stellar-core", "stellar-xdr", "stellar-rpc", "wallet-backend"].includes(g.name))
                .map((g) => (
                  <TableRow key={g.name}>
                    <TableCell className="font-medium">{g.name}</TableCell>
                    <TableCell>
                      <Id>{g.ref ? g.ref.replace(/^refs\/tags\//, "") : short(g.sha ?? "", 10)}</Id>
                    </TableCell>
                  </TableRow>
                ))}
              {conformance.sdks.map((x) => (
                <TableRow key={x.sdk}>
                  <TableCell className="font-medium">{x.package}</TableCell>
                  <TableCell>
                    <Id>{x.current}</Id>
                  </TableCell>
                </TableRow>
              ))}
              {meta.pins.toolchains.map((t) => (
                <TableRow key={t.name}>
                  <TableCell className="font-medium">{t.name} toolchain</TableCell>
                  <TableCell>
                    <Id>{t.version}</Id>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Section>
      </div>

      <Section
        title="Where these numbers come from"
        id="sources"
        description={
          <>
            <Id>shapekit site data</Id> read these committed files at commit <Id>{short(meta.commit, 10)}</Id>. Each
            sha256 is of the file as read.
          </>
        }
      >
        <Collapsible>
          <CollapsibleTrigger asChild>
            <Button variant="outline" size="sm" className="w-fit">
              Show {meta.sources.length} source files
              <ChevronDownIcon data-icon="inline-end" />
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <Table className="mt-3">
              <TableHeader>
                <TableRow>
                  <TableHead>File</TableHead>
                  <TableHead className="text-right">Bytes</TableHead>
                  <TableHead>sha256</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {meta.sources.map((f) => (
                  <TableRow key={f.path}>
                    <TableCell>
                      <RepoLink path={f.path} />
                    </TableCell>
                    <TableCell className="num text-right">{fmt.format(f.bytes)}</TableCell>
                    <TableCell>
                      <Id className="text-muted-foreground">{f.sha256}</Id>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CollapsibleContent>
        </Collapsible>
      </Section>
    </div>
  )
}
