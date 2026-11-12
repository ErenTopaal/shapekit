<!-- repo: stellar/js-stellar-sdk -->
# 16.x still diverges from the network where 17.x was fixed (map key order and collisions, textual ids, strkey lengths, W addresses, decode depth)

## What happens

`@stellar/stellar-sdk` 16.3.1 and 16.4.1 took two backports from 17.x (issuer byte order in
`Asset.compare`, #1768, and the protocol 30 XDR, #1828). Six other divergences fixed on 17.x
are still present on the newest 16.x release, 16.4.1, and each one makes the sdk answer
differently from stellar-core:

| | fixed on 17.x by | still on 16.4.1 |
|---|---|---|
| `scvSortedMap` orders keys by their native form, not by the ScVal ordering core requires | #1766 (17.2.1) | `soroban.scval-shapes.synth-map-keys-addresses`, `synth-map-keys-mixed-types` |
| `scValToNative` merges map entries whose keys collide as object properties | #1789 / #1778 (17.3.0) | `soroban.scval-shapes.synth-map-*` (8 entries) |
| textual muxed and offer ids accept non-canonical text (leading zeros, sign, past 2^64) | #1785 (17.3.0) | `cap27.muxed-accounts.synth-textual-muxed-id-noncanonical`, `cap03.offer-liabilities.synth-textual-offer-id-*` |
| `StrKey` decoders accept payloads of the wrong length for the version byte | 17.x | `*.synth-strkey-{b,g,m,s,l,p,t,x,c}-invalid` |
| no `StrKey` codec for CAP-84 muxed contract (`W...`) addresses, although 16.4.1 carries the protocol 30 XDR that defines them | #1811 | `cap84.muxed-contract.synth-strkey-w-valid` |
| js-xdr 4 stops at 200 nested types, below depths the network accepts | js-xdr#155 (js-xdr 5, 17.x) | `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`, `soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted`, `synth-scmap-depth-197-deepest-accepted`, `synth-event-vec-depth-101`, and recorded protocol 30 ledgers (`soroban.auth-trees.p30-declared-depth-245/246/492`, `soroban.scval-shapes.p30-*-depth-*`) |

Examples on 16.4.1, each one request of the shapekit testee protocol:

- `cap27.muxed-accounts.synth-strkey-m-invalid` case 2, `strkey_decode`: the network refuses
  the strkey; 16.4.1 decodes it to `{"kind": "muxed_account", "payload": "Pww0v5Ot...AAAAAAAA"}`
  (a payload one byte short).
- `cap84.muxed-contract.synth-strkey-w-valid` case 0, `strkey_decode`: expected
  `{"kind": "muxed_contract", ...}`; 16.4.1 has no `StrKey.decodeMuxedContract`.
- `soroban.scval-shapes.synth-map-bool-and-text` case 0, `to_native`: a map of three entries
  comes back with one; the other two are lost without an error.
- `soroban.scval-shapes.synth-scvec-depth-246-deepest-accepted`, `decode_encode`: a
  transaction core accepts is refused with `XDR Read Error: exceeded max decoding depth`.

## Reproduce

```sh
shapekit conformance run --sdk js --version 16.4.1
```

The divergence matrix lists every cell; `conformance/known-failures/js.txt` names the
entries per version.

## Ask

Backport the six fixes to the 16.x line, or document that 16.x does not get correctness
fixes, so integrators know to move to 17.x.

## Sibling issues

- go-stellar-sdk: `cap23.claimable-balances.synth-strkey-b-invalid` also diverges there (B
  type byte not checked).
- rs-stellar-xdr: the deep entries above also fail its XDR-JSON reader (serde_json's
  recursion limit).
