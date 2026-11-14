<!-- repo: stellar/go-stellar-sdk -->
# strkey: Decode, DecodeAny and IsValidClaimableBalance accept a claimable balance id with an unknown type byte

## What happens

The binary form of a claimable balance strkey (`B...`) is one type byte followed by a 32-byte hash, and the only type the protocol defines is V0 (`0`). `strkey.Decode(strkey.VersionByteClaimableBalance, s)`, `strkey.DecodeAny(s)` and `strkey.IsValidClaimableBalance(s)` check that the payload is 33 bytes long, but not that its first byte is 0. A `B...` strkey with type byte 1 (or any other value) is accepted.

`xdr.ClaimableBalanceId.DecodeFromStrkey` does check the type byte, so the two decoding paths in the SDK disagree about the same string.

Source, at v0.9.0 (`5410d34`, the latest release; `strkey/main.go` and `xdr/claimable_balance_id.go` are unchanged on `main` at `578d9e7`):

- [`DecodeAny`](https://github.com/stellar/go-stellar-sdk/blob/5410d3410e212331399d1518cc13150279969dac/strkey/main.go#L72-L101) and [`Decode`](https://github.com/stellar/go-stellar-sdk/blob/5410d3410e212331399d1518cc13150279969dac/strkey/main.go#L107-L145) end with `checkPayloadLength`.
- [`checkPayloadLength`](https://github.com/stellar/go-stellar-sdk/blob/5410d3410e212331399d1518cc13150279969dac/strkey/main.go#L151-L172) requires 33 bytes for `VersionByteClaimableBalance` and nothing else.
- [`IsValidClaimableBalance`](https://github.com/stellar/go-stellar-sdk/blob/5410d3410e212331399d1518cc13150279969dac/strkey/main.go#L363-L370) is `Decode(VersionByteClaimableBalance, ...) == nil`.
- [`ClaimableBalanceId.DecodeFromStrkey`](https://github.com/stellar/go-stellar-sdk/blob/5410d3410e212331399d1518cc13150279969dac/xdr/claimable_balance_id.go#L50-L62) refuses any type byte other than V0.

## Reproduce

The input is the SEP-23 test vector "Invalid claimable balance type (first byte of binary key is not 0)":

```
BAAT6DBUX6J22DMZOHIEZTEQ64CVCHEDRKWZONFEUL5Q26QD7R76RGXACA
```

```go
package main

import (
	"fmt"

	"github.com/stellar/go-stellar-sdk/strkey"
	"github.com/stellar/go-stellar-sdk/xdr"
)

func main() {
	const id = "BAAT6DBUX6J22DMZOHIEZTEQ64CVCHEDRKWZONFEUL5Q26QD7R76RGXACA"

	version, payload, err := strkey.DecodeAny(id)
	fmt.Printf("DecodeAny: version=%d type byte=%d len=%d err=%v\n", version, payload[0], len(payload), err)

	payload, err = strkey.Decode(strkey.VersionByteClaimableBalance, id)
	fmt.Printf("Decode: type byte=%d err=%v\n", payload[0], err)

	fmt.Printf("IsValidClaimableBalance: %v\n", strkey.IsValidClaimableBalance(id))

	var cb xdr.ClaimableBalanceId
	fmt.Printf("ClaimableBalanceId.DecodeFromStrkey: err=%v\n", cb.DecodeFromStrkey(id))
}
```

Output with v0.9.0, and the same with `main` at `578d9e7`:

```
DecodeAny: version=8 type byte=1 len=33 err=<nil>
Decode: type byte=1 err=<nil>
IsValidClaimableBalance: true
ClaimableBalanceId.DecodeFromStrkey: err=invalid claimable balance id type: 1
```

A second input with the same result is `BAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADQFQ` (type byte 1, all-zero hash).

## Expected behaviour

`Decode` and `DecodeAny` return an error for both strings and `IsValidClaimableBalance` returns `false`, as `ClaimableBalanceId.DecodeFromStrkey` already does.

- SEP-23 defines the claimable balance payload as "1-byte value indicating the type of claimable balance, where 0x00 maps to V0, and a 32-byte SHA256 hash" ([sep-0023.md, lines 80-82](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/ecosystem/sep-0023.md#L80-L82)) and lists the first string above as an invalid strkey ([lines 357-359](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/ecosystem/sep-0023.md#L357-L359)).
- The XDR defines `ClaimableBalanceIDType` with the single value `CLAIMABLE_BALANCE_ID_TYPE_V0 = 0` ([Stellar-types.x, lines 144-154](https://github.com/stellar/stellar-xdr/blob/4f524bbac80c781c06e4fb5fc93a2d0d331d6705/Stellar-types.x#L144-L154)).
- rs-stellar-strkey 1.0.0 refuses it with `UnsupportedClaimableBalanceVersion` ([src/strkey.rs, lines 739-748](https://github.com/stellar/rs-stellar-strkey/blob/7e541840c6f55361dd18e850163e8d072206580d/src/strkey.rs#L739-L748)), and js-stellar-sdk 17.4.0 `StrKey.decodeClaimableBalance` throws `unknown discriminant 1`.

A fix is one more check in the `VersionByteClaimableBalance` case of `checkPayloadLength`: `payload[0]` must be 0. [#5977](https://github.com/stellar/go-stellar-sdk/pull/5977) added the length checks there; this is the one SEP-23 invalid claimable balance vector they do not cover.

## Impact

Code that validates user input with `strkey.IsValidClaimableBalance` (or `Decode`) and later converts it with `xdr.ClaimableBalanceId.DecodeFromStrkey` gets an error at the second step for input the first step accepted. A caller that reads the hash from the `Decode` payload itself (`payload[1:]`) without checking `payload[0]` treats an id of a type that does not exist as a V0 id. No ledger contains such an id, so this is input validation, not a decoding error on network data.

## Versions

Diverges on every release from v0.7.1 to v0.9.0 (the shapekit conformance run below covers v0.7.1, v0.7.2, v0.7.3, v0.8.0 and v0.9.0).

## Additional reproduction

From a checkout of https://github.com/ErenTopaal/shapekit at tag v0.1.0:

```sh
shapekit conformance run --sdk go --version v0.9.0 --entry cap23.claimable-balances.synth-strkey-b-invalid
```

Cases 13 and 15 of [corpus/vectors/strkey/cap23.claimable-balances.synth-strkey-b-invalid.json](https://github.com/ErenTopaal/shapekit/blob/v0.1.0/corpus/vectors/strkey/cap23.claimable-balances.synth-strkey-b-invalid.json) are the two strings above.

## Sibling issues

js-stellar-sdk 16.4.1 `StrKey.decodeClaimableBalance` and `StrKey.isValidClaimableBalance` accept the same strings; 17.x refuses them. A separate issue for stellar/js-stellar-sdk asks for that 16.x decoding fix to be backported.

shapekit vector: `cap23.claimable-balances.synth-strkey-b-invalid`
