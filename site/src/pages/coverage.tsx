import { use, useMemo } from "react"
import { Link, useSearchParams } from "react-router"
import { ChevronDownIcon } from "lucide-react"
import { load, type CapRow } from "@/lib/data"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Id, useHashScroll, PageHeader, RepoLink, Section, Status, Term, Terms, TextLink, fmt } from "@/components/common"
import { docs, sourceClasses, sources } from "@/lib/terms"

type Scope = "visible" | "all" | "gaps"

// registry/fields.toml's validated_by_core values, in words
const checked: Record<string, string> = { partial: "in part", no: "never" }

// what one obligation of each kind of the ledger is
const obligationKinds: Record<string, string> = {
  format: "A decision point of the XDR schema, in its accept or its reject direction.",
  gap: "A point where a language's native types lose information an XDR value carries.",
  "core-test-section": "A test section of stellar-core's own transaction tests.",
  "gate-side": "One side of a protocol version gate in stellar-core's source.",
  field: "An envelope field stellar-core was asked about on a standalone node.",
  "sdk-version": "A published SDK version the corpus runs against.",
  seed: "A fault seeded into an SDK by its language's mutation tool.",
}

// the states a cap row can be in; the terms block below explains each
function state(c: CapRow): { tone: "pass" | "diverge" | "idle" | "known"; label: string } {
  if (!c.indexer_visible) return { tone: "idle", label: "not indexer-visible" }
  if (c.core_emitted_entries > 0) return { tone: "pass", label: "covered" }
  if (c.uncovered) return { tone: "known", label: "gap, reason recorded" }
  if (c.entries > 0) return { tone: "known", label: "non-core entries only" }
  return { tone: "diverge", label: "no entries" }
}

export function CoveragePage() {
  const coverage = use(load("coverage"))
  const [params, setParams] = useSearchParams()
  useHashScroll()
  const q = params.get("q") ?? ""
  const scope = (params.get("scope") as Scope | null) ?? "visible"
  const set = (k: string, v: string, dflt: string) => {
    const next = new URLSearchParams(params)
    if (v === dflt) next.delete(k)
    else next.set(k, v)
    setParams(next, { replace: true })
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
  return (
    <div className="flex flex-col">
      <PageHeader title="CAP coverage">
        <p>
          Every CAP (Core Advancement Proposal) in the registry, with the corpus entries that carry its shapes. An
          entry covers a shape only when stellar-core emitted it: core&apos;s own goldens, a local core, or public
          network history. Synthetic bytes and envelopes core refused are counted apart. Source:{" "}
          <RepoLink path="corpus/coverage/coverage.json" />, readable as <RepoLink path={sources.coverageReport} />.
        </p>
      </PageHeader>

      <Terms>
        <Term name="indexer-visible">
          The CAP changes what an indexer reads: new XDR in transactions, results or ledger meta. The others change only
          consensus, fees or arithmetic and leave the shapes as they were; each has its reason in{" "}
          <RepoLink path={sources.caps} />.
        </Term>
        <Term name="covered">At least one entry tagged with the CAP was emitted by stellar-core.</Term>
        <Term name="gap, reason recorded">
          An indexer-visible CAP with no core-emitted entry (the Gaps filter); <RepoLink path={sources.uncovered} />{" "}
          says why, with one of the blockers listed under
          Union arms.
        </Term>
        <Term name="core-emitted">
          From {sourceClasses.filter((c) => c.core).map((c) => c.id).join(", ")}. Not:{" "}
          {sourceClasses.filter((c) => !c.core).map((c) => `${c.id} (${c.meaning})`).join("; ")}.
        </Term>
        <Term name="family">
          One rare shape, defined by signature fragments in <RepoLink path={sources.families} />.{" "}
          <TextLink to={docs.familyRecord}>The family record</TextLink>.
        </Term>
      </Terms>

      <div className="flex flex-col gap-3 pt-6 sm:flex-row sm:flex-wrap sm:items-center">
        <label className="sr-only" htmlFor="cap-filter">
          Filter CAPs
        </label>
        <Input
          id="cap-filter"
          type="search"
          placeholder="Filter by CAP, title, family or XDR name"
          value={q}
          onChange={(e) => set("q", e.target.value, "")}
          className="sm:max-w-sm"
        />
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          value={scope}
          onValueChange={(v) => v && set("scope", v, "visible")}
          aria-label="Which CAPs to list"
        >
          <ToggleGroupItem value="visible">Indexer-visible ({s.caps_indexer_visible})</ToggleGroupItem>
          <ToggleGroupItem value="gaps">Gaps ({s.caps_indexer_visible - s.caps_indexer_visible_core_emitted})</ToggleGroupItem>
          <ToggleGroupItem value="all">All ({s.caps})</ToggleGroupItem>
        </ToggleGroup>
      </div>

      <p className="pt-3 text-sm text-muted-foreground" aria-live="polite">
        {rows.length} of {coverage.caps.length} CAPs shown.
      </p>

      {rows.length === 0 ? (
        <Empty className="my-6 border">
          <EmptyHeader>
            <EmptyTitle>No CAP matches</EmptyTitle>
            <EmptyDescription>Clear the filter or list all CAPs.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <Table className="mt-2">
          <caption className="sr-only">CAPs with their coverage state and entry counts</caption>
          <TableHeader>
            <TableRow>
              <TableHead>CAP</TableHead>
              <TableHead className="min-w-44 sm:min-w-56">Title</TableHead>
              <TableHead className="hidden text-right sm:table-cell">Protocol</TableHead>
              <TableHead>State</TableHead>
              <TableHead className="hidden text-right sm:table-cell">Families</TableHead>
              <TableHead className="text-right">Entries</TableHead>
              <TableHead className="text-right">Core-emitted</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((c) => {
              const st = state(c)
              return (
                <TableRow key={c.id}>
                  <TableCell>
                    <Link to={`/dashboard/coverage/${c.id}`} className="font-mono text-primary underline decoration-primary/40 hover:decoration-primary">
                      {c.id}
                    </Link>
                  </TableCell>
                  <TableCell className="whitespace-normal">{c.title}</TableCell>
                  <TableCell className="num hidden text-right sm:table-cell">{c.protocol ?? ""}</TableCell>
                  <TableCell>
                    <Status tone={st.tone}>{st.label}</Status>
                  </TableCell>
                  <TableCell className="num hidden text-right sm:table-cell">{c.families.length}</TableCell>
                  <TableCell className="num text-right">{fmt.format(c.entries)}</TableCell>
                  <TableCell className="num text-right">{fmt.format(c.core_emitted_entries)}</TableCell>
                </TableRow>
              )
            })}
          </TableBody>
        </Table>
      )}

      <Section
        title="Union arms"
        id="arms"
        description={
          <>
            A union arm is one variant an XDR union can take, such as an operation type or a result code. {s.arms_covered}{" "}
            of the {s.arms_reachable} arms reachable from <Id>{coverage.root_type}</Id> under stellar-xdr{" "}
            {coverage.stellar_xdr} have a core-emitted entry. Each of the {s.arms_uncovered} others has a recorded reason
            in <RepoLink path={sources.uncovered} />, under one of these blockers.
          </>
        }
      >
        <Table>
          <caption className="sr-only">Uncovered union arms by blocker</caption>
          <TableHeader>
            <TableRow>
              <TableHead>Blocker</TableHead>
              <TableHead className="min-w-56">Meaning</TableHead>
              <TableHead className="text-right">Arms</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {coverage.blockers.map((b) => (
              <TableRow key={b.blocker}>
                <TableCell className="align-top">
                  <Id>{b.blocker}</Id>
                </TableCell>
                <TableCell className="text-sm whitespace-normal text-muted-foreground">{b.meaning}</TableCell>
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
                <TableHead>Arm only history holds</TableHead>
                <TableHead>Entries</TableHead>
                <TableHead className="min-w-64 sm:min-w-80">Why a local core cannot write it</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {coverage.arms.history_only.map((h) => (
                <TableRow key={h.arm}>
                  <TableCell className="align-top">
                    <Id>{h.arm}</Id>
                  </TableCell>
                  <TableCell className="align-top">
                    {h.entries.map((e) => (
                      <Id key={e}>{e}</Id>
                    ))}
                  </TableCell>
                  <TableCell className="text-sm whitespace-normal text-muted-foreground">{h.reason}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <Collapsible>
          <CollapsibleTrigger asChild>
            <Button variant="outline" size="sm" className="w-fit">
              Show the {coverage.arms.uncovered.length} uncovered arms
              <ChevronDownIcon data-icon="inline-end" />
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <Table className="mt-3">
              <caption className="sr-only">Every uncovered union arm with its reason</caption>
              <TableHeader>
                <TableRow>
                  <TableHead>Arm</TableHead>
                  <TableHead>Blocker</TableHead>
                  <TableHead className="min-w-64 sm:min-w-80">Reason</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {coverage.arms.uncovered.map((a) => (
                  <TableRow key={a.arm}>
                    <TableCell className="align-top">
                      <Id>{a.arm}</Id>
                    </TableCell>
                    <TableCell className="align-top">
                      <Id>{a.uncovered?.blocker ?? ""}</Id>
                    </TableCell>
                    <TableCell className="text-sm whitespace-normal text-muted-foreground">
                      {a.uncovered?.reason ?? ""}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CollapsibleContent>
        </Collapsible>
      </Section>

      <Section
        title="Obligations"
        id="obligations"
        description={
          <>
            Everything the corpus owes a witness for, merged in <RepoLink path="corpus/coverage/obligations.json" />.
            An obligation is met when an entry witnesses it, unmeetable when the source shows that no input can, and open
            otherwise. A protocol gate side is met by a branch count when a corpus chain run on the instrumented core takes
            it, or inferred from an execution count when a recipe run at a protocol that takes the side executes the gate;
            the two are counted apart.
          </>
        }
      >
        <Table>
          <caption className="sr-only">Obligations by kind and status</caption>
          <TableHeader>
            <TableRow>
              <TableHead>Kind</TableHead>
              <TableHead className="min-w-56">What one obligation is</TableHead>
              <TableHead className="text-right">Met</TableHead>
              <TableHead className="text-right">Unmeetable</TableHead>
              <TableHead className="text-right">Open</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {coverage.obligations.map((o) => (
              <TableRow key={o.kind}>
                <TableCell className="align-top">
                  <Id>{o.kind}</Id>
                </TableCell>
                <TableCell className="text-sm whitespace-normal text-muted-foreground">{obligationKinds[o.kind] ?? ""}</TableCell>
                <TableCell className="num text-right align-top">
                  {o.met_branch_count !== null && o.met_inferred !== null ? (
                    <>
                      {fmt.format(o.met_branch_count)} by branch count, {fmt.format(o.met_inferred)} inferred from execution
                      count
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
      </Section>

      <Section
        title="Fields core does not fully validate"
        id="fields"
        description={
          <>
            {s.fields_not_fully_validated_exercised} of the {s.fields_not_fully_validated} transaction fields that
            stellar-core passes through partly or wholly unchecked are exercised by at least one entry. An indexer must not
            trust these fields; the{" "}
            <TextLink to="/docs/guide/indexer-hardening#validated-and-unvalidated-fields">indexer-hardening guide</TextLink>{" "}
            explains each, from the same <RepoLink path={sources.fields} />.
          </>
        }
      >
        <Table>
          <caption className="sr-only">Fields core does not fully validate, with their entries</caption>
          <TableHeader>
            <TableRow>
              <TableHead>Field</TableHead>
              <TableHead>XDR path</TableHead>
              <TableHead>Checked by core</TableHead>
              <TableHead className="text-right">Entries</TableHead>
              <TableHead className="text-right">Core-emitted</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {coverage.fields.map((f) => (
              <TableRow key={f.id}>
                <TableCell>
                  <Id>{f.id}</Id>
                </TableCell>
                <TableCell>
                  <Id className="text-muted-foreground">{f.xdr_path}</Id>
                </TableCell>
                <TableCell>{checked[f.validated_by_core] ?? f.validated_by_core}</TableCell>
                <TableCell className="num text-right">{f.entries}</TableCell>
                <TableCell className="num text-right">{f.core_emitted_entries}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Section>
    </div>
  )
}
