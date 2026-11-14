<!-- repo: stellar/js-stellar-sdk -->
# 16.x: six fixes from 17.x are missing (map key order and collisions, textual ids, W addresses, decode depth, StrKey checks)

Checked at 16.4.1 (`cefe570`, the current `lts-16` release; `release/v16` has no commits after it) and 17.4.0.

## What happens

16.3.1 and 16.4.1 took two backports from 17.x: issuer byte order in `Asset.compare` ([#1768](https://github.com/stellar/js-stellar-sdk/pull/1768)) and the protocol 30 XDR ([#1828](https://github.com/stellar/js-stellar-sdk/pull/1828)). Six other fixes are on 17.x only. In each case 16.4.1 gives an answer the network does not:

| | 16.4.1 | fixed on 17.x |
|---|---|---|
| ScVal map key order | `scvSortedMap` orders keys by their native form, so it puts a contract address key before an account address key; the host requires keys in ScVal order, which puts the account key first | [#1766](https://github.com/stellar/js-stellar-sdk/pull/1766), 17.2.1 |
| ScVal map key collisions | `scValToNative` keys a map by the string form of each key, so distinct keys (bool `true`, string `"true"`, symbol `"true"`) collapse into one entry with no error ([#1789](https://github.com/stellar/js-stellar-sdk/issues/1789)) | [#1778](https://github.com/stellar/js-stellar-sdk/pull/1778), 17.3.0 |
| textual ids | `offerId` and `MuxedAccount` ids given as text accept text that is not the canonical decimal spelling of an id, such as `"0x10"` and `"+16"`, and read both as 16 ([#1785](https://github.com/stellar/js-stellar-sdk/issues/1785)) | 17.3.0 |
| `W...` addresses | 16.4.1 carries the protocol 30 XDR that defines CAP-84 muxed contract addresses, but has no `StrKey` codec for them, so `Address.fromString` refuses a valid `W...` strkey | [#1811](https://github.com/stellar/js-stellar-sdk/pull/1811), 17.3.0 |
| decode depth | 16.4.1 bundles js-xdr 4.0.0 ([package.json, line 188](https://github.com/stellar/js-stellar-sdk/blob/cefe570745d481a6c7fe96b7d7d569bb0b818871/package.json#L188)), whose decoders stop at [`DEFAULT_MAX_DEPTH = 200`](https://github.com/stellar/js-xdr/blob/b4f5d09f2e2524047a61436b648ecfc5f21fad79/src/xdr-type.js#L173) nested types. The deepest `ScVal` it decodes is 66 `ScVec` levels, below the host's [`DEFAULT_HOST_DEPTH_LIMIT = 100`](https://github.com/stellar/rs-soroban-env/blob/0c99950e96128beba3319aa96b116646ce974e69/soroban-env-host/src/budget/limits.rs#L41) (soroban-env v30.0.0), so v16 cannot decode values a successful contract call returns, emits and stores, nor the meta and ledgers that hold them | js-xdr 5 ([js-xdr#155](https://github.com/stellar/js-xdr/pull/155), default limit 1500), 17.2.0 |
| StrKey checks | the typed `StrKey` decoders accept a payload of the wrong length when the checksum is correct, and `StrKey.decodeClaimableBalance` and `StrKey.isValidClaimableBalance` accept a claimable balance id whose type byte is not 0, which SEP-23 lists as invalid ([sep-0023.md, lines 357-359](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/ecosystem/sep-0023.md#L357-L359)) | length: [#1583](https://github.com/stellar/js-stellar-sdk/pull/1583); both 17.2.0 |

The release column is the first release among 17.2.0, 17.2.1, 17.3.0 and 17.4.0 on which the reproduction below gives the 17.4.0 answer.

## Reproduce

```js
const fs = require("node:fs");
const sdk = require("@stellar/stellar-sdk");
const { xdr, StrKey, Address, scValToNative, scvSortedMap, Operation, Asset, MuxedAccount, Account } = sdk;
const attempt = (f) => { try { return f(); } catch (e) { return `throws "${String(e.message).slice(0, 60)}"`; } };
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32BE(n); return b; };
const issuer = "GD7777777777777777777777777777777777777777777777777773DB";

// 1. map key order: the host sorts ScAddress keys by their XDR arm, account (G) before contract (C)
const c = xdr.ScVal.scvAddress(Address.fromString("CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4").toScAddress());
const g = xdr.ScVal.scvAddress(Address.fromString(issuer).toScAddress());
const sorted = scvSortedMap([
  new xdr.ScMapEntry({ key: c, val: xdr.ScVal.scvU32(0) }),
  new xdr.ScMapEntry({ key: g, val: xdr.ScVal.scvU32(1) }),
]);
// the same two entries with the account key first, as the host orders them
const hostOrder = "AAAAEQAAAAEAAAACAAAAEgAAAAAAAAAA//////////////////////////////////////////8AAAADAAAAAQAAABIAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADAAAAAA==";
console.log(`1 scvSortedMap puts the account key first: ${sorted.toXDR("base64") === hostOrder}`);

// 2. map key collisions: bool true, string "true" and symbol "true" are three distinct keys
const m = xdr.ScVal.scvMap([
  new xdr.ScMapEntry({ key: xdr.ScVal.scvBool(true), val: xdr.ScVal.scvU32(1) }),
  new xdr.ScMapEntry({ key: xdr.ScVal.scvString("true"), val: xdr.ScVal.scvU32(2) }),
  new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol("true"), val: xdr.ScVal.scvU32(3) }),
]);
console.log(`2 scValToNative of a 3-entry map: ${attempt(() => JSON.stringify(scValToNative(m)))}`);

// 3. textual ids: "0x10" and "+16" are not the decimal text of a uint64 / int64
const offer = (id) => attempt(() => {
  const raw = Operation.manageSellOffer({ selling: Asset.native(), buying: new Asset("USD", issuer), amount: "1", price: "1", offerId: id }).toXDR();
  return "offerID " + Buffer.from(raw).readBigInt64BE(raw.length - 8).toString();
});
const muxed = (id) => attempt(() => {
  const a = new MuxedAccount(new Account(issuer, "0"), id).accountId();
  return "id " + Buffer.from(StrKey.decodeMed25519PublicKey(a)).readBigUInt64BE(32).toString();
});
console.log(`3 offerId "0x10": ${offer("0x10")}; offerId "+16": ${offer("+16")}; MuxedAccount id "0x10": ${muxed("0x10")}`);

// 4. W address (CAP-84 muxed contract)
const w = "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAAWWC";
console.log(`4 StrKey.decodeMuxedContract is a ${typeof StrKey.decodeMuxedContract}; Address.fromString(W): ${attempt(() => Address.fromString(w).toString().slice(0, 8) + "...")}`);

// 5. decode depth: `levels` nested ScVecs around the u32 0
const nested = (levels) => {
  let v = Buffer.concat([u32(3), u32(0)]);
  for (let i = 0; i < levels; i++) v = Buffer.concat([u32(16), u32(1), u32(1), v]);
  return v;
};
let n = 0;
while (attempt(() => xdr.ScVal.fromXDR(nested(n + 1))) instanceof xdr.ScVal) n++;
console.log(`5 deepest nested ScVec that ScVal.fromXDR decodes: ${n}; ScVal of 100 levels: ${attempt(() => (xdr.ScVal.fromXDR(nested(99)), "decodes"))}`);
if (process.argv[2]) {
  // TransactionResultMetaV1 of a successful call that returns a 100-level ScVal
  const meta = fs.readFileSync(process.argv[2]);
  console.log(`  TransactionResultMetaV1.fromXDR: ${attempt(() => (xdr.TransactionResultMetaV1.fromXDR(meta), "decodes"))}`);
}

// 6. StrKey: an M address one byte short with a correct checksum, and the SEP-23 claimable balance with type byte 1
const shortM = "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAM36A";
console.log(`6 decodeMed25519PublicKey(short M): ${attempt(() => StrKey.decodeMed25519PublicKey(shortM).length + " bytes")}`);
const badB = "BAAT6DBUX6J22DMZOHIEZTEQ64CVCHEDRKWZONFEUL5Q26QD7R76RGXACA";
console.log(`  decodeClaimableBalance(type 1): ${attempt(() => "type byte " + StrKey.decodeClaimableBalance(badB)[0])}`);
```

The optional argument is the path of [mainnet.xdr](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.p30-vec-depth-max-meta/mainnet.xdr), the `TransactionResultMetaV1` of a successful call that returns a 100-level `ScVal`, written by a standalone stellar-core 30.0.0 node at protocol 30. Output with `@stellar/stellar-sdk` 16.4.1 and 17.4.0, Node.js v26.13.1:

```
== 16.4.1
1 scvSortedMap puts the account key first: false
2 scValToNative of a 3-entry map: {"true":3}
3 offerId "0x10": offerID 16; offerId "+16": offerID 16; MuxedAccount id "0x10": id 16
4 StrKey.decodeMuxedContract is a undefined; Address.fromString(W): throws "Unsupported address type: WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVW"
5 deepest nested ScVec that ScVal.fromXDR decodes: 66; ScVal of 100 levels: throws "XDR Read Error: exceeded max decoding depth"
  TransactionResultMetaV1.fromXDR: throws "XDR Read Error: exceeded max decoding depth"
6 decodeMed25519PublicKey(short M): 39 bytes
  decodeClaimableBalance(type 1): type byte 1
== 17.4.0
1 scvSortedMap puts the account key first: true
2 scValToNative of a 3-entry map: throws "scvMap has more than one key that converts to "true"; read t"
3 offerId "0x10": throws "offerId must be a string of decimal digits"; offerId "+16": throws "offerId must be a string of decimal digits"; MuxedAccount id "0x10": throws "id is not a valid uint64 string: 0x10"
4 StrKey.decodeMuxedContract is a function; Address.fromString(W): WA7QYNF7...
5 deepest nested ScVec that ScVal.fromXDR decodes: 374; ScVal of 100 levels: decodes
  TransactionResultMetaV1.fromXDR: decodes
6 decodeMed25519PublicKey(short M): throws "invalid encoded string length: expected 69, got 68"
  decodeClaimableBalance(type 1): throws "claimable balance: unknown discriminant 1, expected 0 (CLAIM"
```

16.3.0 and 16.3.1 give the 16.4.1 answers.

## Expected behaviour

On 16.x, the 17.4.0 answers above:

- `scvSortedMap` orders keys the way the host does, and `scValToNative` throws on keys that collide instead of dropping entries;
- textual ids accept only canonical decimal text in range;
- `StrKey` and `Address` read and write `W...` addresses;
- decoding covers every value the network accepts: at least the host's 100-level `ScVal`s, and envelopes and ledgers that hold them;
- the typed `StrKey` decoders refuse a payload of the wrong length, and the claimable balance decoders refuse a type byte other than 0.

If some of these do not go to 16.x, a line in the 16.x README or release notes naming them tells integrators which ones need 17.x.

## Impact

An application on 16.x that builds a map argument with `scvSortedMap` gets keys in an order other than the one the host requires; one that reads contract data with `scValToNative` can lose map entries without an error; one that takes an offer or muxed id as text can build an operation for another id; one that receives a `W...` address cannot parse it; and one that decodes recorded protocol 30 ledgers fails on values nested deeper than 66 levels, which the network accepts. The `StrKey` gaps accept input that 17.x and the network refuse.

## Also found with shapekit

From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0:

```sh
shapekit conformance run --sdk js --version 16.4.1
```

Entries that show each divergence on 16.3.0, 16.3.1 and 16.4.1 (the `W...` entry runs on 16.4.1 only):

- map key order: `soroban.scval-shapes.synth-map-keys-addresses`, `soroban.scval-shapes.synth-map-keys-mixed-types`;
- map key collisions: the other `soroban.scval-shapes.synth-map-*` entries, such as `soroban.scval-shapes.synth-map-bool-and-text`;
- textual ids: `cap03.offer-liabilities.synth-textual-offer-id-noncanonical`, `cap27.muxed-accounts.synth-textual-muxed-id-noncanonical`;
- `W...` addresses: `cap84.muxed-contract.synth-strkey-w-valid`;
- decode depth: `soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted`, `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`, and the recorded protocol 30 entries `soroban.scval-shapes.p30-vec-depth-max-meta` and `soroban.auth-trees.p30-declared-depth-245`;
- StrKey checks: `cap27.muxed-accounts.synth-strkey-m-invalid`, `cap23.claimable-balances.synth-strkey-b-invalid`.

## Sibling issues

go-stellar-sdk v0.9.0 has the same claimable balance type byte gap in `strkey.Decode`, `strkey.DecodeAny` and `strkey.IsValidClaimableBalance`; a separate issue for stellar/go-stellar-sdk describes it.
