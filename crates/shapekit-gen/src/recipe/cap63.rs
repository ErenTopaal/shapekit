//! `cap63.parallel-txset`: CAP-63 parallel Soroban transaction sets. Five
//! Soroban invocations are queued and closed in one ledger, so its
//! generalized transaction set carries a parallel Soroban phase. Two of them
//! write the same entry and must share a dependent cluster; two others are
//! independent of everything and get clusters of their own; the fifth only
//! reads, changes nothing, and is packed into whichever cluster has room.
//!
//! | ledger  | content                                                     |
//! |---------|-------------------------------------------------------------|
//! | 2       | protocol upgrade                                            |
//! | 3 to 6  | testnet Soroban settings (three dependent clusters a stage) |
//! | 7       | root funds five sources                                     |
//! | 8 to 11 | four echo instances deployed                                |
//! | 12      | five echo calls in one parallel phase                       |
//!
//! Core packs independent clusters first-fit into at most
//! `ledgerMaxDependentTxClusters` bins of `ledgerMaxInstructions / stages`
//! declared instructions each, and every bin becomes one cluster in the XDR
//! (`ParallelTxSetBuilder.cpp`, v30.0.0). Small calls all fit one bin, so
//! the calls declare more instructions than they use: the two independent
//! ones the per-transaction maximum, the conflicting pair just under half a
//! ledger's worth each, so no two clusters fit one bin under testnet's
//! limits and the phase is one stage of three clusters.
//!
//! Every call also carries a different inclusion fee: the builder breaks
//! fee ties with a per-process random seed
//! (`buildSurgePricedParallelSorobanPhase`), and distinct fees keep its
//! choice the same on every run.

use anyhow::{Context, Result, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::contracts::ECHO_WASM;
use shapekit_core_driver::keys::account_id;
use shapekit_core_driver::sim::AuthMode;
use shapekit_core_driver::sim_legacy::AnySimulator;
use shapekit_core_driver::tx::assemble_soroban;
use stellar_xdr::{
    ConfigSettingEntry, ConfigSettingId, ContractId, GeneralizedTransactionSet, Hash, HostFunction,
    InvokeContractArgs, InvokeHostFunctionOp, LedgerCloseMeta, OperationBody, ScAddress, ScSymbol,
    ScVal, SorobanCredentials, TransactionEnvelope, TransactionPhase,
};

use super::cap46::{config_key, live_setting};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap63.parallel-txset";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

pub struct ParallelTxSet {
    pub protocol: u32,
}

/// Hash of the echo wasm the Soroban bootstrap uploaded, so recipes can
/// deploy more instances without uploading it again.
pub(crate) fn echo_code() -> Hash {
    Hash(Sha256::digest(ECHO_WASM).into())
}

/// Deterministic deploy salt for one named instance of a recipe.
pub(crate) fn salt(recipe: &str, name: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"shapekit/recipe-salt/v1");
    h.update(recipe.as_bytes());
    h.update([0]);
    h.update(name.as_bytes());
    h.finalize().into()
}

/// A call of `function` on `contract`.
pub(crate) fn call(
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

/// A simulated, assembled and signed invocation of `hf` from `source`
/// paying `inclusion_fee` on top of the resource fee. With `declared`, the
/// transaction declares that many instructions instead of the simulated
/// count, and its resource fee grows by what they cost at the live
/// instruction fee rate. Only source-account authorization is accepted.
pub(crate) fn invocation(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    hf: HostFunction,
    inclusion_fee: u32,
    declared: Option<u32>,
) -> Result<TransactionEnvelope> {
    let rate = match live_setting(ctx, ConfigSettingId::ContractComputeV0)? {
        ConfigSettingEntry::ContractComputeV0(c) => c.fee_rate_per_instructions_increment,
        other => anyhow::bail!("compute setting is {other:?}"),
    };
    let node = ctx.driver().node();
    let sim = AnySimulator::new(node.snapshot()?, &node.ledger_context()?)?;
    let r = sim.simulate_invoke(
        hf.clone(),
        AuthMode::Record {
            allow_non_root: false,
            address_v2: false,
        },
        &account_id(&source.verifying_key()),
    )?;
    ensure!(
        r.auth
            .iter()
            .all(|a| matches!(a.credentials, SorobanCredentials::SourceAccount)),
        "the invocation needs signed auth entries: {:?}",
        r.auth
    );
    let tx = ctx
        .tx(source)?
        .fee(inclusion_fee)
        .operation(
            None,
            OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: hf,
                auth: Default::default(),
            }),
        )
        .build()?;
    let mut data = r.transaction_data;
    if let Some(declared) = declared {
        let simulated = data.resources.instructions;
        ensure!(
            declared >= simulated,
            "the call needs {simulated} instructions, more than the {declared} it would declare"
        );
        // the instruction fee is charged per 10000 instructions, rounded up
        let extra = (i64::from(declared - simulated) * rate + 9_999) / 10_000;
        data.resources.instructions = declared;
        data.resource_fee += extra;
    }
    let tx = assemble_soroban(tx, data, Some(r.auth))?;
    ctx.sign(tx, &[source])
}

/// The clusters of every stage of the ledger's parallel Soroban phase.
fn stages(lcm: &LedgerCloseMeta) -> Result<Vec<Vec<Vec<TransactionEnvelope>>>> {
    let LedgerCloseMeta::V2(m) = lcm else {
        anyhow::bail!("the ledger's meta is not LedgerCloseMetaV2");
    };
    let GeneralizedTransactionSet::V1(set) = &m.tx_set;
    let parallel = set
        .phases
        .iter()
        .find_map(|p| match p {
            TransactionPhase::V1(c) => Some(c),
            TransactionPhase::V0(_) => None,
        })
        .context("the transaction set has no parallel phase")?;
    Ok(parallel
        .execution_stages
        .iter()
        .map(|stage| stage.iter().map(|cluster| cluster.to_vec()).collect())
        .collect())
}

impl Recipe for ParallelTxSet {
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
        let settings_ledger = ctx.driver().last_closed();
        let clusters = match live_setting(ctx, ConfigSettingId::ContractParallelComputeV0)? {
            ConfigSettingEntry::ContractParallelComputeV0(c) => c.ledger_max_dependent_tx_clusters,
            other => anyhow::bail!("parallel compute setting is {other:?}"),
        };
        ensure!(
            clusters >= 3,
            "testnet settings allow {clusters} dependent clusters, the recipe needs three"
        );
        ctx.capture(
            "parallel-compute-setting",
            Selector::LedgerEntry {
                ledger: settings_ledger,
                key: config_key(ConfigSettingId::ContractParallelComputeV0),
            },
            Spec::new(
                "the parallel compute config setting entry testnet's settings upgrade wrote, bounding the dependent transaction clusters per stage",
            ),
        )?;

        let roles = ["s1", "s2", "s3", "s4", "s5"];
        ctx.fund(&roles, BALANCE)?;
        let keys: Vec<SigningKey> = roles.iter().map(|r| ctx.account(r)).collect();
        let code = echo_code();
        let mut instances = Vec::new();
        for name in ["a", "b", "c", "d"] {
            let id = ctx.driver().deploy(&keys[0], &code, &salt(FAMILY, name))?;
            instances.push(id);
        }

        let (ledger_max, tx_max) = match live_setting(ctx, ConfigSettingId::ContractComputeV0)? {
            ConfigSettingEntry::ContractComputeV0(c) => {
                (c.ledger_max_instructions, c.tx_max_instructions)
            }
            other => anyhow::bail!("compute setting is {other:?}"),
        };
        let big = u32::try_from(tx_max)?;
        let half = u32::try_from(ledger_max / 2 - 1)?;
        // no two of the clusters {pair}, {big}, {big} fit one bin of
        // ledger_max instructions, while the pair alone does
        ensure!(
            2 * i64::from(big) > ledger_max && i64::from(big) + 2 * i64::from(half) > ledger_max,
            "testnet's compute limits ({ledger_max} a ledger, {tx_max} a transaction) let two clusters share a bin"
        );

        // s1 and s4 both write instance a's stored value, so they conflict
        let plan = [
            (0usize, 0usize, 4000u32, half),
            (1, 1, 3000, big),
            (2, 2, 2000, big),
            (3, 0, 1000, half),
        ];
        let mut envs = Vec::new();
        for (i, (who, instance, fee, declared)) in plan.iter().enumerate() {
            let hf = call(
                &instances[*instance],
                "echo",
                vec![ScVal::U32(u32::try_from(i)? + 1)],
            )?;
            let env = invocation(ctx, &keys[*who], hf, *fee, Some(*declared))?;
            envs.push(env);
        }
        // s5 only reads instance d, which nothing ever stored into, so its
        // call changes no entry and conflicts with nobody
        let hf = call(&instances[3], "last", vec![])?;
        envs.push(invocation(ctx, &keys[4], hf, 500, None)?);

        let applied = ctx.driver().apply_together(&envs)?;
        for a in &applied {
            ensure!(
                a.succeeded(),
                "ledger {}: call {} failed: {:?}",
                a.ledger,
                hex::encode(a.hash),
                a.result.result
            );
        }
        let ledger = applied[0].ledger;
        let built = stages(ctx.driver().last_meta())?;
        let shape: Vec<Vec<usize>> = built
            .iter()
            .map(|stage| stage.iter().map(Vec::len).collect())
            .collect();
        let pair_shared = built
            .iter()
            .flatten()
            .any(|c| c.contains(&envs[0]) && c.contains(&envs[3]));
        ensure!(
            shape.len() == 1 && shape[0].len() == 3 && pair_shared,
            "ledger {ledger}: expected one stage of three clusters, the conflicting calls sharing one, core built {shape:?}"
        );
        ensure!(
            shape.iter().flatten().sum::<usize>() == envs.len(),
            "ledger {ledger}: the parallel phase holds {shape:?}, not the {} calls",
            envs.len()
        );
        ctx.capture(
            "five-calls-parallel-phase",
            Selector::Ledger(ledger),
            Spec::new(format!(
                "a LedgerCloseMetaV2 whose transaction set has a parallel Soroban phase of one stage with clusters of {:?} transactions: two calls writing the same entry share a cluster, two independent calls get their own, and a read-only call that changes nothing is packed in with them; each result meta carries the post-apply fee refund changes",
                shape[0]
            )),
        )
    }
}
