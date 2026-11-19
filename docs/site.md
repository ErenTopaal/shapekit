# The website

The shapekit.org website is built from `site/`: the documentation under `/docs` and a
dashboard over the committed results under `/dashboard`. The dashboard has one page for each
question a reader asks: how an SDK does, how an indexer does, which defects shapekit found and
where they are reported, and what the corpus covers. It also has a page for every CAP and every
corpus entry, and one glossary. The site root is a landing
page (`site/src/routes/home.tsx`) that reads every number it shows from the same data files at build time
(`site/src/landing/stats.ts`).

Every page is prerendered to a static HTML file at build time, so the site needs no server-side
code: any static file server can host it. It has no analytics, no cookies and no third-party
requests. Fonts are bundled: IBM Plex Sans and IBM Plex Mono for the docs and the dashboard, and Plus Jakarta
Sans and Instrument Serif for the landing page, docs search runs in the browser
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
| `/` | the landing page; an old dashboard link of the form `/#/sdk` is redirected to `/dashboard/sdk` | `index.html` |
| `/docs`, `/docs/<group>/<page>` | one page per document, see below | `docs/.../index.html` |
| `/api/search.json` | the docs search index, downloaded by the search dialog | `api/search.json` |
| `/llms.txt`, `/llms-full.txt` | the docs for llms: an index of every page, and every page's markdown in one file | `llms.txt`, `llms-full.txt` |
| `/llms.mdx/docs/<group>/<page>/content.md` | one docs page as markdown; `/docs/<group>/<page>.md` serves the same file | `llms.mdx/docs/.../content.md` |
| `/dashboard` | the start page: one question for each reader, with a short answer and a link | `dashboard/index.html` |
| `/dashboard/sdk`, `/dashboard/indexer`, `/dashboard/findings` | SDK results, indexer results, findings | `dashboard/<page>/index.html` |
| `/dashboard/corpus`, `/dashboard/corpus/cap/<CAP>` | corpus coverage, and one page per CAP in `coverage.json` | `dashboard/corpus/.../index.html` |
| `/dashboard/corpus/entries`, `/dashboard/corpus/<entry>` | the list of corpus entries, and one page per entry in `entries.json` | `dashboard/corpus/.../index.html` |
| `/dashboard/glossary`, `/dashboard/sources` | the terms of the dashboard, and the source files and pinned versions of its data | `dashboard/<page>/index.html` |
| `/dashboard/coverage`, `/dashboard/coverage/<CAP>` | old addresses, which forward to `/dashboard/corpus` and `/dashboard/corpus/cap/<CAP>` | `dashboard/coverage/.../index.html` |
| `/corpus/...` | a copy of every corpus file the manifest names, for download and for the explorer's XDR-JSON | the files themselves |
| any other address | not-found page | `__spa-fallback.html`, served with status 404 |

Docs pages are complete HTML and read without JavaScript. Each has a "Copy Markdown" button and
an "Open" menu (the page on GitHub, its markdown, or a prompt to read it in an AI chat app; these
are plain links, nothing is sent until you follow one), and ends with an "Edit on GitHub"
link to its source file. Dashboard pages are prerendered as their navigation with a loading
state; the browser then fetches `/data/*.json` and renders the
page. Filters live in the query string (`/dashboard/sdk?entry=...`), so a filtered view can be
linked; a finding can be linked as `/dashboard/findings#WB-W3` (it opens) and a corpus entry as
`/dashboard/corpus/<entry-id>`. Each dashboard page links its terms to one glossary,
`/dashboard/glossary` (verdicts, severities, statuses, check results, source classes, blockers);
the words are kept in `site/src/lib/terms.ts`, next to the source each one follows. The numbers
that more than one page shows, the landing page included, come from one function each in
`site/src/lib/metrics.ts`. The findings total is `reported.findings` in `findings.json`: the
findings of both records with the status filed, duplicate-of or private-report. Old dashboard links of the form
`/#/sdk` are redirected to `/dashboard/sdk`. Next to each route's HTML file the build writes a `.data` file that the client router fetches on in-site
navigation.

## The corpus entries

`/dashboard/corpus/entries` lists every corpus entry. Its filters (CAP, shape, an SDK that fails on
the entry and the failing version, and the entry id) are kept in the query string, for example
`/dashboard/corpus/entries?sdk=go&version=v0.7.1`. An old link to `/dashboard/corpus` with these
filters forwards to this list. Each entry has its own page at
`/dashboard/corpus/<entry-id>` with:

- the manifest record: shape, kind, XDR type, CAPs (linked to their coverage pages), protocol
  range, expected decode result, source class and provenance, flags and upstream sightings;
- how the entry was recorded: its recipe IR file or recorded chain;
- its results: each SDK version whose committed matrix has a failing check on it, with the
  checks and the known-failures issue, each committed indexer run's verdict per network, and
  the findings that name the entry;
- commands that run it again (`shapekit check sdk` per SDK, at the newest failing version
  when there is one, and `shapekit check indexer` on its chain);
- each file with its size, a download link and its decoded XDR-JSON. XDR-JSON files over
  512 KiB are offered as a download instead of shown;
- the fields the entry exercises, with their rows in the guide's field table, and the guide
  sections and checklist items that cover them.

The committed matrices keep failing cells only, so the explorer cannot tell a pass from an
unsupported answer: an entry without a failing SDK version is shown as such, not as passing.

The files come from `site/scripts/corpus-files.ts`, which runs before every dev server start
and build. It copies each file `corpus/manifest.json` names (every variant's `.xdr` and
`.expected.json`, and every vector file) to `site/public/corpus/`, so the site serves
`/corpus/entries/<family>/<entry>/<variant>.xdr` and the explorer reads the XDR-JSON from the
site's own origin. That is about 3,800 files and 95 MiB, which makes the built site about
150 MiB; the 1,092 entry pages bring the build to about 1,220 prerendered HTML pages, and `npm run build`
takes about 20 seconds on a 10-core machine.

## Docs come from docs/

`docs/` and `README.md` stay the only copies of the documentation. Before every build
`site/scripts/docs-content.ts` writes `site/content/docs/` (ignored by git) from them:

| Group | Page | Source |
|---|---|---|
| Getting started | `/docs` (Introduction) | `README.md`: introduction, "What is in it", "Infrastructure and privacy", "Maintenance", "License" |
| | `/docs/install` | `README.md`: "Install" and "Building and checking" |
| | `/docs/quick-start` | `README.md`: "Quick start" |
| Corpus | `/docs/corpus/adding-a-shape` | `docs/adding-a-shape.md` (the sidebar also links the corpus coverage page and the list of corpus entries) |
| Harness | `/docs/harness/adapter-interface`, `/docs/harness/mapping-adapter`, `/docs/harness/replay-server`, `/docs/harness/indexer-ci` | `docs/adapter-interface.md`, `docs/mapping-adapter.md`, `docs/replay-server.md`, `docs/indexer-ci.md` |
| SDK conformance | `/docs/sdk/testee-protocol`, `/docs/sdk/sdk-ci`, `/docs/sdk/seeded-faults` | `docs/testee-protocol.md`, `docs/sdk-ci.md`, `docs/seeded-faults.md` |
| Results | `/docs/results/report` | `docs/report.md` |
| Linter | `/docs/linter/indexerlint` | `docs/linter.md` |
| Guide | `/docs/guide/indexer-hardening` | `docs/guide/indexer-hardening.mdx` |
| Operations | `/docs/operations/infrastructure`, `maintenance`, `protocol-upgrade-runbook`, `differential-fuzzing`, `site` | the `docs/` file of the same name |

On the way the script:

- takes each page's title from its first heading and its description (used for the page's
  meta description and for search) from its first sentence; the readme-derived pages have fixed
  descriptions, and the guide keeps the title and description of its own frontmatter. The page
  header shows the description only where the page does not already open with it (the
  readme-derived pages and the guide);
- gives each sidebar group a lucide icon, and puts cards on the index page: the install and
  quick start pages after the introduction, every other page at the end;
- rewrites relative links between these files to site routes, including anchors (the readme's
  own anchors point at the page their section landed on), and links to any other repository
  file to `https://github.com/ErenTopaal/shapekit/blob/main/...` (`tree/` for a directory); a
  link to a file that does not exist is reported on the console. A link whose text is a doc's
  file name (`docs/linter.md`) reads as the page's title;
- adapts the guide, which is written for stellar-docs: `{#id}` heading ids become Fumadocs'
  `[#id]`, `<Warning>` becomes a warn callout, its site-absolute links point at
  developers.stellar.org, and a callout at the top says that the page is the draft for review
  before publication on developers.stellar.org. Its generated field and caps tables, whose
  five and six columns hold paragraphs, become one record per row on the site; the source keeps
  the tables.

Numbered headings (`## 1. The family record`) render as Fumadocs steps (`remarkSteps`, set in
`site/source.config.ts`), and a code block's `title="..."` names the file it shows.

## Markdown for llms

The build also writes the docs as markdown, with Fumadocs' llms integration
(`site/src/lib/source.ts`): `/llms.txt` lists every page with its description, `/llms-full.txt`
holds every page, and `/llms.mdx/docs/<page>/content.md` holds one page. The markdown is the
generated content with Fumadocs' processed markdown (`includeProcessedMarkdown`), so links point
at site routes; callouts become quotes, cards a list, and the guide's records nested lists
(`site/src/lib/markdown.tsx`). These are prerendered files like the rest. The Caddyfile serves
them with markdown and text content types and maps `/docs/<page>.md` (and `/docs.md`) onto them.
Fumadocs answers `Accept: text/markdown` at the page's own address with server middleware,
which a static site does not run; the Caddyfile does it instead: a request for a docs page
whose `Accept` names `text/markdown` and not `text/html` gets the page's markdown, and docs
responses carry `Vary: Accept`. Another server needs the same mappings for those addresses.

Each docs page ends with a link to the repository file it was generated from. To add a page,
add the document under `docs/` and list it in the `groups` table of the script.

## Where the numbers come from

Every number the dashboard shows is read from JSON that `shapekit site data` writes. The command
(`crates/shapekit-cli/src/site_cmd.rs`) reads committed files only, never a run output under
`target/`, so CI rebuilds the same data from a checkout:

| File read | What it gives the site |
|---|---|
| `corpus/manifest.json` | every entry: family, CAPs, protocol, source class, expected decode, file paths, fields exercised, notes, recipe and chain |
| the corpus files the manifest names | their sizes only (the explorer shows sizes and decides whether XDR-JSON is shown inline) |
| `registry/guide-checklist.toml` | the guide's checklist items and the fields each names |
| `corpus/coverage/coverage.json` | per-CAP, per-family, per-fragment and per-arm coverage, and the summary |
| `corpus/coverage/COVERAGE.md` | the meaning of each blocker, from its "Blockers" table |
| `registry/*.toml` | CAP texts (linked at the pinned stellar-protocol commit), XDR additions, family priority and protocol range |
| `findings/triage.toml` | the paths of the committed harness reports, conformance matrices and linter result |
| `findings/inputs/*` | those reduced run outputs, as `findings/triage.toml` names them |
| `adapters/walletbackend/reproductions.toml` | the wallet-backend sightings `shapekit harness reproduce` checks, with their commits and expected verdicts |
| `findings/findings.json` | the findings record and its summary |
| `candidate/findings/findings.json` | the candidate lane's findings record and its summary, beside the main one (the record next to the triage `findings/triage.toml` names as `candidates`) |
| `conformance/versions.toml` | the SDK version matrix and the current version of each SDK |
| `conformance/known-failures/*.txt` | known-failure lines per SDK and version |
| `conformance/reproductions.toml` | the upstream sightings the runner must reproduce |
| `pins.toml` | pinned upstream versions shown on the data sources page |

The commit is passed with `--commit`; the command does not read git. It writes six files:
`meta.json` (commit, repository, corpus header, pins, and the path, size and sha256 of every
file it read and of every file it wrote), `coverage.json`, `entries.json`, `conformance.json`,
`harness.json` and `findings.json`. The output is deterministic: the same inputs and flags give
the same bytes.

Three things the site shows are derived rather than copied, and all are plain counts over the
committed files:

- SDK totals per version: failing cells by result, cells with a known issue, every check of
  those cells by result, and failing checks by operation, counted over the committed matrix. A
  failing check is one whose result is `error`, `diverges-from-network` or
  `diverges-from-peers`; a failing cell also lists the checks its version could not answer
  (`unsupported`), which are counted by result only. The committed matrices keep failing cells
  only (see the `reduction` field of `findings/inputs/provenance.json`), so an entry that passed
  or was unsupported at every version is not listed.
- Reproduction rows: for each version a row names, whether the committed matrix has every named
  check of the row failing (before the fix) or no counted check failing (after it). Rows built
  from probes are not in the matrix and say so. `shapekit conformance reproduce` remains the
  check that actually runs them.
- Harness verdicts per entry and network, the distinct entries replayed and the chains per
  network, from each committed report. The wallet-backend reproduction rows carry, for each
  expected verdict, the verdict the committed run gave the same entry at the pinned commit; the
  rows' own pre-fix and fixed commits are run by `shapekit harness reproduce` in the nightly
  workflow, which commits no result.

`crates/shapekit-cli/tests/site_data.rs` runs the command over the committed files and checks
that the summaries equal the source files', that per-version and per-family counts add up to
the source totals, that every linked file exists, that the recorded sha256 values match, and
that two runs give identical bytes. It also checks that every blocker the coverage names has a
meaning taken verbatim from `COVERAGE.md`, that the per-entry verdicts equal the report's rows,
that failing checks by operation add up to the failing results, and that every entry a
reproduction row names is a corpus entry.

## Building it locally

Needs the Rust toolchain of the workspace and Node.js 26.13.1.

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit site data --out site/public/data --commit "$(git rev-parse HEAD)"
cd site
npm ci
npm run typecheck       # generates the docs content and route types, then tsc
npm run lint            # oxlint
npm run build           # prerenders every route into dist/client
npm run preview         # serves dist/client with site/Caddyfile in the pinned caddy image (docker), port 5182
```

`npm run preview -- stop` stops the preview container. For a development server that reloads on
every change, run `npm run dev -- --host` in `site/` instead of the build and preview; it keeps
running until you stop it.

`site/public/data/`, `site/public/corpus/`, `site/content/` and `site/.source/`
(fumadocs-mdx's output for `site/source.config.ts`) are ignored by git; all are always
regenerated. Run
`npm run preview` again after a build, since the build replaces `dist/`.

## Checks in CI

`.github/workflows/site.yml` runs on pull requests and pushes to `main` that touch the site, its
docs or its inputs. It builds the CLI, runs `shapekit site data`, then `npm ci`, the typecheck,
the linter and the build; checks that every docs page, every CAP page, every dashboard page
and every corpus entry's explorer page has a prerendered file; validates `site/Caddyfile` with
the pinned caddy image; and uploads `site/dist/client` as an artifact. It does not deploy; hosting is outside this repository.

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
