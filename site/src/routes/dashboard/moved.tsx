import { useEffect } from "react"
import { useLocation, useNavigate, useParams } from "react-router"
import { PageHeader, TextLink } from "@/components/common"

// the new address of a page that moved; the query and the anchor go with it. the coverage
// page's own filter became `capq`, so it does not read as a filter of the entry list
function target(path: string, cap: string | undefined, search: string): string {
  if (path === "/dashboard/coverage" || path === "/dashboard/coverage/") {
    const p = new URLSearchParams(search)
    const q = p.get("q")
    if (q !== null) {
      p.delete("q")
      p.set("capq", q)
    }
    const s = p.toString()
    return `/dashboard/corpus${s ? `?${s}` : ""}`
  }
  return `/dashboard/corpus/cap/${cap ?? ""}${search}`
}

export function meta({ params }: { params: { cap?: string } }) {
  const to = params.cap ? `/dashboard/corpus/cap/${params.cap}` : "/dashboard/corpus"
  return [
    { title: "This page moved | shapekit" },
    { name: "robots", content: "noindex" },
    { httpEquiv: "refresh", content: `0; url=${to}` },
    { tagName: "link", rel: "canonical", href: to },
  ]
}

// /dashboard/coverage and /dashboard/coverage/<CAP>, the addresses of the coverage pages
// before they moved under the corpus page
export default function Route() {
  const { pathname, search, hash } = useLocation()
  const { cap } = useParams()
  const navigate = useNavigate()
  const to = target(pathname, cap, search)
  useEffect(() => {
    void navigate(`${to}${hash}`, { replace: true })
  }, [navigate, to, hash])
  return (
    <PageHeader title="This page moved">
      <p>
        The page is now at <TextLink to={`${to}${hash}`}>{to}</TextLink>.
      </p>
    </PageHeader>
  )
}
