import { Suspense } from "react"
import { load } from "@/lib/data"
import { DataBoundary, Loading } from "@/components/common"
import { CapPage } from "@/pages/cap"

// fetches this page's data files before the page renders, so navigation shows no skeleton
export async function clientLoader() {
  await Promise.all([load("coverage"), load("entries"), load("conformance"), load("harness"), load("findings")])
  return null
}

// the data files do not change while the page is open, so filter changes need no reload
export function shouldRevalidate() {
  return false
}

export function meta({ params }: { params: { cap?: string } }) {
  return [
    { title: `${params.cap ?? "CAP"} coverage | shapekit` },
    { name: "description", content: `The corpus entries, shape families, findings and results for ${params.cap ?? "this CAP"}.` },
  ]
}

export default function Route() {
  return (
    <DataBoundary>
      <Suspense fallback={<Loading />}>
        <CapPage />
      </Suspense>
    </DataBoundary>
  )
}
