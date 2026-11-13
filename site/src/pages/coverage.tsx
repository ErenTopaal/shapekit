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
import { Id, useHashScroll, PageHeader, RepoLink, Section, Status, fmt } from "@/components/common"

type Scope = "visible" | "all" | "gaps"

function state(c: CapRow): { tone: "pass" | "diverge" | "idle" | "known"; label: string } {
  if (!c.indexer_visible) return { tone: "idle", label: "not indexer-visible" }
  if (c.core_emitted_entries > 0) return { tone: "pass", label: "core-emitted" }
  if (c.uncovered) return { tone: "known", label: "reason recorded" }
  if (c.entries > 0) return { tone: "known", label: "non-core only" }
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
          Every CAP in the registry, with the corpus entries that carry its shapes. An entry covers a shape only
          when stellar-core emitted it: core goldens, a local core, or public network history. Synthetic bytes
          and envelopes core refused are counted apart. Source:{" "}
          <RepoLink path="corpus/coverage/coverage.json" />.
        </p>
      </PageHeader>

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
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
          <ToggleGroupItem value="gaps">Without core entries</ToggleGroupItem>
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
          <TableHeader>
            <TableRow>
              <TableHead>CAP</TableHead>
              <TableHead className="min-w-56">Title</TableHead>
              <TableHead className="text-right">Protocol</TableHead>
              <TableHead>State</TableHead>
              <TableHead className="text-right">Families</TableHead>
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
                  <TableCell className="num text-right">{c.protocol ?? ""}</TableCell>
                  <TableCell>
                    <Status tone={st.tone}>{st.label}</Status>
                  </TableCell>
                  <TableCell className="num text-right">{c.families.length}</TableCell>
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
        description={`${s.arms_covered} of ${s.arms_reachable} union arms reachable from ${coverage.root_type} under stellar-xdr ${coverage.stellar_xdr} have a core-emitted entry. Each of the ${s.arms_uncovered} others has a recorded reason in registry/uncovered.toml.`}
      >
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Why an arm is uncovered</TableHead>
              <TableHead className="text-right">Arms</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {Object.entries(s.arms_uncovered_by_blocker).map(([k, v]) => (
              <TableRow key={k}>
                <TableCell>
                  <Id>{k}</Id>
                </TableCell>
                <TableCell className="num text-right">{v}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <Collapsible>
          <CollapsibleTrigger asChild>
            <Button variant="outline" size="sm" className="w-fit">
              Show the {coverage.arms.uncovered.length} uncovered arms
              <ChevronDownIcon data-icon="inline-end" />
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <Table className="mt-3">
              <TableHeader>
                <TableRow>
                  <TableHead>Arm</TableHead>
                  <TableHead>Blocker</TableHead>
                  <TableHead className="min-w-80">Reason</TableHead>
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
        title="Fields core does not fully validate"
        id="fields"
        description={`${s.fields_not_fully_validated_exercised} of ${s.fields_not_fully_validated} fields that core passes through partly or wholly unchecked are exercised by at least one entry. The guide's field table is built from the same registry.`}
      >
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Field</TableHead>
              <TableHead>XDR path</TableHead>
              <TableHead>Validated by core</TableHead>
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
                <TableCell>{f.validated_by_core}</TableCell>
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
