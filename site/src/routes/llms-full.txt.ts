import { docsLlms } from "@/lib/source"

// every docs page as markdown in one file; prerendered to /llms-full.txt
export async function loader() {
  return new Response(await docsLlms.full(), { headers: { "Content-Type": "text/plain; charset=utf-8" } })
}
