import { use, useEffect, useMemo } from "react"
import { Link, useNavigate, useSearchParams } from "react-router"
import { load, type CapRow } from "@/lib/data"
import { Input } from "@/components/ui/input"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Meter } from "@/components/charts"
import { FamilyStrip } from "@/components/fragments"
import {
  GlossaryLink,
  Id,
  More,
  NextSteps,
  PageHeader,
  PageTerms,
  RepoLink,
  Section,
  Status,
  TextLink,
  fmt,
  useHashScroll,
} from "@/components/common"
import { betterText, coverageMetrics, ledgerArms } from "@/lib/metrics"
import { capPath, entriesPath } from "@/lib/paths"
import { docs, sourceClasses, sources } from "@/lib/terms"

// the filters of the entry list; an old link to the entry list at this address carries them
const entryFilters = ["q", "cap", "shape", "sdk", "version"]

type Scope = "visible" | "all" | "gaps"

// registry/fields.toml's validated_by_core values, in words
const checked: Record<string, string> = { partial: "in part", no: "never" }

// what one obligation of each kind of the ledger is
const obligationKinds: Record<string, string> = {
  format: "A decision point of the XDR schema, in the accept or the reject direction.",
  gap: "A point where the native types of a language lose information that an XDR value holds.",
  "core-test-section": "A test section of the transaction tests of stellar-core.",
  "gate-side": "One side of a protocol version gate in the source code of stellar-core.",
  field: "An envelope field that shapekit tested on a standalone stellar-core node.",
  "sdk-version": "A published SDK version that the corpus runs against.",
  seed: "A fault that the mutation tool of a language put into an SDK.",
}

// the states a cap row can be in; the terms block below explains each
function state(c: CapRow): { tone: "pass" | "diverge" | "idle" | "known"; label: string; term?: string } {
  if (!c.indexer_visible) return { tone: "idle", label: "not indexer-visible", term: "indexer-visible" }
  if (c.core_emitted_entries > 0) return { tone: "pass", label: "covered", term: "core-emitted" }
  if (c.uncovered) return { tone: "known", label: "gap, with a reason", term: "blocker" }
  if (c.entries > 0) return { tone: "known", label: "no entry from stellar-core", term: "core-emitted" }
  return { tone: "diverge", label: "no entries" }
}

export function CorpusPage() {
  const coverage = use(load("coverage"))
  const [params, setParams] = useSearchParams()
  const navigate = useNavigate()
  useHashScroll()
  // this address was the entry list before the coverage page moved here; a filtered link
  // to the entry list goes on to its new address
  const old = entryFilters.some((k) => params.has(k))
  useEffect(() => {
    if (old) void navigate(`/dashboard/corpus/entries?${params.toString()}`, { replace: true })
  }, [old, params, navigate])
  const q = params.get("capq") ?? ""
  const scope = (params.get("scope") as Scope | null) ?? "visible"
  const set = (k: string, v: string, dflt: string) => {
    const next = new URLSearchParams(params)
    if (v === dflt) next.delete(k)
    else next.set(k, v)
    setParams(next, { replace: true, preventScrollReset: true })
  }

  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase()
    return coverage.caps.filter((c) => {
      if (scope === "visible" && !c.indexer_visible) return false
      if (scope === "gaps" && !(c.indexer_visible && c.core_emitted_entries === 0)) return false
      if (!needle) return true
      return (
        c.id.toLowerCase().includes(needle) ||
        c.title.toLowerCase().includes(needle) ||
        c.families.some((f) => f.includes(needle)) ||
        c.xdr_additions.some((x) => x.toLowerCase().includes(needle))
      )
    })
  }, [coverage.caps, q, scope])

  const s = coverage.summary
  const metrics = coverageMetrics(s, coverage.root_type)
  const arms = ledgerArms(s)
  const families = coverage.families.toSorted((a, b) => a.priority - b.priority || a.id.localeCompare(b.id))
  return (
    <div className="flex flex-col gap-6">
      <PageHeader title="Corpus coverage">
        <p>
          The corpus holds {fmt.format(s.entries)} <GlossaryLink id="entry">entries</GlossaryLink> in {s.families}{" "}
          <GlossaryLink id="family">shape families</GlossaryLink>. An entry counts as coverage only when{" "}
          <GlossaryLink id="core-emitted">stellar-core emitted</GlossaryLink> its bytes. This page shows what the corpus
          covers and gives the reason for each gap. The readable report is <RepoLink path={sources.coverageReport} />.
        </p>
      </PageHeader>

      <NextSteps>
        <li>
          <TextLink to="/dashboard/corpus/entries">Find an entry</TextLink> and get its bytes and the commands that run it.
        </li>
        <li>
          <TextLink to={docs.addingShape}>Add a shape to the corpus</TextLink> to close a gap.
        </li>
      </NextSteps>

      <Section title="Summary" id="summary" description="Each bar shows a share of a total. For each of these numbers, higher is better.">
        <ul className="border-b">
          {metrics.map((m) => (
            <Meter key={m.id} label={m.label} value={m.value} total={m.total} to={`#${m.section}`}>
              {betterText[m.better]}
            </Meter>
          ))}
        </ul>
      </Section>

      <PageTerms ids={["entry", "core-emitted", "indexer-visible", "union-arm", "fragment", "blocker", "obligation", "unvalidated-field"]} />

      <Section title="Entries by source" id="sources" description="Where the bytes of each entry come from. Only the sources from stellar-core count as coverage.">
        <Table>
          <caption className="sr-only">Corpus entries by source</caption>
          <TableHeader>
            <TableRow>
              <TableHead>Source</TableHead>
              <TableHead className="text-right">
                <GlossaryLink id="entry">Entries</GlossaryLink>
              </TableHead>
              <TableHead>
                <GlossaryLink id="core-emitted">Counts as coverage</GlossaryLink>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sourceClasses
              .filter((c) => (s.entries_by_source_class[c.id] ?? 0) > 0)
              .map((c) => (
                <TableRow key={c.id}>
                  <TableCell className="whitespace-normal">
                    <GlossaryLink id={`source-${c.id}`} className="font-mono text-[0.85em]">
                      {c.id}
                    </GlossaryLink>
                  </TableCell>
                  <TableCell className="num text-right">{fmt.format(s.entries_by_source_class[c.id] ?? 0)}</TableCell>
                  <TableCell>{c.core ? <Status tone="pass">yes</Status> : <Status tone="idle">no</Status>}</TableCell>
                </TableRow>
              ))}
          </TableBody>
        </Table>
        <p className="text-sm text-muted-foreground">
          <TextLink to={entriesPath()}>Browse all {fmt.format(s.entries)} entries</TextLink>.
        </p>
      </Section>

      <Section
        title="CAPs"
        id="caps"
        description="Each CAP (Core Advancement Proposal) of the registry, with the entries that show its shapes. Select a CAP to see its families, entries and results."
      >
        <div className="flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-center">
          <label className="sr-only" htmlFor="cap-filter">
            Filter the CAPs
          </label>
          <Input
            id="cap-filter"
            type="search"
            placeholder="Filter by CAP, title, family or XDR name"
            value={q}
            onChange={(e) => set("capq", e.target.value, "")}
            className="sm:max-w-sm"
          />
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            className="flex-wrap"
            value={scope}
            onValueChange={(v) => v && set("scope", v, "visible")}
            aria-label="Which CAPs to list"
          >
            <ToggleGroupItem value="visible">Indexer-visible ({s.caps_indexer_visible})</ToggleGroupItem>
            <ToggleGroupItem value="gaps">Gaps ({s.caps_indexer_visible - s.caps_indexer_visible_core_emitted})</ToggleGroupItem>
            <ToggleGroupItem value="all">All ({s.caps})</ToggleGroupItem>
          </ToggleGroup>
        </div>

        <p className="text-sm text-muted-foreground" aria-live="polite">
          {rows.length} of {coverage.caps.length} CAPs shown.
        </p>

        {rows.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyTitle>No CAP matches</EmptyTitle>
              <EmptyDescription>Clear the filter or list all CAPs.</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <Table>
            <caption className="sr-only">CAPs with their coverage state and entry counts</caption>
            <TableHeader>
              <TableRow>
                <TableHead>CAP</TableHead>
                <TableHead className="hidden min-w-44 md:table-cell">Title</TableHead>
                <TableHead className="hidden text-right sm:table-cell">Protocol</TableHead>
                <TableHead>State</TableHead>
                <TableHead className="hidden text-right sm:table-cell">
                  <GlossaryLink id="entry">Entries</GlossaryLink>
                </TableHead>
                <TableHead className="text-right">
                  <GlossaryLink id="core-emitted">From stellar-core</GlossaryLink>
                  <span className="block text-xs font-normal text-muted-foreground">higher is better</span>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((c) => {
                const st = state(c)
                return (
                  <TableRow key={c.id}>
                    <TableCell className="whitespace-normal">
                      <Link to={capPath(c.id)} className="font-mono text-primary underline decoration-primary/40 hover:decoration-primary">
                        {c.id}
                      </Link>
                      <span className="block text-xs text-muted-foreground md:hidden">{c.title}</span>
                    </TableCell>
                    <TableCell className="hidden whitespace-normal md:table-cell">{c.title}</TableCell>
                    <TableCell className="num hidden text-right sm:table-cell">{c.protocol ?? ""}</TableCell>
                    <TableCell className="whitespace-normal">
                      <Status tone={st.tone} wrap>
                        {st.term ? <GlossaryLink id={st.term}>{st.label}</GlossaryLink> : st.label}
                      </Status>
                    </TableCell>
                    <TableCell className="num hidden text-right sm:table-cell">{fmt.format(c.entries)}</TableCell>
                    <TableCell className="num text-right">{fmt.format(c.core_emitted_entries)}</TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        )}
      </Section>

      <Section
        title="Shape families"
        id="families"
        description={
          <>
            <RepoLink path={sources.families} /> defines each family by the{" "}
            <GlossaryLink id="fragment">fragments</GlossaryLink> that its entries must contain. Each square is one
            fragment. A filled square has an entry from stellar-core. An outlined square has none. The list starts with
            the family of the highest known damage. Select a family to see its CAP page.
          </>
        }
      >
        <FamilyStrip families={families} />
      </Section>

      <Section
        title="Union arms"
        id="arms"
        description={
          <>
            A <GlossaryLink id="union-arm">union arm</GlossaryLink> is one variant that an XDR union can take.{" "}
<Id>{coverage.root_type}</Id> can reach {arms.all} arms under stellar-xdr {coverage.stellar_xdr}. {arms.covered}{" "}
            of the {arms.total} <GlossaryLink id="union-arm">arms that a ledger can hold</GlossaryLink> have an entry from
            stellar-core. {arms.refused + arms.never} arms cannot occur in any ledger: stellar-core refuses {arms.refused}{" "}
            before a ledger and never emits {arms.never}. Each of the {s.arms_uncovered} arms without an entry has a reason in{" "}
            <RepoLink path={sources.uncovered} />, under one of these <GlossaryLink id="blocker">blockers</GlossaryLink>.
          </>
        }
      >
        <Table>
          <caption className="sr-only">Uncovered union arms by blocker</caption>
          <TableHeader>
            <TableRow>
              <TableHead>
                <GlossaryLink id="blocker">Blocker</GlossaryLink>
              </TableHead>
              <TableHead className="hidden md:table-cell">Meaning</TableHead>
              <TableHead className="text-right">Arms</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {coverage.blockers.map((b) => (
              <TableRow key={b.blocker}>
                <TableCell className="align-top whitespace-normal">
                  <Id>{b.blocker}</Id>
                  <span className="block text-xs text-muted-foreground md:hidden">{b.meaning}</span>
                </TableCell>
                <TableCell className="hidden text-sm whitespace-normal text-muted-foreground md:table-cell">{b.meaning}</TableCell>
                <TableCell className="num text-right align-top">{s.arms_uncovered_by_blocker[b.blocker] ?? 0}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        {coverage.arms.history_only.length > 0 && (
          <Table>
            <caption className="sr-only">Covered arms that only public network history holds</caption>
            <TableHeader>
              <TableRow>
                <TableHead>
                  <GlossaryLink id="union-arm">Arm</GlossaryLink> that only history holds
                </TableHead>
                <TableHead className="hidden md:table-cell">Entries</TableHead>
                <TableHead>Why a local stellar-core cannot write it</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {coverage.arms.history_only.map((h) => (
                <TableRow key={h.arm}>
                  <TableCell className="align-top whitespace-normal">
                    <Id>{h.arm}</Id>
                  </TableCell>
                  <TableCell className="hidden align-top whitespace-normal md:table-cell">
                    {h.entries.map((e) => (
                      <Id key={e} className="block">
                        {e}
                      </Id>
                    ))}
                  </TableCell>
                  <TableCell className="text-sm whitespace-normal text-muted-foreground">{h.reason}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <More label={`Show the ${coverage.arms.uncovered.length} arms without an entry from stellar-core`}>
          <Table>
            <caption className="sr-only">Every union arm without an entry from stellar-core, with its reason</caption>
            <TableHeader>
              <TableRow>
                <TableHead>
                  <GlossaryLink id="union-arm">Arm</GlossaryLink>
                </TableHead>
                <TableHead>Reason</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {coverage.arms.uncovered.map((a) => (
                <TableRow key={a.arm}>
                  <TableCell className="align-top whitespace-normal">
                    <Id>{a.arm}</Id>
                    <span className="block text-xs text-muted-foreground">
                      <GlossaryLink id="blocker">blocker</GlossaryLink> <Id>{a.uncovered?.blocker ?? ""}</Id>
                    </span>
                  </TableCell>
                  <TableCell className="text-sm whitespace-normal text-muted-foreground">{a.uncovered?.reason ?? ""}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </More>
      </Section>

      <Section
        title="Obligations"
        id="obligations"
        description={
          <>
            An <GlossaryLink id="obligation">obligation</GlossaryLink> is a point that the corpus must show with an entry.
            The list is <RepoLink path="corpus/coverage/obligations.json" />. For met, higher is better. For open, lower is
            better. A side of a protocol gate is met in one of two ways. A branch count shows that a corpus chain takes the
            side on the instrumented stellar-core. Or an execution count shows that a recipe runs the gate at a protocol
            that takes the side. The table gives the two counts apart.
          </>
        }
      >
        <div className="overflow-x-auto">
        <Table>
          <caption className="sr-only">Obligations by kind and status</caption>
          <TableHeader>
            <TableRow>
              <TableHead>Kind</TableHead>
              <TableHead className="text-right">Met</TableHead>
              <TableHead className="text-right">Unmeetable</TableHead>
              <TableHead className="text-right">Open</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {coverage.obligations.map((o) => (
              <TableRow key={o.kind}>
                <TableCell className="max-w-64 align-top whitespace-normal">
                  <Id>{o.kind}</Id>
                  <span className="block text-xs text-muted-foreground">{obligationKinds[o.kind] ?? ""}</span>
                </TableCell>
                <TableCell className="num text-right align-top whitespace-normal">
                  {o.met_branch_count !== null && o.met_inferred !== null ? (
                    <>
                      {fmt.format(o.met_branch_count)} by branch count, {fmt.format(o.met_inferred)} by execution count
                    </>
                  ) : (
                    fmt.format(o.met ?? 0)
                  )}
                </TableCell>
                <TableCell className="num text-right align-top">{fmt.format(o.unmeetable)}</TableCell>
                <TableCell className="num text-right align-top">{fmt.format(o.open)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        </div>
      </Section>

      <Section
        title="Fields that stellar-core does not fully check"
        id="fields"
        description={
          <>
            stellar-core does not fully check {s.fields_not_fully_validated} transaction fields. At least one entry contains{" "}
            {s.fields_not_fully_validated_exercised} of them (higher is better). An indexer must not trust these fields.
            The{" "}
            <TextLink to="/docs/guide/indexer-hardening#validated-and-unvalidated-fields">indexer-hardening guide</TextLink>{" "}
            explains each field. The list is <RepoLink path={sources.fields} />.
          </>
        }
      >
        <Table>
          <caption className="sr-only">Fields that stellar-core does not fully check, with their entries</caption>
          <TableHeader>
            <TableRow>
              <TableHead>
                <GlossaryLink id="unvalidated-field">Field</GlossaryLink>
              </TableHead>
              <TableHead className="hidden md:table-cell">XDR path</TableHead>
              <TableHead>Checked by stellar-core</TableHead>
              <TableHead className="hidden text-right sm:table-cell">Entries</TableHead>
              <TableHead className="text-right">
                <GlossaryLink id="core-emitted">From stellar-core</GlossaryLink>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {coverage.fields.map((f) => (
              <TableRow key={f.id}>
                <TableCell className="whitespace-normal">
                  <Id>{f.id}</Id>
                </TableCell>
                <TableCell className="hidden whitespace-normal md:table-cell">
                  <Id className="text-muted-foreground">{f.xdr_path}</Id>
                </TableCell>
                <TableCell>{checked[f.validated_by_core] ?? f.validated_by_core}</TableCell>
                <TableCell className="num hidden text-right sm:table-cell">{f.entries}</TableCell>
                <TableCell className="num text-right">{f.core_emitted_entries}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Section>
    </div>
  )
}
