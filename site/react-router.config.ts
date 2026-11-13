import { readFile } from "node:fs/promises"
import { glob } from "node:fs/promises"
import type { Config } from "@react-router/dev/config"
import { createGetUrl, getSlugs } from "fumadocs-core/source"

const docsUrl = createGetUrl("/docs")

// every page is prerendered to plain html, so the output is a static site with no server.
// cap pages are listed from the same coverage.json the dashboard reads at runtime.
export default {
  appDirectory: "src",
  buildDirectory: "dist",
  ssr: false,
  async prerender({ getStaticPaths }) {
    const paths = new Set(getStaticPaths())
    for await (const entry of glob("**/*.{md,mdx}", { cwd: "content/docs" })) {
      paths.add(docsUrl(getSlugs(entry)))
    }
    const coverage = JSON.parse(await readFile("public/data/coverage.json", "utf8")) as { caps: { id: string }[] }
    for (const cap of coverage.caps) paths.add(`/dashboard/coverage/${cap.id}`)
    return [...paths]
  },
} satisfies Config
