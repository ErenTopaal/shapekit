import { use } from "react"
import { DocsLayout } from "fumadocs-ui/layouts/docs"
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
  EditOnGitHub,
  MarkdownCopyButton,
  ViewOptionsPopover,
} from "fumadocs-ui/layouts/docs/page"
import { useFumadocsLoader } from "fumadocs-core/source/client"
import type { Route } from "./+types/docs"
import { docs } from "@/lib/collections"
import { pageMarkdownUrl } from "@/lib/llms-url"
import { source } from "@/lib/source"
import { baseOptions, blob, edit } from "@/lib/site"
import { useMDXComponents } from "@/components/mdx"
import pages from "../../content/pages.json"

interface PageInfo {
  // the repository file the page is generated from
  source: string
  // whether the header shows the description, which most pages open with anyway
  describe: boolean
}

export async function loader({ params }: Route.LoaderArgs) {
  const slugs = (params["*"] ?? "").split("/").filter((v) => v.length > 0)
  const page = source.getPage(slugs)
  if (!page) throw new Response("Not found", { status: 404 })
  return {
    path: page.path,
    markdownUrl: pageMarkdownUrl(page.slugs),
    pageTree: await source.serializePageTree(source.getPageTree()),
  }
}

function Content({ path, markdownUrl }: { path: string; markdownUrl: string }) {
  const page = docs.getPage(path)
  if (!page) throw new Error(`unknown page: ${path}`)
  const info = (pages as Record<string, PageInfo>)[path]
  if (!info) throw new Error(`no entry for ${path} in content/pages.json`)
  const { toc } = use(page.load())
  const Mdx = page.body
  return (
    <DocsPage toc={toc}>
      <title>{`${page.title} | shapekit docs`}</title>
      <meta name="description" content={page.description} />
      <link rel="alternate" type="text/markdown" href={markdownUrl} />
      <DocsTitle>{page.title}</DocsTitle>
      {info.describe ? <DocsDescription className="mb-0">{page.description}</DocsDescription> : null}
      <div className="flex flex-row flex-wrap items-center gap-2 border-b pb-6">
        <MarkdownCopyButton markdownUrl={markdownUrl} />
        <ViewOptionsPopover markdownUrl={markdownUrl} githubUrl={blob(info.source)} />
      </div>
      <DocsBody>
        <Mdx components={useMDXComponents()} />
      </DocsBody>
      <div className="flex flex-col gap-3 border-t pt-4 text-sm text-fd-muted-foreground sm:flex-row sm:items-center sm:justify-between">
        <p>
          Generated from{" "}
          <a href={blob(info.source)} className="font-mono text-fd-primary underline underline-offset-4">
            {info.source}
          </a>
        </p>
        <EditOnGitHub href={edit(info.source)} />
      </div>
    </DocsPage>
  )
}

export default function Page({ loaderData }: Route.ComponentProps) {
  const { path, markdownUrl, pageTree } = useFumadocsLoader(loaderData)
  return (
    <DocsLayout {...baseOptions()} tree={pageTree}>
      <Content path={path} markdownUrl={markdownUrl} />
    </DocsLayout>
  )
}
