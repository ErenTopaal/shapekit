<!-- repo: stellar/js-stellar-sdk -->
# 16.x: XDR decoding stops at 200 nested types, so v16 cannot decode Soroban values the network produces

## What happens

16.4.1, the current `lts-16` release, bundles js-xdr 4.0.0 ([package.json, line 188](https://github.com/stellar/js-stellar-sdk/blob/cefe570745d481a6c7fe96b7d7d569bb0b818871/package.json#L188)), whose decoders stop at [`DEFAULT_MAX_DEPTH = 200`](https://github.com/stellar/js-xdr/blob/b4f5d09f2e2524047a61436b648ecfc5f21fad79/src/xdr-type.js#L173) nested types. That limit is below what the network produces:

- `xdr.ScVal.fromXDR` refuses an `ScVal` nested more than 66 `ScVec` levels deep. The Soroban host accepts values up to 100 levels deep ([`DEFAULT_HOST_DEPTH_LIMIT = 100`](https://github.com/stellar/rs-soroban-env/blob/0c99950e96128beba3319aa96b116646ce974e69/soroban-env-host/src/budget/limits.rs#L41), soroban-env v30.0.0), so a successful contract call can return, emit and store a value that v16 cannot decode.
- The `TransactionResultMetaV1` of such a call, written by a local stellar-core 30.0.0 node at protocol 30, fails with `XDR Read Error: exceeded max decoding depth`.
- stellar-core accepts envelopes whose authorization tree is up to 493 invocations deep or whose argument is nested up to 246 `ScVec` levels deep. v16 refuses both, and the `LedgerCloseMeta` of ledgers that contain them.

17.x decodes all of these: js-xdr 5 raised the default limit to 1500 ([js-xdr#155](https://github.com/stellar/js-xdr/pull/155)).

The 16.x `StrKey` decoders also accept two kinds of strkey that 17.x refuses:

- `StrKey.decodeMed25519PublicKey` (and the other typed decoders) accept a payload of the wrong length for the version byte, when the checksum is correct. Fixed on 17.x by [#1583](https://github.com/stellar/js-stellar-sdk/pull/1583).
- `StrKey.decodeClaimableBalance` and `StrKey.isValidClaimableBalance` accept a claimable balance id whose type byte is not 0, which SEP-23 lists as invalid ([sep-0023.md, lines 357-359](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/ecosystem/sep-0023.md#L357-L359)). 17.4.0 throws `unknown discriminant 1`.

## Reproduce

```js
const fs = require("node:fs");
const { xdr, StrKey } = require("@stellar/stellar-sdk");

const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32BE(n); return b; };
const attempt = (f) => { try { return f(); } catch (e) { return `throws "${e.message.slice(0, 60)}"`; } };

// an ScVal: `levels` nested ScVecs around the u32 0
const nested = (levels) => {
  let v = Buffer.concat([u32(3), u32(0)]);
  for (let i = 0; i < levels; i++) v = Buffer.concat([u32(16), u32(1), u32(1), v]);
  return v;
};
let n = 0;
while (attempt(() => xdr.ScVal.fromXDR(nested(n + 1))) instanceof xdr.ScVal) n++;
console.log(`deepest nested ScVec that ScVal.fromXDR decodes: ${n}`);
console.log(`ScVal nested 99 levels (100 ScVal levels): ${attempt(() => (xdr.ScVal.fromXDR(nested(99)), "decodes"))}`);
if (process.argv[2]) {
  // TransactionResultMetaV1 of a successful call that returns a 100-level ScVal
  const meta = fs.readFileSync(process.argv[2]);
  console.log(`TransactionResultMetaV1.fromXDR: ${attempt(() => (xdr.TransactionResultMetaV1.fromXDR(meta), "decodes"))}`);
}

// an M address whose payload is one byte short, with a correct checksum
const shortM = "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAM36A";
console.log(`decodeMed25519PublicKey(short M): ${attempt(() => StrKey.decodeMed25519PublicKey(shortM).length + " bytes")}`);
// SEP-23 invalid vector: claimable balance type byte 1
const badB = "BAAT6DBUX6J22DMZOHIEZTEQ64CVCHEDRKWZONFEUL5Q26QD7R76RGXACA";
console.log(`decodeClaimableBalance(type 1): ${attempt(() => "type byte " + StrKey.decodeClaimableBalance(badB)[0])}`);
```

The optional argument is the path of [mainnet.xdr](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/mainnet.xdr), the `TransactionResultMetaV1` described above. Output, Node.js v26.13.1:

```
== 16.4.1
deepest nested ScVec that ScVal.fromXDR decodes: 66
ScVal nested 99 levels (100 ScVal levels): throws "XDR Read Error: exceeded max decoding depth"
TransactionResultMetaV1.fromXDR: throws "XDR Read Error: exceeded max decoding depth"
decodeMed25519PublicKey(short M): 39 bytes
decodeClaimableBalance(type 1): type byte 1
== 17.4.0
deepest nested ScVec that ScVal.fromXDR decodes: 374
ScVal nested 99 levels (100 ScVal levels): decodes
TransactionResultMetaV1.fromXDR: decodes
decodeMed25519PublicKey(short M): throws "invalid encoded string length: expected 69, got 68"
decodeClaimableBalance(type 1): throws "claimable balance: unknown discriminant 1, expected 0 (CLAIM"
```

`release/v16` has no commits after v16.4.1 (`cefe570`).

## Ask

[RELEASE_POLICY.md](https://github.com/stellar/js-stellar-sdk/blob/5243062fdf7a874d33cbf3089e898eb0777337a8/RELEASE_POLICY.md#L33-L37) gives the Maintenance line "Non-breaking protocol support (regenerated XDR, parsing fixes)". The decoding limit is a parsing problem with data the network produces, so this asks for it on 16.x: for example decoding with js-xdr 4's `maxDepth` raised to a value that covers the host's 100-level `ScVal`s and the envelopes core accepts. The two `StrKey` checks are smaller parsing fixes of the same kind and are listed here so they can be decided together.

Other 17.x fixes that 16.4.1 lacks (`scvSortedMap` key order from #1766, map key collisions in `scValToNative` from #1778, canonical textual ids from #1785, `W...` addresses from #1811) are general bug fixes or new SDK features under that policy, and [#1898](https://github.com/stellar/js-stellar-sdk/issues/1898) already settled the map key collision case, so they are not requested here.

## Versions

The decoding limit diverges on 16.3.0, 16.3.1 and 16.4.1; the `StrKey` checks on the same three (shapekit conformance run).

## Additional reproduction

From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0:

```sh
shapekit conformance run --sdk js --version 16.4.1 --entry soroban.scval-shapes.p30-vec-depth-max-meta
```

Other entries that show the decoding limit on 16.4.1: `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`, `soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted`, `soroban.scval-shapes.synth-scmap-depth-197-deepest-accepted`, `soroban.auth-trees.p30-declared-depth-245`. For the `StrKey` checks: `cap27.muxed-accounts.synth-strkey-m-invalid` and `cap23.claimable-balances.synth-strkey-b-invalid`.

## Sibling issues

go-stellar-sdk v0.9.0 has the same claimable balance type byte gap in `strkey.Decode`, `strkey.DecodeAny` and `strkey.IsValidClaimableBalance`; a separate issue for stellar/go-stellar-sdk describes it.
