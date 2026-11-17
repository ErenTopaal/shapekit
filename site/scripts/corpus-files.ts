// copies the corpus files the explorer offers into public/corpus/, so every entry page can
// download its bytes and show its decoded XDR-JSON from the site's own origin. only the files
// corpus/manifest.json names are copied: each variant's xdr and XDR-JSON, and each vector.
// run before every dev server start and build, like docs-content.ts.

import { copyFileSync, mkdirSync, readFileSync, rmSync, statSync } from "node:fs"
import path from "node:path"

const site = path.resolve(import.meta.dirname, "..")
const corpus = path.resolve(site, "..", "corpus")
const out = path.join(site, "public", "corpus")

interface Variant {
  xdr_file: string
  expected_json_file?: string
}

interface ManifestEntry {
  variants?: Record<string, Variant>
  vector?: { path: string }
}

const manifest = JSON.parse(readFileSync(path.join(corpus, "manifest.json"), "utf8")) as { entries: ManifestEntry[] }

// the same rule as shapekit_manifest::is_safe_relative: no absolute path, no `..`
function inside(rel: string): string {
  const parts = rel.split("/")
  if (rel.startsWith("/") || parts.some((p) => p === ".." || p === "" || p === "."))
    throw new Error(`corpus/manifest.json names ${JSON.stringify(rel)}, which is not a path inside the corpus`)
  return rel
}

const files = new Set<string>()
for (const e of manifest.entries) {
  for (const v of Object.values(e.variants ?? {})) {
    files.add(inside(v.xdr_file))
    if (v.expected_json_file) files.add(inside(v.expected_json_file))
  }
  if (e.vector) files.add(inside(e.vector.path))
}

rmSync(out, { recursive: true, force: true })
let bytes = 0
for (const rel of files) {
  const to = path.join(out, rel)
  mkdirSync(path.dirname(to), { recursive: true })
  copyFileSync(path.join(corpus, rel), to)
  bytes += statSync(to).size
}
console.log(`corpus-files: ${files.size} files, ${(bytes / (1 << 20)).toFixed(1)} MiB written to public/corpus`)
