<!-- repo: stellar/stellar-cli -->
# stellar-cli 28.1.0, 30.0.1: stellar xdr decode reserves 4 GiB for a 12-byte input whose length prefix asks for it, and aborts under an address-space limit

## What happens

Stellar xdr decode reserves 4 GiB for a 12-byte input whose length prefix asks for it, and aborts under an address-space limit.

## Cause

Stellar xdr decode is stellar-xdr's cli (stellar-xdr 30.0.0 src/cli/decode.rs L78-L100), which reads every input under Limits::none(), and BytesM::read_xdr allocates the length the prefix declares before reading the bytes (src/generated.rs L2003-L2026, vec![0u8; len] at L2016; VecM<u8> L1536 and StringM L2490 the same). For the 12-byte ScVal 00 00 00 0d ff ff ff ff 01 02 03 04 the process reserves 4,294,967,295 bytes of address space and returns a clean error ("failed to fill whole buffer"); resident memory stays near 22 MiB. Only with RLIMIT_AS below about 5 GB, or under strict overcommit (not tested), does the allocation fail, and the process then aborts with SIGABRT ("memory allocation of 4294967295 bytes failed"). rs-stellar-xdr#623 reports the crate-level cause.

## Affected versions

`stellar-cli` 28.1.0, 30.0.1, every version shapekit ran.

## Entries

shapekit's finding CLI-ALLOC takes 1 corpus entry on this CLI: `soroban.scval-shapes.synth-bytes-length-overrun`. The checks below are those of `soroban.scval-shapes.synth-bytes-length-overrun`.

## Checks on `soroban.scval-shapes.synth-bytes-length-overrun`

On this entry, `stellar-cli` 28.1.0, 30.0.1 answers differently from the Stellar network in 1 check on 2 versions.

A check is one call into the SDK with a known right answer. Each check below shows the call as a JSON request of the [shapekit testee protocol](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/docs/testee-protocol.md) (`op` names the SDK operation), the expected answer and the answer the SDK gave.

Corpus entry `soroban.scval-shapes.synth-bytes-length-overrun` (family `soroban.scval-shapes`) of the [shapekit](https://github.com/ErenTopaal/shapekit) corpus 0.2.0.

### `any/decode_encode` on 28.1.0, 30.0.1

The CLI crashed instead of answering: signal: 6 (SIGABRT): memory allocation of 4294967295 bytes failed
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

From a checkout of https://github.com/ErenTopaal/shapekit at tag `v0.2.0`, with the Rust toolchain of its `rust-toolchain.toml` and the toolchain `stellar-cli` builds with (`shapekit doctor sdk --sdk stellar-cli` lists what is missing):

```sh
cargo build --release --locked -p shapekit-cli
target/release/shapekit conformance run --sdk stellar-cli --version 30.0.1 --entry soroban.scval-shapes.synth-bytes-length-overrun
```

The run builds the CLI's testee at that version as `conformance/versions.toml` describes, then runs the entry's checks.

## Where the expected answers come from

The expected answers are recorded in:

- [`corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-bytes-length-overrun/any.xdr`](https://github.com/ErenTopaal/shapekit/blob/v0.2.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-bytes-length-overrun/any.xdr): decode = must-reject (validity NotEmittable)

How the corpus entry was built and checked: An ScVal bytes value claiming 0xffffffff bytes with four present. stellar-xdr 30.0.0 rejects it (ScVal::from_xdr) with LengthLimitExceeded under Limits { depth: 1000 (xdr.envelope.depth), len: 12 (the input length) }.

Related: https://github.com/stellar/rs-stellar-xdr/pull/581

## Sibling issues

None: no other package of shapekit's conformance matrices diverges on this entry, in any matrix or in its list of known failures.

shapekit vector: `soroban.scval-shapes.synth-bytes-length-overrun`
