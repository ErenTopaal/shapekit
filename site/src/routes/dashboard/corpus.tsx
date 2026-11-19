import { Suspense } from "react"
import { load } from "@/lib/data"
import { DataBoundary, Loading } from "@/components/common"
import { CorpusPage } from "@/pages/corpus"

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
  return [{ title: "Corpus coverage | shapekit" }, { name: "description", content: "What the shapekit corpus covers: entries by source, CAPs, union arms, shape families, obligations and the fields that stellar-core does not fully check." }]
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
