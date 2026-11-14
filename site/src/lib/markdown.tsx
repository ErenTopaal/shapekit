import type { ReactNode } from "react"
import { asMarkdown, md } from "fumadocs-core/server"

// how the jsx in the generated docs reads in the markdown served for llms (/llms.txt,
// /llms-full.txt, /llms.mdx/...). only route loaders import this, at build time; the pages use
// the components in src/components/mdx.tsx.

// the markdown of a component's children on one line: mdx wraps text inside a block element in
// a paragraph, and the guide's table cells break lines with <br />
async function inline(children: ReactNode): Promise<string> {
  return (await md`${children}`)
    .replace(/([.;:]?)\s*<br\s*\/?>\s*/g, (_, p: string) => (p ? `${p} ` : "; "))
    .replace(/\s*\n\s*/g, " ")
    .trim()
}

async function Callout({ title, children }: { title?: ReactNode; children?: ReactNode }) {
  asMarkdown()
  const body = (await md`${children}`).trim()
  return title ? md.linePrefix("> ")`**${title}**\n\n${body}` : md.linePrefix("> ")`${body}`
}

async function Cards({ children }: { children?: ReactNode }) {
  asMarkdown()
  return `\n${(await md`${children}`).trim()}\n\n`
}

async function Card({ href, title, children }: { href?: string; title?: ReactNode; children?: ReactNode }) {
  asMarkdown()
  return `- [${await inline(title)}](${href ?? ""}): ${await inline(children)}\n`
}

function PageIcon() {
  asMarkdown()
  return ""
}

// a row of the guide's field or caps table: its head, then one line per column
async function Record({ children }: { children?: ReactNode }) {
  asMarkdown()
  return `\n${(await md`${children}`).trim()}\n\n`
}

async function RecordHead({ children }: { children?: ReactNode }) {
  asMarkdown()
  return `- ${await inline(children)}\n`
}

async function RecordField({ label, children }: { label: string; children?: ReactNode }) {
  asMarkdown()
  return `  - ${label}: ${await inline(children)}\n`
}

export const markdownComponents = { Callout, Cards, Card, PageIcon, Record, RecordHead, RecordField }
