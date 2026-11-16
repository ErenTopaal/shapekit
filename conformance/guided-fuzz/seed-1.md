# Coverage-guided fuzzing across the three sdks

Seed 1, 100000 inputs per run, the same seed and budget with and without the feedback.

| testee | tool | ranges with feedback | ranges without | ranges at start | inputs that grew it (with / without) | pool entries it credited |
|---|---|---|---|---|---|---|
| rust@30.0.0 | llvm source-based region coverage | 16128 | 16010 | 0 | 1040 / 1006 | 1040 |
| go@v0.9.0 | go -cover statement blocks | 6526 | 6443 | 160 | 1103 / 1075 | 1103 |
| js@17.4.0 | v8 14.6.202.34-node.40 precise block coverage | 4106 | 4086 | 1095 | 326 / 334 | 326 |

## Where the inputs came from

| run | source | inputs | grew some sdk's coverage |
|---|---|---|---|
| with feedback | seed-mutation | 29482 | 733 |
| with feedback | generated | 25131 | 138 |
| with feedback | pool-xdr-mutation | 36943 | 279 |
| with feedback | pool-raw-mutation | 8444 | 40 |
| without feedback | seed-mutation | 74938 | 990 |
| without feedback | generated | 25062 | 150 |

## Disagreements with stellar-xdr, with feedback

| sdk | class | inputs |
|---|---|---|
| go | `ContractEvent\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 1 |
| go | `LedgerCloseMeta\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 1 |
| go | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 56 |
| go | `SorobanAuthorizationEntry\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 3 |
| go | `TransactionEnvelope\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 4 |
| js | `ContractEvent\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ContractEvent.body.vN.vN.data.scvBool.b: invalid bool discriminant N` | 1 |
| js | `LedgerCloseMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: LedgerCloseMeta.vN.vN.txProcessing[N].txApplyProcessing.vN.vN.events[N].event.body.vN.vN.data.scvBool.b: invalid bool discriminant N` | 1 |
| js | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ScVal.scvBool.b: invalid bool discriminant N` | 56 |
| js | `SorobanAuthorizationEntry\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: SorobanAuthorizationEntry.rootInvocation.function.sorobanAuthorizedFunctionTypeContractFn.contractFn.args[N].scvBool.b: invalid bool discriminant N` | 3 |
| js | `TransactionEnvelope\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionEnvelope.envelopeTypeTx.vN.tx.operations[N].body.invokeHostFunction.invokeHostFunctionOp.hostFunction.hostFunctionTypeInvokeContract.invokeContract.args[N].scvBool.b: invalid bool discriminant N` | 3 |
| js | `TransactionEnvelope\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionEnvelope.envelopeTypeTx.vN.tx.operations[N].body.invokeHostFunction.invokeHostFunctionOp.hostFunction.hostFunctionTypeInvokeContract.invokeContract.args[N].scvMap.map.value[N].val.scvMap.map.value[N].val.scvMap.map.valu` | 1 |

## Disagreements with stellar-xdr, without feedback

| sdk | class | inputs |
|---|---|---|
| go | `ContractEvent\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 3 |
| go | `LedgerCloseMeta\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 1 |
| go | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 52 |
| go | `SorobanAuthorizationEntry\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 5 |
| go | `TransactionEnvelope\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 8 |
| go | `TransactionResultMetaV1\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 2 |
| go | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 7 |
| js | `ContractEvent\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ContractEvent.body.vN.vN.data.scvBool.b: invalid bool discriminant N` | 2 |
| js | `ContractEvent\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ContractEvent.body.vN.vN.data.scvMap.map.value[N].val.scvBool.b: invalid bool discriminant N` | 1 |
| js | `LedgerCloseMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: LedgerCloseMeta.vN.vN.txProcessing[N].txApplyProcessing.vN.vN.operations[N].events[N].body.vN.vN.data.scvMap.map.value[N].val.scvBool.b: invalid bool discriminant N` | 1 |
| js | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ScVal.scvBool.b: invalid bool discriminant N` | 51 |
| js | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ScVal.scvVec.vec.value[N].scvBool.b: invalid bool discriminant N` | 1 |
| js | `SorobanAuthorizationEntry\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: SorobanAuthorizationEntry.rootInvocation.function.sorobanAuthorizedFunctionTypeContractFn.contractFn.args[N].scvBool.b: invalid bool discriminant N` | 4 |
| js | `SorobanAuthorizationEntry\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: SorobanAuthorizationEntry.rootInvocation.subInvocations[N].function.sorobanAuthorizedFunctionTypeContractFn.contractFn.args[N].scvBool.b: invalid bool discriminant N` | 1 |
| js | `TransactionEnvelope\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionEnvelope.envelopeTypeTx.vN.tx.operations[N].body.invokeHostFunction.invokeHostFunctionOp.hostFunction.hostFunctionTypeInvokeContract.invokeContract.args[N].scvBool.b: invalid bool discriminant N` | 8 |
| js | `TransactionResultMetaV1\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionResultMetaVN.txApplyProcessing.vN.vN.events[N].event.body.vN.vN.data.scvBool.b: invalid bool discriminant N` | 2 |
| js | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionResultMeta.txApplyProcessing.vN.vN.diagnosticEvents[N].inSuccessfulContractCall: invalid bool discriminant N` | 3 |
| js | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionResultMeta.txApplyProcessing.vN.vN.operations[N].changes[N].ledgerEntryState.state.data.contractData.contractData.val.scvMap.map.value[N].val.scvBool.b: invalid bool discriminant N` | 2 |
| js | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionResultMeta.txApplyProcessing.vN.vN.operations[N].changes[N].ledgerEntryUpdated.updated.data.contractData.contractData.val.scvMap.map.value[N].val.scvBool.b: invalid bool discriminant N` | 2 |
