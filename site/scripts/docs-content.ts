// builds the fumadocs content directory from the repository's own docs, so ../docs and
// ../README.md stay the only copies. run before every dev server start and build.
//
// what it changes on the way: a title and description in frontmatter (from the first heading
// and paragraph), relative links between docs rewritten to site routes, links to any other
// repository file pointed at github, and the few stellar-docs constructs in the guide that
// fumadocs spells differently.

import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs"
import path from "node:path"

const site = path.resolve(import.meta.dirname, "..")
const repo = path.resolve(site, "..")
const out = path.join(site, "content", "docs")
const blobBase = "https://github.com/ErenTopaal/shapekit"
const stellarDocs = "https://developers.stellar.org"

interface Page {
  // route under /docs, "" for the index
  route: string
  // repository path the page is made from
  source: string
  // file written under content/docs
  file: string
  title: string
  description: string
  // the description is shown under the title only when the body does not already open with it
  describe: boolean
  body: string
}

interface Group {
  title: string
  // lucide icon name, shown next to the group's separator in the sidebar and on its index cards
  icon: string
  // folder under content/docs, "" for the root
  dir: string
  docs: { source: string; slug: string }[]
  // extra sidebar links placed after the group's pages
  links?: { text: string; url: string }[]
}

// sidebar order. readme-derived pages are made separately and lead "getting started".
const groups: Group[] = [
  { title: "Getting started", icon: "BookOpen", dir: "", docs: [] },
  {
    title: "Corpus",
    icon: "Boxes",
    dir: "corpus",
    docs: [{ source: "docs/adding-a-shape.md", slug: "adding-a-shape" }],
    links: [
      { text: "Coverage per CAP", url: "/dashboard/coverage" },
      { text: "Corpus explorer", url: "/dashboard/corpus" },
    ],
  },
  {
    title: "Harness",
    icon: "Repeat",
    dir: "harness",
    docs: [
      { source: "docs/adapter-interface.md", slug: "adapter-interface" },
      { source: "docs/mapping-adapter.md", slug: "mapping-adapter" },
      { source: "docs/replay-server.md", slug: "replay-server" },
      { source: "docs/indexer-ci.md", slug: "indexer-ci" },
    ],
  },
  {
    title: "SDK conformance",
    icon: "ListChecks",
    dir: "sdk",
    docs: [
      { source: "docs/testee-protocol.md", slug: "testee-protocol" },
      { source: "docs/sdk-ci.md", slug: "sdk-ci" },
      { source: "docs/seeded-faults.md", slug: "seeded-faults" },
    ],
  },
  { title: "Results", icon: "FileText", dir: "results", docs: [{ source: "docs/report.md", slug: "report" }] },
  { title: "Linter", icon: "ScanSearch", dir: "linter", docs: [{ source: "docs/linter.md", slug: "indexerlint" }] },
  { title: "Guide", icon: "ShieldCheck", dir: "guide", docs: [{ source: "docs/guide/indexer-hardening.mdx", slug: "indexer-hardening" }] },
  {
    title: "Operations",
    icon: "Wrench",
    dir: "operations",
    docs: [
      { source: "docs/infrastructure.md", slug: "infrastructure" },
      { source: "docs/maintenance.md", slug: "maintenance" },
      { source: "docs/protocol-upgrade-runbook.md", slug: "protocol-upgrade-runbook" },
      { source: "docs/differential-fuzzing.md", slug: "differential-fuzzing" },
      { source: "docs/site.md", slug: "site" },
    ],
  },
]

// the readme is split by its second-level sections; the rest of it stays on the index page
const readmePages = [
  {
    route: "install",
    title: "Install",
    icon: "Download",
    description: "Release binaries and their checks, building from source, and the checks CI runs.",
    sections: ["Install", "Building and checking"],
  },
  {
    route: "quick-start",
    title: "Quick start",
    icon: "Terminal",
    description:
      "Commands for checking the corpus, running an SDK over it, replaying chains into an indexer, linting Go indexer code, applying the guide to stellar-docs and regenerating the corpus.",
    sections: ["Quick start"],
  },
]

function slugify(heading: string): string {
  // github's anchor rule, which fumadocs' heading ids follow too
  return heading
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, "")
    .replace(/\s/g, "-")
}

// splits markdown into fenced code and prose so rewrites never touch code
function mapProse(md: string, fn: (prose: string) => string): string {
  const parts = md.split(/(^```[\s\S]*?^```[^\n]*$)/m)
  return parts.map((p, i) => (i % 2 === 1 ? p : fn(p))).join("")
}

function plain(md: string): string {
  return md
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[`*_]/g, "")
    .replace(/\s+/g, " ")
    .trim()
}

function firstSentence(text: string): string {
  const m = text.match(/^.+?[.:](?=\s|$)/)
  const s = (m ? m[0] : text).replace(/:$/, ".")
  return s.length > 240 ? `${s.slice(0, s.lastIndexOf(" ", 237))}...` : s
}

function splitTitle(md: string): { title: string; rest: string } {
  const m = md.match(/^# (.+)\n/m)
  if (!m || m.index === undefined) throw new Error("no first-level heading")
  return { title: plain(m[1] ?? ""), rest: md.slice(m.index + m[0].length) }
}

function firstParagraph(md: string): string {
  for (const block of md.split(/\n\s*\n/)) {
    const b = block.trim()
    if (b && !b.startsWith("#") && !b.startsWith("```") && !b.startsWith("|") && !b.startsWith("<")) return plain(b)
  }
  return ""
}

function frontmatterField(raw: string, k: string, source: string): string {
  const fm = raw.match(/^---\n([\s\S]*?)\n---\n/)
  if (!fm) throw new Error(`${source}: no frontmatter`)
  const m = (fm[1] ?? "").match(new RegExp(`^${k}:\\s*(.+)$`, "m"))
  if (!m) throw new Error(`${source}: no ${k} in frontmatter`)
  const v = (m[1] ?? "").trim()
  return v.startsWith('"') ? (JSON.parse(v) as string) : v
}

function readmeSections(md: string): { intro: string; sections: Map<string, string> } {
  // the badge lines load images from github.com and shields.io, which the site's same-origin
  // content security policy blocks, and the site shows the same facts on its own pages
  md = md.replace(/^\[!\[[^\]]*\]\([^)]*\)\]\([^)]*\)\n/gm, "")
  const chunks = md.split(/^## /m)
  const intro = chunks.shift() ?? ""
  const sections = new Map<string, string>()
  for (const c of chunks) {
    const nl = c.indexOf("\n")
    sections.set(c.slice(0, nl).trim(), c.slice(nl + 1))
  }
  return { intro, sections }
}

const readme = readFileSync(path.join(repo, "README.md"), "utf8")
const { intro: readmeIntro, sections: readmeSecs } = readmeSections(readme)

// readme anchors resolve to the page their section landed on
const readmeAnchor = new Map<string, string>()
for (const [name, body] of readmeSecs) {
  const owner = readmePages.find((p) => p.sections.includes(name))
  const route = owner ? `/docs/${owner.route}` : "/docs"
  readmeAnchor.set(slugify(name), owner && owner.sections[0] === name ? route : `${route}#${slugify(name)}`)
  for (const m of body.matchAll(/^###+ (.+)$/gm)) readmeAnchor.set(slugify(m[1] ?? ""), `${route}#${slugify(m[1] ?? "")}`)
}

// repository path of every doc that becomes a page, to its route
const routes = new Map<string, string>([["README.md", "/docs"]])
for (const g of groups) for (const d of g.docs) routes.set(d.source, `/docs/${g.dir}/${d.slug}`)

// every page's title by route, for link text that names a doc file
const titles = new Map<string, string>([["/docs", "Introduction"]])
for (const p of readmePages) titles.set(`/docs/${p.route}`, p.title)
for (const g of groups) {
  for (const d of g.docs) {
    const raw = readFileSync(path.join(repo, d.source), "utf8")
    titles.set(`/docs/${g.dir}/${d.slug}`, d.source.endsWith(".mdx") ? frontmatterField(raw, "title", d.source) : splitTitle(raw).title)
  }
}

let missing = 0
function rewriteLinks(md: string, source: string): string {
  const dir = path.posix.dirname(source)
  const fix = (target: string): string => {
    if (/^[a-z]+:/i.test(target)) return target
    // the readme is split over several pages, so its own anchors may now live on another page
    if (target.startsWith("#")) return source === "README.md" ? (readmeAnchor.get(target.slice(1)) ?? target) : target
    // site-absolute paths in the guide are developers.stellar.org pages
    if (target.startsWith("/")) return `${stellarDocs}${target}`
    const [file = "", anchor] = target.split("#")
    const resolved = path.posix.normalize(path.posix.join(dir, file))
    if (resolved === "README.md" && anchor) return readmeAnchor.get(anchor) ?? `/docs#${anchor}`
    const route = routes.get(resolved)
    if (route) return anchor ? `${route}#${anchor}` : route
    const abs = path.join(repo, resolved)
    if (!existsSync(abs)) {
      missing++
      console.warn(`docs-content: ${source} links to ${target}, which is not in the repository`)
    }
    const kind = existsSync(abs) && statSync(abs).isDirectory() ? "tree" : "blob"
    return `${blobBase}/${kind}/main/${resolved}${anchor ? `#${anchor}` : ""}`
  }
  return mapProse(md, (prose) =>
    prose
      .replace(/(\]\()([^)\s]+)((?:\s+"[^"]*")?\))/g, (_, a: string, t: string, b: string) => `${a}${fix(t)}${b}`)
      .replace(/^(\[[^\]]+\]:\s+)(\S+)/gm, (_, a: string, t: string) => `${a}${fix(t)}`)
      // on github a link reading "docs/linter.md" names the file it opens; on the site it opens
      // a page, so it reads as that page's title
      .replace(/\[`?((?:[\w-]+\/)*[\w-]+\.mdx?)`?\]\((\/docs[^)#\s]*)(#[^)\s]*)?\)/g, (m: string, _f: string, route: string, anchor?: string) => {
        const title = titles.get(route)
        return title ? `[${title}](${route}${anchor ?? ""})` : m
      }),
  )
}

function frontmatter(title: string, description: string): string {
  return `---\ntitle: ${JSON.stringify(title)}\ndescription: ${JSON.stringify(description)}\n---\n\n`
}

const pages: Page[] = []
// mark where the index page's cards go, filled in once every page's description is known: the
// readme's own pages after the introduction, as the next step, and every other page at the end
const START_CARDS = "{/* start cards */}"
const ALL_CARDS = "{/* all cards */}"

// index: the readme's introduction and its closing sections
{
  const { rest } = splitTitle(readmeIntro)
  const tail = ["What is in it", "Infrastructure and privacy", "Maintenance", "License"]
    .filter((n) => readmeSecs.has(n))
    .map((n) => `## ${n}\n${readmeSecs.get(n)}`)
    .join("\n")
  // mdx, so the cards to every other page can sit between the introduction and the rest; the
  // readme has no jsx-like text outside code, which mdx would otherwise read as markup
  pages.push({
    route: "",
    source: "README.md",
    file: "index.mdx",
    title: "Introduction",
    description: firstSentence(firstParagraph(rest)),
    describe: false,
    body: rewriteLinks(`${rest.trim()}\n\n${START_CARDS}\n\n${tail}\n\n## All pages\n\n${ALL_CARDS}`, "README.md"),
  })
}

for (const p of readmePages) {
  const body = p.sections
    .map((name) => {
      const sec = readmeSecs.get(name)
      if (sec === undefined) throw new Error(`README.md has no "## ${name}" section`)
      // the first section's subsections become the page's own sections
      return p.sections[0] === name ? sec.replace(/^###/gm, "##") : `## ${name}\n${sec}`
    })
    .join("\n")
  pages.push({
    route: p.route,
    source: "README.md",
    file: `${p.route}.md`,
    title: p.title,
    description: p.description,
    describe: true,
    body: rewriteLinks(body.trim(), "README.md"),
  })
}

for (const g of groups) {
  for (const d of g.docs) {
    const raw = readFileSync(path.join(repo, d.source), "utf8")
    const file = path.posix.join(g.dir, `${d.slug}${path.extname(d.source)}`)
    if (d.source.endsWith(".mdx")) {
      pages.push({ route: `${g.dir}/${d.slug}`, source: d.source, file, ...guide(raw, d.source) })
      continue
    }
    const { title, rest } = splitTitle(raw)
    pages.push({
      route: `${g.dir}/${d.slug}`,
      source: d.source,
      file,
      title,
      description: firstSentence(firstParagraph(rest)),
      describe: false,
      body: rewriteLinks(rest.trim(), d.source),
    })
  }
}

// the guide is written for stellar-docs: its frontmatter already has a title and description,
// its heading ids use {#id}, which mdx would read as an expression, and <Warning> is a
// stellar-docs component that fumadocs calls a warn callout
function guide(raw: string, source: string): Pick<Page, "title" | "description" | "describe" | "body"> {
  const fm = raw.match(/^---\n[\s\S]*?\n---\n/)
  if (!fm) throw new Error(`${source}: no frontmatter`)
  const field = (k: string) => frontmatterField(raw, k, source)
  let body = raw.slice(fm[0].length)
  body = mapProse(body, (prose) =>
    prose
      .replace(/^(#{1,6} .+?)[ \t]*\{#([\w-]+)\}[ \t]*$/gm, "$1 [#$2]")
      .replace(/<Warning>/g, '<Callout type="warn">')
      .replace(/<\/Warning>/g, "</Callout>"),
  )
  const notice =
    '<Callout type="info" title="Draft for developers.stellar.org">\n\nThis guide is written for developers.stellar.org. ' +
    "It is published there after review by a stellar-core or platform engineer; this page is the draft for that review.\n\n</Callout>\n\n"
  return {
    title: field("title"),
    description: field("description"),
    describe: true,
    body: notice + recordTables(rewriteLinks(body.trim(), source), ["field-table", "caps-table"]),
  }
}

// the cells of a markdown table row; a pipe inside a cell is escaped as \|
function cells(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split(/(?<!\\)\|/)
    .map((c) => c.trim())
}

// mdx text with the characters it would read as markup written as character references
function esc(t: string): string {
  return t.replace(/[{}<>]/g, (c) => `&#${c.charCodeAt(0)};`)
}

// the guide's field and caps tables have five and six columns of paragraphs, which no docs
// column is wide enough for: on the site each row becomes a record, its first cell as the head
// and every other cell under its column name. the source keeps the tables stellar-docs renders.
function recordTables(md: string, blocks: string[]): string {
  for (const name of blocks) {
    const re = new RegExp(`(\\{/\\* shapekit:begin ${name} \\*/\\}\\n)([\\s\\S]*?)(\\{/\\* shapekit:end ${name} \\*/\\})`)
    const m = md.match(re)
    if (!m) throw new Error(`guide: no generated block ${name}`)
    const lines = (m[2] ?? "").split("\n")
    const start = lines.findIndex((l) => l.startsWith("|"))
    const head = cells(lines[start] ?? "")
    if (!/^\|(\s*-+\s*\|)+$/.test(lines[start + 1] ?? "")) throw new Error(`guide: ${name} has no table`)
    let end = start + 2
    while (end < lines.length && (lines[end] ?? "").startsWith("|")) end++
    const records = lines.slice(start + 2, end).map((row) => {
      const c = cells(row)
      if (c.length !== head.length) throw new Error(`guide: ${name} row has ${c.length} cells, not ${head.length}: ${row.slice(0, 80)}`)
      const fields = head
        .slice(1)
        .map((h, i) => (c[i + 1] ? `<RecordField label=${JSON.stringify(h)}>${c[i + 1]}</RecordField>` : ""))
        .filter(Boolean)
      return [`<Record>`, `<RecordHead>${c[0]}</RecordHead>`, ...fields, `</Record>`].join("\n")
    })
    const replaced = [...lines.slice(0, start), records.join("\n\n"), ...lines.slice(end)].join("\n")
    md = md.replace(re, (_, a: string, _b: string, z: string) => `${a}${replaced}${z}`)
  }
  return md
}

// cards to pages, in sidebar order, for the index page: the getting-started group's pages, or
// every other group's
function cards(start: boolean): string {
  const items: string[] = []
  for (const g of groups.filter((x) => (x.dir === "") === start)) {
    const list =
      g.dir === ""
        ? readmePages.map((p) => ({ route: p.route, icon: p.icon }))
        : g.docs.map((d) => ({ route: `${g.dir}/${d.slug}`, icon: g.icon }))
    for (const it of list) {
      const p = pages.find((x) => x.route === it.route)
      if (!p) throw new Error(`no page for ${it.route}`)
      items.push(
        `<Card href=${JSON.stringify(`/docs/${it.route}`)} title={${JSON.stringify(p.title)}} icon={<PageIcon name=${JSON.stringify(it.icon)} />}>\n  ${esc(p.description)}\n</Card>`,
      )
    }
  }
  return `<Cards>\n${items.join("\n")}\n</Cards>`
}
const indexPage = pages.find((p) => p.route === "")
if (!indexPage) throw new Error("no index page")
indexPage.body = indexPage.body.replace(START_CARDS, cards(true)).replace(ALL_CARDS, cards(false))

rmSync(path.join(site, "content"), { recursive: true, force: true })
for (const p of pages) {
  const dest = path.join(out, p.file)
  mkdirSync(path.dirname(dest), { recursive: true })
  writeFileSync(dest, `${frontmatter(p.title, p.description)}${p.body}\n`)
}

// sidebar: one separator per group, then the group's pages and links
const rootPages: string[] = []
for (const g of groups) {
  rootPages.push(`---[${g.icon}]${g.title}---`)
  if (g.dir === "") {
    rootPages.push("index", ...readmePages.map((p) => p.route))
  } else {
    rootPages.push(`...${g.dir}`)
    writeFileSync(path.join(out, g.dir, "meta.json"), `${JSON.stringify({ pages: g.docs.map((d) => d.slug) }, null, 2)}\n`)
  }
  for (const l of g.links ?? []) rootPages.push(`[${l.text}](${l.url})`)
}
writeFileSync(path.join(out, "meta.json"), `${JSON.stringify({ pages: rootPages }, null, 2)}\n`)

// per page: the repository file it came from, for the github links, and whether the page
// header shows its description
const info = Object.fromEntries(pages.map((p) => [p.file, { source: p.source, describe: p.describe }]))
writeFileSync(path.join(site, "content", "pages.json"), `${JSON.stringify(info, null, 2)}\n`)

console.log(`docs-content: ${pages.length} pages written to ${path.relative(site, out)}${missing ? `, ${missing} links to missing files` : ""}`)
