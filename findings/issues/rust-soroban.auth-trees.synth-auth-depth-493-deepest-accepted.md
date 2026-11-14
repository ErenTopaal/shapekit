<!-- repo: stellar/rs-stellar-xdr -->
# XDR-JSON reader stops at serde_json's recursion limit of 128, so values the crate writes as JSON cannot be read back

## What happens

Writing a value as XDR-JSON has no depth limit, but reading it back goes through serde_json with its default recursion limit of 128 nested JSON arrays and objects. Every nested `ScVal` costs two JSON levels (an object and an array), so an `ScVal` nested more than 63 `ScVec` levels deep is written to JSON and then refused with `recursion limit exceeded` when read. The binary decoder accepts these values at the depth stellar-core uses (1000).

Values deeper than that occur in valid ledgers:

- The Soroban host accepts `ScVal`s up to 100 levels deep ([`DEFAULT_HOST_DEPTH_LIMIT = 100`](https://github.com/stellar/rs-soroban-env/blob/0c99950e96128beba3319aa96b116646ce974e69/soroban-env-host/src/budget/limits.rs#L41), soroban-env v30.0.0), so a successful contract call can return, emit and store a value that cannot be read back from JSON.
- stellar-core accepts transactions whose authorization tree is up to 493 invocations deep or whose argument is nested up to 246 `ScVec` levels deep (its decode check at depth 1000). The host refuses to run them, and the failed transaction stays in the ledger with the full tree. Neither envelope can be read back from its JSON.

## Cause

`Type::from_json` calls `serde_json::from_reader` for every type variant ([src/generated/type_enum.rs, lines 13325-13329](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated/type_enum.rs#L13325-L13329)), and `Type::from_json_collecting_ignored_fields`, which the `encode` CLI command uses for JSON input, builds a default `serde_json::Deserializer` ([lines 16179-16187](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated/type_enum.rs#L16179-L16187)). Both keep serde_json's recursion limit, and the crate has no way to pass a different one. Links are at v30.0.0 (`3c2aa43`); `src/generated/type_enum.rs` is unchanged on `main` at `ebec5fb`.

## Reproduce

```rust
// Cargo.toml: stellar-xdr = { version = "=30.0.0", features = ["std", "serde", "serde_json", "type_enum"] }, serde_json = "1"
use stellar_xdr::{Limits, ReadXdr, ScVal, ScVec, Type, TypeVariant, WriteXdr};

// `levels` ScVec levels around the u32 0
fn nested(levels: usize) -> ScVal {
    let mut v = ScVal::U32(0);
    for _ in 0..levels {
        v = ScVal::Vec(Some(ScVec(vec![v].try_into().unwrap())));
    }
    v
}

fn main() {
    // 99 levels around a u32 is 100 ScVal levels, the deepest value the Soroban host accepts
    let v = nested(99);
    let xdr = v.to_xdr(Limits::none()).unwrap();
    assert_eq!(ScVal::from_xdr(&xdr, Limits { depth: 1000, len: xdr.len() }).unwrap(), v);
    let json = serde_json::to_string(&v).unwrap();
    println!("XDR: {} bytes, decodes at depth 1000; to JSON: {} bytes", xdr.len(), json.len());
    println!("Type::from_json: {:?}", Type::from_json(TypeVariant::ScVal, json.as_bytes()).err());
    println!("serde_json::from_str::<ScVal>: {:?}", serde_json::from_str::<ScVal>(&json).err());

    let deepest = (1..).take_while(|&n| serde_json::from_str::<ScVal>(&serde_json::to_string(&nested(n)).unwrap()).is_ok()).last();
    println!("deepest nesting that reads back from JSON: {deepest:?}");
}
```

Output with stellar-xdr 30.0.0 (serde_json 1.0.154):

```
XDR: 1196 bytes, decodes at depth 1000; to JSON: 999 bytes
Type::from_json: Some(Json(Error("recursion limit exceeded", line: 1, column: 512)))
serde_json::from_str::<ScVal>: Some(Error("recursion limit exceeded", line: 1, column: 512))
deepest nesting that reads back from JSON: Some(63)
```

Built the same way, a `TransactionEnvelope` with a 493-level authorization chain (27748 bytes of XDR) and one with a 246-level `ScVec` argument both decode under `Limits { depth: 1000, len }`, serialize to JSON, and fail in `Type::from_json(TypeVariant::TransactionEnvelope, ..)` with `recursion limit exceeded`.

## Expected behaviour

Anything the crate decodes under the depth limit core uses, and writes as XDR-JSON, reads back from that JSON. serde_json can lift its limit ([`Deserializer::disable_recursion_limit`](https://docs.rs/serde_json/1.0.154/serde_json/struct.Deserializer.html#method.disable_recursion_limit), `unbounded_depth` feature); the reader then needs a bound of its own against stack exhaustion, for example one derived from `Limits::depth` as the binary reader uses.

## Impact

Tools that move ledger data through XDR-JSON (the `stellar-xdr` CLI's `encode` command, and services that store or forward JSON and convert it back) fail on valid ledger contents: return values, events and contract data at the host's maximum depth, and failed transactions with deep authorization trees or arguments. The binary path is unaffected.

## Versions

Diverges on 28.0.0, 28.0.1 and 30.0.0 (shapekit conformance run).

## Additional reproduction

From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0:

```sh
shapekit conformance run --sdk rust --version 30.0.0 --entry soroban.auth-trees.synth-auth-depth-493-deepest-accepted
```

That entry is the 493-level envelope ([corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.xdr](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.xdr), with its JSON form next to it). [soroban.scval-shapes.p30-vec-depth-max-meta](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/mainnet.xdr) is the `TransactionResultMetaV1` that a local stellar-core 30.0.0 node at protocol 30 wrote for a successful call returning a 100-level value; it fails the same way.

## Sibling issues

None. go-stellar-sdk has no XDR-JSON reader, and js-stellar-sdk 17.4.0 reads the 100-level `ScVal` above back from its own JSON (`xdr.ScVal.fromJson`).

shapekit vector: `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`
