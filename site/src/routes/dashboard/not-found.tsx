import { NotFound } from "@/pages/not-found"

export function meta() {
  return [{ title: "Not found | shapekit" }]
}

export default function Route() {
  return <NotFound />
}
