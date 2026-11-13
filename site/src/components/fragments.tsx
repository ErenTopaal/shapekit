import { Link } from "react-router"
import type { FamilyRow, Fragment } from "@/lib/data"
import { cn } from "@/lib/utils"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"

export function FragmentSquare({ f }: { f: Fragment }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={`${f.fragment}: ${f.covered ? `${f.core_emitted} core-emitted entries` : "not covered"}`}
          className={cn(
            "block size-3.5 rounded-[3px] border focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none",
            f.covered ? "border-pass bg-pass" : "border-diverge bg-transparent",
          )}
        />
      </TooltipTrigger>
      <TooltipContent className="max-w-xs">
        <p className="font-mono text-xs break-all">{f.fragment}</p>
        <p className="text-xs">
          {f.covered
            ? `${f.core_emitted} core-emitted, ${f.non_core} other entries`
            : `not covered${f.uncovered ? `: ${f.uncovered.blocker}` : ""}`}
        </p>
      </TooltipContent>
    </Tooltip>
  )
}

export function Fragments({ fragments }: { fragments: Fragment[] }) {
  return (
    <ul className="flex flex-wrap gap-1" aria-label="Fragments">
      {fragments.map((f) => (
        <li key={f.fragment} className="leading-none">
          <FragmentSquare f={f} />
        </li>
      ))}
    </ul>
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
              to={cap ? `/dashboard/coverage/${cap}#${f.id}` : "/dashboard/coverage"}
              className="min-w-0 truncate font-mono text-sm hover:text-primary hover:underline"
              title={f.title}
            >
              {f.id}
            </Link>
            <span className="num text-right text-xs text-muted-foreground">
              {covered}/{f.fragments.length}
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
