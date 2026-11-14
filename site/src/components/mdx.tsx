import type { ReactNode } from "react"
import defaultMdxComponents from "fumadocs-ui/mdx"
import type { MDXComponents } from "mdx/types"
import { icons } from "@/lib/icons"
import { cn } from "@/lib/utils"

// the icon of a card on the index page, by the lucide name the content script gives it
function PageIcon({ name }: { name: string }) {
  const Icon = icons[name]
  if (!Icon) throw new Error(`no icon ${name}`)
  return <Icon />
}

// a row of the guide's field and caps tables, which the content script turns into records
// because their columns hold paragraphs that no table in a docs column can fit
function Record({ children }: { children?: ReactNode }) {
  return (
    <section
      className={cn(
        "not-prose my-4 rounded-lg border bg-fd-card text-sm",
        // the records sit outside the prose styles, so links and inline code get theirs here
        "[&_a]:font-medium [&_a]:text-fd-primary [&_a]:underline [&_a]:decoration-fd-primary/40 [&_a]:underline-offset-4 hover:[&_a]:decoration-fd-primary",
        "[&_code]:font-mono [&_code]:text-[0.8125rem] [&_code]:[overflow-wrap:anywhere]",
      )}
    >
      {children}
    </section>
  )
}

function RecordHead({ children }: { children?: ReactNode }) {
  return <div className="border-b px-4 py-2.5 font-medium">{children}</div>
}

function RecordField({ label, children, className }: { label: string; children?: ReactNode; className?: string }) {
  return (
    <div className={cn("grid gap-x-4 gap-y-0.5 px-4 py-2 sm:grid-cols-[10rem_minmax(0,1fr)]", className)}>
      <div className="text-xs font-medium tracking-wide text-fd-muted-foreground uppercase sm:pt-0.5">{label}</div>
      <div className="min-w-0 leading-relaxed break-words">{children}</div>
    </div>
  )
}

export function getMDXComponents(components?: MDXComponents) {
  return { ...defaultMdxComponents, PageIcon, Record, RecordHead, RecordField, ...components } satisfies MDXComponents
}

export const useMDXComponents = getMDXComponents

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>
}
