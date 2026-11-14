<!-- repo: stellar/stellar-core -->
# An XDR bool word other than 0 or 1 is accepted as true and re-encoded as 1, so the submitted bytes and the transaction hash differ

## What happens

core decodes XDR with xdrpp, whose bool reader takes any `u32` and converts it with
`from_uint(u) { return u != 0; }`, so the words 2 to `0xffffffff` all decode as `true`. The
enum check that follows (`validate_enum`) sees the already converted `true`, not the word, so
nothing is refused. Writing the value back produces `00000001`.

The same code is on `master` at `f2b91947`, whose `lib/xdrpp` now points at xdrpp `04eebeae`: there [`from_uint` is still `u != 0`](https://github.com/xdrpp/xdrpp/blob/04eebeae49a1eeedb21b964912565ad2727a8955/xdrpp/types.h#L305-L306), with the comment "could throw for u > 1, but would be slower", and the [reader](https://github.com/xdrpp/xdrpp/blob/04eebeae49a1eeedb21b964912565ad2727a8955/xdrpp/marshal.h#L181-L183) validates the converted value.

| source (core v30.0.0, commit 0e49751a; xdrpp submodule a29a1703) | what it does |
|---|---|
| [`lib/xdrpp/xdrpp/types.h:335-349`](https://github.com/xdrpp/xdrpp/blob/a29a1703699ad2b6cb4b28538d6ed4173eacdb60/xdrpp/types.h#L335-L349), line 341 | `xdr_traits<bool>::from_uint` is `u != 0` |
| [`lib/xdrpp/xdrpp/marshal.h:176-178`](https://github.com/xdrpp/xdrpp/blob/a29a1703699ad2b6cb4b28538d6ed4173eacdb60/xdrpp/marshal.h#L176-L178) | reads the word with `from_uint`, then calls `validate_enum<T>::validate(t)` on the converted value |
| [`src/main/CommandHandler.cpp:1069-1072`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/main/CommandHandler.cpp#L1069-L1072) | `CommandHandler::tx` decodes the submitted blob into a `TransactionEnvelope`; the raw bytes are not used again |
| [`src/transactions/TransactionFrame.cpp:154-179`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/transactions/TransactionFrame.cpp#L154-L179) (line 172-173) | `getContentsHash` is `sha256(xdr_to_opaque(networkID, ENVELOPE_TYPE_TX, mEnvelope.v1().tx))`, a re-encoding of the decoded value |
| [`src/transactions/TransactionFrame.cpp:143-151`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/transactions/TransactionFrame.cpp#L143-L151) | `getFullHash` is `xdrSha256(mEnvelope)`, also a re-encoding |
| [`src/transactions/TransactionFrame.cpp:1859-1861`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/transactions/TransactionFrame.cpp#L1859-L1861) | signatures are checked against `getContentsHash()` |
| [`src/transactions/TransactionFrame.cpp:1923`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/transactions/TransactionFrame.cpp#L1923), [`src/transactions/TransactionUtils.cpp:1350-1379`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/transactions/TransactionUtils.cpp#L1350-L1379) (line 1376-1377) | from protocol 28 the rust `can_parse_transaction` check runs on `xdr_to_opaque(envelope)`, the re-encoding, so it never sees the submitted word |

Other decoders of the same bytes disagree:

| decoder | bool word `00000002` |
|---|---|
| stellar-core v30.0.0 (xdrpp) | accepted, read as `true`, re-encoded as `00000001` |
| stellar-xdr (Rust) 30.0.0 | accepted, read as `false` ([`impl ReadXdr for bool`](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L903-L912) is `i == 1`), re-encoded as `00000000` |
| go-stellar-sdk v0.8.0 and v0.9.0 (go-xdr [`xdr3/decode.go:291-306`](https://github.com/stellar/go-xdr/blob/dc590f17552a5063bc59411b7e6b9e928b852435/xdr3/decode.go#L291-L306)), js-stellar-sdk 17.4.0 | refused: `xdr:DecodeBool: bool not 0 or 1` |

RFC 4506 section 4.4 declares bool as `enum { FALSE = 0, TRUE = 1 }`.

## Local experiment

A fresh standalone stellar-core 30.0.0 node (image `stellar/stellar-core:30.0.0-3642.0e49751aa.noble`)
at protocol 30, with testnet Soroban settings, an `echo(value)` contract uploaded and deployed,
and `echo(true)` invoked normally first (it returned `true`). One `echo(true)` invocation was
then simulated and assembled, the bool word of its argument (tx offset 124) was set to
`00000002`, and a v1 envelope was built around those bytes with one signature from the source
account, in two variants that differ only in what was signed:

| | signed over | core `tx` answer |
|---|---|---|
| A | sha256(network id, `ENVELOPE_TYPE_TX`, the submitted tx bytes, word 2) = `b04d967f93b98af1d0dbc7754d470200c418998f8500c7f945066a57fad46032` | `ERROR`, `txBAD_AUTH` |
| B | sha256(network id, `ENVELOPE_TYPE_TX`, the canonical tx bytes, word 1) = `29ec95b4c2da676944d7178e428acf0c854d545132dfac73b085e967f247d8e6` | `PENDING` |

After B the next ledger closed with:

- `transactionHash` = `29ec95b4...d8e6`, the hash of the canonical re-encoding, not of the
  submitted bytes (`b04d967f...6032`);
- result `txSUCCESS`, `InvokeHostFunction` success, return value `true`;
- in the raw meta stream (searched as bytes, not decoded) the tx body with word 2 occurs 0
  times and the tx body with word 1 once: the ledger stores the canonical form.

Submission over the overlay instead of the `tx` endpoint was not tested.

## Minimal vector

A v1 `TransactionEnvelope`, 248 bytes, whose one operation invokes `echo` with the argument
`ScVal` bool encoded as the word 2 (contract id and signature are placeholders), base64:

```
AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARlY2hvAAAAAQAAAAAAAAACAAAAAAAAAAEAAAAAAAAAAAAAAAAAD0JAAAAAAAAAAAAAAAAAAAGGoAAAAAEHBwcHAAAAQAcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=
```

It is [corpus/entries/soroban.invoke-basics/soroban.invoke-basics.synth-invoke-arg-bool-2/any.xdr](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.invoke-basics/soroban.invoke-basics.synth-invoke-arg-bool-2/any.xdr)
in the shapekit corpus (https://github.com/ErenTopaal/shapekit, tag v0.1.0). The bare `ScVal`
form, `00000000 00000002`, is [soroban.scval-shapes.synth-bool-value-2](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.scval-shapes/soroban.scval-shapes.synth-bool-value-2/any.xdr).

## Impact

Low severity; no loss of funds and no authorization bypass found.

- Signatures bind core's canonical re-encoding, so the bytes with 2 execute exactly what the
  signer signed (`true`); signing the submitted bytes themselves fails with `txBAD_AUTH`.
- Every node decodes and re-encodes the same way, so this is not a consensus split, and the
  ledger only ever holds the canonical form.
- The hash of the submitted bytes differs from the transaction hash core reports, signs
  against and records, and any non-zero word gives the same transaction. A client that keys a
  submission by the hash of the bytes it sent does not find it in the ledger under that hash.
- Decoders disagree on the value: a tool decoding the submitted envelope with stellar-xdr shows
  `false` for an argument core executes as `true`, and go-stellar-sdk and js-stellar-sdk refuse
  the bytes, while core's own `tx` endpoint accepts them.

## Expected value and its source

Refuse a bool word other than 0 or 1 at decode, as go-xdr does (`ErrBadEnumValue`, "bool not 0
or 1") and as RFC 4506 section 4.4 implies. In xdrpp this means checking the raw word before
conversion, for example `if (u > 1) throw xdr_invariant_failed("Invalid enum value");` in the
bool reader (or checking `enum_name(u)` on the word instead of on the converted value), so
the submitted bytes and core's re-encoding can never differ in a bool. The comment on `master`'s
xdrpp says such a check "would be slower"; it is one comparison per bool read.

## Sibling issues

stellar-xdr's lenient read (anything but 1 is false) is the other half of the disagreement:
[stellar/rs-stellar-xdr#622](https://github.com/stellar/rs-stellar-xdr/issues/622) (open).

shapekit vector: `soroban.invoke-basics.synth-invoke-arg-bool-2`
