import type { ReactNode } from "react"
import { Link } from "react-router"
import { cn } from "@/lib/utils"
import { label, verdicts, verdictMeaning } from "@/lib/terms"
import type { Counts } from "@/lib/data"
import { fmt } from "@/components/common"

// verdict marks: pass and the two failures differ in colour and, for dropped, in texture, so
// the bar still reads without colour; the legend names every part with its count
const verdictFill: Record<string, string> = {
  pass: "bg-pass",
  mishandled: "bg-diverge",
  dropped: "hatch border border-diverge bg-diverge/15 text-diverge",
  "not-evaluated": "hatch border border-idle bg-idle/15 text-idle",
  "out-of-scope": "bg-idle/55",
}

export const verdictOrder = verdicts.map((v) => v.id)

export function VerdictSwatch({ verdict, className }: { verdict: string; className?: string }) {
  return <span aria-hidden className={cn("inline-block size-3 shrink-0 rounded-[3px]", verdictFill[verdict], className)} />
}

// one horizontal bar split by verdict, 2px apart, widths from the counts
export function VerdictBar({ counts, className }: { counts: Counts; className?: string }) {
  const parts = verdictOrder.filter((v) => (counts[v] ?? 0) > 0)
  return (
    <span aria-hidden className={cn("flex h-3 w-full gap-[2px]", className)}>
      {parts.map((v) => (
        <span
          key={v}
          title={`${counts[v]} ${label(v)}`}
          className={cn("h-full min-w-[3px] first:rounded-l-[4px] last:rounded-r-[4px]", verdictFill[v])}
          style={{ flexGrow: counts[v] ?? 0, flexBasis: 0 }}
        />
      ))}
    </span>
  )
}

// the bar's legend, which is also its text equivalent: every verdict with its count
export function VerdictLegend({ counts, className, describe = false }: { counts: Counts; className?: string; describe?: boolean }) {
  const parts = verdictOrder.filter((v) => (counts[v] ?? 0) > 0)
  return (
    <ul className={cn("flex flex-wrap gap-x-4 gap-y-1 text-sm", className)}>
      {parts.map((v) => (
        <li key={v} className="flex items-center gap-1.5" title={describe ? undefined : verdictMeaning(v)}>
          <VerdictSwatch verdict={v} />
          <span className="num font-medium">{fmt.format(counts[v] ?? 0)}</span>
          <span className="text-muted-foreground">{label(v)}</span>
        </li>
      ))}
    </ul>
  )
}

// a share of a whole: label, bar on a muted track, "n of m"
export function Meter({
  label,
  value,
  total,
  to,
  children,
}: {
  label: ReactNode
  value: number
  total: number
  to: string
  children?: ReactNode
}) {
  const pct = total > 0 ? (value / total) * 100 : 0
  return (
    <li className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1.5 border-t py-3 sm:grid-cols-[minmax(12rem,16rem)_minmax(0,1fr)_7.5rem]">
      <Link
        to={to}
        className="font-medium text-foreground underline decoration-foreground/25 hover:decoration-foreground sm:col-start-1 sm:row-start-1"
      >
        {label}
      </Link>
      <span className="num text-right text-sm sm:col-start-3 sm:row-start-1">
        <span className="font-semibold">{fmt.format(value)}</span>
        <span className="text-muted-foreground"> of {fmt.format(total)}</span>
      </span>
      <span
        aria-hidden
        className="col-span-2 flex h-2 overflow-hidden rounded-full bg-muted sm:col-span-1 sm:col-start-2 sm:row-start-1"
        title={`${pct.toFixed(1)}%`}
      >
        <span className="h-full rounded-full bg-primary" style={{ width: `${pct}%` }} />
      </span>
      {children ? <span className="col-span-2 text-sm text-muted-foreground sm:col-span-3">{children}</span> : null}
    </li>
  )
}

// a small column chart over an sdk's versions: one series, a value on every column (there
// are few), the current version marked under its column. a table carries the same numbers for
// screen readers
export function VersionColumns({
  caption,
  rows,
  max,
}: {
  caption: string
  rows: { version: string; value: number; current: boolean }[]
  max: number
}) {
  return (
    <figure className="flex flex-col gap-1">
      <div aria-hidden className="grid h-28 items-end gap-[2px]" style={{ gridTemplateColumns: `repeat(${rows.length}, minmax(0, 1fr))` }}>
        {rows.map((r) => (
          <div key={r.version} className="flex h-full flex-col items-center justify-end gap-1" title={`${r.version}: ${r.value}`}>
            <span className="num text-xs text-foreground">{r.value}</span>
            <span
              className={cn("w-full max-w-9 rounded-t-[4px]", r.value > 0 ? "bg-primary" : "bg-border")}
              style={{ height: r.value > 0 ? `${Math.max((r.value / max) * 80, 3)}%` : "1px" }}
            />
          </div>
        ))}
      </div>
      <div aria-hidden className="grid gap-[2px] border-t pt-1" style={{ gridTemplateColumns: `repeat(${rows.length}, minmax(0, 1fr))` }}>
        {rows.map((r) => (
          <span key={r.version} className="flex flex-col items-center text-center leading-tight">
            <span className={cn("font-mono text-[0.68rem] [overflow-wrap:anywhere]", r.current ? "font-semibold text-foreground" : "text-muted-foreground")}>
              {r.version}
            </span>
            {r.current ? <span className="text-[0.68rem] text-muted-foreground">current</span> : null}
          </span>
        ))}
      </div>
      {/* sr-only on the table itself would not shrink it: a table keeps its content width */}
      <div className="sr-only">
      <table>
        <caption>{caption}</caption>
        <tbody>
          {rows.map((r) => (
            <tr key={r.version}>
              <th scope="row">
                {r.version}
                {r.current ? " (current)" : ""}
              </th>
              <td>{r.value}</td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    </figure>
  )
}
