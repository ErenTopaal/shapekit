import { Outlet, isRouteErrorResponse } from "react-router"
import type { Route } from "./+types/layout"
import { load } from "@/lib/data"
import { MetaContext } from "@/lib/meta"
import { TooltipProvider } from "@/components/ui/tooltip"
import { DashboardShell } from "@/components/layout"
import { DataError, Loading } from "@/components/common"

// the dashboard reads /data/*.json in the browser, so its prerendered html is this shell
// with a loading state, and the pages render once the files have arrived
export async function clientLoader() {
  return { meta: await load("meta") }
}

// the data files do not change while the page is open, so filter changes need no reload
export function shouldRevalidate() {
  return false
}

export function HydrateFallback() {
  return (
    <DashboardShell meta={null}>
      <Loading />
      <noscript>
        <p className="text-muted-foreground">The dashboard needs JavaScript to read its data files.</p>
      </noscript>
    </DashboardShell>
  )
}

export default function DashboardLayout({ loaderData }: Route.ComponentProps) {
  return (
    <MetaContext value={loaderData.meta}>
      <TooltipProvider>
        <DashboardShell meta={loaderData.meta}>
          <Outlet />
        </DashboardShell>
      </TooltipProvider>
    </MetaContext>
  )
}

export function ErrorBoundary({ error }: Route.ErrorBoundaryProps) {
  const message = isRouteErrorResponse(error)
    ? `${error.status} ${error.statusText}`
    : error instanceof Error
      ? error.message
      : String(error)
  return (
    <DashboardShell meta={null}>
      <DataError message={message} />
    </DashboardShell>
  )
}
