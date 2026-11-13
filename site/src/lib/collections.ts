import { defineDocs } from "fumadocs-mdx/macro"

// content/docs is generated from ../docs and ../README.md by scripts/docs-content.ts.
// async keeps each page's compiled body in its own chunk, loaded when the page is opened.
// kept apart from source.ts so the browser bundle does not pull in the page-tree loader.
export const docs = defineDocs({
  dir: "content/docs",
  docs: { async: true },
})
