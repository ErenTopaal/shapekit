// the vocabulary the dashboard shows, in plain words, with where each term is defined. the
// words follow the doc comments and docs they cite; when one of those changes, change this.

// crates/shapekit-findings/src/triage.rs, enum Severity, in its order (worst first)
export const severities: { id: string; meaning: string }[] = [
  { id: "halt", meaning: "ingestion stops: the target retries the same ledger or exits" },
  { id: "data-loss", meaning: "facts the ledger shows are not stored at all" },
  { id: "wrong-data", meaning: "facts are stored with other values than the ledger shows, or invented" },
  {
    id: "divergence",
    meaning:
      "an SDK answers differently from the network: it accepts what core refuses or the reverse, or orders or encodes differently",
  },
  {
    id: "robustness",
    meaning:
      "a missing limit: input the network never emits can exhaust the reader, or a cap is wider than core's",
  },
]

// crates/shapekit-findings/src/triage.rs, enum Status
export const statuses: { id: string; meaning: string }[] = [
  { id: "draft", meaning: "an issue body is written in the repository but nothing is filed upstream yet" },
  { id: "filed", meaning: "filed upstream; the link is the issue" },
  { id: "duplicate-of", meaning: "an upstream issue or fix already covers it; the link is that issue or pull request" },
  { id: "private-report", meaning: "reported through the upstream's private security channel" },
  { id: "wont-fix", meaning: "deliberately not filed, with the reason recorded" },
]

export function severityRank(s: string): number {
  const i = severities.findIndex((x) => x.id === s)
  return i < 0 ? severities.length : i
}

// docs/adapter-interface.md, "Verdicts", in the harness's order
export const verdicts: { id: string; meaning: string }[] = [
  {
    id: "pass",
    meaning:
      "every fact the adapter claims for the entry's transactions was stored with equal values, and nothing undecodable or extra was stored",
  },
  {
    id: "mishandled",
    meaning:
      "a fact was stored with another value, an address that does not decode was stored, or the indexer halted, crashed or ran out of memory at the entry's ledger",
  },
  { id: "dropped", meaning: "a participant or change the ledger meta shows has no counterpart in what the indexer stored" },
  { id: "not-evaluated", meaning: "the indexer stopped at an earlier ledger, or the chain could not be run" },
  {
    id: "out-of-scope",
    meaning:
      "the entry holds no fact the adapter claims the indexer records, its protocol is outside what the indexer ingests, or the only claimed fact is one the meta cannot show",
  },
]

export const verdictMeaning = (v: string) => verdicts.find((x) => x.id === v)?.meaning ?? ""

// crates/shapekit-registry/src/model.rs, enum SourceClass, and corpus/coverage/COVERAGE.md
export const sourceClasses: { id: string; meaning: string; core: boolean }[] = [
  { id: "core-golden", meaning: "taken from stellar-core's own golden ledger close meta (its --check-lcm test files)", core: true },
  { id: "local-core", meaning: "recorded from the pinned stellar-core on a private standalone network", core: true },
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
export const checkResults: { id: string; meaning: string }[] = [
  {
    id: "diverges-from-network",
    meaning: "the SDK accepted what core refuses, refused what core emits, or read other values than the expected ones",
  },
  {
    id: "diverges-from-peers",
    meaning:
      "on an entry whose answer the network leaves open (implementation-defined), the SDK answered unlike the majority of the SDKs",
  },
  { id: "error", meaning: "the SDK's testee failed to answer" },
  {
    id: "unsupported",
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
