<!-- repo: stellar/rs-stellar-xdr -->
# stellar-xdr 28.0.0, 28.0.1, 30.0.0: the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 9810 (soroban.auth-trees.synth-auth-depth-493-deepest-accepted)

## What happens

`stellar-xdr` 28.0.0, 28.0.1, 30.0.0 answers 3 of the checks of corpus entry `soroban.auth-trees.synth-auth-depth-493-deepest-accepted` differently from the Stellar network.

## Minimal vector

Entry `soroban.auth-trees.synth-auth-depth-493-deepest-accepted` (kind Xdr, family `soroban.auth-trees`) of the shapekit corpus 0.1.0. Each check below is one request of the shapekit testee protocol (`docs/testee-protocol.md`).

### `any/encode_json` on 28.0.0, 28.0.1, 30.0.0

the sdk refused a value the network accepts: recursion limit exceeded at line 1 column 9810

```json
{"json":{"tx":{"signatures":[{"hint":"07070707","signature":"07070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707"}],"tx":{"cond":"none","ext":{"v1":{"ext":"v0","resource_fee":"100000","resources":{"disk_read_bytes":0,"footprint":{"read_only":[],"read_write":[]},"instructions":1000000,"write_bytes":0}}},"fee":100,"memo":"none","operations":[{"body":{"invoke_host_function":{"auth":[{"credentials":"source_account","root_invocation":{"function":{"contract_fn":{"args":[],"contract_address":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","function_name":"deep"}},"sub_invocations":[{"function":{"contract_fn":{"args":[],"contract_address":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","function_name":"deep"}},"sub_invocations":[{"function":{"contract_fn":{"args":[],"contract_address":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","function_name":"deep"}},"sub_invocations":[{"function":{"contract_fn":{"args":[],"contract_address":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","function_name":"deep"}},"sub_invocations":[{"function":{"contract_fn":{"args":[],"contract_address":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","function_name":"deep"}},"sub_invocations":[{"function":{"contract_fn":{"args":[],"contract_address":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","function_name":"deep"}},"sub_invocations":[{"function":{"contract_fn":{"args":[],"contract_address":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","function_name":"deep"}},"sub_invocations":[{"function":{"contract_fn":{"args":[],"contract_address":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","function_name":"deep"}},"sub_invocations":[{"function":{"contract_fn":{"args":[],"contract_address":"CAZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDEMRSGIZDFJRF","function_name":"deep"}},"sub_invocations":[{"function":{"contract_fn":{"args":[],"contract_ad... (80704 chars; the corpus file holds the whole value)
```

Expected: `{"xdr":"AAAAAgAAAAABAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQAAAGQAAAABAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyAAAABGRlZXAAAAAAAAAAAQAAAAAAAAABMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIAAAAEZGVlcAAAAAAAAAABAAAAAAAAAAEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMgAAAARkZWVwAAAAAAAAAAEAAAAAAAAAATIyMjIyMjIyMjIyMjIyMjIyMjIy... (37136 chars)"}`

Actual: `{"rejected":"recursion limit exceeded at line 1 column 9810"}`

## Reproduce

```sh
shapekit conformance run --sdk rust --version 28.0.0 --entry soroban.auth-trees.synth-auth-depth-493-deepest-accepted
```

## Expected value and its source

- corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.expected.json: SEP-51 rendering by stellar-xdr 30.0.0; must encode back to corpus/entries/soroban.auth-trees/soroban.auth-trees.synth-auth-depth-493-deepest-accepted/any.xdr

Entry notes: a source-account authorization entry whose invocation tree is a single chain 493 invocations deep, at the deepest an envelope decodes at depth 1000. core's depth check accepts it; the host converts at most 100 levels (scval.depth) and runs at most 100 frames, so an invocation this deep fails and the transaction lands in the ledger as failed with the full tree intact. a decoder must accept it.

## Sibling issues

None: only this SDK diverges on this entry.

shapekit vector: `soroban.auth-trees.synth-auth-depth-493-deepest-accepted`
