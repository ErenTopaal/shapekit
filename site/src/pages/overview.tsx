import { use, type ReactNode } from "react"
import { Link } from "react-router"
import { ArrowRightIcon } from "lucide-react"
import { load } from "@/lib/data"
import { bySdkOrder, sdkName } from "@/lib/sdks"
import { coverageMetrics, reportedFindings, runSummary, sdkSummary } from "@/lib/metrics"
import { docs, statuses } from "@/lib/terms"
import { GlossaryLink, TextLink, fmt } from "@/components/common"

// one card for each question a reader of the dashboard asks: who asks it, a short answer from
// the same functions the answer's page uses, the page, and the next step
function Question({
  id,
  readers,
  title,
  to,
  open,
  next,
  children,
}: {
  id: string
  readers: string
  title: string
  to: string
  open: string
  next: ReactNode
  children: ReactNode
}) {
  return (
    <section aria-labelledby={`${id}-h`} className="flex flex-col gap-3 rounded-lg border bg-card p-4 sm:p-5">
      <span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{readers}</span>
      <h2 id={`${id}-h`} className="text-lg font-semibold tracking-tight">
        {title}
      </h2>
      <div className="flex flex-col gap-1.5 text-sm text-muted-foreground">{children}</div>
      <div className="mt-auto flex flex-col gap-2 pt-1 text-sm">
        <Link
          to={to}
          className="inline-flex w-fit items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 font-medium text-primary-foreground hover:bg-primary/90 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
        >
          {open}
          <ArrowRightIcon aria-hidden className="size-4" />
        </Link>
        <span className="text-muted-foreground">Next step: {next}</span>
      </div>
    </section>
  )
}

export function Overview() {
  const coverage = use(load("coverage"))
  const conformance = use(load("conformance"))
  const harness = use(load("harness"))
  const findings = use(load("findings"))
  const r = findings.reported
  const reported = reportedFindings(findings)
  const sdks = bySdkOrder(conformance.sdks, (x) => x.sdk).map(sdkSummary)
  const diverging = sdks.filter((x) => x.diverging > 0)
  const runs = harness.runs.map((run, i) => runSummary(run, i))
  const corpus = coverageMetrics(coverage.summary, coverage.root_type)
  const s = coverage.summary
  const caps = corpus.find((m) => m.id === "caps")

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-3">
        <h1 className="text-3xl font-semibold tracking-tight text-balance sm:text-4xl">shapekit results</h1>
        <p className="max-w-[70ch] text-muted-foreground sm:text-lg">
          shapekit runs a corpus of rare but valid Stellar ledger data through SDKs and indexers. This dashboard shows the
          committed results. Select the question that you have.
        </p>
      </header>

      <div className="grid gap-4 md:grid-cols-2">
        <Question
          id="q-sdk"
          readers="For SDK maintainers"
          title="How does my SDK do?"
          to="/dashboard/sdk"
          open="Open the SDK results"
          next={
            <>
              <TextLink to={docs.sdkCi}>add the corpus check to your SDK CI</TextLink>.
            </>
          }
        >
          <p>
            shapekit tests {sdks.length} SDKs and tools at their current versions.{" "}
            {diverging.length === 0
              ? "No current version has a diverging entry."
              : `At the current version, ${diverging.length} of them have `}
            {diverging.length > 0 ? <GlossaryLink id="diverging-entry">diverging entries</GlossaryLink> : null}
            {diverging.length > 0 ? " (lower is better):" : null}
          </p>
          {diverging.length > 0 ? (
            <ul className="flex flex-wrap gap-x-4 gap-y-1">
              {diverging.map((x) => (
                <li key={x.sdk}>
                  {sdkName(x.sdk)} <span className="num font-semibold text-foreground">{x.diverging}</span>
                </li>
              ))}
            </ul>
          ) : null}
        </Question>

        <Question
          id="q-indexer"
          readers="For indexer maintainers"
          title="How does my indexer do?"
          to="/dashboard/indexer"
          open="Open the indexer results"
          next={
            <>
              <TextLink to={docs.indexerCi}>add the replay to your indexer CI</TextLink>.
            </>
          }
        >
          <p>
            {runs.length} indexer runs. Each run has {fmt.format(runs[0]?.replays ?? 0)}{" "}
            <GlossaryLink id="replay">replays</GlossaryLink>. Replays with the verdict pass (higher is better):
          </p>
          <ul className="flex flex-col gap-0.5">
            {runs.map((x) => (
              <li key={x.target}>
                {x.target} <span className="num font-semibold text-foreground">{fmt.format(x.pass)}</span> of{" "}
                {fmt.format(x.replays)}
              </li>
            ))}
          </ul>
        </Question>

        <Question
          id="q-findings"
          readers="For SDF reviewers and RFP evaluators"
          title="Which defects were found, and where are they reported?"
          to="/dashboard/findings"
          open="Open the findings"
          next={<>open a finding to get its upstream issue and the command that reproduces it.</>}
        >
          <p>
            <span className="num font-semibold text-foreground">{fmt.format(reported.length)}</span>{" "}
            <GlossaryLink id="reported-finding">reported findings</GlossaryLink> in {r.components} components:{" "}
            {statuses
              .filter((st) => (r.by_status[st.id] ?? 0) > 0)
              .map((st) => `${fmt.format(r.by_status[st.id] ?? 0)} ${st.label}`)
              .join(", ")}
            .
          </p>
        </Question>

        <Question
          id="q-corpus"
          readers="For RFP evaluators and SDF reviewers"
          title="What does the corpus cover?"
          to="/dashboard/corpus"
          open="Open the corpus coverage"
          next={
            <>
              <TextLink to={docs.addingShape}>add a shape to the corpus</TextLink>.
            </>
          }
        >
          <p>
            {fmt.format(s.entries)} corpus <GlossaryLink id="entry">entries</GlossaryLink>.{" "}
            <GlossaryLink id="core-emitted">stellar-core emitted</GlossaryLink>{" "}
            <span className="num font-semibold text-foreground">{fmt.format(s.core_emitted_entries)}</span> of them (higher
            is better).
          </p>
          {caps ? (
            <p>
              <span className="num font-semibold text-foreground">{caps.value}</span> of {caps.total}{" "}
              <GlossaryLink id="indexer-visible">indexer-visible CAPs</GlossaryLink> have an entry from stellar-core.
            </p>
          ) : null}
        </Question>
      </div>

      <section aria-labelledby="more-h" className="flex flex-col gap-2 text-sm">
        <h2 id="more-h" className="text-lg font-semibold tracking-tight">
          Other pages
        </h2>
        <ul className="flex flex-col gap-1.5">
          <li>
            <TextLink to="/dashboard/glossary">Glossary</TextLink>: the terms of the dashboard, each with one meaning.
          </li>
          <li>
            <TextLink to="/dashboard/corpus/entries">Corpus entries</TextLink>: find one entry, its bytes and its results.
          </li>
          <li>
            <TextLink to="/dashboard/sources">Data sources</TextLink>: the committed files and the pinned versions.
          </li>
          <li>
            <TextLink to="/docs">Documentation</TextLink>: how shapekit works and how to use it.
          </li>
        </ul>
      </section>
    </div>
  )
}
