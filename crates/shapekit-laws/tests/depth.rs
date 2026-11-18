//! Inputs decode within the shared depth bound: the deepest envelope core accepts decodes in a
//! ledger and in a batch, and one level deeper is refused instead of decoded without limit.

use shapekit_laws::{Checker, Input};
use stellar_xdr::{
    ContractId, DependentTxCluster, GeneralizedTransactionSet, Hash, HostFunction,
    InvokeContractArgs, InvokeHostFunctionOp, LedgerCloseMeta, LedgerCloseMetaBatch,
    LedgerCloseMetaV2, Limits, Operation, OperationBody, ParallelTxExecutionStage,
    ParallelTxsComponent, ScAddress, ScSymbol, SorobanAuthorizationEntry,
    SorobanAuthorizedFunction, SorobanAuthorizedInvocation, SorobanCredentials, Transaction,
    TransactionEnvelope, TransactionPhase, TransactionSetV1, TransactionV1Envelope, VecM, WriteXdr,
};

fn contract_fn() -> InvokeContractArgs {
    InvokeContractArgs {
        contract_address: ScAddress::Contract(ContractId(Hash([7; 32]))),
        function_name: ScSymbol("f".try_into().unwrap()),
        args: VecM::default(),
    }
}

/// A ledger whose parallel Soroban phase holds an envelope with an authorization chain
/// `levels` invocations deep; 493 is the deepest core accepts (caps-limits.toml
/// `auth.invocation.depth`).
fn ledger(levels: u32) -> LedgerCloseMeta {
    let mut node = SorobanAuthorizedInvocation {
        function: SorobanAuthorizedFunction::ContractFn(contract_fn()),
        sub_invocations: VecM::default(),
    };
    for _ in 1..levels {
        node = SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(contract_fn()),
            sub_invocations: vec![node].try_into().unwrap(),
        };
    }
    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: HostFunction::InvokeContract(contract_fn()),
            auth: vec![SorobanAuthorizationEntry {
                credentials: SorobanCredentials::SourceAccount,
                root_invocation: node,
            }]
            .try_into()
            .unwrap(),
        }),
    };
    let env = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: Transaction {
            operations: vec![op].try_into().unwrap(),
            ..Transaction::default()
        },
        signatures: VecM::default(),
    });
    let soroban = TransactionPhase::V1(ParallelTxsComponent {
        base_fee: Some(100),
        execution_stages: vec![ParallelTxExecutionStage(
            vec![DependentTxCluster(vec![env].try_into().unwrap())]
                .try_into()
                .unwrap(),
        )]
        .try_into()
        .unwrap(),
    });
    let mut m = LedgerCloseMetaV2::default();
    m.ledger_header.header.ledger_version = 30;
    m.tx_set = GeneralizedTransactionSet::V1(TransactionSetV1 {
        previous_ledger_hash: Hash([0; 32]),
        phases: vec![TransactionPhase::V0(VecM::default()), soroban]
            .try_into()
            .unwrap(),
    });
    LedgerCloseMeta::V2(m)
}

fn decodes(lcm: LedgerCloseMeta, input: Input) -> bool {
    let bytes = match input {
        Input::Ledger => lcm.to_xdr(Limits::none()).unwrap(),
        Input::Batch => LedgerCloseMetaBatch {
            start_sequence: 0,
            end_sequence: 0,
            ledger_close_metas: vec![lcm].try_into().unwrap(),
        }
        .to_xdr(Limits::none())
        .unwrap(),
    };
    Checker::without_network().bytes(&bytes, input).is_ok()
}

/// Runs `f` on a thread with room to build and encode a thousand nested levels.
fn big(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn the_deepest_accepted_envelope_decodes_and_one_level_more_does_not() {
    big(|| {
        for input in [Input::Ledger, Input::Batch] {
            assert!(decodes(ledger(493), input), "{input:?} at the bound");
            assert!(!decodes(ledger(494), input), "{input:?} past the bound");
        }
    });
}
