import { defineDocs } from "fumadocs-mdx/macro"
import { llms, loader } from "fumadocs-core/source"
import { lucideIconsPlugin } from "fumadocs-core/source/lucide-icons"
import { markdownComponents } from "@/lib/markdown"

// used by route loaders only, which run at build time when the pages are prerendered, so
// nothing here reaches the browser.
//
// the same documents as the browser's collection in collections.ts, compiled once more with
// their processed markdown (remarkLLMs) for /llms.txt, /llms-full.txt and the per-page
// markdown. a separate collection keeps that markdown out of the page chunks the browser loads.
const docsWithMarkdown = defineDocs({
  dir: "content/docs",
  docs: { async: true, postprocess: { includeProcessedMarkdown: true } },
})

export const source = loader({
  source: docsWithMarkdown.toFumadocsSource(),
  baseUrl: "/docs",
  // sidebar separators name a lucide icon (---[Boxes]Corpus---); the loader renders it, and the
  // serialized page tree carries it to the browser as markup
  plugins: [lucideIconsPlugin()],
})

export type DocsPage = NonNullable<ReturnType<typeof source.getPage>>

// the page's markdown, with the components that give the jsx in it a markdown form
async function pageMarkdown(page: DocsPage): Promise<string> {
  const withComponents = source.getPage(page.slugs, { props: { components: markdownComponents } })
  if (!withComponents) throw new Error(`no page ${page.url}`)
  return withComponents.data.getText("processed")
}

export const docsLlms = llms(source, {
  renderPage: async (page) => `# ${page.data.title} (${page.url})\n\n> ${page.data.description}\n\n${await pageMarkdown(page)}`,
})

// llms.txt: the index fumadocs builds from the page tree, under the project's name and the
// introduction's description rather than the tree root's name ("Docs", which the search
// breadcrumbs use)
export function llmsIndex(): string {
  const intro = source.getPage([])
  if (!intro) throw new Error("no docs index page")
  const index = docsLlms.index()
  if (!index.startsWith("# Docs\n")) throw new Error("llms index no longer opens with the tree root's name")
  return `# shapekit\n\n> ${intro.data.description}\n${index.slice("# Docs\n".length)}`
}
