import { Component, useEffect, type ReactNode } from "react"
import { useLocation } from "react-router"
import { cn } from "@/lib/utils"
import { blobUrl, useMeta } from "@/lib/meta"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Skeleton } from "@/components/ui/skeleton"

// the data arrives after the first paint, so an in-page anchor is scrolled to once the page renders
export function useHashScroll() {
  const { hash } = useLocation()
  useEffect(() => {
    if (!hash) return
    document.getElementById(decodeURIComponent(hash.slice(1)))?.scrollIntoView()
  }, [hash])
}

export function Id({ children, className }: { children: ReactNode; className?: string }) {
  return <code className={cn("font-mono text-[0.85em] [overflow-wrap:anywhere]", className)}>{children}</code>
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
      className={cn("text-primary underline decoration-primary/40 hover:decoration-primary", className)}
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
      className={cn("text-primary underline decoration-primary/40 hover:decoration-primary", className)}
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

const toneText: Record<Tone, string> = {
  pass: "text-pass",
  diverge: "text-diverge",
  known: "text-known",
  idle: "text-idle",
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
