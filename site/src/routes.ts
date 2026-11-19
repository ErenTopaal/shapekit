import { index, layout, prefix, route, type RouteConfig } from "@react-router/dev/routes"

export default [
  index("routes/home.tsx"),
  route("docs/*", "routes/docs.tsx"),
  // a resource route prerendered to one json file; the search dialog loads it in the browser
  route("api/search.json", "routes/search.ts"),
  // the docs as markdown for llms and agents, prerendered like the search index
  route("llms.txt", "routes/llms.txt.ts"),
  route("llms-full.txt", "routes/llms-full.txt.ts"),
  route("llms.mdx/docs/*", "routes/llms.mdx.docs.ts"),
  ...prefix("dashboard", [
    layout("routes/dashboard/layout.tsx", [
      // the start page: one question for each reader, each answered by one page below
      index("routes/dashboard/overview.tsx"),
      route("sdk", "routes/dashboard/sdk.tsx"),
      route("indexer", "routes/dashboard/indexer.tsx"),
      route("findings", "routes/dashboard/findings.tsx"),
      route("corpus", "routes/dashboard/corpus.tsx"),
      route("corpus/entries", "routes/dashboard/entries.tsx"),
      route("corpus/cap/:cap", "routes/dashboard/cap.tsx"),
      route("corpus/:entry", "routes/dashboard/entry.tsx"),
      route("glossary", "routes/dashboard/glossary.tsx"),
      route("sources", "routes/dashboard/sources.tsx"),
      // the coverage pages moved under the corpus page; their old addresses forward
      route("coverage", "routes/dashboard/moved.tsx", { id: "moved-coverage" }),
      route("coverage/:cap", "routes/dashboard/moved.tsx", { id: "moved-cap" }),
      route("*", "routes/dashboard/not-found.tsx"),
    ]),
  ]),
  route("*", "routes/not-found.tsx"),
] satisfies RouteConfig
