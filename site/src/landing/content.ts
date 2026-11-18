// text of the known-bug cards, keyed by reproduction id. versions, commits, entries and verdicts
// come from adapters/walletbackend/reproductions.toml and conformance/reproductions.toml through
// site data; the build fails when an id here and the tables disagree (stats.ts).
export const proofText: Record<string, { title: string; observed: string; why: string }> = {
  "wallet-backend-729": {
    title: "Signed-payload signer stops ingestion",
    observed: "A SetOptions adds a signed-payload signer. Ingestion retried ledger 65 forever.",
    why: "wallet-backend stopped at ledger 65 with an error.",
  },
  "wallet-backend-731": {
    title: "Asset Contract trustline has no row",
    observed: "The trust function of the Stellar Asset Contract creates a trustline and emits no event. No trustline row was written.",
    why: "The trustline-created fact has no row.",
  },
  "wallet-backend-716-signed-payload": {
    title: "Signed-payload signer is cut short",
    observed: "The signer was stored cut to 32 bytes, so it does not decode back.",
    why: "A stored signer does not decode.",
  },
  "wallet-backend-716-muxed": {
    title: "Muxed Soroban source does not decode",
    observed: "A muxed Soroban source was stored as a participant that does not decode back.",
    why: "A stored participant does not decode.",
  },
  "wallet-backend-753": {
    title: "Muxed Soroban source stops ingestion",
    observed: "A muxed source and its base account were stored as two rows with one key.",
    why: "wallet-backend stopped at ledger 77 with a duplicate key error.",
  },
  "wallet-backend-792": {
    title: "Transfer to a muxed contract loses the mux id",
    observed: "An XLM transfer goes to a muxed contract address. The mux id of the destination was not stored.",
    why: "The mux id 18446744073709551615 is not stored.",
  },
  "js-1766-issuer-order": {
    title: "Issuers are sorted as text",
    observed: "Asset.compare ordered issuers by strkey text, not by key bytes. Valid pool pairs were refused.",
    why: "The asset order is not the order of stellar-core.",
  },
  "js-1766-map-key-order": {
    title: "Map keys are sorted in the wrong order",
    observed: "scvSortedMap sorted map keys in a different order from the Soroban host.",
    why: "The key order is not the order of the Soroban host.",
  },
  "js-1785-non-canonical-ids": {
    title: "Non-canonical ids are accepted",
    observed: "Muxed and offer ids were accepted in text that is not canonical.",
    why: "Text that is not canonical is accepted as an id.",
  },
  "js-1789-colliding-map-keys": {
    title: "Colliding map keys are merged",
    observed: "scValToNative merged two map entries whose keys collide.",
    why: "One of the two map entries is lost.",
  },
  "go-asset-less-than": {
    title: "Issuers are sorted as text",
    observed: "xdr.Asset.LessThan ordered issuers by strkey text, not by key bytes.",
    why: "The asset order is not the order of stellar-core.",
  },
  "go-strkey-noncanonical-chars": {
    title: "Strkeys with added bytes are accepted",
    observed: "Strkeys with added carriage returns, newlines or 0xff bytes were accepted.",
    why: "A strkey with added bytes is accepted.",
  },
  "rust-ghsa-3xhm-p452-9wjh": {
    title: "JSON input ignores maximum lengths",
    observed: "JSON with vectors over their maximum length was read.",
    why: "A value over its maximum length is accepted.",
  },
}
