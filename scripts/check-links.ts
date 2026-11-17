// checks that every link in the public text resolves: README.md, STATUS.md, SECURITY.md and
// the pages under docs/, and, when it is given, the prerendered site.
//
// usage: node scripts/check-links.ts [--site site/dist/client] [--offline]
//
// a relative link must name a tracked file or directory, and its #fragment a heading or an id
// of the target; a root-relative link in docs/guide/ is a developers.stellar.org route, for
// which the guide is written. a link to this repository on github (blob, tree or raw at main) is checked
// against the working tree, because it resolves once this commit is published. an absolute
// link on the site, or a shapekit.org link anywhere, must name a prerendered page or file, and
// its fragment an id in it; without --site, shapekit.org links are fetched like the others,
// from the site as it is published now. every other http(s) link is fetched, following redirects, and must answer 2xx; --offline skips
// the fetches. exits 1 when a link does not resolve, and lists each with the files naming it.

import { execFileSync } from "node:child_process"
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs"
import path from "node:path"

const repo = execFileSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" }).trim()
// the site itself: checked against the prerendered pages when --site is given, since the
// published site is the previous release until this tree is deployed
const ownSite = /^https:\/\/shapekit\.org(\/[^#?]*)?(?:\?[^#]*)?(#.*)?$/
const ownRepo = /^https:\/\/(?:github\.com\/ErenTopaal\/shapekit\/(?:blob|tree|raw)\/main|raw\.githubusercontent\.com\/ErenTopaal\/shapekit\/main)\/([^#?]*)(#.*)?$/

const args = process.argv.slice(2)
const siteIdx = args.indexOf("--site")
const site = siteIdx >= 0 ? path.resolve(args[siteIdx + 1]) : undefined
const offline = args.includes("--offline")

// url or path -> files that name it
const external = new Map<string, Set<string>>()
// shapekit.org links, checked against the prerendered site
const siteLinks = new Map<string, Set<string>>()
const broken: string[] = []

function note(map: Map<string, Set<string>>, key: string, from: string) {
  if (!map.has(key)) map.set(key, new Set())
  map.get(key)!.add(from)
}

// github's heading anchors: lowercase, punctuation other than '-' and '_' dropped, spaces to
// '-', and -1, -2 for repeats
function slugs(markdown: string): Set<string> {
  const out = new Set<string>()
  const seen = new Map<string, number>()
  let fence: string | undefined
  for (const line of markdown.split("\n")) {
    const f = line.match(/^\s*(```|~~~)/)
    if (f) {
      fence = fence === undefined ? f[1] : fence === f[1] ? undefined : fence
      continue
    }
    if (fence) continue
    const h = line.match(/^#{1,6}\s+(.*?)\s*#*\s*$/)
    // an mdx heading may set its id, `## Title {#id}`
    const custom = h?.[1].match(/\{#([^}]+)\}\s*$/)
    if (custom) {
      out.add(custom[1])
    } else if (h) {
      const base = h[1]
        .replace(/`([^`]*)`/g, "$1")
        .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
        .replace(/<[^>]+>/g, "")
        .toLowerCase()
        .replace(/[^\p{L}\p{N}\s_-]/gu, "")
        .replace(/\s/g, "-")
      const n = seen.get(base) ?? 0
      seen.set(base, n + 1)
      out.add(n === 0 ? base : `${base}-${n}`)
    }
    for (const m of line.matchAll(/\bid=["']([^"']+)["']/g)) out.add(m[1])
  }
  return out
}

// link targets of one markdown file, outside code
function markdownLinks(text: string): string[] {
  const out: string[] = []
  let fence: string | undefined
  for (const raw of text.split("\n")) {
    const f = raw.match(/^\s*(```|~~~)/)
    if (f) {
      fence = fence === undefined ? f[1] : fence === f[1] ? undefined : fence
      continue
    }
    if (fence) continue
    const line = raw.replace(/`[^`]*`/g, "")
    for (const m of line.matchAll(/\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)/g)) out.push(m[1])
    for (const m of line.matchAll(/<(https?:\/\/[^>\s]+)>/g)) out.push(m[1])
    for (const m of line.matchAll(/\bhref=["']([^"']+)["']/g)) out.push(m[1])
    const def = line.match(/^\s*\[[^\]]+\]:\s*<?(\S+?)>?(?:\s+"[^"]*")?\s*$/)
    if (def) out.push(def[1])
  }
  return out
}

const tracked = new Set(
  execFileSync("git", ["ls-files"], { cwd: repo, encoding: "utf8", maxBuffer: 1 << 30 }).split("\n").filter(Boolean),
)
const trackedDirs = new Set<string>()
for (const f of tracked) {
  let d = path.posix.dirname(f)
  while (d !== "." && !trackedDirs.has(d)) {
    trackedDirs.add(d)
    d = path.posix.dirname(d)
  }
}

function checkRepoPath(rel: string, fragment: string, from: string, shown: string) {
  const clean = rel.replace(/\/$/, "")
  if (clean === "" || clean === ".") return
  if (!tracked.has(clean) && !trackedDirs.has(clean)) {
    broken.push(`${from}: ${shown}: ${clean} is not a tracked file or directory`)
    return
  }
  if (fragment && /\.mdx?$/.test(clean)) {
    const ids = slugs(readFileSync(path.join(repo, clean), "utf8"))
    if (!ids.has(fragment)) broken.push(`${from}: ${shown}: ${clean} has no heading #${fragment}`)
  }
}

function checkMarkdown(file: string) {
  const text = readFileSync(path.join(repo, file), "utf8")
  for (const link of markdownLinks(text)) {
    if (/^(mailto|tel):/.test(link)) continue
    const own = link.match(ownRepo)
    if (own) {
      checkRepoPath(decodeURIComponent(own[1]), (own[2] ?? "").slice(1), file, link)
      continue
    }
    if (site && ownSite.test(link)) {
      note(siteLinks, link, file)
      continue
    }
    if (/^https?:\/\//.test(link)) {
      note(external, link, file)
      continue
    }
    if (/^[a-z][a-z0-9+.-]*:/i.test(link)) {
      broken.push(`${file}: ${link}: not an http(s) or relative link`)
      continue
    }
    // the guide is written for developers.stellar.org, whose root-relative routes it uses
    if (link.startsWith("/") && file.startsWith("docs/guide/")) {
      note(external, `https://developers.stellar.org${link}`, file)
      continue
    }
    const [p, fragment = ""] = link.split("#", 2)
    if (p === "") {
      if (!slugs(text).has(fragment)) broken.push(`${file}: ${link}: no heading #${fragment} in the file`)
      continue
    }
    const rel = path.posix.normalize(path.posix.join(path.posix.dirname(file), decodeURIComponent(p)))
    if (rel.startsWith("..")) {
      broken.push(`${file}: ${link}: leaves the repository`)
      continue
    }
    checkRepoPath(rel, fragment, file, link)
  }
}

const pages = [...tracked].filter(
  (f) => ["README.md", "STATUS.md", "SECURITY.md"].includes(f) || (f.startsWith("docs/") && /\.mdx?$/.test(f)),
)
for (const f of pages) checkMarkdown(f)

// the prerendered site: every html file's hrefs
function walk(dir: string, out: string[] = []): string[] {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name)
    if (e.isDirectory()) walk(p, out)
    else out.push(p)
  }
  return out
}

let sitePages = 0
if (site) {
  if (!existsSync(site)) {
    console.error(`${site} does not exist; build the site first (npm run build in site/)`)
    process.exit(2)
  }
  const idCache = new Map<string, Set<string>>()
  const ids = (file: string) => {
    if (!idCache.has(file)) {
      const html = readFileSync(file, "utf8")
      idCache.set(file, new Set([...html.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1])))
    }
    return idCache.get(file)!
  }
  const resolve = (route: string): string | undefined => {
    const p = path.join(site, decodeURIComponent(route))
    for (const c of [p, path.join(p, "index.html"), `${p}.html`]) {
      if (existsSync(c) && statSync(c).isFile()) return c
    }
    return undefined
  }
  for (const file of walk(site).filter((f) => f.endsWith(".html"))) {
    sitePages++
    const rel = path.relative(site, file)
    const route = "/" + rel.replace(/(^|\/)index\.html$/, "$1").replace(/\.html$/, "")
    const html = readFileSync(file, "utf8")
    for (const m of html.matchAll(/<a\b[^>]*?\bhref="([^"]+)"/g)) {
      const href = m[1].replace(/&amp;/g, "&")
      if (/^(mailto|tel|javascript):/.test(href)) continue
      const own = href.match(ownRepo)
      if (own) {
        checkRepoPath(decodeURIComponent(own[1]), (own[2] ?? "").slice(1), `site ${route}`, href)
        continue
      }
      if (ownSite.test(href)) {
        note(siteLinks, href, `site ${route}`)
        continue
      }
      if (/^https?:\/\//.test(href)) {
        note(external, href, `site ${route}`)
        continue
      }
      checkSitePath(href, file, route, `site ${route}`)
    }
  }
  for (const [link, from] of siteLinks) {
    const m = link.match(ownSite)!
    for (const f of from) checkSitePath(`${m[1] ?? "/"}${m[2] ?? ""}`, undefined, "/", f)
  }

  function checkSitePath(href: string, file: string | undefined, route: string, from: string) {
    const [p, fragment = ""] = href.split("#", 2)
    const clean = p.split("?")[0]
    const target =
      p === "" ? file : resolve(clean.startsWith("/") ? clean : path.posix.join(path.posix.dirname(route), clean))
    if (!target) {
      broken.push(`${from}: ${href}: no prerendered page or file`)
      return
    }
    if (fragment && target.endsWith(".html") && !ids(target).has(decodeURIComponent(fragment))) {
      broken.push(`${from}: ${href}: no id #${fragment} on the page`)
    }
  }
}

async function fetchOk(url: string): Promise<string | undefined> {
  let last = ""
  for (let attempt = 0; attempt < 6; attempt++) {
    if (attempt > 0) await new Promise((r) => setTimeout(r, 1000 * 2 ** attempt))
    try {
      const res = await fetch(url, {
        redirect: "follow",
        signal: AbortSignal.timeout(30000),
        headers: { "user-agent": "shapekit-link-check", accept: "text/html,*/*" },
      })
      await res.body?.cancel()
      if (res.ok) return undefined
      last = `HTTP ${res.status}`
      if (res.status !== 429 && res.status < 500) return last
    } catch (e) {
      last = String((e as Error).cause ?? e)
    }
  }
  return last
}

let fetched = 0
if (!offline) {
  const urls = [...external.keys()].map((u) => u.split("#")[0]).filter((u, i, a) => a.indexOf(u) === i)
  let next = 0
  const workers = Array.from({ length: 3 }, async () => {
    while (next < urls.length) {
      const url = urls[next++]
      const err = await fetchOk(url)
      fetched++
      if (err) {
        const from = [...external.entries()]
          .filter(([u]) => u.split("#")[0] === url)
          .flatMap(([, f]) => [...f])
        broken.push(`${[...new Set(from)].join(", ")}: ${url}: ${err}`)
      }
    }
  })
  await Promise.all(workers)
}

broken.sort()
for (const b of broken) console.log(`broken: ${b}`)
console.log(
  `${pages.length} markdown files${site ? `, ${sitePages} site pages` : ""}; ${external.size} external links, ${offline ? "not fetched" : `${fetched} distinct urls fetched`}; ${broken.length} broken`,
)
process.exit(broken.length === 0 ? 0 : 1)
