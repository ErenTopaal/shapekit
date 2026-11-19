import { use, useMemo } from "react"
import { Link, useSearchParams } from "react-router"
import { load } from "@/lib/data"
import { bySdkOrder, sdkName } from "@/lib/sdks"
import { docs, verdictMeaning } from "@/lib/terms"
import { sdkFailures } from "@/lib/results"
import { entryPath } from "@/lib/paths"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Breadcrumb, Id, NextSteps, PageHeader, PageTerms, Section, Status, TextLink, fmt, indexerResults } from "@/components/common"

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

// the list of every corpus entry, with filters kept in the address
export function EntriesPage() {
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
    setParams(next, { replace: true, preventScrollReset: true })
  }

  const fails = useMemo(() => sdkFailures(conformance), [conformance])
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
    <div className="flex flex-col gap-6">
      <div>
        <Breadcrumb items={[{ to: "/dashboard/corpus", label: "Corpus" }, { label: "Entries" }]} />
        <PageHeader title="Corpus entries">
          <p>
            This list shows every corpus <TextLink to="/dashboard/glossary#entry">entry</TextLink> with its CAPs, its
            source and the results of the SDKs and indexers. Select an entry to see its bytes, its decoded XDR-JSON and the
            commands that run it again. The address keeps the filters, so you can share a filtered list.
          </p>
        </PageHeader>
      </div>

      <NextSteps>
        <li>Select an SDK in the filters to list the entries on which that SDK diverges.</li>
        <li>Open an entry to get the command that runs it in your SDK or indexer.</li>
        <li>
          <TextLink to={docs.addingShape}>Add a shape to the corpus</TextLink> when the list does not have an entry that you
          need.
        </li>
      </NextSteps>

      <PageTerms ids={["entry", "family", "core-emitted", "diverging-entry", "verdict"]} />

      <Section
        title="Entries"
        id="entries"
        description={`${fmt.format(shown.length)} of ${fmt.format(entries.entries.length)} entries of corpus ${entries.corpus_version} shown.`}
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
          <FilterSelect label="Filter by CAP" value={cap} any="Any CAP" options={caps.map((c) => ({ value: c, label: c }))} onChange={(v) => set("cap", v, "all")} />
          <FilterSelect
            label="Filter by shape family"
            value={family}
            any="Any shape family"
            options={families.map((f) => ({ value: f, label: f }))}
            onChange={(v) => set("shape", v, "all")}
          />
          <FilterSelect
            label="Filter by an SDK that diverges"
            value={sdk}
            any="Any SDK result"
            options={bySdkOrder(conformance.sdks, (s) => s.sdk).map((s) => ({ value: s.sdk, label: `${sdkName(s.sdk)} diverges` }))}
            onChange={(v) => set("sdk", v, "all")}
          />
          {sdk !== "all" ? (
            <FilterSelect
              label="Filter by the diverging version"
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
                <TableHead className="hidden md:table-cell">CAPs</TableHead>
                <TableHead className="hidden md:table-cell">Source</TableHead>
                <TableHead>
                  Diverging SDK versions
                  <span className="block text-xs font-normal text-muted-foreground">none is best</span>
                </TableHead>
                <TableHead className="hidden lg:table-cell">Indexers</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {shown.map((e) => {
                const f = fails.get(e.id) ?? []
                const hx = indexerResults(harness.runs, e.id)
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
                      <span className="block text-xs text-muted-foreground md:hidden">
                        {e.source}
                        {e.caps.length > 0 ? `, ${e.caps.join(", ")}` : ""}
                      </span>
                    </TableCell>
                    <TableCell className="hidden align-top text-xs whitespace-normal md:table-cell">{e.caps.join(", ") || "none"}</TableCell>
                    <TableCell className="hidden align-top text-xs md:table-cell">{e.source}</TableCell>
                    <TableCell className="align-top text-xs whitespace-normal">
                      {f.length === 0 ? (
                        <span className="text-muted-foreground">none</span>
                      ) : (
                        <Status tone="diverge" wrap>
                          {f.map((x) => `${sdkName(x.sdk)} ${x.version}`).join(", ")}
                        </Status>
                      )}
                    </TableCell>
                    <TableCell className="hidden align-top text-xs whitespace-normal lg:table-cell">
                      {hx.length === 0 ? (
                        <span className="text-muted-foreground">not replayed</span>
                      ) : (
                        hx.map((r) => (
                          <span key={r.target} className="block" title={verdictMeaning(r.verdict)}>
                            <Status tone={r.tone} wrap>
                              {r.target}: {r.text}
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
