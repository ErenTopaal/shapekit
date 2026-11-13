import { createFromSource } from "fumadocs-core/search/server"
import { source } from "@/lib/source"

const server = createFromSource(source, { language: "english" })

// prerendered once at build time; the browser downloads the index and searches it locally
export async function loader() {
  return server.staticGET()
}
