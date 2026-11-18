import { use, type ReactNode } from "react"
import { Link } from "react-router"
import { ChevronDownIcon } from "lucide-react"
import { load, sum, type Counts } from "@/lib/data"
import { useMeta } from "@/lib/meta"
import { and, bySdkOrder, sdkLabel, sdkList, sdkName } from "@/lib/sdks"
import type { HarnessRun } from "@/lib/data"
import { severities, severityRank, sourceClasses, statuses, docs as docPages, sources } from "@/lib/terms"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Button } from "@/components/ui/button"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { FamilyStrip } from "@/components/fragments"
import { Meter, VerdictBar, VerdictLegend, VersionColumns } from "@/components/charts"
import { Id, RepoLink, Section, Term, Terms, TextLink, fmt, indexerHref, issueLabel, short } from "@/components/common"


// pages of this site's docs first, then committed files that only exist in the repository
const docs: { to?: string; path?: string; label: string }[] = [
  { to: "/docs", label: "What shapekit is" },
  { to: "/docs/quick-start", label: "Quick start" },
  { to: docPages.addingShape, label: "Adding a shape to the corpus" },
  { to: docPages.testee, label: "Testee protocol for SDKs" },
  { to: docPages.sdkCi, label: "Running the corpus in an SDK's CI" },
  { to: docPages.adapter, label: "Indexer adapter interface" },
  { to: docPages.linter, label: "indexerlint, the Go indexer linter" },
  { to: docPages.guide, label: "Indexer-hardening guide (draft)" },
  { to: docPages.infrastructure, label: "CI and infrastructure" },
  { to: docPages.site, label: "How this site's data is made" },
  { path: "STATUS.md", label: "Status log, in the repository" },
  { path: sources.coverageReport, label: "Coverage per CAP, as committed" },
  { path: sources.findingsReport, label: "Findings, as committed" },
]

function Tile({ value, label, to, children }: { value: ReactNode; label: string; to: string; children: ReactNode }) {
  return (
    <Link
      to={to}
      className="group grid grid-cols-[4.5rem_minmax(0,1fr)] gap-x-3 gap-y-0.5 rounded-lg border bg-card p-3 transition-colors sm:flex sm:flex-col sm:gap-1 sm:p-4 hover:border-primary/50 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
    >
      <span className="num row-span-2 text-3xl font-semibold tracking-tight">{value}</span>
      <span className="font-medium underline decoration-foreground/25 group-hover:decoration-foreground">{label}</span>
      <span className="text-sm text-muted-foreground">{children}</span>
    </Link>
  )
}

function findingsHref(q: Record<string, string>) {
  return `/dashboard/findings?${new URLSearchParams(q).toString()}`
}

// component by severity, the table FINDINGS.md opens with; every count opens the findings it counts
function FindingsGrid({ byComponent }: { byComponent: Record<string, Counts> }) {
  const present = severities.filter((s) => Object.values(byComponent).some((c) => (c[s.id] ?? 0) > 0))
  const components = Object.entries(byComponent).toSorted(
    ([a, x], [b, y]) =>
      Math.min(...Object.keys(x).map(severityRank)) - Math.min(...Object.keys(y).map(severityRank)) ||
      sum(y) - sum(x) ||
      a.localeCompare(b),
  )
  return (
    <Table>
      <caption className="sr-only">Findings by affected component and severity</caption>
      <TableHeader>
        <TableRow>
          <TableHead>Component</TableHead>
          {present.map((s) => (
            <TableHead key={s.id} className="text-right" title={s.meaning}>
              {s.id}
            </TableHead>
          ))}
          <TableHead className="text-right">All</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody className="num">
        {components.map(([c, counts]) => (
          <TableRow key={c}>
            <TableCell>
              <TextLink to={findingsHref({ component: c })} className="font-mono text-[0.85em]">
                {c}
              </TextLink>
            </TableCell>
            {present.map((s) => (
              <TableCell key={s.id} className="text-right">
                {counts[s.id] ? (
                  <TextLink to={findingsHref({ component: c, severity: s.id })} aria-label={`${counts[s.id]} ${s.id} findings in ${c}`}>
                    {counts[s.id]}
                  </TextLink>
                ) : (
                  <span className="text-muted-foreground">0</span>
                )}
              </TableCell>
            ))}
            <TableCell className="text-right font-semibold">{sum(counts)}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  )
}

function IndexerSection({
  run,
  reproductions,
}: {
  run: HarnessRun
  reproductions: { rows: number; named: number; passing: number } | null
}) {
  const target = run.describe?.target ?? "replay run"
  // the networks of the run, and whether they agree
  const networks = Object.keys(run.by_network).toSorted()
  const sameEverywhere =
    networks.length > 1 && networks.every((n) => JSON.stringify(run.by_network[n]) === JSON.stringify(run.by_network[networks[0] ?? ""]))
  return (
    <Section
      title={`Indexer: ${target}`}
      id={`indexer-${target}`}
      description={
        <>
          The replay harness fed {fmt.format(run.distinct_entries)} corpus entries, ledger by ledger, to {target} at commit{" "}
          <Id>{short(run.describe?.target_version ?? "", 8)}</Id> on {networks.join(" and ")} ({fmt.format(run.totals.entries)}{" "}
          replays) and compared what it stored with what the ledger meta shows.{" "}
          {sameEverywhere ? "Both networks gave the same verdicts, so one bar shows both." : null}{" "}
          <TextLink to={docPages.verdicts}>What the verdicts mean</TextLink>.
        </>
      }
    >
      {(sameEverywhere ? networks.slice(0, 1) : networks).map((n) => (
        <div key={n} className="flex flex-col gap-2">
          {!sameEverywhere ? <span className="text-sm font-medium">{n}</span> : null}
          <VerdictBar counts={run.by_network[n] ?? {}} className="h-4" />
          <VerdictLegend counts={run.by_network[n] ?? {}} />
        </div>
      ))}
      <p className="text-sm text-muted-foreground">
        {reproductions ? (
          <>
            {reproductions.rows} {target} bugs reported upstream are reproduced by <Id>shapekit harness reproduce</Id> on
            commits before and after their fixes. At the pinned commit the committed run passes {reproductions.passing} of
            the {reproductions.named} entries they name.{" "}
            <TextLink to={`${indexerHref(target)}#reproductions`}>The reproductions</TextLink>,{" "}
          </>
        ) : null}
        <TextLink to={indexerHref(target)}>{reproductions ? "the whole run" : `The whole ${target} run`}</TextLink>.
      </p>
    </Section>
  )
}

export function Overview() {
  const meta = useMeta()
  const coverage = use(load("coverage"))
  const conformance = use(load("conformance"))
  const harness = use(load("harness"))
  const findings = use(load("findings"))
  const s = coverage.summary
  const fs = findings.summary
  const runs = harness.runs
  const targets = runs.map((r) => r.describe?.target ?? "").filter(Boolean)
  const sdks = bySdkOrder(conformance.sdks, (x) => x.sdk)
  const gw = meta.corpus.generated_with
  const families = coverage.families.toSorted((a, b) => a.priority - b.priority || a.id.localeCompare(b.id))
  const tiers = meta.corpus.protocol_tiers
  const nonCore = sourceClasses.filter((c) => !c.core && (s.entries_by_source_class[c.id] ?? 0) > 0)
  const visibleGaps = coverage.caps.filter((c) => c.indexer_visible && c.core_emitted_entries === 0)


  // wallet-backend sightings: the entries they name, and how the committed run at the pin judged them
  const named = new Map<string, string[]>()
  for (const r of harness.reproductions.rows)
    for (const c of r.cases) for (const x of c.expect) named.set(`${x.entry}@${r.network}`, x.at_pin.map((a) => a.verdict))
  const namedPassing = [...named.values()].filter((v) => v.length > 0 && v.every((x) => x === "pass")).length

  // sdk sightings: reproduced in the committed matrix before and after the fix
  const sdkRepro = conformance.reproductions
  const inMatrix = sdkRepro.filter((r) => [...Object.values(r.fail), ...Object.values(r.pass)].every((o) => o !== "not-in-matrix"))
  const outside = sdkRepro.filter((r) => !inMatrix.includes(r))
  const holding = inMatrix.filter(
    (r) => Object.values(r.fail).every((o) => o === "named-checks-diverge") && Object.values(r.pass).every((o) => o === "no-counted-check-fails"),
  )

  const pinNames: Record<string, string> = {
    "stellar-core": "stellar-core",
    "stellar-xdr": "stellar-xdr (XDR definitions)",
    "stellar-rpc": "stellar-rpc",
    "wallet-backend": "wallet-backend",
  }

  return (
    <div className="flex flex-col">
      <header className="flex flex-col gap-3 pb-6">
        <h1 className="text-3xl font-semibold tracking-tight text-balance sm:text-4xl">What shapekit found</h1>
        <p className="max-w-[70ch] text-muted-foreground sm:text-lg">
          shapekit replays a corpus of rare but valid Stellar ledger shapes, most of them recorded on a real stellar-core,
          through SDKs and indexers, and records where they read it wrong. This page summarises the results for{" "}
          {sdkList(conformance.sdks.map((x) => x.sdk))}
          {targets.length > 0 ? `, and for the ${targets.length === 1 ? "indexer" : "indexers"} ${and(targets)},` : ""} committed
          at <Id>{short(meta.commit, 10)}</Id>, built against stellar-core{" "}
          <Id>{gw.core_image.tag.split("-")[0]}</Id> and stellar-xdr <Id>{gw.xdr_definitions.tag}</Id>.{" "}
          <TextLink to="/docs">What shapekit is</TextLink>.
        </p>
      </header>

      <Section
        title="Findings"
        id="findings"
        description={
          <>
            Every failure the indexer runs, the SDK runs and the linter reported is traced to a root cause, a finding.
            Each finding names its severity, the component it affects, its CAPs and the smallest corpus entry that shows
            it. Source: <RepoLink path={sources.findingsReport} />.
          </>
        }
      >
        <div className="grid gap-3 sm:grid-cols-3">
          <Tile value={fs.findings} label="findings" to="/dashboard/findings">
            root causes behind {fmt.format(fs.failures)} failures; {fs.unmapped === 0 ? "every failure has one" : `${fs.unmapped} failures have none`}
          </Tile>
          {statuses
            .filter((st) => (fs.by_status[st.id] ?? 0) > 0)
            .map((st) => (
              <Tile key={st.id} value={fs.by_status[st.id]} label={st.id} to={findingsHref({ status: st.id })}>
                {st.meaning}
              </Tile>
            ))}
        </div>
        <FindingsGrid byComponent={fs.by_component} />
        <Terms label="What the severities mean">
          {severities
            .filter((x) => (fs.by_severity[x.id] ?? 0) > 0 || x.id === "halt")
            .map((x) => (
              <Term key={x.id} name={x.id}>
                {x.meaning}.{" "}
                {fs.by_severity[x.id] ? `${fs.by_severity[x.id]} findings.` : "No finding has this severity."}
              </Term>
            ))}
          <Term name="defined in">
            <RepoLink path={sources.triage} />
          </Term>
        </Terms>
      </Section>

      {runs.map((run) => (
        <IndexerSection
          key={run.report}
          run={run}
          reproductions={
            run.describe?.target === "wallet-backend" ? { rows: harness.reproductions.rows.length, named: named.size, passing: namedPassing } : null
          }
        />
      ))}

      <Section
        title="SDKs"
        id="sdks"
        description={
          <>
            The conformance runner decodes, re-encodes and parses every corpus entry with each SDK version and compares
            the answers with the network&apos;s. The columns count the entries on which a version diverges.
          </>
        }
      >
        <div className="grid gap-3 md:grid-cols-3">
          {sdks.map((x) => {
            const rows = x.versions.map((v) => ({ version: v.version, value: sum(v.failing_cells_by_result), current: v.current }))
            const cur = x.versions.find((v) => v.current)
            const curFailing = sum(cur?.failing_cells_by_result)
            const curKnown = cur?.failing_cells_with_known_issue ?? 0
            const max = Math.max(...rows.map((r) => r.value), 1)
            return (
              <div key={x.sdk} className="flex flex-col gap-3 rounded-lg border bg-card p-4">
                <div className="flex flex-col">
                  <h3 className="font-semibold">{sdkName(x.sdk)}</h3>
                  <Id className="text-muted-foreground">{x.package}</Id>
                </div>
                <VersionColumns caption={`${x.package}: entries that diverge, per version`} rows={rows} max={max} />
                <p className="text-sm text-muted-foreground">
                  Current <Id>{x.current}</Id>: <span className="num font-semibold text-foreground">{curFailing}</span>{" "}
                  {curFailing === 1 ? "entry diverges" : "entries diverge"}
                  {curFailing > 0
                    ? curKnown === curFailing
                      ? ", each a known failure with a finding."
                      : `, ${curKnown} of them known failures.`
                    : "."}
                </p>
                <TextLink to={`/dashboard/sdk?sdk=${x.sdk}`} className="mt-auto text-sm">
                  The {sdkName(x.sdk)} matrix
                </TextLink>
              </div>
            )
          })}
        </div>
        <p className="text-sm text-muted-foreground">
          {sdkRepro.length} SDK bugs reported upstream are tracked as reproduction rows. {holding.length} of them fail
          before their fix and pass after it in the committed matrices
          {inMatrix.length > holding.length ? `; ${inMatrix.length - holding.length} do not` : ""}
          {outside.map((r) => `; ${issueLabel(r.link)} ${r.probes.length > 0 ? "runs as a probe outside the matrix" : "is not in the committed matrix"}`).join("")}. <TextLink to="/dashboard/sdk#reproductions">The reproductions</TextLink>.
        </p>
      </Section>

      <Section
        title="Corpus coverage"
        id="coverage"
        description={
          <>
            {fmt.format(s.entries)} corpus entries in {s.families} shape families, at {tiers.length} protocol versions from{" "}
            {tiers[0]} to {tiers[tiers.length - 1]}. An entry counts as coverage only when stellar-core itself emitted its
            bytes. Source: <RepoLink path={sources.coverageReport} />.
          </>
        }
      >
        <ul className="border-b">
          <Meter label="Entries emitted by stellar-core" value={s.core_emitted_entries} total={s.entries} to="/dashboard/coverage">
            The rest: {nonCore.map((c) => `${s.entries_by_source_class[c.id]} ${c.id}`).join(", ")}.
          </Meter>
          <Meter
            label="Indexer-visible CAPs covered"
            value={s.caps_indexer_visible_core_emitted}
            total={s.caps_indexer_visible}
            to="/dashboard/coverage"
          >
            {visibleGaps.length > 0 ? (
              <>
                Without a core-emitted entry, with the reason recorded:{" "}
                {visibleGaps.map((c, i) => (
                  <span key={c.id}>
                    <TextLink to={`/dashboard/coverage/${c.id}`}>{c.id}</TextLink>
                    {i < visibleGaps.length - 1 ? ", " : ""}
                  </span>
                ))}
                .
              </>
            ) : (
              "Every indexer-visible CAP has a core-emitted entry."
            )}
          </Meter>
          <Meter label={`Union arms of ${coverage.root_type}`} value={s.arms_covered} total={s.arms_reachable} to="/dashboard/coverage#arms">
            Every uncovered arm has a recorded reason; {s.arms_uncovered_by_blocker["refused-before-ledger"] ?? 0} of the{" "}
            {s.arms_uncovered} exist only in results core returns before a ledger.
          </Meter>
          <Meter label="Shape family fragments" value={s.fragments_covered} total={s.fragments} to="#families" />
          <Meter
            label="Unvalidated fields exercised"
            value={s.fields_not_fully_validated_exercised}
            total={s.fields_not_fully_validated}
            to="/dashboard/coverage#fields"
          >
            Fields stellar-core passes through partly or wholly unchecked, each with at least one entry.
          </Meter>
        </ul>
      </Section>

      <Section
        title="Shape families"
        id="families"
        description={
          <>
            A family is one rare shape, defined in <RepoLink path={sources.families} /> by the signature fragments its
            entries must contain. Each square is one fragment: filled when a core-emitted entry contains it, outlined
            when none does. Highest demonstrated damage first. Select a family for its CAP page.
          </>
        }
      >
        <FamilyStrip families={families} />
      </Section>

      <div className="grid gap-x-10 md:grid-cols-2 [&>section]:min-w-0">
        <Section title="Documentation" id="docs">
          <ul className="flex flex-col gap-1.5 text-sm">
            {docs.map((d) => (
              <li key={d.to ?? d.path}>
                {d.to ? <TextLink to={d.to}>{d.label}</TextLink> : <RepoLink path={d.path ?? ""}>{d.label}</RepoLink>}
              </li>
            ))}
          </ul>
        </Section>
        <Section
          title="Pinned versions"
          id="pins"
          description={
            <>
              From <RepoLink path="pins.toml" /> and <RepoLink path="conformance/versions.toml" />.
            </>
          }
        >
          <Table>
            <caption className="sr-only">Pinned upstream versions</caption>
            <TableBody>
              {meta.pins.git
                .filter((g) => g.name in pinNames)
                .map((g) => (
                  <TableRow key={g.name}>
                    <TableCell className="font-medium">{pinNames[g.name]}</TableCell>
                    <TableCell>
                      <Id>{g.ref ? g.ref.replace(/^refs\/tags\//, "") : short(g.sha ?? "", 10)}</Id>
                    </TableCell>
                  </TableRow>
                ))}
              {sdks.map((x) => (
                <TableRow key={x.sdk}>
                  <TableCell className="font-medium">
                    {x.package} <span className="font-normal text-muted-foreground">({x.package === sdkName(x.sdk) ? "current" : `${sdkLabel(x.sdk)}, current`})</span>
                  </TableCell>
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
            <Id>shapekit site data</Id> read these committed files at commit <Id>{short(meta.commit, 10)}</Id>; each sha256
            is of the file as read. <TextLink to={docPages.site}>How the data is made</TextLink>.
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
              <caption className="sr-only">Files read to make this site's data</caption>
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
