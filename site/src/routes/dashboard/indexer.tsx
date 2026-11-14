import { Suspense } from "react"
import { load } from "@/lib/data"
import { DataBoundary, Loading } from "@/components/common"
import { IndexerPage } from "@/pages/indexer"

// fetches this page's data files before the page renders, so navigation shows no skeleton
export async function clientLoader() {
  await Promise.all([load("harness"), load("coverage")])
  return null
}

// the data files do not change while the page is open, so filter changes need no reload
export function shouldRevalidate() {
  return false
}

export function meta() {
  return [{ title: "Indexer run | shapekit" }]
}

export default function Route() {
  return (
    <DataBoundary>
      <Suspense fallback={<Loading />}>
        <IndexerPage />
      </Suspense>
    </DataBoundary>
  )
}
