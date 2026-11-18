<!-- repo: Beans-BV/dotnet-stellar-sdk -->
# stellar-dotnet-sdk 17.0.0: the sdk refused a value the network accepts: NotSupportedException: AllowTrustOperation is no longer supported (cap18.maintain-liabilities.golden-3f24be68a9962089-4)

## What happens

On the input below, `stellar-dotnet-sdk` 17.0.0 answers differently from the Stellar network in 1 check.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap18.maintain-liabilities.golden-3f24be68a9962089-4` (family `cap18.maintain-liabilities`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/tx_hash 0` on 17.0.0

the sdk refused a value the network accepts: NotSupportedException: AllowTrustOperation is no longer supported.

```json
{
  "depth_limit": 1000,
  "len_limit": 200,
  "network_passphrase": "(V) (;,,;) (V)",
  "op": "tx_hash",
  "xdr": "AAAAAgAAAACMTXz01XEANUvVc2WIDk4YQsp9Hd4bQ+GMXuPR6va8NgAAAGQAAAACAAAAAgAAAAAAAAAAAAAAAQAAAAAAAAAHAAAAAKzPvHNItJ7B2BpAit6MFbiug14LUhz00tODySgBTIW7AAAAAVVTRAAAAAABAAAAAAAAAAHq9rw2AAAAQJr0wBNq0MU7i416LjF/hT3x1ftN443zD+Eq+gYZKkGn62fDvhX7O6VqnTJifKFJ0hLPA1az1O+xbLylKuqRrgA="
}
```

Expected: `{"hash":"0b31cc9e2768310382ecbcc4840bdd4e3f35de9fe5cddc5ebdd1c2cd09f3176d"}`

Actual: `{"rejected":"NotSupportedException: AllowTrustOperation is no longer supported."}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk dotnet --version 17.0.0 --entry cap18.maintain-liabilities.golden-3f24be68a9962089-4
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap18.maintain-liabilities/cap18.maintain-liabilities.golden-3f24be68a9962089-4/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap18.maintain-liabilities/cap18.maintain-liabilities.golden-3f24be68a9962089-4/any.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: core test "deauth removes offers on frozen account", ledger 6 (record 5 of 12 in the upstream file). smallest golden ledger matching OperationBody:allow_trust, .body:allow_trust.asset:credit_alphanum4

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap18.maintain-liabilities.golden-3f24be68a9962089-4`
