import { isRouteErrorResponse, Links, Meta, Outlet, Scripts, ScrollRestoration } from "react-router"
import { RootProvider } from "fumadocs-ui/provider/react-router"
import { HomeLayout } from "fumadocs-ui/layouts/home"
import type { Route } from "./+types/root"
import "./index.css"
import DocsSearchDialog from "@/components/search"
import { baseOptions, homeStyle } from "@/lib/site"
import { NotFoundBody } from "@/routes/not-found"

export const links: Route.LinksFunction = () => [{ rel: "icon", href: "/favicon.svg", type: "image/svg+xml" }]

export function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="referrer" content="no-referrer" />
        <Meta />
        <Links />
      </head>
      <body className="flex min-h-dvh flex-col">
        {/* the theme choice is the only thing the site stores, in this browser's localStorage */}
        <RootProvider search={{ SearchDialog: DocsSearchDialog }} theme={{ storageKey: "shapekit-theme" }}>
          {children}
        </RootProvider>
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  )
}

export default function App() {
  return <Outlet />
}

// shown by the static fallback page, which the server returns for addresses with no
// prerendered file; the client router then renders the page for the address
export function HydrateFallback() {
  return (
    <HomeLayout {...baseOptions()} style={homeStyle}>
      <div id="main" className="mx-auto w-full max-w-(--fd-layout-width) flex-1 px-4 py-10">
        <noscript>
          <p>This address has no prerendered page. The landing page and the docs work without JavaScript.</p>
        </noscript>
      </div>
    </HomeLayout>
  )
}

export function ErrorBoundary({ error }: Route.ErrorBoundaryProps) {
  const notFound = isRouteErrorResponse(error) && error.status === 404
  return (
    <HomeLayout {...baseOptions()} style={homeStyle}>
      <div id="main" className="mx-auto w-full max-w-(--fd-layout-width) flex-1 px-4 py-10">
        {notFound ? (
          <NotFoundBody />
        ) : (
          <>
            <h1 className="text-2xl font-semibold tracking-tight">This page failed to render</h1>
            <p className="mt-2 text-fd-muted-foreground">
              {error instanceof Error ? error.message : isRouteErrorResponse(error) ? error.statusText : String(error)}
            </p>
          </>
        )}
      </div>
    </HomeLayout>
  )
}
