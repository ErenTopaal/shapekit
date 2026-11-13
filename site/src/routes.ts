import { index, layout, prefix, route, type RouteConfig } from "@react-router/dev/routes"

export default [
  index("routes/home.tsx"),
  route("docs/*", "routes/docs.tsx"),
  // a resource route prerendered to one json file; the search dialog loads it in the browser
  route("api/search.json", "routes/search.ts"),
  ...prefix("dashboard", [
    layout("routes/dashboard/layout.tsx", [
      index("routes/dashboard/overview.tsx"),
      route("coverage", "routes/dashboard/coverage.tsx"),
      route("coverage/:cap", "routes/dashboard/cap.tsx"),
      route("sdk", "routes/dashboard/sdk.tsx"),
      route("indexer", "routes/dashboard/indexer.tsx"),
      route("findings", "routes/dashboard/findings.tsx"),
      route("*", "routes/dashboard/not-found.tsx"),
    ]),
  ]),
  route("*", "routes/not-found.tsx"),
] satisfies RouteConfig
