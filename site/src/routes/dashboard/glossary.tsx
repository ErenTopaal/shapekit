import { GlossaryPage } from "@/pages/glossary"

export function meta() {
  return [{ title: "Glossary | shapekit" }, { name: "description", content: "The terms of the shapekit dashboard, each with one meaning." }]
}

export default function Route() {
  return <GlossaryPage />
}
