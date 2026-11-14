<!-- repo: stellar/rs-stellar-xdr -->
# bool decodes any u32 and reads everything but 1 as false, where stellar-core reads non-zero as true

## What happens

[`impl ReadXdr for bool`](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L903-L912) reads a `u32` and returns `i == 1`. It never refuses a value, so a bool encoded as 2 (or 5, or `0xffffffff`) decodes as `false`, and writing it back produces `0`. Decoding and re-encoding is not the identity on these bytes. The link is at v30.0.0 (`3c2aa43`); `src/generated.rs` is unchanged on `main` at `ebec5fb`.

The crate already refuses the same word in the one other place a bool-like flag is on the wire: [`impl ReadXdr for Option<T>`](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L924-L939) returns `Error::Invalid` for a presence flag other than 0 and 1.

Three readers of the same bytes give three answers:

| decoder | bool word `00000002` |
|---|---|
| stellar-xdr 30.0.0 | accepted, read as `false`, re-encoded as `00000000` |
| stellar-core v30.0.0 (xdrpp, [`xdr_traits<bool>::from_uint`](https://github.com/xdrpp/xdrpp/blob/a29a1703699ad2b6cb4b28538d6ed4173eacdb60/xdrpp/types.h#L335-L349) is `u != 0`) | accepted, read as `true`, re-encoded as `00000001` |
| go-stellar-sdk v0.9.0 (go-xdr [`DecodeBool`](https://github.com/stellar/go-xdr/blob/dc590f17552a5063bc59411b7e6b9e928b852435/xdr3/decode.go#L291-L306)), js-stellar-sdk 17.4.0 | refused (`bool not 0 or 1`, `invalid bool discriminant 2`) |

## Minimal input

An `ScVal` of type `SCV_BOOL` with the value word 2, 8 bytes, base64 `AAAAAAAAAAI=`:

```
00000000 00000002
```

## Reproduce

```rust
// Cargo.toml: stellar-xdr = { version = "=30.0.0", features = ["std"] }
use stellar_xdr::{Limits, ReadXdr, ScVal, WriteXdr};

fn main() {
    // ScVal: discriminant SCV_BOOL (0), then the bool word
    for word in [0u8, 1, 2, 0xff] {
        let bytes = [0, 0, 0, 0, 0, 0, 0, word];
        let v = ScVal::from_xdr(bytes, Limits::none()).unwrap();
        println!("bool word {word:#04x}: {v:?}, re-encodes as {:02x?}", v.to_xdr(Limits::none()).unwrap());
    }
    // the presence flag of an optional is the same kind of word, and 2 is refused there
    println!("Option<u32> with flag 2: {:?}", Option::<u32>::from_xdr([0, 0, 0, 2, 0, 0, 0, 7], Limits::none()));
}
```

Output with stellar-xdr 30.0.0:

```
bool word 0x00: Bool(false), re-encodes as [00, 00, 00, 00, 00, 00, 00, 00]
bool word 0x01: Bool(true), re-encodes as [00, 00, 00, 00, 00, 00, 00, 01]
bool word 0x02: Bool(false), re-encodes as [00, 00, 00, 00, 00, 00, 00, 00]
bool word 0xff: Bool(false), re-encodes as [00, 00, 00, 00, 00, 00, 00, 00]
Option<u32> with flag 2: Err(Invalid)
```

## Expected behaviour

`Error::Invalid` for any bool word other than 0 and 1, as for an optional's presence flag. RFC 4506 section 4.4 declares bool as `enum { FALSE = 0, TRUE = 1 }`, and go-stellar-sdk and js-stellar-sdk refuse the word. Refusing is the only answer under which the same bytes cannot stand for two different values.

## Why it matters

stellar-core decodes a submitted envelope with xdrpp and keeps only the decoded value: the transaction hash and the signature check use core's re-encoding, with the word 1. An envelope whose contract argument carries the bool word 2 is therefore accepted by core and executed with `true`, while a tool that decodes the submitted bytes with stellar-xdr shows `false` for that argument, and a hash it computes from its decoded value matches neither the hash of the submitted bytes nor core's transaction hash. In a local run against stellar-core 30.0.0 at protocol 30, an `echo(true)` invocation rewritten to carry the word 2 and signed over the hash of the canonical re-encoding was applied with `txSUCCESS`, returned `true`, and the ledger stored the envelope with the word 1.

This is not an authorization bypass: the signature binds core's canonical form, whose meaning (`true`) is what the signer signed. Ledger meta is unaffected, since core writes it from its own decoded values. What breaks is agreement between decoders: tools built on different SDKs show different contract arguments for one submitted transaction, and a stellar-xdr round trip silently rewrites the bytes.

## Additional reproduction

The input above is [corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-bool-value-2/any.xdr](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-bool-value-2/any.xdr) in the shapekit corpus (https://github.com/ErenTopaal/shapekit, tag v0.1.0); the envelope form is [soroban.invoke-basics.synth-invoke-arg-bool-2](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.invoke-basics/soroban.invoke-basics.synth-invoke-arg-bool-2/any.xdr). Its differential fuzzer (stellar-xdr against go-stellar-sdk) found the same class through every fuzzed type that carries a bool, including `TransactionEnvelope` and `LedgerCloseMeta`; the minimised inputs are listed in [findings/fuzz/known-classes.txt](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/findings/fuzz/known-classes.txt).

## Sibling issues

A separate issue for stellar/stellar-core describes the other half: xdrpp reads any non-zero bool word as `true`, so the submitted bytes and core's re-encoding can differ.

shapekit vector: `soroban.scval-shapes.synth-bool-value-2`
