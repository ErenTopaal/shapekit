import { useMeta } from "@/lib/meta"
import { docs as docPages, sources } from "@/lib/terms"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Id, More, PageHeader, RepoLink, Section, TextLink, fmt, short } from "@/components/common"

// pages of this site's docs first, then committed files that only exist in the repository
const docs: { to?: string; path?: string; label: string }[] = [
  { to: "/docs", label: "What shapekit is" },
  { to: "/docs/quick-start", label: "Quick start" },
  { to: docPages.addingShape, label: "Add a shape to the corpus" },
  { to: docPages.testee, label: "Testee protocol for SDKs" },
  { to: docPages.sdkCi, label: "Run the corpus in the CI of an SDK" },
  { to: docPages.adapter, label: "Indexer adapter interface" },
  { to: docPages.indexerCi, label: "Run the corpus in the CI of an indexer" },
  { to: docPages.linter, label: "indexerlint, the linter for Go indexers" },
  { to: docPages.guide, label: "Indexer-hardening guide (draft)" },
  { to: docPages.infrastructure, label: "CI and infrastructure" },
  { to: docPages.site, label: "How shapekit makes the data of this site" },
  { path: "STATUS.md", label: "Status log, in the repository" },
  { path: sources.coverageReport, label: "Coverage report, in the repository" },
  { path: sources.findingsReport, label: "Findings report, in the repository" },
]

const pinNames: Record<string, string> = {
  "stellar-core": "stellar-core",
  "stellar-xdr": "stellar-xdr (XDR definitions)",
  "stellar-rpc": "stellar-rpc",
  "wallet-backend": "wallet-backend",
  "stellar-etl": "stellar-etl",
}

// where the dashboard's data comes from: the pinned upstream versions, the docs, and every
// committed file `shapekit site data` read
export function SourcesPage() {
  const meta = useMeta()
  const gw = meta.corpus.generated_with
  return (
    <div className="flex flex-col">
      <PageHeader title="Data sources">
        <p>
          <Id>shapekit site data</Id> reads committed files and writes the data of this dashboard. This page lists the
          pinned versions and the files. The data is from commit <Id>{short(meta.commit, 10)}</Id>, corpus{" "}
          {meta.corpus.version}, shapekit {meta.shapekit_version}. <TextLink to={docPages.site}>How shapekit makes the data</TextLink>.
        </p>
      </PageHeader>

      <Section
        title="Pinned versions"
        id="pins"
        description={
          <>
            shapekit builds the corpus with stellar-core <Id>{gw.core_image.tag.split("-")[0]}</Id> and stellar-xdr{" "}
            <Id>{gw.xdr_definitions.tag}</Id>. The versions are in <RepoLink path="pins.toml" />. The SDK versions are in{" "}
            <RepoLink path="conformance/versions.toml" />. The <TextLink to="/dashboard/sdk">SDK results</TextLink> show each
            SDK version.
          </>
        }
      >
        <Table>
          <caption className="sr-only">Pinned upstream versions</caption>
          <TableHeader>
            <TableRow>
              <TableHead>Project</TableHead>
              <TableHead>Version</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {meta.pins.git
              .filter((g) => g.name in pinNames)
              .map((g) => (
                <TableRow key={g.name}>
                  <TableCell className="font-medium whitespace-normal">{pinNames[g.name]}</TableCell>
                  <TableCell>
                    <Id>{g.ref ? g.ref.replace(/^refs\/tags\//, "") : short(g.sha ?? "", 10)}</Id>
                  </TableCell>
                </TableRow>
              ))}
            {meta.pins.toolchains.map((t) => (
              <TableRow key={t.name}>
                <TableCell className="font-medium">{t.name} toolchain</TableCell>
                <TableCell>
                  <Id>{t.version}</Id>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Section>

      <Section title="Documentation" id="docs">
        <ul className="flex flex-col gap-1.5 text-sm">
          {docs.map((d) => (
            <li key={d.to ?? d.path}>
              {d.to ? <TextLink to={d.to}>{d.label}</TextLink> : <RepoLink path={d.path ?? ""}>{d.label}</RepoLink>}
            </li>
          ))}
        </ul>
      </Section>

      <Section
        title="Files that the data comes from"
        id="files"
        description="The sha256 of each file is the digest of the file as shapekit read it."
      >
        <More label={`Show the ${meta.sources.length} source files`}>
          <Table>
            <caption className="sr-only">The files read to make the data of this site</caption>
            <TableHeader>
              <TableRow>
                <TableHead>File</TableHead>
                <TableHead className="text-right">Bytes</TableHead>
                <TableHead className="hidden md:table-cell">sha256</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {meta.sources.map((f) => (
                <TableRow key={f.path}>
                  <TableCell className="max-w-[60vw] whitespace-normal">
                    <RepoLink path={f.path} />
                  </TableCell>
                  <TableCell className="num text-right">{fmt.format(f.bytes)}</TableCell>
                  <TableCell className="hidden md:table-cell">
                    <Id className="text-muted-foreground">{f.sha256}</Id>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </More>
      </Section>
    </div>
  )
}
