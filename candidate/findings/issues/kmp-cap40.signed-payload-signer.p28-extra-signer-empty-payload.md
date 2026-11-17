<!-- repo: Soneso/kmp-stellar-sdk -->
# com.soneso.stellar:stellar-sdk-jvm 1.18.0: the sdk refused a value the network accepts: IllegalArgumentException: SignerKeyEd25519SignedPayloadXdr: has an empty signed payload, which has no strkey form (cap40.signed-payload-signer.p28-extra-signer-empty-payload)

## What happens

On the input below, `com.soneso.stellar:stellar-sdk-jvm` 1.18.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap40.signed-payload-signer.p28-extra-signer-empty-payload` (family `cap40.signed-payload-signer`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_json` on 1.18.0

the sdk refused a value the network accepts: IllegalArgumentException: SignerKeyEd25519SignedPayloadXdr: has an empty signed payload, which has no strkey form

```json
{
  "depth_limit": 1000,
  "len_limit": 268,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAABAAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABAubjRLadWry055p2FmhZLMrRVYMmCBj+7DBwXGTholz5BUocWpW9I4ZlBXcDXWW4arBIPZeJlBiwySjdj9s1UBQ=="
}
```

Expected: `{"any json, reference":{"tx":{"signatures":[{"hint":"2722451e","signature":"b9b8d12da756af2d39e69d859a164b32b45560c982063fbb0c1c17193868973e41528716a56f48e199415dc0d7596e1aac120f65e265062c324a3763f6cd5405"}],"tx":{"cond":{"v2":{"extra_signers":["<INVALID:GBJNRNVABFPR26UZT3UE2D5Y73U33YQ7O6UI2YEKD6T4JM5AZQL5HXJT:EMPTY_PAYLOAD>"],"ledger_bounds":null,"min_seq_age":"0","min_seq_ledger_gap":0,"min_seq_num":null,"time_bounds":null}},"ext":"v0","fee":100,"memo":"none","operations":[{"body":{"payment":{"amount":"10000000","asset":"native","destination":"GCCKY2DTW66FL46YTD52NMFQAXOHAIKZDISNRQMOE2DN77OM... (740 chars, see the source below)`

Actual: `{"rejected":"IllegalArgumentException: SignerKeyEd25519SignedPayloadXdr: has an empty signed payload, which has no strkey form"}`

### `mainnet/decode_json` on 1.18.0

the sdk refused a value the network accepts: IllegalArgumentException: SignerKeyEd25519SignedPayloadXdr: has an empty signed payload, which has no strkey form

```json
{
  "depth_limit": 1000,
  "len_limit": 268,
  "op": "decode_json",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAABAAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABASXHzkik4pUq4n6JvGWyXqROS8rgBBquEcwag1RSq87ZSnDfAkk3ateOFJ4u6q+WNrpGnp8geLS+mxTyKvkx6CQ=="
}
```

Expected: `{"any json, reference":{"tx":{"signatures":[{"hint":"2722451e","signature":"4971f3922938a54ab89fa26f196c97a91392f2b80106ab847306a0d514aaf3b6529c37c0924ddab5e385278bbaabe58dae91a7a7c81e2d2fa6c53c8abe4c7a09"}],"tx":{"cond":{"v2":{"extra_signers":["<INVALID:GBJNRNVABFPR26UZT3UE2D5Y73U33YQ7O6UI2YEKD6T4JM5AZQL5HXJT:EMPTY_PAYLOAD>"],"ledger_bounds":null,"min_seq_age":"0","min_seq_ledger_gap":0,"min_seq_num":null,"time_bounds":null}},"ext":"v0","fee":100,"memo":"none","operations":[{"body":{"payment":{"amount":"10000000","asset":"native","destination":"GCCKY2DTW66FL46YTD52NMFQAXOHAIKZDISNRQMOE2DN77OM... (740 chars, see the source below)`

Actual: `{"rejected":"IllegalArgumentException: SignerKeyEd25519SignedPayloadXdr: has an empty signed payload, which has no strkey form"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk kmp --version 1.18.0 --entry cap40.signed-payload-signer.p28-extra-signer-empty-payload
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-extra-signer-empty-payload/mainnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-extra-signer-empty-payload/mainnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; holds <INVALID:, which has no canonical SEP-51 form, so any rendering is accepted
- [`corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-extra-signer-empty-payload/testnet.expected.json`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-extra-signer-empty-payload/testnet.expected.json): SEP-51 rendering by stellar-xdr 30.0.0; holds <INVALID:, which has no canonical SEP-51 form, so any rendering is accepted

How the corpus entry was built and checked: an extra signer with an empty signed payload, which core refuses as malformed before the transaction reaches a ledger. submitted with ledger 72 as the last closed ledger

Related: https://github.com/stellar/wallet-backend/pull/729, https://github.com/stellar/wallet-backend/issues/716

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap40.signed-payload-signer.p28-extra-signer-empty-payload`
