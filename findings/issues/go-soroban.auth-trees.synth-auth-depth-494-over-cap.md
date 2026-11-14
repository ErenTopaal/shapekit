<!-- repo: stellar/go-stellar-sdk -->
# xdr: DecodeOptions.MaxDepth counts fewer levels than stellar-core, so envelopes core refuses as too deep decode

## What happens

stellar-core refuses a transaction envelope whose nesting exceeds its decode depth limit. The xdr package's `MaxDepth` counts nesting in a different unit from the one core uses, so a decoder configured with the same number accepts envelopes about twice as deep as core does, and the default (`SafeUnmarshal`, 1500) accepts about three times as deep.

| envelope (bytes built by the program below) | stellar-core (stellar-xdr 30.0.0 at depth 1000) | go-stellar-sdk v0.9.0, `MaxDepth: 1000` | go-stellar-sdk v0.9.0, default (1500) |
|---|---|---|---|
| deepest authorization chain accepted | 493 levels | 988 levels | 1488 levels |
| deepest nested `ScVec` call argument accepted | 246 levels | 495 levels | 745 levels |

So an authorization tree 494 invocations deep, or a call argument nested 247 `ScVec` levels deep, decodes and re-encodes in the Go SDK, and stellar-core refuses the envelope with `txMALFORMED`.

## Cause

From protocol 28, core checks every submitted envelope by parsing it with stellar-xdr (Rust) under `Limits { depth: 1000, len }`:

- [`validateXDRForProtocol`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/transactions/TransactionUtils.cpp#L1350-L1379) sets `rustDepthLimit = 1000` and calls `can_parse_transaction`, which runs [`TransactionEnvelope::read_xdr` with that depth](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/rust/src/soroban_proto_any.rs#L781-L790); a failure is [`txMALFORMED`](https://github.com/stellar/stellar-core/blob/0e49751aa9ddf892302cca02cea7f3f5d97df9f1/src/transactions/TransactionFrame.cpp#L1923-L1926) (stellar-core v30.0.0).
- stellar-xdr spends one unit of depth on every value it reads ([`with_limited_depth`](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L440-L452)), including each optional ([`Option<T>`](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L924-L939)) and each variable-length array ([`VecM<T>`](https://github.com/stellar/rs-stellar-xdr/blob/3c2aa43d2ab54d429ae44aa5143d805bf5dba7e2/src/generated.rs#L1570-L1573)). One level of an invocation chain costs 2 (`SorobanAuthorizedInvocation`, then its `subInvocations` array); one `ScVec` level costs 4 (`ScVal`, the optional, `ScVec`, its array).

The Go decoders spend one unit only on entering a named type: [`SorobanAuthorizedInvocation.DecodeFrom`](https://github.com/stellar/go-stellar-sdk/blob/5410d3410e212331399d1518cc13150279969dac/xdr/xdr_generated.go#L31313-L31357) decrements `maxDepth` once and passes it unchanged into the `SubInvocations` loop, and [`ScVec.DecodeFrom`](https://github.com/stellar/go-stellar-sdk/blob/5410d3410e212331399d1518cc13150279969dac/xdr/xdr_generated.go#L60439-L60478) and [`ScVal.DecodeFrom`](https://github.com/stellar/go-stellar-sdk/blob/5410d3410e212331399d1518cc13150279969dac/xdr/xdr_generated.go#L61724-L61728) do the same, with nothing for the optional or the array. One invocation level costs 1 and one `ScVec` level costs 2. [`UnmarshalWithOptions`](https://github.com/stellar/go-stellar-sdk/blob/5410d3410e212331399d1518cc13150279969dac/xdr/xdr_generated.go#L66-L73) passes `options.MaxDepth` straight to `DecodeFrom`, and `SafeUnmarshal` uses `xdr.DefaultDecodeOptions`, whose depth is go-xdr's [`DecodeDefaultMaxDepth = 1500`](https://github.com/stellar/go-xdr/blob/dc590f17552a5063bc59411b7e6b9e928b852435/xdr3/decode.go#L36) (the go-xdr version v0.9.0 requires).

## Reproduce

The program builds the envelopes byte by byte, so the input does not depend on the SDK's encoder, and decodes them with the SDK:

```go
package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"fmt"

	"github.com/stellar/go-stellar-sdk/xdr"
)

func u32(n uint32) []byte { return binary.BigEndian.AppendUint32(nil, n) }

func cat(parts ...[]byte) []byte { return bytes.Join(parts, nil) }

// call is InvokeContractArgs: contract 0x32..32, function "deep", the given arguments.
func call(args ...[]byte) []byte {
	return cat(u32(1), bytes.Repeat([]byte{0x32}, 32), u32(4), []byte("deep"), u32(uint32(len(args))), cat(args...))
}

// envelope is a v1 TransactionEnvelope with one InvokeHostFunction operation. chain > 0 adds one
// source-account auth entry whose invocation tree is a single chain that many levels deep;
// vecDepth > 0 passes one argument, an ScVec nested that many levels around the u32 0.
func envelope(chain, vecDepth int) []byte {
	var args [][]byte
	if vecDepth > 0 {
		v := cat(u32(3), u32(0)) // SCV_U32 0
		for i := 0; i < vecDepth; i++ {
			v = cat(u32(16), u32(1), u32(1), v) // SCV_VEC, present, one element
		}
		args = append(args, v)
	}
	auth := u32(0)
	if chain > 0 {
		fn := cat(u32(0), call()) // SOROBAN_AUTHORIZED_FUNCTION_TYPE_CONTRACT_FN
		tree := bytes.Repeat(cat(fn, u32(1)), chain-1)
		auth = cat(u32(1), u32(0), tree, fn, u32(0)) // one entry, SOROBAN_CREDENTIALS_SOURCE_ACCOUNT
	}
	return cat(
		u32(2), u32(0), make([]byte, 32), // ENVELOPE_TYPE_TX, ed25519 source account
		u32(100), make([]byte, 4), u32(1), // fee 100, sequence number 1
		u32(0), u32(0), // no preconditions, no memo
		u32(1), u32(0), u32(24), // one operation, no source account, INVOKE_HOST_FUNCTION
		u32(0), call(args...), auth, // HOST_FUNCTION_TYPE_INVOKE_CONTRACT
		u32(0), u32(0), // transaction ext v0, no signatures
	)
}

func decodes(b []byte, maxDepth uint) bool {
	var env xdr.TransactionEnvelope
	_, err := xdr.UnmarshalWithOptions(bytes.NewReader(b), &env, xdr.DecodeOptions{MaxDepth: maxDepth, MaxInputLen: len(b)})
	return err == nil
}

func deepest(f func(int) []byte, maxDepth uint) int {
	n := 1
	for decodes(f(n+1), maxDepth) {
		n++
	}
	return n
}

func main() {
	auth, vec := envelope(494, 0), envelope(0, 247)
	fmt.Printf("auth chain 494 (sha256 %x): MaxDepth 1000 accepts %v, SafeUnmarshal accepts %v\n",
		sha256.Sum256(auth), decodes(auth, 1000), xdr.SafeUnmarshal(auth, &xdr.TransactionEnvelope{}) == nil)
	fmt.Printf("ScVec argument 247 (sha256 %x): MaxDepth 1000 accepts %v, SafeUnmarshal accepts %v\n",
		sha256.Sum256(vec), decodes(vec, 1000), xdr.SafeUnmarshal(vec, &xdr.TransactionEnvelope{}) == nil)
	for _, d := range []uint{1000, 1500} {
		fmt.Printf("MaxDepth %d: deepest auth chain %d, deepest ScVec argument %d\n", d,
			deepest(func(n int) []byte { return envelope(n, 0) }, d),
			deepest(func(n int) []byte { return envelope(0, n) }, d))
	}
}
```

Output with v0.9.0 (`5410d34`), and the same with `main` at `578d9e7`:

```
auth chain 494 (sha256 eb86c8f3b087936f57c4fa1d2d314e7b5e678c9c5e991b800566cf83b4e26f49): MaxDepth 1000 accepts true, SafeUnmarshal accepts true
ScVec argument 247 (sha256 cce3f0787921ee719f1ec941fa1bc6be989b6dc6dd2abc2e7c132d24d21a1271): MaxDepth 1000 accepts true, SafeUnmarshal accepts true
MaxDepth 1000: deepest auth chain 988, deepest ScVec argument 495
MaxDepth 1500: deepest auth chain 1488, deepest ScVec argument 745
```

The same bytes through stellar-xdr 30.0.0, `TransactionEnvelope::from_xdr(bytes, Limits { depth: 1000, len: bytes.len() })` (core's check), fail with `DepthLimitExceeded` for both envelopes above; the deepest that pass are a 493-level chain and a 246-level `ScVec` argument.

## Expected behaviour

A caller can decode with the depth limit core applies and get core's answer: an envelope core refuses as too deep is refused, and one core accepts is accepted. Two ways that would work:

- count depth in the generated decoders the way stellar-xdr does (one unit for each optional and each variable-length array as well as each named type), so `MaxDepth: 1000` means what core's 1000 means; or
- if the current unit is kept, document that `MaxDepth` is not comparable with core's limit and provide a way to apply core's check.

## Impact

Low. No ledger contains such an envelope, since core refuses it, so readers of ledger data are not affected. Code that builds, checks or forwards transactions with the SDK cannot detect locally that core will refuse the envelope: it decodes, re-encodes and signs it, and the refusal only comes back from core as `txMALFORMED` at submission.

## Versions

Diverges on v0.7.1, v0.7.2, v0.7.3, v0.8.0 and v0.9.0 (shapekit conformance run).

## Additional reproduction

From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0:

```sh
shapekit conformance run --sdk go --version v0.9.0 --entry soroban.auth-trees.synth-auth-depth-494-over-cap
```

The entry's envelope is [corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-494-over-cap/any.xdr](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-494-over-cap/any.xdr) (a 494-level chain with a Soroban transaction extension and one signature). The same run over `soroban.scval-shapes.synth-scvec-depth-247-over-cap`, `soroban.scval-shapes.synth-scmap-depth-198-over-cap` and `soroban.auth-trees.p30-declared-depth-493` shows the same divergence for nested `ScVec`s, nested `ScMap`s and an address-credential authorization tree. The last one was submitted to a local stellar-core 30.0.0 node at protocol 30, which answered `txMALFORMED`.

## Sibling issues

js-stellar-sdk 17.x has the same gap with its own unit (js-xdr counts every schema it enters, and its default limit is 1500 with no option on `fromXdr`); a separate issue for stellar/js-stellar-sdk describes it. rs-stellar-xdr is the reference here: it is the parser core runs.

shapekit vector: `soroban.auth-trees.synth-auth-depth-494-over-cap`
