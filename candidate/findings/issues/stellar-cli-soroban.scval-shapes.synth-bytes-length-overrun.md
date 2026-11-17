<!-- repo: stellar/stellar-cli -->
# stellar-cli 28.1.0, 30.0.1: the sdk crashed instead of answering: signal: 6 (SIGABRT): memory allocation of 4294967295 bytes failed note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace (soroban.scval-shapes.synth-bytes-length-overrun)

## What happens

On the input below, `stellar-cli` 28.1.0, 30.0.1 answers differently from the Stellar network in 2 checks.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

## Input

Corpus entry `soroban.scval-shapes.synth-bytes-length-overrun` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.1.0.

### `any/decode_encode` on 28.1.0, 30.0.1

the sdk crashed instead of answering: signal: 6 (SIGABRT): memory allocation of 4294967295 bytes failed
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

```json
{
  "depth_limit": 1000,
  "len_limit": 12,
  "op": "decode_encode",
  "type": "ScVal",
  "xdr": "AAAADf////8BAgME"
}
```

Expected: `"rejected"`

Actual: `{"error":"panic","message":"signal: 6 (SIGABRT): memory allocation of 4294967295 bytes failed\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace"}`

## Reproduce

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.1.0`:

```sh
shapekit conformance run --sdk stellar-cli --version 28.1.0 --entry soroban.scval-shapes.synth-bytes-length-overrun
```

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-bytes-length-overrun/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-bytes-length-overrun/any.xdr): decode = must-reject (validity NotEmittable)

How the corpus entry was built and checked: an ScVal bytes value claiming 0xffffffff bytes with four present. stellar-xdr 30.0.0 rejects it (ScVal::from_xdr) with LengthLimitExceeded under Limits { depth: 1000 (xdr.envelope.depth), len: 12 (the input length) }.

Related: https://github.com/stellar/rs-stellar-xdr/pull/581

## Sibling issues

None: of the SDKs tested, only this one diverges on this entry.

shapekit vector: `soroban.scval-shapes.synth-bytes-length-overrun`
