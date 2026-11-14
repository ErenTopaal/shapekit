<!-- repo: stellar/js-stellar-sdk -->
# xdr: fromXdr decodes envelopes nested deeper than stellar-core accepts, and no maxDepth value matches core's limit

## What happens

stellar-core refuses a transaction envelope whose nesting exceeds its decode depth limit. `xdr.TransactionEnvelope.fromXdr` decodes and re-encodes envelopes deeper than that: an authorization tree 494 invocations deep, or a call argument nested 247 `ScVec` levels deep, both of which core refuses with `txMALFORMED`.

| envelope (bytes built by the script below) | stellar-core (stellar-xdr 30.0.0 at depth 1000) | `fromXdr`, 17.4.0 |
|---|---|---|
| deepest authorization chain accepted | 493 levels | 496 levels |
| deepest nested `ScVec` call argument accepted | 246 levels | 372 levels |

Passing a limit does not close the gap. `fromXdr` takes no options, and going through the schema directly (`xdr.TransactionEnvelope.schema.decode(bytes, { maxDepth })`) measures depth in js-xdr's unit, one per schema entered. In that unit core's boundary is at a different value for each shape: a 493-level chain needs `maxDepth` 1489 and a 494-level chain 1492, while a 246-level `ScVec` argument needs 995 and a 247-level one 999. Any value that accepts the 493-level chain (1489 or more) also accepts the 247-level `ScVec` argument that core refuses.

## Cause

- From protocol 28, core checks every submitted envelope by parsing it with stellar-xdr (Rust) under `Limits { depth: 1000, len }`: [`validateXDRForProtocol`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/transactions/TransactionUtils.cpp#L1350-L1379) and [`can_parse_transaction`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/rust/src/soroban_proto_any.rs#L781-L790) (stellar-core v30.0.0). stellar-xdr spends one unit on every value it reads ([`with_limited_depth`](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L440-L452)): 2 per invocation level, 4 per `ScVec` level.
- [`XdrValue.fromXdr`](https://github.com/stellar/js-stellar-sdk/blob/1ef3f77b8f493a4905d1cfd97462f9cc55efcc70/src/xdr/values/xdr-value.ts#L115-L132) calls `this.schema.decode(bytes)` with no options, so js-xdr's [`DEFAULT_MAX_DEPTH = 1500`](https://github.com/stellar/js-xdr/blob/8b0d6d57025a96949d2b5ab6e45b0b66751d3bd2/src/core/reader.ts#L4) applies (js-xdr 5.0.0, raised from 200 in [js-xdr#155](https://github.com/stellar/js-xdr/pull/155)). js-xdr counts each struct, union, option and array it enters ([`Reader.enter`](https://github.com/stellar/js-xdr/blob/8b0d6d57025a96949d2b5ab6e45b0b66751d3bd2/src/core/reader.ts#L106-L114)), about 3 per invocation level and 4 per `ScVec` level.

## Reproduce

The script builds the envelopes byte by byte, so the input does not depend on the SDK's encoder:

```js
import { xdr } from "@stellar/stellar-sdk";

const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32BE(n); return b; };
// InvokeContractArgs: contract 0x32..32, function "deep", the given arguments
const call = (args) => Buffer.concat([u32(1), Buffer.alloc(32, 0x32), u32(4), Buffer.from("deep"), u32(args.length), ...args]);

// a v1 TransactionEnvelope with one InvokeHostFunction operation. chain > 0 adds one
// source-account auth entry whose invocation tree is a single chain that many levels deep;
// vecDepth > 0 passes one argument, an ScVec nested that many levels around the u32 0.
function envelope(chain, vecDepth) {
  const args = [];
  if (vecDepth > 0) {
    let v = Buffer.concat([u32(3), u32(0)]); // SCV_U32 0
    for (let i = 0; i < vecDepth; i++) v = Buffer.concat([u32(16), u32(1), u32(1), v]); // SCV_VEC, present, one element
    args.push(v);
  }
  let auth = u32(0);
  if (chain > 0) {
    const fn = Buffer.concat([u32(0), call([])]); // SOROBAN_AUTHORIZED_FUNCTION_TYPE_CONTRACT_FN
    const tree = [];
    for (let i = 1; i < chain; i++) tree.push(fn, u32(1));
    auth = Buffer.concat([u32(1), u32(0), ...tree, fn, u32(0)]); // one entry, SOROBAN_CREDENTIALS_SOURCE_ACCOUNT
  }
  return Buffer.concat([
    u32(2), u32(0), Buffer.alloc(32), // ENVELOPE_TYPE_TX, ed25519 source account
    u32(100), Buffer.alloc(4), u32(1), // fee 100, sequence number 1
    u32(0), u32(0), // no preconditions, no memo
    u32(1), u32(0), u32(24), // one operation, no source account, INVOKE_HOST_FUNCTION
    u32(0), call(args), auth, // HOST_FUNCTION_TYPE_INVOKE_CONTRACT
    u32(0), u32(0), // transaction ext v0, no signatures
  ]);
}

const decodes = (b) => { try { xdr.TransactionEnvelope.fromXdr(b); return true; } catch { return false; } };
const deepest = (f) => { let n = 1; while (decodes(f(n + 1))) n++; return n; };
for (const [name, b] of [["auth chain 494", envelope(494, 0)], ["ScVec argument 247", envelope(0, 247)]]) {
  const same = decodes(b) && Buffer.from(xdr.TransactionEnvelope.fromXdr(b).toXdr()).equals(b);
  console.log(`${name}: fromXdr accepts ${decodes(b)}, toXdr returns the same bytes ${same}`);
}
console.log(`fromXdr: deepest auth chain ${deepest((n) => envelope(n, 0))}, deepest ScVec argument ${deepest((n) => envelope(0, n))}`);

// the smallest js-xdr maxDepth that accepts each envelope
const schema = xdr.TransactionEnvelope.schema;
const fits = (b, maxDepth) => { try { schema.decode(b, { maxDepth }); return true; } catch { return false; } };
const need = (b) => { let d = 1; while (!fits(b, d)) d++; return d; };
console.log(`maxDepth needed: auth 493 ${need(envelope(493, 0))}, auth 494 ${need(envelope(494, 0))}, ScVec 246 ${need(envelope(0, 246))}, ScVec 247 ${need(envelope(0, 247))}`);
```

Output with `@stellar/stellar-sdk` 17.4.0 (js-xdr 5.0.0), Node.js v26.13.1:

```
auth chain 494: fromXdr accepts true, toXdr returns the same bytes true
ScVec argument 247: fromXdr accepts true, toXdr returns the same bytes true
fromXdr: deepest auth chain 496, deepest ScVec argument 372
maxDepth needed: auth 493 1489, auth 494 1492, ScVec 246 995, ScVec 247 999
```

The same bytes through stellar-xdr 30.0.0, `TransactionEnvelope::from_xdr(bytes, Limits { depth: 1000, len: bytes.len() })` (core's check), fail with `DepthLimitExceeded` for both envelopes; the deepest that pass are a 493-level chain and a 246-level `ScVec` argument. `src/xdr/values/xdr-value.ts` is unchanged on `main` at `5243062`.

## Expected behaviour

A caller can find out whether core's depth check accepts an envelope: an envelope core refuses as too deep is refused, one core accepts is accepted. Since js-xdr's unit differs from core's per shape, one way is a check that counts depth the way stellar-xdr does (one unit per value read, including each optional and each variable-length array) and applies core's limit of 1000, offered as an option or a separate validation call next to `fromXdr`.

This is the opposite direction from [#1901](https://github.com/stellar/js-stellar-sdk/issues/1901): there a ledger core closed does not decode inside a `LedgerCloseMetaBatch`, and open PR [#1902](https://github.com/stellar/js-stellar-sdk/pull/1902) raises the SDK-wide limit to 1550 so that it does. A higher generic limit is right for reading ledger data; it widens the gap described here, which is why a separate check would be needed for envelopes.

## Impact

Low. No ledger contains such an envelope, since core refuses it, so readers of ledger data are not affected. Code that builds, checks or forwards transactions with the SDK cannot detect locally that core will refuse the envelope: it decodes, re-encodes and can be signed, and the refusal only comes back from core as `txMALFORMED` at submission.

## Versions

Diverges on 17.2.0, 17.2.1, 17.3.0 and 17.4.0 (shapekit conformance run).

## Additional reproduction

From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0:

```sh
shapekit conformance run --sdk js --version 17.4.0 --entry soroban.auth-trees.synth-auth-depth-494-over-cap
```

The entry's envelope is [corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-494-over-cap/any.xdr](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-494-over-cap/any.xdr) (a 494-level chain with a Soroban transaction extension and one signature). Running the same command for `soroban.scval-shapes.synth-scvec-depth-247-over-cap`, `soroban.scval-shapes.synth-scmap-depth-198-over-cap` and `soroban.auth-trees.p30-declared-depth-493` shows the same divergence for nested `ScVec`s, nested `ScMap`s and an address-credential authorization tree. The last one was submitted to a local stellar-core 30.0.0 node at protocol 30, which answered `txMALFORMED`.

## Sibling issues

go-stellar-sdk has the same gap with a different unit: its `DecodeOptions.MaxDepth` spends one unit per named type, so `MaxDepth: 1000` accepts authorization chains up to 988 levels and the default (1500) up to 1488. A separate issue for stellar/go-stellar-sdk describes it. rs-stellar-xdr is the reference here: it is the parser core runs.

shapekit vector: `soroban.auth-trees.synth-auth-depth-494-over-cap`
