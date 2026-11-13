import { reactRouter } from "@react-router/dev/vite"
import tailwindcss from "@tailwindcss/vite"
import { fumadocsMdx } from "fumadocs-mdx/vite"
import { defineConfig } from "vite"

// the site is served at the domain root, so there is no base path to configure
export default defineConfig({
  plugins: [fumadocsMdx(), tailwindcss(), reactRouter()],
  resolve: {
    tsconfigPaths: true,
  },
  build: {
    sourcemap: false,
    // the guide's generated field and caps tables compile to one chunk of about 750 kB, loaded
    // only on that page
    chunkSizeWarningLimit: 800,
  },
})
