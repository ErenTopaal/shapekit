import { useEffect } from "react"
import { Link, useLocation, useNavigate } from "react-router"

// the site root has no page of its own yet, so it points at the docs. the redirect is a meta
// refresh in the prerendered html and needs no javascript; a landing page can replace this
// route module without touching the docs or the dashboard.
export function meta() {
  return [
    { title: "shapekit" },
    { httpEquiv: "refresh", content: "0; url=/docs" },
    { tagName: "link", rel: "canonical", href: "/docs" },
  ]
}

export default function Home() {
  const { hash } = useLocation()
  const navigate = useNavigate()
  // a client-side navigation to / does not act on the meta refresh, so the router redirects
  // too. links from before the dashboard moved under /dashboard (/#/sdk) land there.
  useEffect(() => {
    void navigate(hash.startsWith("#/") ? `/dashboard${hash.slice(1)}` : "/docs", { replace: true })
  }, [hash, navigate])
  return (
    <main id="main" className="mx-auto w-full max-w-(--fd-layout-width) px-4 py-10">
      <p>
        The documentation is at <Link to="/docs" className="text-fd-primary underline underline-offset-4">/docs</Link>.
      </p>
    </main>
  )
}
