import { Suspense } from "react"
import { load } from "@/lib/data"
import { DataBoundary, Loading } from "@/components/common"
import { SdkPage } from "@/pages/sdk"

// fetches this page's data files before the page renders, so navigation shows no skeleton
export async function clientLoader() {
  await load("conformance")
  return null
}

// the data files do not change while the page is open, so filter changes need no reload
export function shouldRevalidate() {
  return false
}

export function meta() {
  return [{ title: "SDK matrix | shapekit" }]
}

export default function Route() {
  return (
    <DataBoundary>
      <Suspense fallback={<Loading />}>
        <SdkPage />
      </Suspense>
    </DataBoundary>
  )
}
