# Coverage-guided fuzzing across the three sdks

Seed 2, 100000 inputs per run, the same seed and budget with and without the feedback.

| testee | tool | ranges with feedback | ranges without | ranges at start | inputs that grew it (with / without) | pool entries it credited |
|---|---|---|---|---|---|---|
| rust@30.0.0 | llvm source-based region coverage | 16100 | 16009 | 0 | 1051 / 981 | 1051 |
| go@v0.9.0 | go -cover statement blocks | 6529 | 6441 | 160 | 1121 / 1043 | 1121 |
| js@17.4.0 | v8 14.6.202.34-node.40 precise block coverage | 4086 | 4081 | 1095 | 320 / 316 | 320 |

## Where the inputs came from

| run | source | inputs | grew some sdk's coverage |
|---|---|---|---|
| with feedback | seed-mutation | 29670 | 759 |
| with feedback | generated | 24830 | 141 |
| with feedback | pool-xdr-mutation | 38563 | 278 |
| with feedback | pool-raw-mutation | 6937 | 18 |
| without feedback | seed-mutation | 74960 | 975 |
| without feedback | generated | 25040 | 143 |

## Disagreements with stellar-xdr, with feedback

| sdk | class | inputs |
|---|---|---|
| go | `ContractEvent\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 1 |
| go | `LedgerCloseMeta\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 3 |
| go | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 21 |
| go | `SorobanAuthorizationEntry\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 4 |
| go | `TransactionEnvelope\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 4 |
| go | `TransactionResultMetaV1\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 2 |
| go | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 1 |
| js | `ContractEvent\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ContractEvent.body.vN.vN.data.scvBool.b: invalid bool discriminant N` | 1 |
| js | `LedgerCloseMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: LedgerCloseMeta.vN.vN.txProcessing[N].txApplyProcessing.vN.vN.diagnosticEvents[N].inSuccessfulContractCall: invalid bool discriminant N` | 1 |
| js | `LedgerCloseMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: LedgerCloseMeta.vN.vN.txProcessing[N].txApplyProcessing.vN.vN.events[N].event.body.vN.vN.data.scvBool.b: invalid bool discriminant N` | 1 |
| js | `LedgerCloseMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: LedgerCloseMeta.vN.vN.txSet.vNTxSet.vNTxSet.phases[N].parallelTxsComponent.parallelTxsComponent.executionStages[N][N][N].envelopeTypeTx.vN.tx.operations[N].body.invokeHostFunction.invokeHostFunctionOp.hostFunction.hostFunctionType` | 1 |
| js | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ScVal.scvBool.b: invalid bool discriminant N` | 20 |
| js | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ScVal.scvVec.vec.value[N].scvBool.b: invalid bool discriminant N` | 1 |
| js | `SorobanAuthorizationEntry\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: SorobanAuthorizationEntry.rootInvocation.function.sorobanAuthorizedFunctionTypeContractFn.contractFn.args[N].scvBool.b: invalid bool discriminant N` | 4 |
| js | `TransactionEnvelope\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionEnvelope.envelopeTypeTx.vN.tx.ext.sorobanData.sorobanData.resources.footprint.readOnly[N].contractData.contractData.key.scvBool.b: invalid bool discriminant N` | 1 |
| js | `TransactionEnvelope\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionEnvelope.envelopeTypeTx.vN.tx.operations[N].body.invokeHostFunction.invokeHostFunctionOp.hostFunction.hostFunctionTypeInvokeContract.invokeContract.args[N].scvBool.b: invalid bool discriminant N` | 3 |
| js | `TransactionResultMetaV1\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionResultMetaVN.txApplyProcessing.vN.vN.events[N].event.body.vN.vN.data.scvBool.b: invalid bool discriminant N` | 2 |
| js | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionResultMeta.txApplyProcessing.vN.vN.diagnosticEvents[N].inSuccessfulContractCall: invalid bool discriminant N` | 1 |

## Disagreements with stellar-xdr, without feedback

| sdk | class | inputs |
|---|---|---|
| go | `ContractEvent\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 1 |
| go | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 49 |
| go | `SorobanAuthorizationEntry\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 2 |
| go | `TransactionEnvelope\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 5 |
| go | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'` | 5 |
| js | `ContractEvent\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ContractEvent.body.vN.vN.data.scvBool.b: invalid bool discriminant N` | 1 |
| js | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ScVal.scvBool.b: invalid bool discriminant N` | 47 |
| js | `ScVal\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: ScVal.scvMap.map.value[N].key.scvBool.b: invalid bool discriminant N` | 2 |
| js | `SorobanAuthorizationEntry\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: SorobanAuthorizationEntry.rootInvocation.function.sorobanAuthorizedFunctionTypeContractFn.contractFn.args[N].scvBool.b: invalid bool discriminant N` | 2 |
| js | `TransactionEnvelope\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionEnvelope.envelopeTypeTx.vN.tx.operations[N].body.invokeHostFunction.invokeHostFunctionOp.hostFunction.hostFunctionTypeInvokeContract.invokeContract.args[N].scvBool.b: invalid bool discriminant N` | 5 |
| js | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionResultMeta.txApplyProcessing.vN.vN.diagnosticEvents[N].event.body.vN.vN.data.scvVec.vec.value[N].scvBool.b: invalid bool discriminant N` | 1 |
| js | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionResultMeta.txApplyProcessing.vN.vN.diagnosticEvents[N].inSuccessfulContractCall: invalid bool discriminant N` | 2 |
| js | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionResultMeta.txApplyProcessing.vN.vN.events[N].event.body.vN.vN.data.scvBool.b: invalid bool discriminant N` | 1 |
| js | `TransactionResultMeta\|rust-accepts-go-rejects\|rust:ok\|go:XdrError: TransactionResultMeta.txApplyProcessing.vN.vN.operations[N].changes[N].ledgerEntryState.state.data.contractData.contractData.val.scvMap.map.value[N].val.scvBool.b: invalid bool discriminant N` | 1 |
