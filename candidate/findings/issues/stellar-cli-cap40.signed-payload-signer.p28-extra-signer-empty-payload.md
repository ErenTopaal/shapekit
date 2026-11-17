<!-- repo: stellar/stellar-cli -->
# stellar-cli 28.1.0, 30.0.1: the sdk refused a value the network accepts: ❌ error: error decoding JSON: xdr value invalid at line 1 column 320 (cap40.signed-payload-signer.p28-extra-signer-empty-payload)

## What happens

On the input below, `stellar-cli` 28.1.0, 30.0.1 answers differently from the Stellar network in 4 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap40.signed-payload-signer.p28-extra-signer-empty-payload` (family `cap40.signed-payload-signer`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `testnet/decode_encode` on 28.1.0

the sdk refused a value the network accepts: ❌ error: error decoding JSON: xdr value invalid at line 1 column 320

```json
{
  "depth_limit": 1000,
  "len_limit": 268,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAABAAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABAubjRLadWry055p2FmhZLMrRVYMmCBj+7DBwXGTholz5BUocWpW9I4ZlBXcDXWW4arBIPZeJlBiwySjdj9s1UBQ=="
}
```

Expected: `{"xdr":"AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAABAAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABAubjRLadWry055p2FmhZLMrRVYMmCBj+7DBwXGTholz5BUocWpW9I4ZlBXcDXWW4arBIPZeJlBiwySjdj9s1UBQ=="}`

Actual: `{"rejected":"❌ error: error decoding JSON: xdr value invalid at line 1 column 320"}`

### `mainnet/decode_encode` on 28.1.0

the sdk refused a value the network accepts: ❌ error: error decoding JSON: xdr value invalid at line 1 column 320

```json
{
  "depth_limit": 1000,
  "len_limit": 268,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAABAAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABASXHzkik4pUq4n6JvGWyXqROS8rgBBquEcwag1RSq87ZSnDfAkk3ateOFJ4u6q+WNrpGnp8geLS+mxTyKvkx6CQ=="
}
```

Expected: `{"xdr":"AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAABAAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABASXHzkik4pUq4n6JvGWyXqROS8rgBBquEcwag1RSq87ZSnDfAkk3ateOFJ4u6q+WNrpGnp8geLS+mxTyKvkx6CQ=="}`

Actual: `{"rejected":"❌ error: error decoding JSON: xdr value invalid at line 1 column 320"}`

### `testnet/decode_encode` on 30.0.1

the sdk refused a value the network accepts: ❌ error: error decoding JSON: xdr value invalid at line 1 column 337

```json
{
  "depth_limit": 1000,
  "len_limit": 268,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAABAAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABAubjRLadWry055p2FmhZLMrRVYMmCBj+7DBwXGTholz5BUocWpW9I4ZlBXcDXWW4arBIPZeJlBiwySjdj9s1UBQ=="
}
```

Expected: `{"xdr":"AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAABAAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABAubjRLadWry055p2FmhZLMrRVYMmCBj+7DBwXGTholz5BUocWpW9I4ZlBXcDXWW4arBIPZeJlBiwySjdj9s1UBQ=="}`

Actual: `{"rejected":"❌ error: error decoding JSON: xdr value invalid at line 1 column 337"}`

### `mainnet/decode_encode` on 30.0.1

the sdk refused a value the network accepts: ❌ error: error decoding JSON: xdr value invalid at line 1 column 337

```json
{
  "depth_limit": 1000,
  "len_limit": 268,
  "op": "decode_encode",
  "type": "TransactionEnvelope",
  "xdr": "AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAABAAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABASXHzkik4pUq4n6JvGWyXqROS8rgBBquEcwag1RSq87ZSnDfAkk3ateOFJ4u6q+WNrpGnp8geLS+mxTyKvkx6CQ=="
}
```

Expected: `{"xdr":"AAAAAgAAAACEukrMC4ar0JKMmb6K/edSkB76EXAhN9uMoY7GJyJFHgAAAGQAAABAAAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAA1LYtqAJXx16mZ7oTQ+4/um94h93qI1gih+nxLOgzBfTAAAAAAAAAAAAAAABAAAAAAAAAAEAAAAAhKxoc7e8VfPYmPumsLAF3HAhWRok2MGOJobf/cw1o18AAAAAAAAAAACYloAAAAAAAAAAASciRR4AAABASXHzkik4pUq4n6JvGWyXqROS8rgBBquEcwag1RSq87ZSnDfAkk3ateOFJ4u6q+WNrpGnp8geLS+mxTyKvkx6CQ=="}`

Actual: `{"rejected":"❌ error: error decoding JSON: xdr value invalid at line 1 column 337"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk stellar-cli --version 28.1.0 --entry cap40.signed-payload-signer.p28-extra-signer-empty-payload
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-extra-signer-empty-payload/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-extra-signer-empty-payload/mainnet.xdr): decode = must-accept, roundtrip = true
- [`corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-extra-signer-empty-payload/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/cap40.signed-payload-signer/cap40.signed-payload-signer.p28-extra-signer-empty-payload/testnet.xdr): decode = must-accept, roundtrip = true

How the corpus entry was built and checked: an extra signer with an empty signed payload, which core refuses as malformed before the transaction reaches a ledger. submitted with ledger 72 as the last closed ledger

Related: https://github.com/stellar/wallet-backend/pull/729, https://github.com/stellar/wallet-backend/issues/716

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap40.signed-payload-signer.p28-extra-signer-empty-payload`
