import type { ReactNode } from "react"
import { NavLink } from "react-router"
import { HomeLayout } from "fumadocs-ui/layouts/home"
import { cn } from "@/lib/utils"
import { commitUrl } from "@/lib/meta"
import type { Meta } from "@/lib/data"
import { baseOptions, homeStyle } from "@/lib/site"
import { ExtLink, Id, short } from "@/components/common"

// one page for each question a reader asks, then the glossary
const nav = [
  { to: "/dashboard", label: "Start", end: true },
  { to: "/dashboard/sdk", label: "SDKs", end: false },
  { to: "/dashboard/indexer", label: "Indexers", end: false },
  { to: "/dashboard/findings", label: "Findings", end: false },
  { to: "/dashboard/corpus", label: "Corpus", end: false },
  { to: "/dashboard/glossary", label: "Glossary", end: false },
]

// the dashboard keeps the site navigation on top and its own pages in a second row, which
// scrolls sideways on a narrow screen instead of hiding behind a menu
export function DashboardShell({ meta, children }: { meta: Meta | null; children: ReactNode }) {
  return (
    <HomeLayout {...baseOptions()} style={homeStyle}>
      <div className="border-b">
        <nav aria-label="Dashboard pages" className="mx-auto max-w-(--fd-layout-width) overflow-x-auto px-4">
          <ul className="flex items-center gap-1 py-2">
            {nav.map((n) => (
              <li key={n.to}>
                <NavLink
                  to={n.to}
                  end={n.end}
                  className={({ isActive }) =>
                    cn(
                      "block rounded-md px-2.5 py-1.5 text-sm whitespace-nowrap transition-colors hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none",
                      isActive ? "bg-accent font-medium text-accent-foreground" : "text-muted-foreground",
                    )
                  }
                >
                  {n.label}
                </NavLink>
              </li>
            ))}
          </ul>
        </nav>
      </div>
      {/* HomeLayout already renders the page's main landmark, so this is a plain container */}
      <div id="main" className="mx-auto w-full max-w-(--fd-layout-width) flex-1 px-4 py-8">
        {children}
      </div>
      <footer className="border-t">
        <div className="mx-auto flex max-w-(--fd-layout-width) flex-col gap-1 px-4 py-6 text-sm text-muted-foreground sm:flex-row sm:flex-wrap sm:gap-x-6">
          {meta ? (
            <span>
              Data from commit{" "}
              <ExtLink href={commitUrl(meta)}>
                <Id>{short(meta.commit, 10)}</Id>
              </ExtLink>
              , corpus {meta.corpus.version}, shapekit {meta.shapekit_version}
            </span>
          ) : null}
          <span>No analytics, no cookies, no third-party requests.</span>
          <NavLink to="/dashboard/sources" className="text-primary underline decoration-primary/40 hover:decoration-primary">
            Data sources and pinned versions
          </NavLink>
          <a href="/docs/operations/site" className="text-primary underline decoration-primary/40 hover:decoration-primary">
            How shapekit makes this data
          </a>
        </div>
      </footer>
    </HomeLayout>
  )
}
