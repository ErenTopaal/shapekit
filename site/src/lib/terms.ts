// the vocabulary the dashboard shows, in plain words, with where each term is defined. the
// words follow the doc comments and docs they cite; when one of those changes, change this.

// crates/shapekit-findings/src/triage.rs, enum Severity, in its order (worst first)
export const severities: { id: string; label: string; meaning: string }[] = [
  { id: "halt", label: "halt", meaning: "ingestion stops: the target retries the same ledger or exits" },
  { id: "data-loss", label: "data loss", meaning: "facts the ledger shows are not stored at all" },
  { id: "wrong-data", label: "wrong data", meaning: "facts are stored with other values than the ledger shows, or invented" },
  {
    id: "divergence",
    label: "divergence",
    meaning:
      "an SDK answers differently from the network: it accepts what core refuses or the reverse, or orders or encodes differently",
  },
  {
    id: "robustness",
    label: "robustness",
    meaning:
      "a missing limit: input the network never emits can exhaust the reader, or a cap is wider than core's",
  },
]

// crates/shapekit-findings/src/triage.rs, enum Status
// the order is the order of the findings page: reported statuses first
export const statuses: { id: string; label: string; meaning: string }[] = [
  { id: "filed", label: "filed by shapekit", meaning: "shapekit filed an upstream issue; the link is the issue" },
  {
    id: "duplicate-of",
    label: "reported before",
    meaning: "an upstream issue or fix covered it before shapekit found it; the link is that issue or pull request",
  },
  { id: "private-report", label: "private report", meaning: "shapekit reported it through the private security channel of the upstream project" },
  { id: "draft", label: "draft", meaning: "an issue text is in the repository, but nobody filed it upstream" },
  { id: "wont-fix", label: "intended behavior", meaning: "the behavior is intended, so shapekit did not file it; the record gives the reason" },
]

export function severityRank(s: string): number {
  const i = severities.findIndex((x) => x.id === s)
  return i < 0 ? severities.length : i
}

// docs/adapter-interface.md, "Verdicts", in the harness's order
export const verdicts: { id: string; label: string; meaning: string }[] = [
  {
    id: "pass",
    label: "pass",
    meaning:
      "every fact the adapter claims for the entry's transactions was stored with equal values, and nothing undecodable or extra was stored",
  },
  {
    id: "mishandled",
    label: "mishandled",
    meaning:
      "a fact was stored with another value, an address that does not decode was stored, or the indexer halted, crashed or ran out of memory at the entry's ledger",
  },
  { id: "dropped", label: "dropped", meaning: "a participant or change the ledger meta shows has no counterpart in what the indexer stored" },
  { id: "not-evaluated", label: "not evaluated", meaning: "the indexer stopped at an earlier ledger, or the chain could not be run" },
  {
    id: "out-of-scope",
    label: "out of scope",
    meaning:
      "the entry holds no fact the adapter claims the indexer records, its protocol is outside what the indexer ingests, or the only claimed fact is one the meta cannot show",
  },
]

export const verdictMeaning = (v: string) => verdicts.find((x) => x.id === v)?.meaning ?? ""

// crates/shapekit-registry/src/model.rs, enum SourceClass, and corpus/coverage/COVERAGE.md
export const sourceClasses: { id: string; meaning: string; core: boolean }[] = [
  { id: "core-golden", meaning: "taken from stellar-core's own golden ledger close meta (its --check-lcm test files)", core: true },
  { id: "local-core", meaning: "recorded from the pinned stellar-core, running standalone on a Docker network with no route off the host", core: true },
  { id: "testnet-history", meaning: "captured from testnet history", core: true },
  { id: "mainnet-history", meaning: "captured from mainnet history", core: true },
  {
    id: "local-core-refused",
    meaning: "an envelope the pinned core refused at submission; it never reached a ledger, but core's answer is recorded",
    core: false,
  },
  { id: "synthetic", meaning: "bytes built by hand, marked synthetic in the manifest", core: false },
]

// docs/testee-protocol.md, "How the runner uses the ops"
export const checkResults: { id: string; label: string; meaning: string }[] = [
  {
    id: "diverges-from-network",
    label: "differs from the network",
    meaning: "the SDK accepted what core refuses, refused what core emits, or read other values than the expected ones",
  },
  {
    id: "diverges-from-peers",
    label: "differs from other SDKs",
    meaning:
      "on an entry whose answer the network leaves open (implementation-defined), the SDK answered unlike the majority of the SDKs",
  },
  { id: "error", label: "testee error", meaning: "the SDK's testee failed to answer" },
  {
    id: "unsupported",
    label: "unsupported",
    meaning:
      "the SDK version has no way to answer (a missing op), or the entry is from a protocol newer than the version knows; not a failure",
  },
]

// crates/shapekit-manifest/src/model.rs, Expected, and docs/testee-protocol.md
export const expectations: { id: string; meaning: string }[] = [
  { id: "must-accept", meaning: "an SDK must decode the bytes (and re-encode them to the same bytes when the entry round-trips)" },
  { id: "must-reject", meaning: "an SDK must refuse the bytes, as core does" },
  { id: "implementation-defined", meaning: "the network does not decide; every SDK must give the same answer" },
  { id: "invalid-by-core", meaning: "core refuses the transaction" },
  { id: "not-emittable", meaning: "no conforming encoder writes these bytes" },
]

export const docs = {
  architecture: "/docs/architecture",
  verdicts: "/docs/harness/adapter-interface#verdicts",
  claims: "/docs/harness/adapter-interface#claims",
  adapter: "/docs/harness/adapter-interface",
  harnessReproductions: "/docs/harness/adapter-interface#reproductions",
  walletBackendAdapter: "/docs/harness/adapter-interface#the-wallet-backend-adapter",
  checks: "/docs/sdk/testee-protocol#how-the-runner-uses-the-ops",
  knownFailures: "/docs/sdk/testee-protocol#known-failures",
  ops: "/docs/sdk/testee-protocol#operations",
  testee: "/docs/sdk/testee-protocol",
  sdkCi: "/docs/sdk/sdk-ci",
  indexerCi: "/docs/harness/indexer-ci",
  indexerCiLocal: "/docs/harness/indexer-ci#reproducing-a-result-locally",
  addingShape: "/docs/corpus/adding-a-shape",
  familyRecord: "/docs/corpus/adding-a-shape#1-the-family-record",
  linter: "/docs/linter/indexerlint",
  guide: "/docs/guide/indexer-hardening",
  site: "/docs/operations/site",
  infrastructure: "/docs/operations/infrastructure",
}

// docs/linter.md heading anchors of the three rules
export const lintRules: Record<string, string> = {
  IDX001: "idx001-unvalidated-field-trusted",
  IDX002: "idx002-missing-depth-or-size-cap",
  IDX003: "idx003-participants-from-envelope",
}

// repository files that define a term the docs do not
export const sources = {
  triage: "crates/shapekit-findings/src/triage.rs",
  signature: "crates/shapekit-manifest/src/signature.rs",
  uncovered: "registry/uncovered.toml",
  coverageReport: "corpus/coverage/COVERAGE.md",
  caps: "registry/caps.toml",
  families: "registry/families.toml",
  fields: "registry/fields.toml",
  findingsReport: "findings/FINDINGS.md",
}

// the display label of an id of the lists above; an id the lists do not hold shows as it is
const labels = new Map<string, string>(
  [...severities, ...statuses, ...verdicts, ...checkResults].map((x) => [x.id, x.label]),
)
export const label = (id: string) => labels.get(id) ?? id

// the anchor of a term on the glossary page
export const glossaryHref = (id: string) => `/dashboard/glossary#${id}`

export interface GlossaryTerm {
  id: string
  name: string
  // the word in the repository, the cli output and the docs, when it is not the name
  repo?: string
  meaning: string
}

// the one glossary of the dashboard. every page links its terms here. each term has one
// meaning; the name is the word the pages show
export const glossary: { id: string; title: string; note?: string; terms: GlossaryTerm[] }[] = [
  {
    id: "findings-terms",
    title: "Findings",
    terms: [
      {
        id: "finding",
        name: "finding",
        meaning:
          "One root cause behind one or more failures. A finding names the affected component, its severity, the smallest corpus entry that shows it and a command that reproduces it.",
      },
      {
        id: "reported-finding",
        name: "reported finding",
        meaning:
          "A finding with an upstream decision: filed by shapekit, reported before, or reported privately. The findings total on every page counts reported findings. Drafts and intended behavior are not in the total.",
      },
      {
        id: "upstream-issue",
        name: "upstream issue",
        meaning:
          "The issue or pull request in the upstream project that a reported finding links. Two or more findings can link the same issue, so the pages count each issue one time: issues filed upstream by shapekit, and issues reported before by others. An issue is filed by shapekit when a finding with the status filed by shapekit links it.",
      },
      {
        id: "failure",
        name: "failure",
        meaning:
          "One wrong result in one run: a check that diverges, an indexer verdict that is not pass, or a linter result. A record maps each failure to one finding.",
      },
      {
        id: "unmapped",
        name: "unmapped failure",
        meaning: "A failure that no finding explains. The gate fails when a record has an unmapped failure.",
      },
      {
        id: "record",
        name: "first record, second record",
        repo: "findings/findings.json, candidate/findings/findings.json",
        meaning:
          "The two committed files that list the findings. The first record holds the findings of the first runs: the Rust, Go and JavaScript SDKs, wallet-backend and stellar-core. The second record holds the findings of later runs, most of them in the Dart, PHP, .NET and Kotlin Multiplatform SDKs, stellar-cli and stellar-etl. The repository calls the second record the candidate lane.",
      },
      {
        id: "component",
        name: "component",
        meaning: "The upstream project that has the defect, for example go-stellar-sdk or wallet-backend.",
      },
      {
        id: "siblings",
        name: "sibling finding",
        meaning: "Another finding that shapekit filed together with this finding, for example the same defect in another SDK.",
      },
      {
        id: "related",
        name: "related issue",
        meaning: "An upstream issue or pull request about the same code. It is not the issue of the finding.",
      },
    ],
  },
  {
    id: "severity-terms",
    title: "Severities",
    note: "The list starts with the worst severity.",
    terms: severities.map((x) => ({ id: `severity-${x.id}`, name: x.label, repo: x.id === x.label ? undefined : x.id, meaning: sentence(x.meaning) })),
  },
  {
    id: "status-terms",
    title: "Finding statuses",
    terms: statuses.map((x) => ({ id: `status-${x.id}`, name: x.label, repo: x.id, meaning: sentence(x.meaning) })),
  },
  {
    id: "sdk-terms",
    title: "SDK results",
    terms: [
      {
        id: "testee",
        name: "testee",
        meaning: "A small program that gives the corpus entries to one SDK version and returns the answers of the SDK.",
      },
      {
        id: "check",
        name: "check",
        meaning:
          "One operation on one file of an entry, for example decode and encode again, or parse an id. The runner compares the answer of the SDK with the answer of the network.",
      },
      {
        id: "diverging-entry",
        name: "diverging entry",
        meaning:
          "An entry on which at least one check of an SDK version fails. A lower count is better. The SDK results count the entries that the known-failures file of the SDK lists for the version, which are the entries a conformance run finds diverging.",
      },
      {
        id: "cell",
        name: "cell",
        meaning: "One entry at one SDK version. One cell can hold more than one failing check.",
      },
      {
        id: "known-failure",
        name: "known failure",
        meaning:
          "A diverging check that the known-failures file of the SDK lists with its issue. CI reports a known failure but does not fail on it. CI fails on a divergence that the file does not list.",
      },
      ...checkResults.map((x) => ({ id: `check-${x.id}`, name: x.label, repo: x.id === x.label ? undefined : x.id, meaning: sentence(x.meaning) })),
      {
        id: "pass-some-unsupported",
        name: "pass, some checks unsupported",
        repo: "pass (some checks unsupported)",
        meaning:
          "All checks that ran pass, but the SDK version cannot do some checks. This result is not a failure.",
      },
      {
        id: "sdk-reproduction",
        name: "reproduction",
        meaning:
          "A bug that somebody reported upstream before, which shapekit runs again. The run must fail on the version before the fix and pass on the version with the fix.",
      },
    ],
  },
  {
    id: "indexer-terms",
    title: "Indexer results",
    terms: [
      {
        id: "replay",
        name: "replay",
        meaning:
          "One corpus entry on one network, given ledger by ledger to an indexer. An indexer run replays each entry on testnet and on mainnet, so a run has two replays for each entry.",
      },
      {
        id: "verdict",
        name: "verdict",
        meaning: "The result of one replay. The harness compares the facts that the indexer stored with the facts in the ledger meta.",
      },
      ...verdicts.map((x) => ({ id: `verdict-${x.id}`, name: x.label, repo: x.id === x.label ? undefined : x.id, meaning: sentence(x.meaning) })),
      {
        id: "claim",
        name: "claim",
        meaning:
          "A fact that the adapter says the indexer records, for example participants or balance changes. A fact that the indexer does not claim is never a failure.",
      },
      {
        id: "adapter",
        name: "adapter",
        meaning: "A small program that connects the harness to one indexer. It gives the ledgers to the indexer and reads back what the indexer stored.",
      },
      {
        id: "oracle",
        name: "oracle file",
        meaning: "The expected facts of one entry on one network, read from the ledger meta. The harness needs this file to compare.",
      },
      {
        id: "linter",
        name: "indexerlint",
        meaning: "A linter for Go indexer code. It finds three ingestion mistakes in the source code.",
      },
    ],
  },
  {
    id: "corpus-terms",
    title: "Corpus",
    terms: [
      {
        id: "entry",
        name: "entry",
        meaning: "One test case of the corpus: XDR bytes, the expected answer and where the bytes come from.",
      },
      {
        id: "family",
        name: "shape family",
        meaning: "One rare shape of XDR, for example a muxed payment destination. The registry defines each family by its signature fragments.",
      },
      {
        id: "fragment",
        name: "fragment",
        meaning: "A part of the XDR that the entries of a family must contain, for example a union arm or a length.",
      },
      {
        id: "core-emitted",
        name: "emitted by stellar-core",
        repo: "core-emitted",
        meaning:
          "The bytes come from stellar-core: its test goldens, a local stellar-core, or testnet or mainnet history. Only these entries count as coverage.",
      },
      ...sourceClasses.map((x) => ({ id: `source-${x.id}`, name: x.id, meaning: sentence(x.meaning) })),
      {
        id: "indexer-visible",
        name: "indexer-visible CAP",
        meaning:
          "A CAP that changes what an indexer reads: new XDR in transactions, results or ledger meta. Other CAPs change only consensus, fees or arithmetic.",
      },
      {
        id: "union-arm",
        name: "union arm",
        meaning:
          "One variant that an XDR union can take, for example an operation type or a result code. An arm that a ledger can hold is an arm that stellar-core can write to a ledger. Arms with the blocker refused-before-ledger or never-emitted cannot occur in any ledger, so arm coverage counts only the other arms.",
      },
      {
        id: "blocker",
        name: "blocker",
        meaning: "The recorded reason why no entry from stellar-core covers a union arm, a fragment or a CAP.",
      },
      {
        id: "obligation",
        name: "obligation",
        meaning:
          "A point that the corpus must show with an entry. An obligation is met, unmeetable (no input can show it) or open.",
      },
      {
        id: "unvalidated-field",
        name: "unvalidated field",
        meaning: "A transaction field that stellar-core does not fully check. An indexer must not trust it.",
      },
      ...expectations.map((x) => ({ id: `expected-${x.id}`, name: x.id, meaning: sentence(x.meaning) })),
    ],
  },
]

// a meaning of the lists above as a sentence for the glossary
function sentence(s: string): string {
  return `${s.charAt(0).toUpperCase()}${s.slice(1)}.`
}

const glossaryTerms = new Map(glossary.flatMap((g) => g.terms.map((t) => [t.id, t] as const)))

// the short definition a page shows in place of a term: the first sentence of its glossary
// meaning. a group id or an unknown id has none
export function termDefinition(id: string): string | undefined {
  const m = glossaryTerms.get(id)?.meaning
  return m?.match(/^.*?\.(?=\s+[A-Z]|$)/s)?.[0]
}
