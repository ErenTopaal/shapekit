import { use } from "react"
import { DocsLayout } from "fumadocs-ui/layouts/docs"
import { DocsBody, DocsPage, DocsTitle } from "fumadocs-ui/layouts/docs/page"
import { useFumadocsLoader } from "fumadocs-core/source/client"
import type { Route } from "./+types/docs"
import { docs } from "@/lib/collections"
import { source } from "@/lib/source"
import { baseOptions, blob } from "@/lib/site"
import { useMDXComponents } from "@/components/mdx"
import sources from "../../content/sources.json"

export async function loader({ params }: Route.LoaderArgs) {
  const slugs = (params["*"] ?? "").split("/").filter((v) => v.length > 0)
  const page = source.getPage(slugs)
  if (!page) throw new Response("Not found", { status: 404 })
  return {
    path: page.path,
    pageTree: await source.serializePageTree(source.getPageTree()),
  }
}

function Content({ path }: { path: string }) {
  const page = docs.getPage(path)
  if (!page) throw new Error(`unknown page: ${path}`)
  const { toc } = use(page.load())
  const Mdx = page.body
  const repoPath = (sources as Record<string, string>)[path]
  return (
    <DocsPage toc={toc}>
      <title>{`${page.title} | shapekit docs`}</title>
      <meta name="description" content={page.description} />
      <DocsTitle>{page.title}</DocsTitle>
      <DocsBody>
        <Mdx components={useMDXComponents()} />
      </DocsBody>
      {repoPath ? (
        <p className="border-t pt-4 text-sm text-fd-muted-foreground">
          This page is generated from{" "}
          <a href={blob(repoPath)} className="font-mono text-fd-primary underline underline-offset-4">
            {repoPath}
          </a>{" "}
          in the repository.
        </p>
      ) : null}
    </DocsPage>
  )
}

export default function Page({ loaderData }: Route.ComponentProps) {
  const { path, pageTree } = useFumadocsLoader(loaderData)
  return (
    <DocsLayout {...baseOptions()} tree={pageTree}>
      <Content path={path} />
    </DocsLayout>
  )
}
