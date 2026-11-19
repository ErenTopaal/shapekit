import { readFile } from "node:fs/promises"
import { glob } from "node:fs/promises"
import type { Config } from "@react-router/dev/config"
import { createGetUrl, getSlugs } from "fumadocs-core/source"

const docsUrl = createGetUrl("/docs")
// the same layout as src/lib/llms-url.ts
const markdownUrl = createGetUrl("/llms.mdx/docs")

// every page is prerendered to plain html, so the output is a static site with no server.
// the llms routes (/llms.txt, /llms-full.txt, a markdown file per docs page) are resource
// routes prerendered to files the same way.
// cap pages and corpus entry pages are listed from the same coverage.json and entries.json the
// dashboard reads at runtime.
export default {
  appDirectory: "src",
  buildDirectory: "dist",
  ssr: false,
  async prerender({ getStaticPaths }) {
    const paths = new Set(getStaticPaths())
    for await (const entry of glob("**/*.{md,mdx}", { cwd: "content/docs" })) {
      const slugs = getSlugs(entry)
      paths.add(docsUrl(slugs))
      // the page's markdown for llms, see src/routes/llms.mdx.docs.ts
      paths.add(markdownUrl([...slugs, "content.md"]))
    }
    const coverage = JSON.parse(await readFile("public/data/coverage.json", "utf8")) as { caps: { id: string }[] }
    for (const cap of coverage.caps) {
      paths.add(`/dashboard/corpus/cap/${cap.id}`)
      // the address before the cap pages moved under the corpus page, which forwards
      paths.add(`/dashboard/coverage/${cap.id}`)
    }
    // one explorer page per corpus entry, from the entries.json the explorer reads
    const entries = JSON.parse(await readFile("public/data/entries.json", "utf8")) as { entries: { id: string }[] }
    for (const e of entries.entries) paths.add(`/dashboard/corpus/${encodeURIComponent(e.id)}`)
    return [...paths]
  },
} satisfies Config
