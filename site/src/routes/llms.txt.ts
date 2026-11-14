import { llmsIndex } from "@/lib/source"

// the index of the docs for llms, from the page tree; prerendered to /llms.txt
export async function loader() {
  return new Response(llmsIndex(), { headers: { "Content-Type": "text/plain; charset=utf-8" } })
}
