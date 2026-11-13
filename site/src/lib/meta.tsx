import { createContext, use } from "react"
import type { Meta } from "./data"

export const MetaContext = createContext<Meta | null>(null)

export function useMeta(): Meta {
  const m = use(MetaContext)
  if (!m) throw new Error("useMeta outside MetaContext")
  return m
}

// links go to main, which is what the published site is built from
export function blobUrl(meta: Meta, path: string): string {
  return `${meta.repo_url}/blob/main/${path}`
}

export function commitUrl(meta: Meta): string {
  return `${meta.repo_url}/commit/${meta.commit}`
}
