import { Link } from "react-router"
import { HomeLayout } from "fumadocs-ui/layouts/home"
import { baseOptions, homeStyle } from "@/lib/site"

export function meta() {
  return [{ title: "Not found | shapekit" }]
}

export function NotFoundBody() {
  return (
    <div className="flex max-w-[60ch] flex-col gap-3">
      <h1 className="text-2xl font-semibold tracking-tight">No page at this address</h1>
      <p className="text-fd-muted-foreground">
        This build of the site has no page at this address. Check the link, or start from the docs or the dashboard.
      </p>
      <ul className="flex flex-wrap gap-x-6 gap-y-2">
        <li>
          <Link to="/docs" className="text-fd-primary underline underline-offset-4">
            Read the docs
          </Link>
        </li>
        <li>
          <Link to="/dashboard" className="text-fd-primary underline underline-offset-4">
            Open the dashboard
          </Link>
        </li>
      </ul>
    </div>
  )
}

export default function NotFound() {
  return (
    <HomeLayout {...baseOptions()} style={homeStyle}>
      <div id="main" className="mx-auto w-full max-w-(--fd-layout-width) flex-1 px-4 py-10">
        <NotFoundBody />
      </div>
    </HomeLayout>
  )
}
