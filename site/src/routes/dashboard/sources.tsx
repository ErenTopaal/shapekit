import { SourcesPage } from "@/pages/sources"

export function meta() {
  return [{ title: "Data sources | shapekit" }, { name: "description", content: "The committed files and pinned versions that the shapekit dashboard data comes from." }]
}

export default function Route() {
  return <SourcesPage />
}
