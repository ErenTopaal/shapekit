import { Suspense } from "react"
import { load } from "@/lib/data"
import { DataBoundary, Loading } from "@/components/common"
import { EntryPage } from "@/pages/corpus"

// fetches this page's data files before the page renders, so navigation shows no skeleton
export async function clientLoader() {
  await Promise.all([load("entries"), load("conformance"), load("harness"), load("findings"), load("coverage")])
  return null
}

export function shouldRevalidate() {
  return false
}

export function meta({ params }: { params: { entry?: string } }) {
  return [{ title: `${params.entry ?? "Corpus entry"} | shapekit` }]
}

export default function Route() {
  return (
    <DataBoundary>
      <Suspense fallback={<Loading />}>
        <EntryPage />
      </Suspense>
    </DataBoundary>
  )
}
