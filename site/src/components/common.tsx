import { Component, useEffect, type ReactNode } from "react"
import { Link, useLocation } from "react-router"
import { CheckIcon, ChevronDownIcon, CircleAlertIcon, CopyIcon, InfoIcon, MinusIcon, XIcon } from "lucide-react"
import { cn } from "@/lib/utils"
import { blobUrl, useMeta } from "@/lib/meta"
import { glossary, glossaryHref, label as termLabel } from "@/lib/terms"
import { betterText, type Better } from "@/lib/metrics"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Skeleton } from "@/components/ui/skeleton"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"

const linkClass = "text-primary underline decoration-primary/40 hover:decoration-primary"

// a page of this site
export function TextLink({
  to,
  children,
  className,
  "aria-label": label,
}: {
  to: string
  children: ReactNode
  className?: string
  "aria-label"?: string
}) {
  return (
    <Link to={to} className={cn(linkClass, className)} aria-label={label}>
      {children}
    </Link>
  )
}

// a known-failure or finding reference: an upstream url, or a drafted issue body in the repository
export function IssueRef({ value, className }: { value: string; className?: string }) {
  if (/^https?:\/\//.test(value)) {
    return (
      <ExtLink href={value} className={className}>
        {issueLabel(value)}
      </ExtLink>
    )
  }
  return (
    <RepoLink path={value} className={className}>
      issue draft <Id>{value.replace(/^(candidate\/)?findings\/issues\//, "")}</Id>
    </RepoLink>
  )
}

// a term of the glossary page, linked from the page that uses it
export function GlossaryLink({ id, children, className }: { id: string; children: ReactNode; className?: string }) {
  return (
    <Link to={glossaryHref(id)} className={cn("underline decoration-dotted decoration-foreground/40 underline-offset-2 hover:decoration-foreground", className)}>
      {children}
    </Link>
  )
}

// a page can name a whole group of terms (its id) or one term
const termNames = new Map([
  ...glossary.map((g) => [g.id, g.title.toLowerCase()] as const),
  ...glossary.flatMap((g) => g.terms.map((t) => [t.id, t.name] as const)),
])

// the terms a page uses, each linked to its one definition on the glossary page
export function PageTerms({ ids }: { ids: string[] }) {
  return (
    <p className="text-sm text-muted-foreground">
      Terms on this page:{" "}
      {ids.map((id, i) => (
        <span key={id}>
          <TextLink to={glossaryHref(id)}>{termNames.get(id) ?? id}</TextLink>
          {i < ids.length - 1 ? ", " : ". "}
        </span>
      ))}
      <TextLink to="/dashboard/glossary">All terms</TextLink>.
    </p>
  )
}

// the next action of the page's reader: a short box near the top of the page
export function NextSteps({ children }: { children: ReactNode }) {
  return (
    <aside aria-label="Next steps" className="flex flex-col gap-1.5 rounded-lg border border-primary/30 bg-primary/5 px-4 py-3 text-sm">
      <span className="font-semibold">Next steps</span>
      <ul className="flex list-disc flex-col gap-1 pl-5">{children}</ul>
    </aside>
  )
}

// a number with what it counts and which direction is better
export function Metric({ value, label, better, children }: { value: ReactNode; label: ReactNode; better?: Better; children?: ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="num text-2xl font-semibold tracking-tight">{value}</span>
      <span className="text-sm font-medium">{label}</span>
      {better ? <span className="text-xs text-muted-foreground">{betterText[better]}</span> : null}
      {children ? <span className="text-xs text-muted-foreground">{children}</span> : null}
    </div>
  )
}

// a shell command, with a button that copies it
export function Command({ children }: { children: string }) {
  return (
    <div className="flex min-w-0 items-start gap-2 rounded-md border bg-muted/40 px-3 py-2">
      <pre className="min-w-0 flex-1 font-mono text-xs whitespace-pre-wrap [overflow-wrap:anywhere]">{children}</pre>
      <button
        type="button"
        onClick={() => void navigator.clipboard?.writeText(children)}
        className="shrink-0 rounded-sm p-1 text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
        aria-label="Copy the command"
        title="Copy the command"
      >
        <CopyIcon aria-hidden className="size-3.5" />
      </button>
    </div>
  )
}

// the path from the dashboard to a page below a section page
export function Breadcrumb({ items }: { items: { to?: string; label: ReactNode; mono?: boolean }[] }) {
  return (
    <nav aria-label="Breadcrumb" className="pb-3 text-sm">
      <ol className="flex flex-wrap items-center gap-1.5 text-muted-foreground">
        {items.map((x, i) => (
          <li key={i} className="flex items-center gap-1.5">
            {i > 0 ? <span aria-hidden>/</span> : null}
            {x.to ? (
              <TextLink to={x.to}>{x.label}</TextLink>
            ) : (
              <span aria-current="page" className={cn(x.mono && "font-mono [overflow-wrap:anywhere]")}>
                {x.label}
              </span>
            )}
          </li>
        ))}
      </ol>
    </nav>
  )
}

// a part of a page the reader opens when they need it
export function More({ label, children, className }: { label: ReactNode; children: ReactNode; className?: string }) {
  return (
    <Collapsible className={cn("rounded-lg border", className)}>
      <CollapsibleTrigger className="group flex w-full items-center justify-between gap-2 rounded-lg px-4 py-2.5 text-left text-sm font-medium focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none">
        {label}
        <ChevronDownIcon aria-hidden className="size-4 shrink-0 transition-transform group-data-[state=open]:rotate-180" />
      </CollapsibleTrigger>
      <CollapsibleContent className="flex min-w-0 flex-col gap-3 border-t px-4 py-3">{children}</CollapsibleContent>
    </Collapsible>
  )
}

// the data arrives after the first paint, so an in-page anchor is scrolled to once the page renders
export function useHashScroll() {
  const { hash } = useLocation()
  useEffect(() => {
    if (!hash) return
    document.getElementById(decodeURIComponent(hash.slice(1)))?.scrollIntoView()
  }, [hash])
}

export function Id({ children, className, title }: { children: ReactNode; className?: string; title?: string }) {
  return (
    <code className={cn("font-mono text-[0.85em] [overflow-wrap:anywhere]", className)} title={title}>
      {children}
    </code>
  )
}

// a file in the repository, opened on github
export function RepoLink({
  path,
  children,
  className,
}: {
  path: string
  children?: ReactNode
  className?: string
}) {
  const meta = useMeta()
  return (
    <a
      href={blobUrl(meta, path)}
      className={cn(linkClass, className)}
      rel="noreferrer"
      target="_blank"
    >
      {children ?? <Id>{path}</Id>}
    </a>
  )
}

export function ExtLink({ href, children, className }: { href: string; children: ReactNode; className?: string }) {
  return (
    <a
      href={href}
      className={cn(linkClass, className)}
      rel="noreferrer"
      target="_blank"
    >
      {children}
    </a>
  )
}

// github issue and pull request urls read better as owner/repo#n
export function issueLabel(url: string): string {
  const m = url.match(/^https:\/\/github\.com\/([^/]+\/[^/]+)\/(?:issues|pull)\/(\d+)/)
  if (m) return `${m[1]}#${m[2]}`
  const t = url.match(/^https:\/\/github\.com\/([^/]+\/[^/]+)\/releases\/tag\/(\S+)/)
  if (t) return `${t[1]} ${t[2]}`
  const a = url.match(/^https:\/\/github\.com\/(?:[^/]+\/[^/]+\/security\/)?advisories\/(\S+)/)
  if (a) return a[1] ?? url
  return url
}

export function PageHeader({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <header className="flex flex-col gap-3 pb-6">
      <h1 className="text-2xl font-semibold tracking-tight text-balance sm:text-3xl">{title}</h1>
      {children ? <div className="max-w-[68ch] text-muted-foreground">{children}</div> : null}
    </header>
  )
}

export function Section({
  title,
  id,
  children,
  description,
  level = 2,
}: {
  title: string
  id?: string
  children: ReactNode
  description?: ReactNode
  // a section inside another section is a level 3 heading
  level?: 2 | 3
}) {
  const H = level === 2 ? "h2" : "h3"
  return (
    <section id={id} className="flex min-w-0 scroll-mt-20 flex-col gap-3 py-2" aria-labelledby={id ? `${id}-h` : undefined}>
      <div className="flex flex-col gap-1">
        <H id={id ? `${id}-h` : undefined} className={level === 2 ? "text-lg font-semibold tracking-tight" : "text-base font-semibold"}>
          {title}
        </H>
        {description ? <div className="max-w-[70ch] text-sm text-muted-foreground">{description}</div> : null}
      </div>
      {children}
    </section>
  )
}

export function Loading() {
  return (
    <output className="flex flex-col gap-3 py-6" aria-live="polite">
      <span className="sr-only">Loading data</span>
      <Skeleton className="h-8 w-64" />
      <Skeleton className="h-4 w-full max-w-xl" />
      <Skeleton className="h-64 w-full" />
    </output>
  )
}

export function DataError({ message }: { message: string }) {
  return (
    <Alert variant="destructive" className="my-6">
      <AlertTitle>The data files did not load</AlertTitle>
      <AlertDescription>
        <p>{message}</p>
        <p>
          Run <Id>shapekit site data --out site/public/data --commit $(git rev-parse HEAD)</Id> from the
          repository root, then reload.
        </p>
      </AlertDescription>
    </Alert>
  )
}

export class DataBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null }
  static getDerivedStateFromError(error: Error) {
    return { error }
  }
  render() {
    if (this.state.error) return <DataError message={this.state.error.message} />
    return this.props.children
  }
}

export const fmt = new Intl.NumberFormat("en-US")

export function bytes(n: number): string {
  if (n >= 1 << 30) return `${(n / (1 << 30)).toFixed(1)} GiB`
  if (n >= 1 << 20) return `${(n / (1 << 20)).toFixed(1)} MiB`
  if (n >= 1 << 10) return `${(n / (1 << 10)).toFixed(1)} KiB`
  return `${n} B`
}

export function short(sha: string, n = 8): string {
  return sha.slice(0, n)
}

// the status palette: the only colours with meaning on the site
export type Tone = "pass" | "diverge" | "known" | "idle" | "signal"

// text in the idle grey would fall below 4.5:1, so idle text is muted ink and only its dot is grey
const toneText: Record<Tone, string> = {
  pass: "text-pass",
  diverge: "text-diverge",
  known: "text-known",
  idle: "text-muted-foreground",
  signal: "text-signal",
}

const toneBg: Record<Tone, string> = {
  pass: "bg-pass",
  diverge: "bg-diverge",
  known: "bg-known",
  idle: "bg-idle",
  signal: "bg-signal",
}

export function Dot({ tone, className }: { tone: Tone; className?: string }) {
  return <span aria-hidden className={cn("inline-block size-2 shrink-0 rounded-full", toneBg[tone], className)} />
}

const toneIcon: Record<Tone, typeof CheckIcon> = {
  pass: CheckIcon,
  diverge: XIcon,
  known: CircleAlertIcon,
  idle: MinusIcon,
  signal: InfoIcon,
}

// a status: an icon and a text, so it reads without colour
export function Status({ tone, children, wrap = false }: { tone: Tone; children: ReactNode; wrap?: boolean }) {
  const Icon = toneIcon[tone]
  return (
    <span className={cn("inline-flex items-center gap-1.5 text-sm font-medium", wrap ? "" : "whitespace-nowrap", toneText[tone])}>
      <Icon aria-hidden className="size-3.5 shrink-0" strokeWidth={2.5} />
      {children}
    </span>
  )
}

export function verdictTone(v: string): Tone {
  if (v === "pass") return "pass"
  if (v === "out-of-scope") return "idle"
  return "diverge"
}

// each committed indexer run's verdicts on one entry, one phrase per distinct verdict
// ("pass on mainnet and testnet"); runs that did not replay the entry are left out
export function indexerResults(
  runs: { describe: { target: string } | null; verdicts: Record<string, Record<string, { verdict: string; reason: string | null }>> }[],
  id: string,
): { target: string; text: string; tone: Tone; verdict: string }[] {
  return runs.flatMap((run, i) => {
    const v = run.verdicts[id]
    if (!v) return []
    const by = new Map<string, string[]>()
    for (const [net, r] of Object.entries(v)) {
      const k = r.reason ? `${termLabel(r.verdict)} (${r.reason})` : termLabel(r.verdict)
      by.set(k, [...(by.get(k) ?? []), net])
    }
    const text = [...by].map(([k, nets]) => `${k} on ${nets.toSorted().join(" and ")}`).join("; ")
    const worst = Object.values(v).find((r) => r.verdict !== "pass" && r.verdict !== "out-of-scope") ?? Object.values(v)[0]
    const verdict = worst?.verdict ?? "pass"
    return [{ target: run.describe?.target ?? `run ${i + 1}`, text, tone: verdictTone(verdict), verdict }]
  })
}

// the indexer page of one run
export function indexerHref(target: string, q?: string): string {
  const p = new URLSearchParams({ target })
  if (q) {
    p.set("q", q)
    p.set("network", "both")
  }
  return `/dashboard/indexer?${p.toString()}${q ? "#problems" : ""}`
}
