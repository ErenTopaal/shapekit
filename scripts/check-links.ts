// checks that every link in the tracked markdown files resolves, and, when it is given, every
// link of the prerendered site.
//
// usage: node scripts/check-links.ts [--site site/dist/client] [--offline]
//
// a relative link must name a tracked file or directory, and its #fragment a heading or an id
// of the target; a root-relative link in docs/guide/ is a developers.stellar.org route, for
// which the guide is written. a link to this repository on github (blob, tree or raw at main,
// or at the tag v<version> of the workspace Cargo.toml) is checked against the working tree,
// because it resolves once this commit is published and tagged. an
// absolute link on the site, or a shapekit.org link anywhere, must name a prerendered page or
// file, and its fragment an id in it; without --site, shapekit.org links are fetched like the
// others, from the site as it is published now. every other http(s) link is fetched, following
// redirects, and must answer 2xx, and its fragment must be an id or name in the page, or for a
// github blob a line of the file; --offline skips the fetches. a link that cannot be fetched
// (429, 5xx, 401, 403, no answer) is listed as unchecked. exits 1 when a link does not resolve,
// 3 when none is broken but some are unchecked, and lists each with the files naming it.

import { execFileSync } from "node:child_process"
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs"
import path from "node:path"

const repo = execFileSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" }).trim()
// the site itself: checked against the prerendered pages when --site is given, since the
// published site is the previous release until this tree is deployed
const ownSite = /^https:\/\/shapekit\.org(\/[^#?]*)?(?:\?[^#]*)?(#.*)?$/
// this repository at main or at the tag of the version this tree declares, which exists once
// the release of this commit is tagged
const version = readFileSync(path.join(repo, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m)![1]
const ref = `(?:main|v${version.replace(/\./g, "\\.")})`
const ownRepo = new RegExp(
  `^https://(?:github\\.com/ErenTopaal/shapekit/(?:blob|tree|raw)/${ref}|raw\\.githubusercontent\\.com/ErenTopaal/shapekit/${ref})/([^#?]*)(#.*)?$`,
)

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

const pages = [...tracked].filter((f) => /\.mdx?$/.test(f))
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

// one fetch per distinct url. a github blob is fetched from raw.githubusercontent.com, which
// answers for the same file and ref without the html page's rate limit, and lets a #L line
// anchor be checked against the file's line count. an answer of 429, 5xx or 401/403, or no
// answer at all, leaves the link unchecked: it is reported as such and never counted as passing
type Answer = { ok: true; body?: string } | { ok: false; broken: boolean; why: string }

const blob = /^https:\/\/github\.com\/([^/]+)\/([^/]+)\/blob\/(.+)$/

async function get(url: string, wantBody: boolean): Promise<Answer> {
  let last = ""
  for (let attempt = 0; attempt < 6; attempt++) {
    if (attempt > 0) await new Promise((r) => setTimeout(r, 1000 * 2 ** attempt))
    try {
      const res = await fetch(url, {
        redirect: "follow",
        signal: AbortSignal.timeout(30000),
        headers: { "user-agent": "shapekit-link-check", accept: "text/html,*/*" },
      })
      if (res.ok) {
        if (wantBody) return { ok: true, body: await res.text() }
        await res.body?.cancel()
        return { ok: true }
      }
      await res.body?.cancel()
      last = `HTTP ${res.status}`
      if (res.status === 401 || res.status === 403) return { ok: false, broken: false, why: `${last}, access refused` }
      if (res.status !== 429 && res.status < 500) return { ok: false, broken: true, why: last }
    } catch (e) {
      last = String((e as Error).cause ?? e)
    }
  }
  return { ok: false, broken: false, why: `${last} after 6 tries` }
}

// does the fetched document have this fragment
function hasFragment(url: string, body: string, fragment: string): boolean {
  const line = fragment.match(/^L(\d+)(?:C\d+)?(?:-L(\d+)(?:C\d+)?)?$/)
  if (blob.test(url) && line) {
    const lines = body.endsWith("\n") ? body.split("\n").length - 1 : body.split("\n").length
    return Number(line[2] ?? line[1]) <= lines
  }
  const f = fragment.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
  return new RegExp(`\\b(?:id|name)=["']?(?:user-content-)?${f}["'\\s>]`).test(body)
}

const unchecked: string[] = []
let fetched = 0
if (!offline) {
  // url without fragment -> fragments linked, and who links each
  const targets = new Map<string, Map<string, Set<string>>>()
  for (const [link, from] of external) {
    const [u, fragment = ""] = link.split("#", 2)
    if (!targets.has(u)) targets.set(u, new Map())
    const frags = targets.get(u)!
    if (!frags.has(fragment)) frags.set(fragment, new Set())
    for (const f of from) frags.get(fragment)!.add(f)
  }
  const urls = [...targets.keys()]
  let next = 0
  const workers = Array.from({ length: 3 }, async () => {
    while (next < urls.length) {
      const url = urls[next++]
      const frags = targets.get(url)!
      const wantBody = [...frags.keys()].some((f) => f !== "")
      const b = url.match(blob)
      const answer = await get(b ? `https://raw.githubusercontent.com/${b[1]}/${b[2]}/${b[3]}` : url, wantBody)
      fetched++
      const who = (fs: Iterable<Set<string>>) => [...new Set([...fs].flatMap((s) => [...s]))].sort().join(", ")
      if (!answer.ok) {
        ;(answer.broken ? broken : unchecked).push(`${who(frags.values())}: ${url}: ${answer.why}`)
        continue
      }
      for (const [fragment, from] of frags) {
        if (fragment && !hasFragment(url, answer.body!, decodeURIComponent(fragment))) {
          broken.push(`${who([from])}: ${url}#${fragment}: no such anchor or line in the page`)
        }
      }
    }
  })
  await Promise.all(workers)
}

const report = (kind: string, list: string[]) => {
  for (const l of [...new Set(list)].sort()) console.log(`${kind}: ${l}`)
}
report("broken", broken)
report("unchecked", unchecked)
console.log(
  `${pages.length} markdown files${site ? `, ${sitePages} site pages` : ""}; ${external.size} external links, ${offline ? "not fetched" : `${fetched} distinct urls fetched`}; ${new Set(broken).size} broken, ${new Set(unchecked).size} unchecked`,
)
// 1: a link does not resolve; 3: every answered link resolves, but some could not be fetched
process.exit(broken.length > 0 ? 1 : unchecked.length > 0 ? 3 : 0)
