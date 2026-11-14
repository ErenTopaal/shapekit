import { createGetUrl } from "fumadocs-core/source"

// the markdown of each docs page is prerendered under /llms.mdx/docs/<slugs>/content.md, the
// layout fumadocs' llms integration uses for react router
export const docsContentRoute = "/llms.mdx/docs"

const getContentUrl = createGetUrl(docsContentRoute)

export function pageMarkdownUrl(slugs: string[]): string {
  return getContentUrl([...slugs, "content.md"])
}
