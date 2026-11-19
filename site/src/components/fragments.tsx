import { Link } from "react-router"
import type { FamilyRow, Fragment } from "@/lib/data"
import { cn } from "@/lib/utils"
import { capPath } from "@/lib/paths"

// one square per fragment, filled when a core-emitted entry has it. the squares are a picture
// of the table on the family's cap page, so they are not focusable: the count next to them is
// the text equivalent, and a native tooltip names the fragment on hover
export function Fragments({ fragments }: { fragments: Fragment[] }) {
  return (
    <span aria-hidden className="flex flex-wrap gap-1">
      {fragments.map((f) => (
        <span
          key={f.fragment}
          title={`${f.fragment}: ${f.covered ? `${f.core_emitted} entries from stellar-core` : `no entry from stellar-core${f.uncovered ? ` (${f.uncovered.blocker})` : ""}`}`}
          className={cn("block size-3.5 rounded-[3px] border", f.covered ? "border-pass bg-pass" : "border-diverge bg-transparent")}
        />
      ))}
    </span>
  )
}

export function FamilyStrip({ families }: { families: FamilyRow[] }) {
  return (
    <ol className="grid gap-x-8 lg:grid-cols-2">
      {families.map((f) => {
        const covered = f.fragments.filter((x) => x.covered).length
        const cap = f.caps[0]
        return (
          <li key={f.id} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 border-t py-2">
            <Link
              to={cap ? capPath(cap, f.id) : "/dashboard/corpus#families"}
              className="min-w-0 truncate font-mono text-sm underline decoration-foreground/20 hover:text-primary hover:decoration-primary"
              title={f.title}
            >
              {f.id}
            </Link>
            <span className="num text-right text-xs text-muted-foreground">
              {covered}/{f.fragments.length}
              <span className="sr-only"> fragments with an entry from stellar-core</span>
            </span>
            <span className="col-span-2">
              <Fragments fragments={f.fragments} />
            </span>
          </li>
        )
      })}
    </ol>
  )
}
