import { Suspense } from "react"
import { load } from "@/lib/data"
import { DataBoundary, Loading } from "@/components/common"
import { FindingsPage } from "@/pages/findings"

// fetches this page's data files before the page renders, so navigation shows no skeleton
export async function clientLoader() {
  await Promise.all([load("findings"), load("entries")])
  return null
}

// the data files do not change while the page is open, so filter changes need no reload
export function shouldRevalidate() {
  return false
}

export function meta() {
  return [{ title: "Findings | shapekit" }, { name: "description", content: "Every failure the indexer runs, the SDK runs and the linter reported, traced to a root cause with its severity, entry and upstream status." }]
}

export default function Route() {
  return (
    <DataBoundary>
      <Suspense fallback={<Loading />}>
        <FindingsPage />
      </Suspense>
    </DataBoundary>
  )
}
