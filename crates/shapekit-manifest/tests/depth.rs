//! Decode depth: the shared bounds match caps-limits.toml, and the toolkit counts levels the
//! way core does, so the deepest envelope core accepts decodes at its bound and in a ledger.

use std::io::Cursor;
use std::path::Path;

use shapekit_manifest::{ENVELOPE_DEPTH, META_DEPTH, decode_depth, decode_limits, xdr};
use shapekit_registry::Registry;
use stellar_xdr::{
    ContractId, DependentTxCluster, GeneralizedTransactionSet, Hash, HostFunction,
    InvokeContractArgs, InvokeHostFunctionOp, LedgerCloseMeta, LedgerCloseMetaV2, Limited, Limits,
    Operation, OperationBody, ParallelTxExecutionStage, ParallelTxsComponent, ReadXdr, ScAddress,
    ScSymbol, ScVal, ScVec, SorobanAuthorizationEntry, SorobanAuthorizedFunction,
    SorobanAuthorizedInvocation, SorobanCredentials, Transaction, TransactionEnvelope,
    TransactionPhase, TransactionSetV1, TransactionV1Envelope, VecM, WriteXdr,
};

fn registry() -> Registry {
    Registry::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../registry")).unwrap()
}

fn recommended(r: &Registry, id: &str) -> u64 {
    r.limits
        .limits
        .iter()
        .find(|l| l.id == id)
        .unwrap_or_else(|| panic!("{id} is not in caps-limits.toml"))
        .recommended
        .try_into()
        .unwrap()
}

fn contract_fn() -> InvokeContractArgs {
    InvokeContractArgs {
        contract_address: ScAddress::Contract(ContractId(Hash([7; 32]))),
        function_name: ScSymbol("f".try_into().unwrap()),
        args: VecM::default(),
    }
}

fn envelope(args: Vec<ScVal>, auth: Vec<SorobanAuthorizationEntry>) -> TransactionEnvelope {
    let mut f = contract_fn();
    f.args = args.try_into().unwrap();
    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: HostFunction::InvokeContract(f),
            auth: auth.try_into().unwrap(),
        }),
    };
    TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: Transaction {
            operations: vec![op].try_into().unwrap(),
            ..Transaction::default()
        },
        signatures: VecM::default(),
    })
}

/// A source-account entry whose invocation tree is one chain `levels` invocations deep, the
/// shape of the synth entries `soroban.auth-trees.synth-auth-depth-*`.
fn auth_envelope(levels: u32) -> TransactionEnvelope {
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
    envelope(
        vec![],
        vec![SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: node,
        }],
    )
}

fn vec_envelope(levels: u32) -> TransactionEnvelope {
    let mut v = ScVal::U32(0);
    for _ in 0..levels {
        v = ScVal::Vec(Some(ScVec(vec![v].try_into().unwrap())));
    }
    envelope(vec![v], vec![])
}

/// A protocol 30 ledger whose parallel Soroban phase holds `env`, the deepest place a
/// ledger carries an envelope.
fn ledger_with(env: TransactionEnvelope) -> LedgerCloseMeta {
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
    let classic = TransactionPhase::V0(VecM::default());
    let mut m = LedgerCloseMetaV2::default();
    m.ledger_header.header.ledger_version = 30;
    m.tx_set = GeneralizedTransactionSet::V1(TransactionSetV1 {
        previous_ledger_hash: Hash([0; 32]),
        phases: vec![classic, soroban].try_into().unwrap(),
    });
    LedgerCloseMeta::V2(m)
}

/// Core's own check, word for word: `TransactionEnvelope::read_xdr` in
/// `can_parse_transaction` (stellar-core v30.0.0, src/rust/src/soroban_proto_any.rs).
fn core_parses(bytes: &[u8], depth: u32) -> bool {
    TransactionEnvelope::read_xdr(&mut Limited::new(
        Cursor::new(bytes),
        Limits {
            depth,
            len: bytes.len(),
        },
    ))
    .is_ok()
}

fn toolkit_decodes(ty: &str, bytes: &[u8], depth: u32) -> bool {
    let limits = Limits {
        depth,
        len: bytes.len(),
    };
    xdr::decode(ty, bytes, limits).is_ok()
}

/// Runs `f` on a thread with room for a thousand nested decode frames.
fn big(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn bounds_are_the_caps_limits_values() {
    let r = registry();
    assert_eq!(
        u64::from(ENVELOPE_DEPTH),
        recommended(&r, "xdr.envelope.depth")
    );
    assert_eq!(u64::from(META_DEPTH), recommended(&r, "xdr.meta.depth"));
    assert_eq!(decode_depth("LedgerCloseMeta"), META_DEPTH);
    assert_eq!(decode_depth("TransactionEnvelope"), ENVELOPE_DEPTH);
    assert_eq!(decode_limits("ScVal", 9).len, 9);
}

#[test]
fn deepest_auth_chain_decodes_like_core() {
    big(|| {
        let cap = u32::try_from(recommended(&registry(), "auth.invocation.depth")).unwrap();
        assert_eq!(cap, 493);
        let accepted = auth_envelope(cap).to_xdr(Limits::none()).unwrap();
        let refused = auth_envelope(cap + 1).to_xdr(Limits::none()).unwrap();
        assert!(core_parses(&accepted, ENVELOPE_DEPTH));
        assert!(!core_parses(&refused, ENVELOPE_DEPTH));
        assert!(toolkit_decodes(
            "TransactionEnvelope",
            &accepted,
            ENVELOPE_DEPTH
        ));
        assert!(!toolkit_decodes(
            "TransactionEnvelope",
            &refused,
            ENVELOPE_DEPTH
        ));
        // the validator's own path, round trip and XDR-JSON included
        let limits = decode_limits("TransactionEnvelope", accepted.len());
        xdr::roundtrip_check("TransactionEnvelope", &accepted, limits.clone()).unwrap();
        xdr::to_xdr_json("TransactionEnvelope", &accepted, limits).unwrap();
    });
}

#[test]
fn toolkit_and_core_agree_at_every_depth_near_the_edge() {
    big(|| {
        for (what, env) in [("auth", auth_envelope(493)), ("scvec", vec_envelope(246))] {
            let bytes = env.to_xdr(Limits::none()).unwrap();
            for depth in ENVELOPE_DEPTH - 3..=ENVELOPE_DEPTH + 3 {
                assert_eq!(
                    toolkit_decodes("TransactionEnvelope", &bytes, depth),
                    core_parses(&bytes, depth),
                    "{what} at depth {depth}"
                );
            }
        }
        // a value that fits exactly: the bound is the depth the value needs, no more
        let flat = vec_envelope(0).to_xdr(Limits::none()).unwrap();
        let needed = (1..64)
            .find(|d| core_parses(&flat, *d))
            .expect("a flat envelope fits in 64 levels");
        assert!(toolkit_decodes("TransactionEnvelope", &flat, needed));
        assert!(!toolkit_decodes("TransactionEnvelope", &flat, needed - 1));
    });
}

#[test]
fn a_ledger_holding_the_deepest_envelope_needs_meta_depth() {
    big(|| {
        let lcm = ledger_with(auth_envelope(493))
            .to_xdr(Limits::none())
            .unwrap();
        assert!(toolkit_decodes("LedgerCloseMeta", &lcm, META_DEPTH));
        // 12 levels above the envelope, exactly: one fewer and the ledger is refused, and at
        // the envelope bound, where the packer and oracle used to decode, it is refused too
        assert!(!toolkit_decodes("LedgerCloseMeta", &lcm, META_DEPTH - 1));
        assert!(!toolkit_decodes("LedgerCloseMeta", &lcm, ENVELOPE_DEPTH));
        let direct = |depth| {
            LedgerCloseMeta::from_xdr(
                &lcm,
                Limits {
                    depth,
                    len: lcm.len(),
                },
            )
            .is_ok()
        };
        assert!(direct(META_DEPTH));
        assert!(!direct(META_DEPTH - 1));
        let limits = decode_limits("LedgerCloseMeta", lcm.len());
        xdr::roundtrip_check("LedgerCloseMeta", &lcm, limits).unwrap();
    });
}
