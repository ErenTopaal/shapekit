//! Synthetic `kind = xdr` vectors: bytes the reference decoder must reject, and values that
//! decode but core never emits or always refuses.
//!
//! Over-bound vectors cannot be built from stellar-xdr values, because its bounded types refuse
//! them. They are composed from the encodings of the surrounding fields instead, using the
//! unbounded `VecM<T>` and `StringM`, which write the same length-prefixed form. Every
//! composition is checked against the real encoder at its in-bound size first.

use anyhow::{Context, Result, ensure};
use shapekit_manifest::{EntryKind, Limits, Validity, xdr};
use shapekit_registry::Registry;
use stellar_xdr as x;
use stellar_xdr::WriteXdr;

use crate::synth::{
    LINK_GHSA_3XHM, LINK_JS_1766, LINK_JS_1785, LINK_PY_1246, LINK_RS_581, Spec, XdrItem,
    limits_for,
};

const ENVELOPE_DEPTH: &str = "xdr.envelope.depth";
const META_DEPTH: &str = "xdr.meta.depth";

/// The bytes of the GHSA-3xhm-p452-9wjh advisory: `{"doc":"","name":"f","inputs":[],
/// "outputs":["u32","u32"]}` as stellar-xdr 28.0.0 encoded it from JSON.
pub const GHSA_3XHM_BASE64: &str = "AAAAAAAAAAFmAAAAAAAAAAAAAAIAAAAEAAAABA==";

pub fn items(registry: &Registry) -> Result<Vec<XdrItem>> {
    let mut out = Vec::new();
    over_bounds(&mut out)?;
    depth(registry, &mut out)?;
    malformed(&mut out)?;
    not_emittable(&mut out)?;
    invalid_by_core(&mut out)?;
    Ok(out)
}

fn item(spec: Spec, xdr_type: &'static str, bytes: Vec<u8>, depth_limit: &'static str) -> XdrItem {
    XdrItem {
        spec,
        xdr_type,
        bytes,
        depth_limit,
    }
}

pub fn enc<T: WriteXdr>(value: &T) -> Result<Vec<u8>> {
    value
        .to_xdr(Limits::none())
        .context("encoding a synthetic value")
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

pub fn key(n: u8) -> [u8; 32] {
    [n; 32]
}

fn muxed(n: u8) -> x::MuxedAccount {
    x::MuxedAccount::Ed25519(x::Uint256(key(n)))
}

fn account(n: u8) -> x::AccountId {
    x::AccountId(x::PublicKey::PublicKeyTypeEd25519(x::Uint256(key(n))))
}

fn credit4(code: &[u8; 4], issuer: u8) -> x::Asset {
    x::Asset::CreditAlphanum4(x::AlphaNum4 {
        asset_code: x::AssetCode4(*code),
        issuer: account(issuer),
    })
}

fn op(body: x::OperationBody) -> x::Operation {
    x::Operation {
        source_account: None,
        body,
    }
}

fn payment(dest: u8) -> x::Operation {
    op(x::OperationBody::Payment(x::PaymentOp {
        destination: muxed(dest),
        asset: x::Asset::Native,
        amount: 1,
    }))
}

fn sig(n: u8) -> Result<x::DecoratedSignature> {
    Ok(x::DecoratedSignature {
        hint: x::SignatureHint([n; 4]),
        signature: x::Signature(vec![n; 64].try_into()?),
    })
}

fn unbounded<T>(items: Vec<T>) -> Result<x::VecM<T>> {
    items.try_into().context("building an unbounded vector")
}

const FEE: u32 = 100;
const SEQ: i64 = (1 << 32) | 1;

fn transaction(
    ops: Vec<x::Operation>,
    memo: x::Memo,
    cond: x::Preconditions,
    ext: x::TransactionExt,
) -> Result<x::Transaction> {
    Ok(x::Transaction {
        source_account: muxed(1),
        fee: FEE,
        seq_num: x::SequenceNumber(SEQ),
        cond,
        memo,
        operations: ops.try_into()?,
        ext,
    })
}

fn envelope(
    tx: x::Transaction,
    sigs: Vec<x::DecoratedSignature>,
) -> Result<x::TransactionEnvelope> {
    Ok(x::TransactionEnvelope::Tx(x::TransactionV1Envelope {
        tx,
        signatures: sigs.try_into()?,
    }))
}

/// The encoded pieces of a v1 envelope, any of which may be over its bound.
struct Parts {
    cond: Vec<u8>,
    memo: Vec<u8>,
    ops: Vec<u8>,
    ext: Vec<u8>,
    sigs: Vec<u8>,
}

impl Parts {
    fn plain() -> Result<Parts> {
        Ok(Parts {
            cond: enc(&x::Preconditions::None)?,
            memo: enc(&x::Memo::None)?,
            ops: enc(&unbounded(vec![payment(2)])?)?,
            ext: enc(&x::TransactionExt::V0)?,
            sigs: enc(&unbounded(vec![sig(7)?])?)?,
        })
    }

    fn compose(&self) -> Result<Vec<u8>> {
        Ok(cat(&[
            &enc(&x::EnvelopeType::Tx)?,
            &enc(&muxed(1))?,
            &enc(&FEE)?,
            &enc(&SEQ)?,
            &self.cond,
            &self.memo,
            &self.ops,
            &self.ext,
            &self.sigs,
        ]))
    }
}

// the composition must equal what the encoder writes for the same in-bound value, or every
// over-bound vector built from it would be testing a different layout
fn check_composition() -> Result<()> {
    let tx = transaction(
        vec![payment(2)],
        x::Memo::None,
        x::Preconditions::None,
        x::TransactionExt::V0,
    )?;
    let real = enc(&envelope(tx, vec![sig(7)?])?)?;
    ensure!(
        Parts::plain()?.compose()? == real,
        "envelope composition differs from the encoder"
    );
    Ok(())
}

fn over_bounds(out: &mut Vec<XdrItem>) -> Result<()> {
    check_composition()?;
    let max = |s: Spec| {
        s.reject(Validity::InvalidByCore)
            .flags(&["over-max-length", "max-length-vec"])
    };

    let mut p = Parts::plain()?;
    p.ops = enc(&unbounded(
        (0..101).map(|i| payment(2 + (i % 3) as u8)).collect(),
    )?)?;
    out.push(item(
        max(Spec::new("classic.max-length", "tx-101-operations", EntryKind::Xdr))
            .fields(&["tx.operations"])
            .notes("a v1 transaction envelope with 101 operations, one past MAX_OPS_PER_TX = 100 (operations<MAX_OPS_PER_TX> in Stellar-transaction.x)."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));

    let mut p = Parts::plain()?;
    p.sigs = enc(&unbounded((0..21).map(sig).collect::<Result<Vec<_>>>()?)?)?;
    out.push(item(
        max(Spec::new(
            "classic.max-length",
            "tx-21-signatures",
            EntryKind::Xdr,
        ))
        .fields(&["tx.signatures"])
        .notes("a v1 transaction envelope with 21 decorated signatures, one past signatures<20>."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));

    let mut p = Parts::plain()?;
    p.memo = cat(&[
        &enc(&x::MemoType::Text)?,
        &enc(&x::StringM::<{ u32::MAX }>::try_from(vec![b'm'; 29])?)?,
    ]);
    out.push(item(
        max(Spec::new(
            "classic.max-length",
            "memo-text-29-bytes",
            EntryKind::Xdr,
        ))
        .fields(&["tx.memo.text"])
        .notes("a memo text of 29 bytes, one past string text<28>."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));

    let mut p = Parts::plain()?;
    p.sigs = cat(&[
        &enc(&1u32)?,
        &enc(&x::SignatureHint([7; 4]))?,
        &enc(&unbounded(vec![7u8; 65])?)?,
    ]);
    out.push(item(
        max(Spec::new(
            "classic.max-length",
            "signature-65-bytes",
            EntryKind::Xdr,
        ))
        .fields(&["tx.signatures"])
        .notes("one decorated signature of 65 bytes, one past typedef opaque Signature<64>."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));

    // path payment with six path assets: the op body is composed field by field
    let path: Vec<x::Asset> = (0..6).map(|i| credit4(b"PATH", 10 + i)).collect();
    let body = cat(&[
        &enc(&x::OperationType::PathPaymentStrictReceive)?,
        &enc(&x::Asset::Native)?,
        &enc(&1000i64)?,
        &enc(&muxed(2))?,
        &enc(&credit4(b"USD\0", 3))?,
        &enc(&1i64)?,
        &enc(&unbounded(path)?)?,
    ]);
    let mut p = Parts::plain()?;
    p.ops = cat(&[&enc(&1u32)?, &enc(&0u32)?, &body]);
    out.push(item(
        max(Spec::new(
            "classic.max-length",
            "path-6-assets",
            EntryKind::Xdr,
        ))
        .fields(&["op.path_payment.path"])
        .notes("a path payment strict receive with 6 path assets, one past path<5>."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));

    // three extra signers in preconditions v2
    let signers: Vec<x::SignerKey> = (0..3)
        .map(|i| x::SignerKey::HashX(x::Uint256(key(20 + i))))
        .collect();
    let mut p = Parts::plain()?;
    p.cond = cat(&[
        &enc(&x::PreconditionType::V2)?,
        &enc(&None::<x::TimeBounds>)?,
        &enc(&None::<x::LedgerBounds>)?,
        &enc(&None::<x::SequenceNumber>)?,
        &enc(&x::Duration(0))?,
        &enc(&0u32)?,
        &enc(&unbounded(signers)?)?,
    ]);
    out.push(item(
        max(Spec::new(
            "cap21.preconditions",
            "extra-signers-3",
            EntryKind::Xdr,
        ))
        .flags(&["max-length-vec"])
        .fields(&["tx.cond.extra_signers"])
        .notes("preconditions v2 with 3 extra signers, one past extraSigners<2>."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));

    // a set-options signer whose signed payload is 65 bytes
    let signer_key = cat(&[
        &enc(&x::SignerKeyType::Ed25519SignedPayload)?,
        &enc(&x::Uint256(key(30)))?,
        &enc(&unbounded(vec![0xabu8; 65])?)?,
    ]);
    let body = cat(&[
        &enc(&x::OperationType::SetOptions)?,
        &enc(&None::<x::AccountId>)?,
        &enc(&None::<u32>)?,
        &enc(&None::<u32>)?,
        &enc(&None::<u32>)?,
        &enc(&None::<u32>)?,
        &enc(&None::<u32>)?,
        &enc(&None::<u32>)?,
        &enc(&None::<x::String32>)?,
        &enc(&1u32)?,
        &signer_key,
        &enc(&1u32)?,
    ]);
    let mut p = Parts::plain()?;
    p.ops = cat(&[&enc(&1u32)?, &enc(&0u32)?, &body]);
    out.push(item(
        max(Spec::new("cap40.signed-payload-signer", "payload-65-bytes", EntryKind::Xdr))
            .flags(&["signed-payload"])
            .fields(&["op.set_options.signer", "signer_key.signed_payload.payload"])
            .notes("a set-options signer of type ed25519 signed payload carrying 65 payload bytes, one past opaque payload<64> (CAP-40)."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));

    // manage data with a 65-byte value
    let body = cat(&[
        &enc(&x::OperationType::ManageData)?,
        &enc(&x::String64::try_from(b"k".to_vec())?)?,
        &enc(&1u32)?,
        &enc(&unbounded(vec![0x5au8; 65])?)?,
    ]);
    let mut p = Parts::plain()?;
    p.ops = cat(&[&enc(&1u32)?, &enc(&0u32)?, &body]);
    out.push(item(
        max(Spec::new(
            "classic.max-length",
            "data-value-65-bytes",
            EntryKind::Xdr,
        ))
        .fields(&["op.manage_data.data_value"])
        .notes("a manage data value of 65 bytes, one past typedef opaque DataValue<64>."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));

    // create claimable balance with eleven claimants
    let claimants: Vec<x::Claimant> = (0..11)
        .map(|i| {
            x::Claimant::ClaimantTypeV0(x::ClaimantV0 {
                destination: account(40 + i),
                predicate: x::ClaimPredicate::Unconditional,
            })
        })
        .collect();
    let body = cat(&[
        &enc(&x::OperationType::CreateClaimableBalance)?,
        &enc(&x::Asset::Native)?,
        &enc(&10i64)?,
        &enc(&unbounded(claimants)?)?,
    ]);
    let mut p = Parts::plain()?;
    p.ops = cat(&[&enc(&1u32)?, &enc(&0u32)?, &body]);
    out.push(item(
        max(Spec::new(
            "cap23.claimable-balances",
            "claimants-11",
            EntryKind::Xdr,
        ))
        .fields(&["op.create_claimable_balance.claimants.destination"])
        .notes("a create claimable balance with 11 claimants, one past claimants<10>."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));

    // a 33-byte symbol
    let sym = cat(&[
        &enc(&x::ScValType::Symbol)?,
        &enc(&x::StringM::<{ u32::MAX }>::try_from(vec![b's'; 33])?)?,
    ]);
    out.push(item(
        Spec::new("soroban.scval-shapes", "symbol-33-bytes", EntryKind::Xdr)
            .reject(Validity::NotEmittable)
            .flags(&["over-max-length"])
            .notes("an ScVal symbol of 33 bytes, one past SCSYMBOL_LIMIT = 32 (scval.symbol.len)."),
        "ScVal",
        sym,
        ENVELOPE_DEPTH,
    ));

    // the advisory bytes, and the same bytes rebuilt from their fields
    let raw = base64_decode(GHSA_3XHM_BASE64)?;
    let rebuilt = cat(&[
        &enc(&x::StringM::<{ u32::MAX }>::default())?,
        &enc(&x::ScSymbol::try_from(b"f".to_vec())?)?,
        &enc(&unbounded(Vec::<x::ScSpecFunctionInputV0>::new())?)?,
        &enc(&unbounded(vec![
            x::ScSpecTypeDef::U32,
            x::ScSpecTypeDef::U32,
        ])?)?,
    ]);
    ensure!(
        raw == rebuilt,
        "the GHSA-3xhm-p452-9wjh bytes are not doc \"\", name f, no inputs, outputs u32 u32"
    );
    out.push(item(
        Spec::new("soroban.invoke-basics", "ghsa-3xhm-spec-outputs-2", EntryKind::Xdr)
            .reject(Validity::NotEmittable)
            .flags(&["over-max-length", "ghsa"])
            .links(&[LINK_GHSA_3XHM])
            .notes("the GHSA-3xhm-p452-9wjh bytes AAAAAAAAAAFmAAAAAAAAAAAAAAIAAAAEAAAABA== as ScSpecFunctionV0: two outputs where outputs<1> allows one. stellar-xdr 28.0.0 produced them from JSON and then could not decode them. contract specs live in uploaded wasm, so the entry sits with the wasm upload family."),
        "ScSpecFunctionV0",
        raw,
        ENVELOPE_DEPTH,
    ));
    Ok(())
}

/// Standard base64 with padding, as the advisory prints it.
pub fn base64_decode(text: &str) -> Result<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let body = text.trim_end_matches('=');
    ensure!(
        text.len().is_multiple_of(4) && text.len() - body.len() <= 2,
        "{text} is not padded base64"
    );
    let mut bits = 0u32;
    let mut held = 0u32;
    let mut out = Vec::new();
    for c in body.bytes() {
        let v = ALPHABET
            .iter()
            .position(|&a| a == c)
            .with_context(|| format!("{c:?} is not a base64 character"))?;
        bits = (bits << 6) | v as u32;
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
            bits &= (1 << held) - 1;
        }
    }
    ensure!(bits == 0, "{text} has non-zero trailing bits");
    Ok(out)
}

fn soroban_ext() -> x::TransactionExt {
    x::TransactionExt::V1(x::SorobanTransactionData {
        ext: x::SorobanTransactionDataExt::V0,
        resources: x::SorobanResources {
            footprint: x::LedgerFootprint {
                read_only: x::VecM::default(),
                read_write: x::VecM::default(),
            },
            instructions: 1_000_000,
            disk_read_bytes: 0,
            write_bytes: 0,
        },
        resource_fee: 100_000,
    })
}

fn contract(n: u8) -> x::ScAddress {
    x::ScAddress::Contract(x::ContractId(x::Hash(key(n))))
}

fn invoke_args(args: Vec<x::ScVal>) -> Result<x::InvokeContractArgs> {
    Ok(x::InvokeContractArgs {
        contract_address: contract(50),
        function_name: x::ScSymbol::try_from(b"deep".to_vec())?,
        args: args.try_into()?,
    })
}

fn invoke_envelope(
    args: Vec<x::ScVal>,
    auth: Vec<x::SorobanAuthorizationEntry>,
) -> Result<Vec<u8>> {
    let body = x::OperationBody::InvokeHostFunction(x::InvokeHostFunctionOp {
        host_function: x::HostFunction::InvokeContract(invoke_args(args)?),
        auth: auth.try_into()?,
    });
    let tx = transaction(
        vec![op(body)],
        x::Memo::None,
        x::Preconditions::None,
        soroban_ext(),
    )?;
    enc(&envelope(tx, vec![sig(7)?])?)
}

pub(crate) fn nested_vec(levels: u32) -> Result<x::ScVal> {
    let mut v = x::ScVal::U32(0);
    for _ in 0..levels {
        v = x::ScVal::Vec(Some(x::ScVec(vec![v].try_into()?)));
    }
    Ok(v)
}

fn nested_map(levels: u32) -> Result<x::ScVal> {
    let mut v = x::ScVal::U32(0);
    for _ in 0..levels {
        let entry = x::ScMapEntry {
            key: x::ScVal::U32(0),
            val: v,
        };
        v = x::ScVal::Map(Some(x::ScMap(vec![entry].try_into()?)));
    }
    Ok(v)
}

fn auth_chain(levels: u32) -> Result<x::SorobanAuthorizationEntry> {
    let leaf = || -> Result<x::SorobanAuthorizedFunction> {
        Ok(x::SorobanAuthorizedFunction::ContractFn(invoke_args(
            vec![],
        )?))
    };
    let mut node = x::SorobanAuthorizedInvocation {
        function: leaf()?,
        sub_invocations: x::VecM::default(),
    };
    for _ in 1..levels {
        node = x::SorobanAuthorizedInvocation {
            function: leaf()?,
            sub_invocations: vec![node].try_into()?,
        };
    }
    Ok(x::SorobanAuthorizationEntry {
        credentials: x::SorobanCredentials::SourceAccount,
        root_invocation: node,
    })
}

/// Decodes `bytes` as the named type directly, the way core and the host do
/// (`TransactionEnvelope::read_xdr` in core's `can_parse_transaction`), rejecting trailing
/// bytes. The toolkit's `xdr::decode` goes through `Type::read_xdr_to_end`, whose dispatch
/// spends one extra depth level, so at the same `Limits` it is one level stricter.
pub fn decode_direct(
    ty: &str,
    bytes: &[u8],
    limits: Limits,
) -> Result<std::result::Result<(), x::Error>> {
    use stellar_xdr::ReadXdr;
    macro_rules! direct {
        ($($t:ident),* $(,)?) => {
            match ty {
                $(stringify!($t) => Ok(x::$t::from_xdr(bytes, limits).map(|_| ())),)*
                other => anyhow::bail!("no direct decoder for {other}"),
            }
        };
    }
    direct!(
        TransactionEnvelope,
        ScVal,
        ContractEvent,
        SorobanAuthorizationEntry,
        SignerKey,
        Asset,
        ScSpecFunctionV0,
        LedgerCloseMeta,
    )
}

/// How a depth probe decodes.
#[derive(Clone, Copy)]
enum Path {
    /// `decode_direct`, what core runs.
    Direct,
    /// the toolkit's `xdr::decode`, what `manifest validate` runs.
    Toolkit,
}

/// The largest `n` for which `build(n)` decodes as `ty` at `depth`.
fn deepest(
    ty: &str,
    depth: u32,
    path: Path,
    build: &dyn Fn(u32) -> Result<Vec<u8>>,
) -> Result<u32> {
    let decodes = |n: u32| -> Result<bool> {
        let bytes = build(n)?;
        let limits = Limits {
            depth,
            len: bytes.len(),
        };
        Ok(match path {
            Path::Direct => decode_direct(ty, &bytes, limits)?.is_ok(),
            Path::Toolkit => xdr::decode(ty, &bytes, limits).is_ok(),
        })
    };
    let (mut lo, mut hi) = (1u32, 4096u32);
    ensure!(decodes(lo)?, "a single level does not decode as {ty}");
    ensure!(!decodes(hi)?, "{hi} levels still decode as {ty}");
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if decodes(mid)? {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok(lo)
}

fn recommended(registry: &Registry, id: &str) -> Result<u32> {
    let limit = registry
        .limits
        .limits
        .iter()
        .find(|l| l.id == id)
        .with_context(|| format!("limit {id} is not in caps-limits.toml"))?;
    u32::try_from(limit.recommended).with_context(|| format!("limit {id} is out of range"))
}

// (caps-limits id, family, shape, builder for n levels, description with {n})
type DepthCase<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a dyn Fn(u32) -> Result<Vec<u8>>,
    &'a str,
);

fn depth(registry: &Registry, out: &mut Vec<XdrItem>) -> Result<()> {
    let envelope_depth = limits_for(registry, ENVELOPE_DEPTH, 0)?.depth;
    let cases: [DepthCase; 3] = [
        (
            "scval.depth.envelope",
            "soroban.scval-shapes",
            "scvec",
            &|n| invoke_envelope(vec![nested_vec(n)?], vec![]),
            "an invoke-contract argument nested {n} ScVec levels deep",
        ),
        (
            "scval.depth.envelope",
            "soroban.scval-shapes",
            "scmap",
            &|n| invoke_envelope(vec![nested_map(n)?], vec![]),
            "an invoke-contract argument nested {n} ScMap levels deep",
        ),
        (
            "auth.invocation.depth",
            "soroban.auth-trees",
            "auth",
            &|n| invoke_envelope(vec![], vec![auth_chain(n)?]),
            "a source-account authorization entry whose invocation tree is a single chain {n} invocations deep",
        ),
    ];
    for (limit_id, family, what, build, text) in cases {
        let deepest_core = deepest("TransactionEnvelope", envelope_depth, Path::Direct, build)?;
        let deepest_toolkit = deepest("TransactionEnvelope", envelope_depth, Path::Toolkit, build)?;
        // the scvec cap in caps-limits is the measured scvec depth; the map bound is derived
        // from the same measurement, so each shape is checked against its own measurement
        let cap = match what {
            "scmap" => {
                let vec_cap = recommended(registry, limit_id)?;
                ensure!(
                    deepest_core <= vec_cap,
                    "ScMap nesting decodes deeper ({deepest_core}) than the ScVec cap {vec_cap}"
                );
                deepest_core
            }
            _ => recommended(registry, limit_id)?,
        };
        ensure!(
            deepest_core == cap,
            "{family} {what}: the deepest envelope that decodes directly at depth {envelope_depth} nests {deepest_core}, but {limit_id} says {cap}"
        );
        let fields: &[&str] = match what {
            "auth" => &["op.invoke_host_function.auth", "auth.root_invocation"],
            _ => &["op.invoke_host_function.invoke_contract.args"],
        };
        let flags: &[&str] = match what {
            "auth" => &["nested-auth", "deep-nesting"],
            "scmap" => &["deep-nesting", "nested-map"],
            _ => &["deep-nesting", "nested-vec"],
        };
        let over = cap + 1;
        out.push(item(
            Spec::new(family, format!("{what}-depth-{over}-over-cap"), EntryKind::Xdr)
                .reject(Validity::InvalidByCore)
                .flags(flags)
                .flags(&["over-depth-cap"])
                .fields(fields)
                .notes(format!(
                    "{}, one past the deepest that decodes inside an envelope at depth {envelope_depth} ({limit_id} = {}). core refuses the envelope from protocol 28 because its rust parser (TransactionEnvelope::read_xdr in can_parse_transaction) fails at depth 1000.",
                    text.replace("{n}", &over.to_string()),
                    recommended(registry, limit_id)?,
                ))
                .valid_from(28),
            "TransactionEnvelope",
            build(over)?,
            ENVELOPE_DEPTH,
        ));
        // the accepted twin has to pass `manifest validate`, which decodes through the
        // toolkit; when that path is one level stricter than core, the twin is one shallower
        let twin = deepest_toolkit.min(cap);
        let gap = if twin < cap {
            format!(
                " core's own decode accepts {cap} levels ({limit_id}); this entry stops at {twin} because shapekit's toolkit decode (Type::read_xdr_to_end) spends one depth level on type dispatch and refuses {cap} at depth {envelope_depth}."
            )
        } else {
            String::new()
        };
        out.push(item(
            Spec::new(family, format!("{what}-depth-{twin}-deepest-accepted"), EntryKind::Xdr)
                .flags(flags)
                .flags(&["at-depth-cap"])
                .fields(fields)
                .notes(format!(
                    "{}, at the deepest an envelope decodes at depth {envelope_depth}.{gap} core's depth check accepts it; the host converts at most 100 levels (scval.depth) and runs at most 100 frames, so an invocation this deep fails and the transaction lands in the ledger as failed with the full tree intact. a decoder must accept it.",
                    text.replace("{n}", &twin.to_string()),
                ))
                .valid_from(28),
            "TransactionEnvelope",
            build(twin)?,
            ENVELOPE_DEPTH,
        ));
    }
    Ok(())
}

fn malformed(out: &mut Vec<XdrItem>) -> Result<()> {
    let plain = Parts::plain()?.compose()?;

    // truncation
    let mut t = plain.clone();
    t.pop();
    out.push(item(
        Spec::new(
            "classic.max-length",
            "truncated-envelope-last-byte",
            EntryKind::Xdr,
        )
        .reject(Validity::InvalidByCore)
        .flags(&["truncated"])
        .notes("a one-payment v1 envelope with its final byte cut off, inside the last signature."),
        "TransactionEnvelope",
        t,
        ENVELOPE_DEPTH,
    ));
    out.push(item(
        Spec::new(
            "classic.max-length",
            "truncated-envelope-discriminant",
            EntryKind::Xdr,
        )
        .reject(Validity::InvalidByCore)
        .flags(&["truncated"])
        .notes(
            "the first three bytes of a v1 envelope: the envelope type discriminant is cut short.",
        ),
        "TransactionEnvelope",
        plain[..3].to_vec(),
        ENVELOPE_DEPTH,
    ));
    let i128v = enc(&x::ScVal::I128(x::Int128Parts { hi: -1, lo: 5 }))?;
    out.push(item(
        Spec::new("soroban.scval-shapes", "truncated-i128", EntryKind::Xdr)
            .reject(Validity::NotEmittable)
            .flags(&["truncated"])
            .notes("an ScVal i128 with the last four bytes of its low half missing."),
        "ScVal",
        i128v[..i128v.len() - 4].to_vec(),
        ENVELOPE_DEPTH,
    ));
    let auth = enc(&auth_chain(3)?)?;
    out.push(item(
        Spec::new("soroban.auth-trees", "truncated-auth-entry", EntryKind::Xdr)
            .reject(Validity::NotEmittable)
            .flags(&["truncated", "nested-auth"])
            .notes(
                "a three-level authorization entry cut in the middle of its innermost invocation.",
            ),
        "SorobanAuthorizationEntry",
        auth[..auth.len() / 2 + auth.len() / 4].to_vec(),
        ENVELOPE_DEPTH,
    ));

    // non-zero padding
    let mut p = Parts::plain()?;
    p.memo = cat(&[&enc(&x::MemoType::Text)?, &enc(&1u32)?, b"a", &[1, 0, 0]]);
    out.push(item(
        Spec::new("classic.memo-bytes", "memo-text-nonzero-padding", EntryKind::Xdr)
            .reject(Validity::InvalidByCore)
            .flags(&["non-zero-padding"])
            .fields(&["tx.memo.text"])
            .notes("a one-byte memo text whose first padding byte is 0x01; XDR requires padding bytes to be zero."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));
    out.push(item(
        Spec::new(
            "soroban.scval-shapes",
            "bytes-nonzero-padding",
            EntryKind::Xdr,
        )
        .reject(Validity::NotEmittable)
        .flags(&["non-zero-padding"])
        .notes("an ScVal bytes value of length 5 whose last padding byte is 0xff."),
        "ScVal",
        cat(&[
            &enc(&x::ScValType::Bytes)?,
            &enc(&5u32)?,
            &[1, 2, 3, 4, 5, 0, 0, 0xff],
        ]),
        ENVELOPE_DEPTH,
    ));
    out.push(item(
        Spec::new(
            "soroban.scval-shapes",
            "string-nonzero-padding",
            EntryKind::Xdr,
        )
        .reject(Validity::NotEmittable)
        .flags(&["non-zero-padding"])
        .notes("an ScVal string \"abc\" whose padding byte is 0x20."),
        "ScVal",
        cat(&[&enc(&x::ScValType::String)?, &enc(&3u32)?, b"abc", &[0x20]]),
        ENVELOPE_DEPTH,
    ));
    out.push(item(
        Spec::new(
            "cap40.signed-payload-signer",
            "signer-key-nonzero-padding",
            EntryKind::Xdr,
        )
        .reject(Validity::NotEmittable)
        .flags(&["non-zero-padding", "signed-payload"])
        .notes("an ed25519 signed-payload signer key with a 1-byte payload and padding 00 00 01."),
        "SignerKey",
        cat(&[
            &enc(&x::SignerKeyType::Ed25519SignedPayload)?,
            &enc(&x::Uint256(key(30)))?,
            &enc(&1u32)?,
            &[0x42, 0, 0, 1],
        ]),
        ENVELOPE_DEPTH,
    ));

    // discriminants and enum values out of range
    let scval_types = x::ScValType::VARIANTS.len() as i32;
    out.push(item(
        Spec::new("soroban.scval-shapes", "scval-type-out-of-range", EntryKind::Xdr)
            .reject(Validity::NotEmittable)
            .flags(&["bad-discriminant"])
            .notes(format!("an ScVal whose type discriminant is {scval_types}, one past the last SCValType (SCV_EXECUTABLE_TAG = {}).", scval_types - 1)),
        "ScVal",
        cat(&[&enc(&scval_types)?, &enc(&0u32)?]),
        ENVELOPE_DEPTH,
    ));
    let mut p = Parts::plain()?;
    p.memo = enc(&5i32)?;
    out.push(item(
        Spec::new(
            "classic.memo-bytes",
            "memo-type-out-of-range",
            EntryKind::Xdr,
        )
        .reject(Validity::InvalidByCore)
        .flags(&["bad-discriminant"])
        .fields(&["tx.memo.type"])
        .notes("a transaction memo with MemoType 5; the enum ends at MEMO_RETURN = 4."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));
    let mut t = plain.clone();
    t[..4].copy_from_slice(&enc(&x::EnvelopeType::Scp)?);
    out.push(item(
        Spec::new("cap15.fee-bump", "envelope-type-scp", EntryKind::Xdr)
            .reject(Validity::InvalidByCore)
            .flags(&["bad-discriminant"])
            .caps(&["CAP-0015", "CAP-0019"])
            .notes("a transaction envelope whose type is ENVELOPE_TYPE_SCP = 1: a valid EnvelopeType value with no arm in TransactionEnvelope, followed by a v1 body."),
        "TransactionEnvelope",
        t,
        ENVELOPE_DEPTH,
    ));
    out.push(item(
        Spec::new("cap38.liquidity-pools", "asset-type-pool-share", EntryKind::Xdr)
            .reject(Validity::NotEmittable)
            .flags(&["bad-discriminant"])
            .notes("an Asset with type ASSET_TYPE_POOL_SHARE = 3, which only ChangeTrustAsset and TrustLineAsset have an arm for."),
        "Asset",
        cat(&[&enc(&x::AssetType::PoolShare)?, &enc(&x::Hash(key(60)))?]),
        ENVELOPE_DEPTH,
    ));
    let ops = x::OperationType::VARIANTS.len() as i32;
    let mut p = Parts::plain()?;
    p.ops = cat(&[&enc(&1u32)?, &enc(&0u32)?, &enc(&ops)?]);
    out.push(item(
        Spec::new(
            "classic.max-length",
            "operation-type-out-of-range",
            EntryKind::Xdr,
        )
        .reject(Validity::InvalidByCore)
        .flags(&["bad-discriminant"])
        .fields(&["tx.operations"])
        .notes(format!(
            "an operation of type {ops}, one past RESTORE_FOOTPRINT = {}.",
            ops - 1
        )),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));
    let addresses = x::ScAddressType::VARIANTS.len() as i32;
    out.push(item(
        Spec::new("cap84.muxed-contract", "scaddress-type-out-of-range", EntryKind::Xdr)
            .reject(Validity::NotEmittable)
            .flags(&["bad-discriminant"])
            .notes(format!("an ScVal address of ScAddressType {addresses}, one past SC_ADDRESS_TYPE_MUXED_CONTRACT = {}.", addresses - 1)),
        "ScVal",
        cat(&[&enc(&x::ScValType::Address)?, &enc(&addresses)?, &enc(&x::Hash(key(61)))?]),
        ENVELOPE_DEPTH,
    ));
    let event_types = x::ContractEventType::VARIANTS.len() as i32;
    out.push(item(
        Spec::new("cap67.unified-events", "event-type-out-of-range", EntryKind::Xdr)
            .reject(Validity::NotEmittable)
            .flags(&["bad-enum-value"])
            .notes(format!("a contract event whose ContractEventType field is {event_types}; the enum ends at DIAGNOSTIC = {}.", event_types - 1)),
        "ContractEvent",
        cat(&[
            &enc(&0i32)?,
            &enc(&None::<x::ContractId>)?,
            &enc(&event_types)?,
            &enc(&0i32)?,
            &enc(&unbounded(Vec::<x::ScVal>::new())?)?,
            &enc(&x::ScVal::Void)?,
        ]),
        ENVELOPE_DEPTH,
    ));
    out.push(item(
        Spec::new("soroban.scval-shapes", "bool-value-2", EntryKind::Xdr)
            .implementation_defined(Validity::NotEmittable)
            .flags(&["bad-enum-value", "references-disagree"])
            .notes("an ScVal bool encoded as 2. RFC 4506 defines bool as an enum of FALSE = 0 and TRUE = 1, but neither reference rejects it: xdrpp, which core uses, maps any non-zero value to TRUE (xdr_traits<bool>::from_uint is u != 0, so validate_enum sees true), while stellar-xdr maps anything but 1 to false. the same bytes are therefore true to core and false to rust consumers, and no conforming encoder writes them. a decoder should refuse them; the entry is implementation-defined because the references do not."),
        "ScVal",
        cat(&[&enc(&x::ScValType::Bool)?, &enc(&2u32)?]),
        ENVELOPE_DEPTH,
    ));
    let mut p = Parts::plain()?;
    p.ops = cat(&[&enc(&1u32)?, &enc(&2u32)?, &enc(&payment(2).body)?]);
    out.push(item(
        Spec::new("cap27.muxed-accounts", "optional-flag-2", EntryKind::Xdr)
            .reject(Validity::InvalidByCore)
            .flags(&["bad-discriminant"])
            .fields(&["op.source_account"])
            .notes("an operation whose optional source account has presence flag 2; optional data is a bool of 0 or 1."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));
    let mut p = Parts::plain()?;
    p.ops = cat(&[
        &enc(&1u32)?,
        &enc(&1u32)?,
        &enc(&0x101i32)?,
        &enc(&x::Uint256(key(9)))?,
        &enc(&payment(2).body)?,
    ]);
    out.push(item(
        Spec::new("cap27.muxed-accounts", "crypto-key-type-out-of-range", EntryKind::Xdr)
            .reject(Validity::InvalidByCore)
            .flags(&["bad-discriminant", "muxed-account"])
            .fields(&["op.source_account"])
            .notes("an operation source MuxedAccount with CryptoKeyType 0x101, between KEY_TYPE_MUXED_ED25519 = 0x100 and nothing; MuxedAccount has arms only for 0 and 0x100."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));

    // length prefixes larger than what follows
    out.push(item(
        Spec::new("soroban.scval-shapes", "vec-count-overrun", EntryKind::Xdr)
            .reject(Validity::NotEmittable)
            .flags(&["length-prefix-overrun"])
            .links(&[LINK_RS_581])
            .notes("an ScVal vec claiming 0x7fffffff elements followed by one u32 element. a decoder that reserves capacity from the prefix allocates gigabytes before it fails (rs-stellar-xdr#581)."),
        "ScVal",
        cat(&[
            &enc(&x::ScValType::Vec)?,
            &enc(&1u32)?,
            &enc(&0x7fff_ffffu32)?,
            &enc(&x::ScVal::U32(1))?,
        ]),
        ENVELOPE_DEPTH,
    ));
    out.push(item(
        Spec::new("soroban.scval-shapes", "map-count-overrun", EntryKind::Xdr)
            .reject(Validity::NotEmittable)
            .flags(&["length-prefix-overrun"])
            .links(&[LINK_RS_581])
            .notes("an ScVal map claiming 16777216 entries with none present."),
        "ScVal",
        cat(&[
            &enc(&x::ScValType::Map)?,
            &enc(&1u32)?,
            &enc(&0x0100_0000u32)?,
        ]),
        ENVELOPE_DEPTH,
    ));
    out.push(item(
        Spec::new(
            "soroban.scval-shapes",
            "bytes-length-overrun",
            EntryKind::Xdr,
        )
        .reject(Validity::NotEmittable)
        .flags(&["length-prefix-overrun"])
        .links(&[LINK_RS_581])
        .notes("an ScVal bytes value claiming 0xffffffff bytes with four present."),
        "ScVal",
        cat(&[
            &enc(&x::ScValType::Bytes)?,
            &enc(&0xffff_ffffu32)?,
            &[1, 2, 3, 4],
        ]),
        ENVELOPE_DEPTH,
    ));
    let mut p = Parts::plain()?;
    p.ops = cat(&[&enc(&100u32)?, &enc(&payment(2))?]);
    out.push(item(
        Spec::new("classic.max-length", "operations-count-overrun", EntryKind::Xdr)
            .reject(Validity::InvalidByCore)
            .flags(&["length-prefix-overrun"])
            .links(&[LINK_RS_581])
            .fields(&["tx.operations"])
            .notes("a v1 envelope whose operation count says 100 (the maximum) with one operation present, followed by the ext and signatures."),
        "TransactionEnvelope",
        p.compose()?,
        ENVELOPE_DEPTH,
    ));
    Ok(())
}

fn event(body: x::ScVal) -> Result<x::ContractEvent> {
    Ok(x::ContractEvent {
        ext: x::ExtensionPoint::V0,
        contract_id: Some(x::ContractId(x::Hash(key(50)))),
        type_: x::ContractEventType::Contract,
        body: x::ContractEventBody::V0(x::ContractEventV0 {
            topics: vec![x::ScVal::Symbol(x::ScSymbol::try_from(b"shape".to_vec())?)].try_into()?,
            data: body,
        }),
    })
}

fn raw_map(entries: Vec<(x::ScVal, x::ScVal)>) -> Result<x::ScVal> {
    let entries: Vec<x::ScMapEntry> = entries
        .into_iter()
        .map(|(key, val)| x::ScMapEntry { key, val })
        .collect();
    Ok(x::ScVal::Map(Some(x::ScMap(entries.try_into()?))))
}

fn sym(s: &str) -> Result<x::ScVal> {
    Ok(x::ScVal::Symbol(x::ScSymbol::try_from(
        s.as_bytes().to_vec(),
    )?))
}

fn not_emittable(out: &mut Vec<XdrItem>) -> Result<()> {
    let nem = |s: Spec| s.validity(Validity::NotEmittable);

    let v0 = x::TransactionEnvelope::TxV0(x::TransactionV0Envelope {
        tx: x::TransactionV0 {
            source_account_ed25519: x::Uint256(key(1)),
            fee: FEE,
            seq_num: x::SequenceNumber(SEQ),
            time_bounds: None,
            memo: x::Memo::None,
            operations: vec![payment(2)].try_into()?,
            ext: x::TransactionV0Ext::V0,
        },
        signatures: vec![sig(7)?].try_into()?,
    });
    out.push(item(
        nem(Spec::new("cap15.fee-bump", "envelope-tx-v0-in-ledger", EntryKind::Xdr))
            .flags(&["legacy-envelope"])
            .caps(&["CAP-0015", "CAP-0019"])
            .valid_from(13)
            .notes("an ENVELOPE_TYPE_TX_V0 envelope. from protocol 13 core answers txNOT_SUPPORTED for a v0 envelope in a transaction set (TransactionFrame::commonValidPreSeqNum) and converts v0 submissions to v1 before they enter the queue (txbridge::convertForV13 in CommandHandler::tx), so no ledger at protocol 13 or later carries one. it decodes, and must be accepted as data."),
        "TransactionEnvelope",
        enc(&v0)?,
        ENVELOPE_DEPTH,
    ));

    let unsorted = raw_map(vec![
        (sym("b")?, x::ScVal::U32(1)),
        (sym("a")?, x::ScVal::U32(2)),
    ])?;
    out.push(item(
        nem(Spec::new("soroban.scval-shapes", "event-map-unsorted", EntryKind::Xdr))
            .flags(&["unsorted-map"])
            .links(&[LINK_JS_1766])
            .notes("a contract event whose data is a map with keys symbol b then symbol a. the host writes maps only from its sorted host objects, so it never emits keys out of order; an indexer that re-sorts or rejects them must not lose the event."),
        "ContractEvent",
        enc(&event(unsorted)?)?,
        ENVELOPE_DEPTH,
    ));
    let dup = raw_map(vec![
        (sym("a")?, x::ScVal::U32(1)),
        (sym("a")?, x::ScVal::U32(2)),
    ])?;
    out.push(item(
        nem(Spec::new("soroban.scval-shapes", "event-map-duplicate-key", EntryKind::Xdr))
            .flags(&["duplicate-map-key"])
            .notes("a contract event whose data map has the key symbol a twice. host maps have unique keys, so the host never emits this; a keyed projection would keep one of the two values."),
        "ContractEvent",
        enc(&event(dup)?)?,
        ENVELOPE_DEPTH,
    ));
    out.push(item(
        nem(Spec::new("soroban.scval-shapes", "event-vec-depth-101", EntryKind::Xdr))
            .flags(&["deep-nesting", "nested-vec"])
            .notes("a contract event whose data nests 101 ScVec levels. every ScVal the host writes passes from_host_val, which fails past DEFAULT_HOST_DEPTH_LIMIT = 100 (scval.depth), so meta never carries this. the bytes decode well inside xdr.meta.depth."),
        "ContractEvent",
        enc(&event(nested_vec(101)?)?)?,
        META_DEPTH,
    ));
    out.push(item(
        nem(Spec::new("soroban.scval-shapes", "event-vec-absent", EntryKind::Xdr))
            .flags(&["absent-container"])
            .notes("a contract event whose data is an ScVal vec with the optional body absent (SCV_VEC with no SCVec). the host has no representation for it and refuses to convert or compare it, so it never emits one."),
        "ContractEvent",
        enc(&event(x::ScVal::Vec(None))?)?,
        ENVELOPE_DEPTH,
    ));

    // a v1 meta whose transaction set holds one transaction but whose processing list is empty
    let mut lcm = x::LedgerCloseMetaV1::default();
    lcm.ledger_header.header.ledger_version = 30;
    lcm.ledger_header.header.ledger_seq = 2;
    let env = x::TransactionEnvelope::Tx(x::TransactionV1Envelope {
        tx: transaction(
            vec![payment(2)],
            x::Memo::None,
            x::Preconditions::None,
            x::TransactionExt::V0,
        )?,
        signatures: vec![sig(7)?].try_into()?,
    });
    let classic = x::TransactionPhase::V0(
        vec![x::TxSetComponent::TxsetCompTxsMaybeDiscountedFee(
            x::TxSetComponentTxsMaybeDiscountedFee {
                base_fee: Some(100),
                txs: vec![env].try_into()?,
            },
        )]
        .try_into()?,
    );
    let soroban = x::TransactionPhase::V0(x::VecM::default());
    lcm.tx_set = x::GeneralizedTransactionSet::V1(x::TransactionSetV1 {
        previous_ledger_hash: x::Hash(key(70)),
        phases: vec![classic, soroban].try_into()?,
    });
    out.push(item(
        nem(Spec::new("cap42.generalized-txset", "lcm-count-mismatch", EntryKind::Xdr))
            .flags(&["count-mismatch"])
            .notes("a LedgerCloseMeta v1 whose generalized transaction set carries one transaction while txProcessing is empty. core writes one TransactionResultMeta per applied transaction, so the counts always match in real meta; an indexer that zips the two lists must not panic or drop the ledger silently."),
        "LedgerCloseMeta",
        enc(&x::LedgerCloseMeta::V1(lcm))?,
        META_DEPTH,
    ));
    Ok(())
}

pub(crate) fn r14_issuers() -> (x::AccountId, x::AccountId) {
    (account(0x00), account(0x3a))
}

fn invalid_by_core(out: &mut Vec<XdrItem>) -> Result<()> {
    let bad = |s: Spec| s.validity(Validity::InvalidByCore);

    // the js-stellar-sdk#1766 issuers in strkey order, which is the wrong order for core
    let (zeros, colons) = r14_issuers();
    let usd = |issuer: x::AccountId| {
        x::Asset::CreditAlphanum4(x::AlphaNum4 {
            asset_code: x::AssetCode4(*b"USD\0"),
            issuer,
        })
    };
    let line =
        x::ChangeTrustAsset::PoolShare(x::LiquidityPoolParameters::LiquidityPoolConstantProduct(
            x::LiquidityPoolConstantProductParameters {
                asset_a: usd(colons),
                asset_b: usd(zeros),
                fee: 30,
            },
        ));
    let tx = transaction(
        vec![op(x::OperationBody::ChangeTrust(x::ChangeTrustOp {
            line,
            limit: i64::MAX,
        }))],
        x::Memo::None,
        x::Preconditions::None,
        x::TransactionExt::V0,
    )?;
    out.push(item(
        bad(Spec::new("cap38.liquidity-pools", "change-trust-strkey-order", EntryKind::Xdr))
            .flags(&["issuer-order-differs"])
            .links(&[LINK_JS_1766, LINK_PY_1246])
            .fields(&["op.change_trust.line.liquidity_pool"])
            .notes("a pool-share change trust whose USD assets are ordered by issuer strkey text (GA5D... before GAAA...). core requires assetA < assetB by raw key bytes (isPoolShareAssetValid), so this is CHANGE_TRUST_MALFORMED and never reaches a ledger."),
        "TransactionEnvelope",
        enc(&envelope(tx, vec![sig(7)?])?)?,
        ENVELOPE_DEPTH,
    ));

    let tx = transaction(
        vec![op(x::OperationBody::SetOptions(x::SetOptionsOp {
            inflation_dest: None,
            clear_flags: None,
            set_flags: None,
            master_weight: None,
            low_threshold: None,
            med_threshold: None,
            high_threshold: None,
            home_domain: None,
            signer: Some(x::Signer {
                key: x::SignerKey::Ed25519SignedPayload(x::SignerKeyEd25519SignedPayload {
                    ed25519: x::Uint256(key(30)),
                    payload: x::BytesM::default(),
                }),
                weight: 1,
            }),
        }))],
        x::Memo::None,
        x::Preconditions::None,
        x::TransactionExt::V0,
    )?;
    out.push(item(
        bad(Spec::new("cap40.signed-payload-signer", "empty-payload-signer", EntryKind::Xdr))
            .flags(&["signed-payload"])
            .fields(&["op.set_options.signer", "signer_key.signed_payload.payload"])
            .notes("a set-options signer of type ed25519 signed payload with an empty payload. the XDR allows 0 to 64 bytes, but core answers SET_OPTIONS_BAD_SIGNER for an empty one (SetOptionsOpFrame::doCheckValid), so it never reaches a ledger."),
        "TransactionEnvelope",
        enc(&envelope(tx, vec![sig(7)?])?)?,
        ENVELOPE_DEPTH,
    ));

    let tx = transaction(
        vec![op(x::OperationBody::ManageSellOffer(
            x::ManageSellOfferOp {
                selling: x::Asset::Native,
                buying: credit4(b"USD\0", 3),
                amount: 10,
                price: x::Price { n: 1, d: 1 },
                offer_id: -16,
            },
        ))],
        x::Memo::None,
        x::Preconditions::None,
        x::TransactionExt::V0,
    )?;
    out.push(item(
        bad(Spec::new("cap03.offer-liabilities", "negative-offer-id", EntryKind::Xdr))
            .flags(&["non-canonical-id"])
            .links(&[LINK_JS_1785])
            .fields(&["op.manage_offer.offer_id"])
            .valid_from(15)
            .notes("a manage sell offer with offerID -16. the XDR type is int64, but from protocol 15 core answers MANAGE_SELL_OFFER_MALFORMED for a negative id (ManageOfferOpFrameBase::doCheckValid)."),
        "TransactionEnvelope",
        enc(&envelope(tx, vec![sig(7)?])?)?,
        ENVELOPE_DEPTH,
    ));
    Ok(())
}
