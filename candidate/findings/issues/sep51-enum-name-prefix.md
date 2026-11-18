<!-- repo: stellar/stellar-protocol -->
# Question: SEP-51 enum names for mixed-case identifiers and for a shared prefix that is not a whole word

SEP-51 v2.0.1, read at [ced008c3](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/ecosystem/sep-0051.md).

## The rule

The Enum section ([lines 384-387](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/ecosystem/sep-0051.md?plain=1#L384-L387)) says:

> The XDR enum data type is represented in JSON with a string, derived from the name-identifier that corresponds to its value. The string is the name-identifier modified to be snake_case, and truncated removing any shared prefix if there are multiple identifiers in the enum.

The Discriminated Union section ([lines 468-471](https://github.com/stellar/stellar-protocol/blob/ced008c3727005ca4a775f430fe6fe346c30f58f/ecosystem/sep-0051.md?plain=1#L468-L471)) names union arms the same way. The examples all use enums whose identifiers are upper case with underscores, such as `SCV_U32`, where the snake_case form and the shared prefix are clear. Three kinds of identifier in the Stellar XDR are not covered by them.

## Case 1: mixed-case identifiers

`IPAddrType` in `Stellar-overlay.x` has the members `IPv4` and `IPv6`.

- stellar-xdr (the Rust crate, 30.0.0) writes `i_pv4` and `i_pv6`. So do the JavaScript, Go, Dart and PHP SDKs.
- The Python and Java SDKs write `ipv4` and `ipv6`.

The text does not say how snake_case treats an identifier that mixes capitals and lower case. It also does not say whether `IPv`, which both identifiers share but which is not a whole word, is a prefix to remove. Under that reading the names would be `4` and `6`.

## Case 2: a shared prefix without an underscore

`TransactionResultCode` in `Stellar-transaction.x` has members such as `txSUCCESS`, `txFAILED` and `txFEE_BUMP_INNER_SUCCESS`. Every member starts with `tx`, and `OperationResultCode` members all start with `op`.

- stellar-xdr writes `tx_success`, `tx_failed` and `tx_fee_bump_inner_success`, keeping the prefix.
- Read literally, "truncated removing any shared prefix" removes `tx` (or `tx_` after snake_case), which gives `success`, `failed` and `fee_bump_inner_success`.

It is not clear whether a prefix counts only when it ends at an underscore in the original identifier. That is what stellar-xdr does: it removes `SCV_` from `SCV_U32` but keeps `tx` in `txSUCCESS`.

## Case 3: a name that starts with a digit after the prefix is removed

`BinaryFuseFilterType` in `Stellar-types.x` has the members `BINARY_FUSE_FILTER_8_BIT`, `BINARY_FUSE_FILTER_16_BIT` and `BINARY_FUSE_FILTER_32_BIT`. Their shared prefix is `BINARY_FUSE_FILTER_`.

- Removing that prefix, as the text says, gives `8_bit`, `16_bit` and `32_bit`. The JavaScript, Python and Java SDKs write these.
- stellar-xdr keeps the first letter of the prefix when the rest starts with a digit, which gives `b8_bit`, `b16_bit` and `b32_bit`. The Go, Dart, PHP and Kotlin Multiplatform SDKs write these too.

The text has no such exception. If one is intended, the Enum section does not show it.

## Question

For these three cases, which strings does SEP-51 intend? If stellar-xdr's choices are the intended ones (`i_pv4`, `tx_success`, `b8_bit`), could the Enum section say three things? How snake_case splits a mixed-case identifier, that a shared prefix is removed only up to an underscore, and that a name starting with a digit keeps a letter of the prefix. SDKs that implement SEP-51 currently arrive at different names for all three, and a reader built from one cannot read JSON written by another.
