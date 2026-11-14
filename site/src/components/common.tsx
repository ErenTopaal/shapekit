import { Component, useEffect, type ReactNode } from "react"
import { Link, useLocation } from "react-router"
import { ChevronDownIcon } from "lucide-react"
import { cn } from "@/lib/utils"
import { blobUrl, useMeta } from "@/lib/meta"
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
      issue draft <Id>{value.replace(/^findings\/issues\//, "")}</Id>
    </RepoLink>
  )
}

// the terms a page uses, defined once near the top so a reader new to the project can look them up
export function Terms({ children, label = "Terms used on this page" }: { children: ReactNode; label?: string }) {
  return (
    <Collapsible className="rounded-lg border bg-card text-sm">
      <CollapsibleTrigger className="group flex w-full items-center justify-between gap-2 rounded-lg px-4 py-2.5 text-left font-medium focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none">
        {label}
        <ChevronDownIcon aria-hidden className="size-4 shrink-0 transition-transform group-data-[state=open]:rotate-180" />
      </CollapsibleTrigger>
      <CollapsibleContent>
        <dl className="grid gap-x-6 gap-y-2.5 border-t px-4 py-3 sm:grid-cols-[minmax(8rem,auto)_1fr]">{children}</dl>
      </CollapsibleContent>
    </Collapsible>
  )
}

export function Term({ name, children }: { name: ReactNode; children: ReactNode }) {
  return (
    <>
      <dt className="font-medium">{name}</dt>
      <dd className="text-muted-foreground [&_a]:break-words">{children}</dd>
    </>
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
}: {
  title: string
  id?: string
  children: ReactNode
  description?: ReactNode
}) {
  return (
    <section id={id} className="flex scroll-mt-20 flex-col gap-3 py-6" aria-labelledby={id ? `${id}-h` : undefined}>
      <div className="flex flex-col gap-1">
        <h2 id={id ? `${id}-h` : undefined} className="text-lg font-semibold tracking-tight">
          {title}
        </h2>
        {description ? <p className="max-w-[68ch] text-sm text-muted-foreground">{description}</p> : null}
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

export function Status({ tone, children }: { tone: Tone; children: ReactNode }) {
  return (
    <span className={cn("inline-flex items-center gap-1.5 text-sm font-medium whitespace-nowrap", toneText[tone])}>
      <Dot tone={tone} />
      {children}
    </span>
  )
}

export function verdictTone(v: string): Tone {
  if (v === "pass") return "pass"
  if (v === "out-of-scope") return "idle"
  return "diverge"
}
