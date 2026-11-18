<!-- repo: Soneso/stellar_flutter_sdk -->
# stellar_flutter_sdk 3.12.0: the sdk refused a value the network accepts: _Exception: Exception: Unknown operation body OperationType.9 (cap26.inflation.p11-inflation-payout)

## What happens

On the input below, `stellar_flutter_sdk` 3.12.0 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `cap26.inflation.p11-inflation-payout` (family `cap26.inflation`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `testnet/tx_hash 0` on 3.12.0

the sdk refused a value the network accepts: _Exception: Exception: Unknown operation body OperationType.9

```json
{
  "depth_limit": 1000,
  "len_limit": 148,
  "network_passphrase": "Test SDF Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAEdtbmJXSbc1sZf26nS8piOAYsvwzbWym1i/Qw0Zcx4vAAAAZAAAAEAAAAABAAAAAAAAAAAAAAABAAAAAAAAAAkAAAAAAAAAARlzHi8AAABAMbg/5pTvIC1XmHzbE9NNxmreSmgPw4953SFTDbGoDppGa7seuEG8kTOdk1deqayIJ4bE8O7Eu8IlbquAVU+LAQ=="
}
```

Expected: `{"hash":"ab4f721233114b85a2119425c64c87907e7a4b988f7ee0b79e4bc9d81002f2fb"}`

Actual: `{"rejected":"_Exception: Exception: Unknown operation body OperationType.9"}`

### `mainnet/tx_hash 0` on 3.12.0

the sdk refused a value the network accepts: _Exception: Exception: Unknown operation body OperationType.9

```json
{
  "depth_limit": 1000,
  "len_limit": 148,
  "network_passphrase": "Public Global Stellar Network ; September 2015",
  "op": "tx_hash",
  "xdr": "AAAAAEdtbmJXSbc1sZf26nS8piOAYsvwzbWym1i/Qw0Zcx4vAAAAZAAAAEAAAAABAAAAAAAAAAAAAAABAAAAAAAAAAkAAAAAAAAAARlzHi8AAABAm738QzgiV0QBrrlJ29nrCWBZNvPll2293iOb2OZ6iy3f3JSsqc1OoC1HuDgo4FF/2t5J6gz4wvZ0fM+ASwFbAg=="
}
```

Expected: `{"hash":"53d2dc73ef881cf38ad42d3a0636e4f2299053c015f4cef678c9b6a224ed629e"}`

Actual: `{"rejected":"_Exception: Exception: Unknown operation body OperationType.9"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`:

```sh
shapekit conformance run --sdk dart --version 3.12.0 --entry cap26.inflation.p11-inflation-payout
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/cap26.inflation/cap26.inflation.p11-inflation-payout/mainnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap26.inflation/cap26.inflation.p11-inflation-payout/mainnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair
- [`corpus/entries/cap26.inflation/cap26.inflation.p11-inflation-payout/testnet.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/cap26.inflation/cap26.inflation.p11-inflation-payout/testnet.xdr), envelope 0 of the transaction set: transactionHash of the envelope's result pair

How the corpus entry was built and checked: an inflation operation at protocol 11 paying the one account the root voted for; the payout is a balance change on an account the envelope never names

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `cap26.inflation.p11-inflation-payout`
