// the addresses of the dashboard pages, in one place, so a page that moves changes here only

export const entryPath = (id: string) => `/dashboard/corpus/${encodeURIComponent(id)}`
export const capPath = (cap: string, anchor?: string) => `/dashboard/corpus/cap/${cap}${anchor ? `#${anchor}` : ""}`
export const findingPath = (id: string) => `/dashboard/findings#${id}`
export const entriesPath = (q?: Record<string, string>) => {
  const s = new URLSearchParams(q).toString()
  return `/dashboard/corpus/entries${s ? `?${s}` : ""}`
}
export const findingsPath = (q?: Record<string, string>) => {
  const s = new URLSearchParams(q).toString()
  return `/dashboard/findings${s ? `?${s}` : ""}`
}
export const sdkPath = (sdk: string, entry?: string) => {
  const p = new URLSearchParams({ sdk })
  if (entry) p.set("entry", entry)
  return `/dashboard/sdk?${p.toString()}#detail`
}
