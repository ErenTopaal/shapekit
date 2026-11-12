//! `soroban.scval-shapes`: ScVal values passed through the echo contract,
//! which takes a value as an argument, stores it, emits it as event data and
//! returns it, so each value shows up in the envelope, a contract data
//! entry, an event and the return value.
//!
//! Recorded at protocol 30 with testnet's Soroban settings:
//!
//! - vec and map nesting at the deepest level the host accepts, found by
//!   simulation, and the one-deeper twin core includes and fails
//! - a map with one key of every convertible ScVal type, in host order, and
//!   the same entries reversed, which the host refuses to convert
//! - a map whose keys collide after native conversion (u32 1, i32 1, the
//!   other integer 1s, bytes "1", string "1", symbol "1")
//! - every integer type at its extremes
//! - a string that is not UTF-8 (accepted) and a symbol with the same kind
//!   of bytes (refused by the host)
//!
//! Twins the host refuses cannot be simulated, so they reuse the footprint
//! and resources of their valid sibling with a larger resource fee, and are
//! captured as whole ledgers: the failed transaction's meta carries no value.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::{Applied, ChainError};
use shapekit_core_driver::contracts::ECHO_WASM;
use shapekit_core_driver::keys::account_address;
use shapekit_core_driver::sim::SimError;
use stellar_xdr::{
    ContractId, Duration, HostFunction, Int128Parts, Int256Parts, InvokeContractArgs,
    InvokeHostFunctionResult, OperationResult, OperationResultTr, ScAddress, ScBytes, ScError,
    ScErrorCode, ScMap, ScMapEntry, ScString, ScSymbol, ScVal, ScVec, SorobanAuthorizationEntry,
    SorobanTransactionData, TimePoint, TransactionEnvelope, TransactionExt,
    TransactionResultResult, UInt128Parts, UInt256Parts,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "soroban.scval-shapes";

/// Starting balance of every funded account, 10,000 XLM.
pub(crate) const BALANCE: i64 = 100_000_000_000;

/// Margins over the deepest priced level for a nesting simulation cannot
/// size: each extra level adds a few dozen bytes to the stored entry.
const UNPRICED_WRITE_MARGIN: u32 = 4096;
const UNPRICED_FEE_MARGIN: i64 = 10_000_000;

/// Added to the resource fee of a twin that reuses its sibling's simulated
/// resources, so the fee for its slightly larger envelope is covered.
pub(crate) const TWIN_FEE_MARGIN: i64 = 1_000_000;

/// Nesting the depth search starts from as refused: one past
/// DEFAULT_HOST_DEPTH_LIMIT (100), checked to be refused.
const DEPTH_SEARCH_MAX: u32 = 101;

/// What simulation says about a host function.
enum Probe {
    /// Ran, and simulation priced it.
    Priced(SorobanTransactionData),
    /// The host ran it, but simulation could not size the transaction: it
    /// encodes a whole envelope under DEFAULT_XDR_RW_LIMITS (depth 500,
    /// soroban-simulation 30.0.0 `estimate_max_transaction_size_for_operation`),
    /// which a deeply nested argument passes before the host's own limits.
    Unpriced,
    /// The host refused it.
    Refused(String),
}

fn probe(ctx: &mut ChainCtx, source: &SigningKey, hf: HostFunction) -> Result<Probe> {
    match ctx.driver().soroban(source, hf) {
        Ok(env) => Ok(Probe::Priced(data_of(env)?)),
        Err(ChainError::Sim(SimError::InvocationFailed { error, .. })) => Ok(Probe::Refused(error)),
        Err(ChainError::Sim(SimError::Setup(m)))
            if m.starts_with("could not compute the maximum transaction size")
                && m.ends_with("depth limit exceeded") =>
        {
            Ok(Probe::Unpriced)
        }
        Err(e) => Err(e.into()),
    }
}

fn data_of(env: TransactionEnvelope) -> Result<SorobanTransactionData> {
    match env {
        TransactionEnvelope::Tx(v1) => match v1.tx.ext {
            TransactionExt::V1(data) => Ok(data),
            TransactionExt::V0 => bail!("an assembled soroban transaction without data"),
        },
        other => bail!("soroban() built a {:?} envelope", other.name()),
    }
}

const ARGS: [&str; 1] = ["op.invoke_host_function.invoke_contract.args"];

pub struct ScvalShapes {
    pub protocol: u32,
}

/// Uploads the echo contract from `deployer` and deploys one instance,
/// salted by `use_`.
pub(crate) fn deploy_echo(
    ctx: &mut ChainCtx,
    deployer: &SigningKey,
    use_: &str,
) -> Result<ContractId> {
    let code = ctx.driver().upload(deployer, ECHO_WASM)?;
    let salt: [u8; 32] = Sha256::digest(format!("shapekit/echo-instance/{use_}")).into();
    Ok(ctx.driver().deploy(deployer, &code, &salt)?)
}

pub(crate) fn invoke(
    contract: &ContractId,
    function: &str,
    args: Vec<ScVal>,
) -> Result<HostFunction> {
    Ok(HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: ScAddress::Contract(contract.clone()),
        function_name: ScSymbol(function.try_into()?),
        args: args.try_into()?,
    }))
}

/// Simulates `hf` and returns the transaction data simulation chose.
pub(crate) fn simulated_data(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    hf: HostFunction,
) -> Result<SorobanTransactionData> {
    data_of(ctx.driver().soroban(source, hf)?)
}

/// Whether simulation runs `hf` to completion. A host error is `false`;
/// anything else (node, fetch, setup) is an error.
pub(crate) fn simulates(ctx: &mut ChainCtx, source: &SigningKey, hf: HostFunction) -> Result<bool> {
    match ctx.driver().soroban(source, hf) {
        Ok(_) => Ok(true),
        Err(ChainError::Sim(SimError::InvocationFailed { .. })) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Submits `hf` with resources taken from a valid sibling (plus
/// [`TWIN_FEE_MARGIN`]) and requires core to include it and fail it.
pub(crate) fn apply_failing(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    hf: HostFunction,
    mut data: SorobanTransactionData,
    auth: Vec<SorobanAuthorizationEntry>,
) -> Result<Applied> {
    data.resource_fee += TWIN_FEE_MARGIN;
    let env = ctx.driver().soroban_with(source, hf, data, auth)?;
    ctx.submit_and_close_failing(&env)
}

/// The invoke result of a failed single-operation Soroban transaction.
pub(crate) fn invoke_failure(applied: &Applied) -> Result<&InvokeHostFunctionResult> {
    let TransactionResultResult::TxFailed(ops) = &applied.result.result else {
        bail!(
            "ledger {}: expected tx_failed, core answered {:?}",
            applied.ledger,
            applied.result.result
        );
    };
    match ops.first() {
        Some(OperationResult::OpInner(OperationResultTr::InvokeHostFunction(r))) => Ok(r),
        other => bail!(
            "ledger {}: unexpected operation result {other:?}",
            applied.ledger
        ),
    }
}

pub(crate) fn ensure_trapped(applied: &Applied, what: &str) -> Result<()> {
    let r = invoke_failure(applied)?;
    ensure!(
        matches!(r, InvokeHostFunctionResult::Trapped),
        "{what}: core answered {r:?}, expected invoke_host_function trapped"
    );
    Ok(())
}

fn sym(s: &str) -> Result<ScVal> {
    Ok(ScVal::Symbol(ScSymbol(s.try_into()?)))
}

fn string(b: &[u8]) -> Result<ScVal> {
    Ok(ScVal::String(ScString(b.to_vec().try_into()?)))
}

fn bytes(b: &[u8]) -> Result<ScVal> {
    Ok(ScVal::Bytes(ScBytes(b.to_vec().try_into()?)))
}

fn vec_of(items: Vec<ScVal>) -> Result<ScVal> {
    Ok(ScVal::Vec(Some(ScVec(items.try_into()?))))
}

fn entry(key: ScVal, val: ScVal) -> ScMapEntry {
    ScMapEntry { key, val }
}

/// `levels` vecs, each holding the next, around a u32 leaf: `levels + 1`
/// ScVal levels in all.
fn nested_vec(levels: u32) -> Result<ScVal> {
    let mut v = ScVal::U32(levels);
    for _ in 0..levels {
        v = vec_of(vec![v])?;
    }
    Ok(v)
}

/// `levels` maps, each holding the next under the key `v`, around a u32 leaf.
fn nested_map(levels: u32) -> Result<ScVal> {
    let mut v = ScVal::U32(levels);
    for _ in 0..levels {
        v = ScVal::Map(Some(ScMap(vec![entry(sym("v")?, v)].try_into()?)));
    }
    Ok(v)
}

/// One key of every ScVal type the host converts, each mapped to its type
/// name, in the order the host requires (ScValType, then content).
fn mixed_key_entries(addr: ScAddress) -> Result<Vec<ScMapEntry>> {
    let keys = vec![
        ("bool", ScVal::Bool(true)),
        ("void", ScVal::Void),
        ("error", ScVal::Error(ScError::Contract(7))),
        ("u32", ScVal::U32(7)),
        ("i32", ScVal::I32(-7)),
        ("u64", ScVal::U64(7)),
        ("i64", ScVal::I64(-7)),
        ("timepoint", ScVal::Timepoint(TimePoint(7))),
        ("duration", ScVal::Duration(Duration(7))),
        ("u128", ScVal::U128(UInt128Parts { hi: 0, lo: 7 })),
        (
            "i128",
            ScVal::I128(Int128Parts {
                hi: -1,
                lo: u64::MAX - 6,
            }),
        ),
        (
            "u256",
            ScVal::U256(UInt256Parts {
                hi_hi: 0,
                hi_lo: 0,
                lo_hi: 0,
                lo_lo: 7,
            }),
        ),
        (
            "i256",
            ScVal::I256(Int256Parts {
                hi_hi: -1,
                hi_lo: u64::MAX,
                lo_hi: u64::MAX,
                lo_lo: u64::MAX - 6,
            }),
        ),
        ("bytes", bytes(&[7])?),
        ("string", string(b"seven")?),
        ("symbol", sym("seven")?),
        ("vec", vec_of(vec![ScVal::U32(7)])?),
        (
            "map",
            ScVal::Map(Some(ScMap(
                vec![entry(ScVal::U32(7), ScVal::U32(7))].try_into()?,
            ))),
        ),
        ("address", ScVal::Address(addr)),
    ];
    keys.into_iter()
        .map(|(name, k)| Ok(entry(k, sym(name)?)))
        .collect()
}

/// Error values of the types besides contract, keyed by type name.
fn error_entries() -> Result<Vec<ScMapEntry>> {
    let errors = vec![
        ("auth", ScError::Auth(ScErrorCode::InvalidAction)),
        ("crypto", ScError::Crypto(ScErrorCode::InvalidInput)),
        ("events", ScError::Events(ScErrorCode::UnexpectedSize)),
    ];
    errors
        .into_iter()
        .map(|(name, e)| Ok(entry(sym(name)?, ScVal::Error(e))))
        .collect()
}

/// Keys that are distinct ScVals but turn into the same native key in sdks
/// that map integers to numbers and strings, symbols and bytes to text.
fn colliding_entries() -> Result<Vec<ScMapEntry>> {
    let keys = vec![
        ("u32", ScVal::U32(1)),
        ("i32", ScVal::I32(1)),
        ("u64", ScVal::U64(1)),
        ("i64", ScVal::I64(1)),
        ("u128", ScVal::U128(UInt128Parts { hi: 0, lo: 1 })),
        ("i128", ScVal::I128(Int128Parts { hi: 0, lo: 1 })),
        (
            "u256",
            ScVal::U256(UInt256Parts {
                hi_hi: 0,
                hi_lo: 0,
                lo_hi: 0,
                lo_lo: 1,
            }),
        ),
        (
            "i256",
            ScVal::I256(Int256Parts {
                hi_hi: 0,
                hi_lo: 0,
                lo_hi: 0,
                lo_lo: 1,
            }),
        ),
        ("bytes", bytes(b"1")?),
        ("string", string(b"1")?),
        ("symbol", sym("1")?),
    ];
    keys.into_iter()
        .map(|(name, k)| Ok(entry(k, sym(name)?)))
        .collect()
}

fn integer_extremes() -> Result<ScVal> {
    vec_of(vec![
        ScVal::U32(u32::MAX),
        ScVal::I32(i32::MIN),
        ScVal::I32(i32::MAX),
        ScVal::U64(u64::MAX),
        ScVal::I64(i64::MIN),
        ScVal::I64(i64::MAX),
        ScVal::Timepoint(TimePoint(u64::MAX)),
        ScVal::Duration(Duration(u64::MAX)),
        ScVal::U128(UInt128Parts { hi: 0, lo: 0 }),
        ScVal::U128(UInt128Parts {
            hi: u64::MAX,
            lo: u64::MAX,
        }),
        ScVal::I128(Int128Parts {
            hi: i64::MIN,
            lo: 0,
        }),
        ScVal::I128(Int128Parts {
            hi: i64::MAX,
            lo: u64::MAX,
        }),
        ScVal::U256(UInt256Parts {
            hi_hi: 0,
            hi_lo: 0,
            lo_hi: 0,
            lo_lo: 0,
        }),
        ScVal::U256(UInt256Parts {
            hi_hi: u64::MAX,
            hi_lo: u64::MAX,
            lo_hi: u64::MAX,
            lo_lo: u64::MAX,
        }),
        ScVal::I256(Int256Parts {
            hi_hi: i64::MIN,
            hi_lo: 0,
            lo_hi: 0,
            lo_lo: 0,
        }),
        ScVal::I256(Int256Parts {
            hi_hi: i64::MAX,
            hi_lo: u64::MAX,
            lo_hi: u64::MAX,
            lo_lo: u64::MAX,
        }),
    ])
}

/// Bytes that are not UTF-8: a lone continuation byte, an invalid lead
/// byte, a truncated two-byte sequence and an encoded surrogate.
const NOT_UTF8: &[u8] = b"\x80shapekit\xff\xc3(\xed\xa0\x80";

impl ScvalShapes {
    /// Deepest nesting `build` produces that the host still runs, by binary
    /// search over simulations of `echo`, and the host's error one deeper.
    fn deepest(
        ctx: &mut ChainCtx,
        source: &SigningKey,
        echo: &ContractId,
        build: fn(u32) -> Result<ScVal>,
    ) -> Result<(u32, String)> {
        let run = |ctx: &mut ChainCtx, d: u32| -> Result<Probe> {
            probe(ctx, source, invoke(echo, "echo", vec![build(d)?])?)
        };
        let (mut ok, mut bad) = (1u32, DEPTH_SEARCH_MAX);
        ensure!(
            !matches!(run(ctx, ok)?, Probe::Refused(_)),
            "the host refuses a single level of nesting"
        );
        let Probe::Refused(mut error) = run(ctx, bad)? else {
            bail!("the host accepts {bad} levels of nesting");
        };
        while bad - ok > 1 {
            let mid = ok + (bad - ok) / 2;
            match run(ctx, mid)? {
                Probe::Refused(e) => {
                    bad = mid;
                    error = e;
                }
                Probe::Priced(_) | Probe::Unpriced => ok = mid,
            }
        }
        Ok((ok, error))
    }

    /// Resources for echoing `build(depth)`. When simulation cannot size
    /// that transaction, the resources of the deepest level it can price
    /// are raised by a margin that covers the extra levels; core's result
    /// is the check that they suffice.
    fn depth_data(
        ctx: &mut ChainCtx,
        source: &SigningKey,
        echo: &ContractId,
        build: fn(u32) -> Result<ScVal>,
        depth: u32,
    ) -> Result<(SorobanTransactionData, bool)> {
        for d in (1..=depth).rev() {
            match probe(ctx, source, invoke(echo, "echo", vec![build(d)?])?)? {
                Probe::Priced(data) if d == depth => return Ok((data, false)),
                Probe::Priced(mut data) => {
                    let r = &mut data.resources;
                    r.instructions += r.instructions / 2;
                    r.write_bytes += UNPRICED_WRITE_MARGIN;
                    data.resource_fee += UNPRICED_FEE_MARGIN;
                    return Ok((data, true));
                }
                Probe::Unpriced => continue,
                Probe::Refused(e) => {
                    bail!("the host refuses depth {d} below the accepted {depth}: {e}")
                }
            }
        }
        bail!("simulation prices no nesting up to {depth}")
    }

    /// Records the deepest accepted nesting of one kind and its one-deeper
    /// twin. Returns the depth.
    fn depth_pair(
        ctx: &mut ChainCtx,
        source: &SigningKey,
        echo: &ContractId,
        kind: &str,
        build: fn(u32) -> Result<ScVal>,
    ) -> Result<u32> {
        let (depth, error) = Self::deepest(ctx, source, echo, build)?;
        let (data, raised) = Self::depth_data(ctx, source, echo, build, depth)?;
        let value = build(depth)?;
        let env = ctx.driver().soroban_with(
            source,
            invoke(echo, "echo", vec![value.clone()])?,
            data.clone(),
            Vec::new(),
        )?;
        let applied = ctx.submit_and_close(&env)?;
        ensure!(
            applied.return_value() == Some(&value),
            "ledger {}: echo of {depth} nested {kind}s returned something else",
            applied.ledger
        );
        let priced = if raised {
            ". simulation cannot size this transaction (it encodes the envelope under xdr depth 500), so its resources are those of the deepest priced level plus a margin"
        } else {
            ""
        };
        ctx.capture(
            &format!("{kind}-depth-max"),
            Selector::transaction(&applied),
            Spec::new(format!(
                "{depth} nested {kind}s around a u32 leaf ({} ScVal levels), the deepest the host accepts, echoed into a contract data entry, an event and the return value{priced}",
                depth + 1
            ))
            .fields(&ARGS)
            .flags(&["max-depth"]),
        )?;
        let twin = apply_failing(
            ctx,
            source,
            invoke(echo, "echo", vec![build(depth + 1)?])?,
            data,
            Vec::new(),
        )?;
        let r = invoke_failure(&twin)?.clone();
        ctx.capture(
            &format!("{kind}-depth-over"),
            Selector::ledger_of(&twin),
            Spec::new(format!(
                "{} nested {kind}s, one more than the host accepts (simulation: {}): core includes the invocation and fails it with invoke_host_function {}, so the value exists only in the envelope",
                depth + 1,
                error.lines().next().unwrap_or_default(),
                r.name()
            ))
            .fields(&ARGS),
        )?;
        Ok(depth)
    }

    /// Echoes `value` and checks the host gives it back unchanged.
    fn echo(
        ctx: &mut ChainCtx,
        source: &SigningKey,
        echo: &ContractId,
        value: ScVal,
    ) -> Result<Applied> {
        let applied = ctx
            .driver()
            .invoke(source, echo, "echo", vec![value.clone()])?;
        ensure!(
            applied.return_value() == Some(&value),
            "ledger {}: echo returned {:?}, not its argument",
            applied.ledger,
            applied.return_value()
        );
        Ok(applied)
    }
}

impl Recipe for ScvalShapes {
    fn id(&self) -> &str {
        FAMILY
    }

    fn family(&self) -> &str {
        FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn soroban_settings(&self) -> bool {
        true
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["caller"], BALANCE)?;
        let caller = ctx.account("caller");
        let echo = deploy_echo(ctx, &caller, FAMILY)?;

        let vec_depth = Self::depth_pair(ctx, &caller, &echo, "vec", nested_vec)?;
        let map_depth = Self::depth_pair(ctx, &caller, &echo, "map", nested_map)?;
        eprintln!("{FAMILY}: deepest accepted nesting: vec {vec_depth}, map {map_depth}");

        let addr = account_address(&caller.verifying_key());
        let mixed = ScMap::sorted_from(mixed_key_entries(addr.clone())?)?;
        ensure!(
            mixed
                .0
                .iter()
                .zip(mixed_key_entries(addr.clone())?)
                .all(|(a, b)| *a == b),
            "the mixed-key map is not listed in ScValType order"
        );
        let value = ScVal::Map(Some(mixed.clone()));
        let data = simulated_data(ctx, &caller, invoke(&echo, "echo", vec![value.clone()])?)?;
        let applied = Self::echo(ctx, &caller, &echo, value)?;
        ctx.capture(
            "map-mixed-keys-sorted",
            Selector::transaction(&applied),
            Spec::new(
                "a map with one key of each of the 19 ScVal types the host converts, in host order (type discriminant first), returned unchanged",
            )
            .fields(&ARGS),
        )?;
        let mut reversed = mixed.0.to_vec();
        reversed.reverse();
        let unsorted = ScVal::Map(Some(ScMap(reversed.try_into()?)));
        ensure!(
            !simulates(ctx, &caller, invoke(&echo, "echo", vec![unsorted.clone()])?)?,
            "the host accepted an unsorted map"
        );
        let twin = apply_failing(
            ctx,
            &caller,
            invoke(&echo, "echo", vec![unsorted])?,
            data,
            Vec::new(),
        )?;
        ensure_trapped(&twin, "unsorted map")?;
        ctx.capture(
            "map-mixed-keys-unsorted",
            Selector::ledger_of(&twin),
            Spec::new(
                "the same map with its entries reversed: the host refuses to convert a map whose keys are not strictly increasing, so core includes the invocation and fails it with invoke_host_function trapped",
            )
            .fields(&ARGS),
        )?;

        let colliding = ScVal::Map(Some(ScMap::sorted_from(colliding_entries()?)?));
        let applied = Self::echo(ctx, &caller, &echo, colliding)?;
        ctx.capture(
            "map-keys-collide-native",
            Selector::transaction(&applied),
            Spec::new(
                "a map keyed by u32 1, i32 1, u64 1, i64 1, u128 1, i128 1, u256 1, i256 1, bytes \"1\", string \"1\" and symbol \"1\": eleven distinct ScVal keys that collapse to one or two keys in a native map that turns numbers and text into the same key",
            )
            .fields(&ARGS)
            .flags(&["colliding-map-keys"]),
        )?;

        let applied = Self::echo(ctx, &caller, &echo, integer_extremes()?)?;
        ctx.capture(
            "integer-extremes",
            Selector::transaction(&applied),
            Spec::new(
                "u32, i32, u64, i64, timepoint, duration, u128, i128, u256 and i256 at their minimum and maximum values",
            )
            .fields(&ARGS),
        )?;

        let not_utf8 = string(NOT_UTF8)?;
        let data = simulated_data(ctx, &caller, invoke(&echo, "echo", vec![not_utf8.clone()])?)?;
        let applied = Self::echo(ctx, &caller, &echo, not_utf8)?;
        ctx.capture(
            "string-not-utf8",
            Selector::transaction(&applied),
            Spec::new(
                "a string holding bytes that are not UTF-8, which the host accepts as opaque bytes and echoes unchanged",
            )
            .fields(&ARGS)
            .flags(&["non-utf8"]),
        )?;
        let bad_symbol = ScVal::Symbol(ScSymbol(NOT_UTF8.to_vec().try_into()?));
        ensure!(
            !simulates(
                ctx,
                &caller,
                invoke(&echo, "echo", vec![bad_symbol.clone()])?
            )?,
            "the host accepted a symbol with non-utf8 bytes"
        );
        let twin = apply_failing(
            ctx,
            &caller,
            invoke(&echo, "echo", vec![bad_symbol])?,
            data,
            Vec::new(),
        )?;
        ensure_trapped(&twin, "non-utf8 symbol")?;
        ctx.capture(
            "symbol-not-utf8",
            Selector::ledger_of(&twin),
            Spec::new(
                "a symbol holding the same non-UTF-8 bytes: XDR allows any bytes, the host only [a-zA-Z0-9_], so core includes the invocation and fails it with invoke_host_function trapped",
            )
            .fields(&ARGS)
            .flags(&["non-utf8"]),
        )
        .context("capturing the refused symbol")?;

        // the mixed-key map holds only a contract error; the host converts
        // errors of every type, so the other arms an indexer decodes ride
        // in a map of their own
        let errors = ScVal::Map(Some(ScMap::sorted_from(error_entries()?)?));
        let applied = Self::echo(ctx, &caller, &echo, errors)?;
        ctx.capture(
            "errors-auth-crypto-events",
            Selector::transaction(&applied),
            Spec::new(
                "a map from symbols to error values of type auth (invalid_action), crypto (invalid_input) and events (unexpected_size), stored, emitted and returned unchanged",
            )
            .fields(&ARGS),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{AccountId, PublicKey, Uint256};

    fn addr() -> ScAddress {
        ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([1; 32]))))
    }

    #[test]
    fn mixed_keys_are_listed_in_host_order() {
        let listed = mixed_key_entries(addr()).unwrap();
        let sorted = ScMap::sorted_from(listed.clone()).unwrap();
        assert_eq!(sorted.0.to_vec(), listed);
        assert_eq!(listed.len(), 19);
    }

    #[test]
    fn colliding_keys_are_distinct_scvals() {
        let listed = colliding_entries().unwrap();
        let sorted = ScMap::sorted_from(listed.clone()).unwrap();
        assert_eq!(sorted.0.len(), listed.len());
    }

    #[test]
    fn nesting_counts_containers() {
        let mut v = nested_vec(3).unwrap();
        let mut levels = 0;
        while let ScVal::Vec(Some(items)) = v {
            levels += 1;
            v = items[0].clone();
        }
        assert_eq!((levels, v), (3, ScVal::U32(3)));
    }
}
