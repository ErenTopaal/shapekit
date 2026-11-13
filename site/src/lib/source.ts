import { loader } from "fumadocs-core/source"
import { docs } from "@/lib/collections"

// used by route loaders only, which run at build time when the pages are prerendered
export const source = loader({
  source: docs.toFumadocsSource(),
  baseUrl: "/docs",
})
