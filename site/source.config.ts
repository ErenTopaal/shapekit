import { defineConfig } from "fumadocs-mdx/config"
import { remarkSteps } from "fumadocs-core/mdx-plugins/remark-steps"

// global mdx options for the collections defined with the macro api in src/lib. numbered
// headings ("## 1. The family record") become fumadocs steps, so the step-by-step docs
// (adding a shape, the protocol upgrade runbook) read as a numbered sequence on the site.
export default defineConfig({
  mdxOptions: {
    remarkPlugins: (v) => [...v, remarkSteps],
  },
})
