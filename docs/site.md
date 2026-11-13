# The website

`site/` is the website for shapekit.org: the documentation under `/docs` and a dashboard over
the committed results under `/dashboard`. The dashboard shows which shape families and CAPs the
corpus covers, which SDK version diverges on which entry, what the wallet-backend reference run
found, and the findings list. The site root redirects to `/docs`; a landing page can replace
that one route module (`site/src/routes/home.tsx`) without touching the docs or the dashboard.

Every page is prerendered to a static HTML file at build time, so the site needs no server-side
code: any static file server can host it. It has no analytics, no cookies and no third-party
requests. Fonts (IBM Plex Sans and IBM Plex Mono) are bundled, docs search runs in the browser
over an index file the build writes, and the only thing kept in the browser is the colour theme
choice, in `localStorage` under `shapekit-theme`. The production server sends a content
security policy that only allows the site's own origin, so a third-party request would fail.

## Stack

React Router 8.5.1 in framework mode on Vite 8.3.8, with `ssr: false` and a `prerender` list
(`site/react-router.config.ts`); the documentation uses Fumadocs (fumadocs-core and
fumadocs-ui 16.19.0, fumadocs-mdx 15.6.1); styles are Tailwind CSS 4 with shadcn/ui components.
Fumadocs takes its colours from the same shadcn variables as the dashboard
(`fumadocs-ui/css/shadcn.css`), so the docs and the dashboard share one palette in the light and
the dark theme, and one navigation bar. Versions in `site/package.json` are exact and
`site/package-lock.json` is committed. ESLint is not used because typescript-eslint does not
support TypeScript 7; oxlint covers the React, hooks and accessibility rules instead.

## Routes

| Route | What it is | Prerendered as |
|---|---|---|
| `/` | redirect to `/docs` (a meta refresh, works without JavaScript) | `index.html` |
| `/docs`, `/docs/<group>/<page>` | one page per document, see below | `docs/.../index.html` |
| `/api/search.json` | the docs search index, downloaded by the search dialog | `api/search.json` |
| `/dashboard` | overview | `dashboard/index.html` |
| `/dashboard/coverage`, `/dashboard/coverage/<CAP>` | coverage per CAP, one page per CAP in `coverage.json` | `dashboard/coverage/.../index.html` |
| `/dashboard/sdk`, `/dashboard/indexer`, `/dashboard/findings` | SDK matrix, indexer run, findings | `dashboard/<page>/index.html` |
| any other address | not-found page | `__spa-fallback.html`, served with status 404 |

Docs pages are complete HTML and read without JavaScript. Dashboard pages are prerendered as
their navigation with a loading state; the browser then fetches `/data/*.json` and renders the
page. Filters live in the query string (`/dashboard/sdk?entry=...`), so a filtered view can be
linked. Old dashboard links of the form `/#/sdk` are redirected to `/dashboard/sdk`. Next to each
route's HTML file the build writes a `.data` file that the client router fetches on in-site
navigation.

## Docs come from docs/

`docs/` and `README.md` stay the only copies of the documentation. Before every build
`site/scripts/docs-content.ts` writes `site/content/docs/` (ignored by git) from them:

| Group | Page | Source |
|---|---|---|
| Getting started | `/docs` (Introduction) | `README.md`: introduction, "What is in it", "Infrastructure and privacy", "Maintenance", "License" |
| | `/docs/install` | `README.md`: "Install" and "Building and checking" |
| | `/docs/quick-start` | `README.md`: "Quick start" |
| Corpus | `/docs/corpus/adding-a-shape` | `docs/adding-a-shape.md` (the sidebar also links the dashboard's coverage page) |
| Harness | `/docs/harness/adapter-interface`, `/docs/harness/replay-server` | `docs/adapter-interface.md`, `docs/replay-server.md` |
| SDK conformance | `/docs/sdk/testee-protocol`, `/docs/sdk/sdk-ci` | `docs/testee-protocol.md`, `docs/sdk-ci.md` |
| Linter | `/docs/linter/indexerlint` | `docs/linter.md` |
| Guide | `/docs/guide/indexer-hardening` | `docs/guide/indexer-hardening.mdx` |
| Operations | `/docs/operations/infrastructure`, `maintenance`, `protocol-upgrade-runbook`, `differential-fuzzing`, `site` | the `docs/` file of the same name |

On the way the script:

- takes each page's title from its first heading and its description (used for the page's
  meta description and for search) from its first sentence; the readme-derived pages have fixed
  descriptions, and the guide keeps the title and description of its own frontmatter;
- rewrites relative links between these files to site routes, including anchors, and links to
  any other repository file to `https://github.com/ErenTopaal/shapekit/blob/main/...` (`tree/`
  for a directory); a link to a file that does not exist is reported on the console;
- adapts the guide, which is written for stellar-docs: `{#id}` heading ids become Fumadocs'
  `[#id]`, `<Warning>` becomes a warn callout, its site-absolute links point at
  developers.stellar.org, and a callout at the top says it is a draft pending SDF review whose
  canonical copy will be on developers.stellar.org.

Each docs page ends with a link to the repository file it was generated from. To add a page,
add the document under `docs/` and list it in the `groups` table of the script.

## Where the numbers come from

Every number the dashboard shows is read from JSON that `shapekit site data` writes. The command
(`crates/shapekit-cli/src/site_cmd.rs`) reads committed files only, never a run output under
`target/`, so CI rebuilds the same data from a checkout:

| File read | What it gives the site |
|---|---|
| `corpus/manifest.json` | every entry: family, CAPs, protocol, source class, expected decode, file paths |
| `corpus/coverage/coverage.json` | per-CAP, per-family, per-fragment and per-arm coverage, and the summary |
| `registry/*.toml` | CAP texts (linked at the pinned stellar-protocol commit), XDR additions, family priority and protocol range |
| `findings/triage.toml` | the paths of the committed harness report, conformance matrices and linter result |
| `findings/inputs/*` | those reduced run outputs, as `findings/triage.toml` names them |
| `findings/findings.json` | the findings record and its summary |
| `conformance/versions.toml` | the SDK version matrix and the current version of each SDK |
| `conformance/known-failures/*.txt` | known-failure lines per SDK and version |
| `conformance/reproductions.toml` | the upstream sightings the runner must reproduce |
| `pins.toml` | pinned upstream versions shown on the overview |

The commit is passed with `--commit`; the command does not read git. It writes six files:
`meta.json` (commit, repository, corpus header, pins, and the path, size and sha256 of every
file it read and of every file it wrote), `coverage.json`, `entries.json`, `conformance.json`,
`harness.json` and `findings.json`. The output is deterministic: the same inputs and flags give
the same bytes.

Two things the site shows are derived rather than copied, and both are plain counts over the
committed files:

- SDK totals per version: failing cells by result, cells with a known issue, and failing checks
  by operation, counted over the committed matrix. The committed matrices keep failing cells
  only (see the `reduction` field of `findings/inputs/provenance.json`), so an entry that passed
  or was unsupported at every version is not listed.
- Reproduction rows: for each version a row names, whether the committed matrix has every named
  check of the row failing (before the fix) or no counted check failing (after it). Rows built
  from probes are not in the matrix and say so. `shapekit conformance reproduce` remains the
  check that actually runs them.

`crates/shapekit-cli/tests/site_data.rs` runs the command over the committed files and checks
that the summaries equal the source files', that per-version and per-family counts add up to
the source totals, that every linked file exists, that the recorded sha256 values match, and
that two runs give identical bytes.

## Building it locally

Needs the Rust toolchain of the workspace and Node.js 26.13.1.

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit site data --out site/public/data --commit "$(git rev-parse HEAD)"
cd site
npm ci
npm run typecheck       # generates the docs content and route types, then tsc
npm run lint            # oxlint
npm run dev -- --host   # development server
npm run build           # prerenders every route into dist/client
npm run preview         # serves dist/client with the production caddy configuration (docker), port 5182
```

`site/public/data/` and `site/content/` are ignored by git; both are always regenerated. Run
`npm run preview` again after a build, since the build replaces `dist/`.

## Checks in CI

`.github/workflows/site.yml` runs on pull requests and pushes to `main` that touch the site, its
docs or its inputs. It builds the CLI, runs `shapekit site data`, then `npm ci`, the typecheck,
the linter and the build; checks that every docs page, every CAP page and every dashboard page
has a prerendered file; validates both caddy configurations with the pinned caddy image; and
uploads `site/dist/client` as an artifact. It does not deploy: the site is hosted on the
owner's server.

## Hosting

The build output, `site/dist/client`, is plain static files; any static file server can host
it. [`site/Caddyfile`](../site/Caddyfile) is a complete configuration for serving it with
caddy from `/srv/current`:

- `try_files {path} {path}/index.html`, so every prerendered route resolves;
- `Cache-Control: public, max-age=31536000, immutable` for `/assets/*`, whose file names carry
  a content hash, and `no-cache` for everything else;
- the content security policy, which allows only the site's own origin;
- `__spa-fallback.html` with status 404 for unknown addresses;
- markdown and text content types for the llms files, and `/docs/<page>.md` and
  `Accept: text/markdown` requests for a docs page mapped onto them.

It serves plain HTTP; TLS, compression and HSTS belong to whatever terminates HTTPS in front of
it. `npm run preview` runs exactly this file in the pinned caddy image, so a preview behaves
like a production host. Another server needs the same five behaviours.
