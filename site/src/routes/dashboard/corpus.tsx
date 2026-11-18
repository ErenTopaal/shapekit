import { Suspense } from "react"
import { load } from "@/lib/data"
import { DataBoundary, Loading } from "@/components/common"
import { CorpusPage } from "@/pages/corpus"

// fetches this page's data files before the page renders, so navigation shows no skeleton
export async function clientLoader() {
  await Promise.all([load("entries"), load("conformance"), load("harness")])
  return null
}

// the data files do not change while the page is open, so filter changes need no reload
export function shouldRevalidate() {
  return false
}

export function meta() {
  return [{ title: "Corpus explorer | shapekit" }, { name: "description", content: "Every corpus entry with its decoded XDR-JSON, its source, what each SDK version and indexer did with it, and how to run it again." }]
}

export default function Route() {
  return (
    <DataBoundary>
      <Suspense fallback={<Loading />}>
        <CorpusPage />
      </Suspense>
    </DataBoundary>
  )
}
