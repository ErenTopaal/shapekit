import { Suspense } from "react"
import { load } from "@/lib/data"
import { DataBoundary, Loading } from "@/components/common"
import { CoveragePage } from "@/pages/coverage"

// fetches this page's data files before the page renders, so navigation shows no skeleton
export async function clientLoader() {
  await load("coverage")
  return null
}

// the data files do not change while the page is open, so filter changes need no reload
export function shouldRevalidate() {
  return false
}

export function meta() {
  return [{ title: "CAP coverage | shapekit" }, { name: "description", content: "Every CAP with the corpus entries that carry its shapes, the union arms covered and the fields core does not fully validate." }]
}

export default function Route() {
  return (
    <DataBoundary>
      <Suspense fallback={<Loading />}>
        <CoveragePage />
      </Suspense>
    </DataBoundary>
  )
}
