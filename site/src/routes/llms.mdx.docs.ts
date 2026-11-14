import type { Route } from "./+types/llms.mdx.docs"
import { docsLlms, source } from "@/lib/source"

// one docs page as markdown, prerendered to /llms.mdx/docs/<slugs>/content.md
export async function loader({ params }: Route.LoaderArgs) {
  const slugs = (params["*"] ?? "").split("/").filter((v) => v.length > 0)
  // the last segment is the file name, content.md
  if (slugs.pop() !== "content.md") throw new Response("Not found", { status: 404 })
  const page = source.getPage(slugs)
  if (!page) throw new Response("Not found", { status: 404 })
  return new Response(await docsLlms.page(page), { headers: { "Content-Type": "text/markdown; charset=utf-8" } })
}
