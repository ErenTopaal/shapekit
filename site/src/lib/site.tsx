import type { BaseLayoutProps } from "fumadocs-ui/layouts/shared"

export const repoUrl = "https://github.com/ErenTopaal/shapekit"

export function blob(path: string): string {
  return `${repoUrl}/blob/main/${path}`
}

export function Wordmark() {
  return <span className="font-mono text-[1.05rem] font-medium tracking-tight">shapekit</span>
}

// pages outside the docs (dashboard, not-found) share one content width with the navigation bar
export const homeStyle = { "--fd-layout-width": "76rem" } as React.CSSProperties

// one navigation for the landing page, the docs and the dashboard
export function baseOptions(): BaseLayoutProps {
  return {
    nav: { title: <Wordmark />, url: "/" },
    githubUrl: repoUrl,
    themeSwitch: { mode: "light-dark-system" },
    links: [
      { text: "Docs", url: "/docs", active: "nested-url" },
      { text: "Dashboard", url: "/dashboard", active: "nested-url" },
    ],
  }
}
