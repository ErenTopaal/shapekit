<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 28.0.0 to 30.0.0: bool decodes any u32 and reads everything but 1 as false, where core reads non-zero as true

## What happens

`impl ReadXdr for bool` (src/generated.rs in 28.0.0, 28.0.1 and 30.0.0) reads a `u32` and
returns `i == 1`. It never refuses a value, so a bool encoded as 2 (or 5, or `0xffffffff`)
decodes as `false`, and writing the value back produces `0`. Decoding and re-encoding is
therefore not the identity on these bytes.

Three readers of the same bytes give three answers:

| decoder | bool word `00000002` |
|---|---|
| stellar-xdr 30.0.0 | accepted, read as `false`, re-encoded as `00000000` |
| xdrpp as linked by stellar-core v30.0.0 (submodule a29a170) | accepted, read as `true` (`xdr_traits<bool>::from_uint` is `u != 0`; the enum check then sees `true`), re-encoded as `00000001` |
| go-stellar-sdk v0.8.0 | refused: `xdr:DecodeBool: bool not 0 or 1` |

RFC 4506 section 4.4 declares bool as `enum { FALSE = 0, TRUE = 1 }`, and stellar-xdr already
refuses the same word in the one other place it is a bool on the wire: the presence flag of an
optional (`impl ReadXdr for Option<T>` returns `Error::Invalid` for anything but 0 and 1).

## Why it matters

core decodes a submitted envelope with xdrpp and keeps only its decoded value: the
transaction hash (`TransactionFrame::getContentsHash`, src/transactions/TransactionFrame.cpp)
and, from protocol 28, the input to stellar-xdr's own parse check (`validateXDRForProtocol`,
src/transactions/TransactionUtils.cpp) are both xdrpp's re-encoding, with the word 1. So an
envelope whose contract argument is the bool word 2 is accepted by core and executed with
`true`. A tool that decodes the submitted bytes with stellar-xdr shows `false` for that
argument, and a hash it computes from its decoded value covers bytes with `0`, which matches
neither the hash of the submitted bytes nor core's transaction hash.

Signatures are checked against core's hash, which covers the canonical form (word 1), so this
is not an authorization bypass: the bytes with 2 carry the meaning the signer signed, `true`,
and only a decoder that reads them as `false` is misled. In a local run against stellar-core
30.0.0 at protocol 30, an `echo(true)` invocation rewritten to carry the word 2 and signed over
the hash of the canonical re-encoding was applied with `txSUCCESS`, returned `true`, and the
ledger stored the envelope with the word 1; the same bytes signed over the hash of the
submitted bytes were refused with `txBAD_AUTH`. Ledger meta is unaffected, since core writes it
from its own decoded values.

Two decoders of the same bytes also disagree with each other (stellar-xdr reads `false`,
go-stellar-sdk refuses), so tools built on different SDKs can show different contract
arguments for one transaction, and a stellar-xdr round trip silently rewrites the bytes.

## Minimal vector

An `ScVal` of type `SCV_BOOL` with the value word 2, 8 bytes, base64 `AAAAAAAAAAI=`:

```
00000000 00000002
```

It is corpus entry `soroban.scval-shapes.synth-bool-value-2` of the shapekit corpus 0.1.0
(implementation-defined, not-emittable). The differential fuzzer found the same class through
every fuzzed type that carries a bool, including `TransactionEnvelope`; the minimised
reproducers are under `findings/fuzz/` (one per type), for example
`findings/fuzz/fuzz.sc-val-rust-accepts-go-rejects-d6bdba39/any.xdr`.

## Reproduce

```sh
shapekit fuzz --replay findings/fuzz --no-build
```

or, in Rust:

```rust
use stellar_xdr::{Limits, ReadXdr, ScVal, WriteXdr};
let v = ScVal::from_xdr([0, 0, 0, 0, 0, 0, 0, 2], Limits::none()).unwrap();
assert_eq!(v, ScVal::Bool(false));
assert_eq!(v.to_xdr(Limits::none()).unwrap(), [0, 0, 0, 0, 0, 0, 0, 0]);
```

## Expected value and its source

`Error::Invalid` for any bool word other than 0 and 1, as for an optional's presence flag:
RFC 4506 section 4.4, and go-stellar-sdk's decoder. Refusing is also the only answer under
which the same bytes cannot stand for two different values.

## Sibling issues

None filed. xdrpp's lenient read is the other half of the disagreement (drafted locally for
stellar/stellar-core); core never emits a non-canonical bool, so the shapekit corpus leaves the
entries implementation-defined. The envelope form is corpus entry
`soroban.invoke-basics.synth-invoke-arg-bool-2`.

shapekit vector: `soroban.scval-shapes.synth-bool-value-2`
