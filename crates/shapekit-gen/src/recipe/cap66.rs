//! `cap66.archival`: persistent entries that expire, are restored
//! explicitly with RestoreFootprint (CAP-62/66), restored automatically by
//! an invocation whose footprint marks them archived
//! (`SorobanResourcesExtV0`), and evicted by the eviction scan.
//!
//! Testnet keeps persistent entries alive for 120960 ledgers and scans for
//! eviction from bucket list level 7, so the chain lowers both through a
//! config upgrade of the state archival setting: minimum persistent TTL 10
//! and minimum temporary TTL 16 (the smallest core accepts,
//! `MinimumSorobanNetworkConfig`, NetworkConfig.h, v30.0.0) and scanning
//! from level 1. Everything else stays testnet's.
//!
//! | ledger   | content                                                   |
//! |----------|-----------------------------------------------------------|
//! | 2        | protocol upgrade                                          |
//! | 3 to 6   | testnet Soroban settings                                  |
//! | 7        | root funds alice, bob and carol                           |
//! | 8 to 10  | three echo instances: restore, auto and evict             |
//! | 11 to 12 | state archival config upgrade                             |
//! | 13 to 16 | each instance stores a value, evict also a temporary one  |
//! | then     | empty ledgers until the stored values are archived        |
//! | next     | bob restores the restore instance's value                 |
//! | next     | carol's call to auto restores its value on the way        |
//! | then     | empty ledgers until the scan evicts the evict instance's  |
//! |          | persistent and temporary values                           |

use anyhow::{Context, Result, bail, ensure};
use shapekit_core_driver::node::EntryState;
use shapekit_core_driver::sim_legacy::AnySimulator;
use shapekit_core_driver::tx::assemble_soroban;
use stellar_xdr::{
    ConfigSettingEntry, ConfigSettingId, ContractDataDurability, ContractId, ExtensionPoint,
    LedgerCloseMeta, LedgerEntryChange, LedgerKey, LedgerKeyContractData, OperationBody,
    RestoreFootprintOp, ScAddress, ScSymbol, ScVal, SorobanTransactionDataExt, TransactionMeta,
};

use super::cap46::{apply_config_upgrade, live_setting};
use super::cap63::{call, echo_code, salt};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap66.archival";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

const MIN_PERSISTENT_TTL: u32 = 10;
const MIN_TEMPORARY_TTL: u32 = 16;
const STARTING_EVICTION_SCAN_LEVEL: u32 = 1;

/// Upper bound on empty ledgers closed while waiting for archival or
/// eviction, so a setting that never takes effect fails the chain instead
/// of running forever.
const MAX_WAIT_LEDGERS: u32 = 400;

pub struct Archival {
    pub protocol: u32,
}

fn symbol(s: &str) -> Result<ScVal> {
    Ok(ScVal::Symbol(ScSymbol(s.try_into()?)))
}

/// The key echo's `echo` stores its value under, in persistent storage.
fn stored_key(contract: &ContractId) -> Result<LedgerKey> {
    data_key(
        contract,
        symbol("last")?,
        ContractDataDurability::Persistent,
    )
}

fn data_key(
    contract: &ContractId,
    key: ScVal,
    durability: ContractDataDurability,
) -> Result<LedgerKey> {
    Ok(LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(contract.clone()),
        key,
        durability,
    }))
}

fn state(ctx: &mut ChainCtx, key: &LedgerKey) -> Result<EntryState> {
    let got = ctx
        .driver()
        .node()
        .ledger_entries(std::slice::from_ref(key))?;
    Ok(got.entries[0].state)
}

/// Keys the eviction scan removed in a ledger.
fn evicted(lcm: &LedgerCloseMeta) -> &[LedgerKey] {
    match lcm {
        LedgerCloseMeta::V2(m) => m.evicted_keys.as_slice(),
        LedgerCloseMeta::V0(_) | LedgerCloseMeta::V1(_) => &[],
    }
}

/// Whether the operation meta of `meta` restores the entry under `key`.
fn restores(meta: &TransactionMeta, key: &LedgerKey) -> bool {
    let TransactionMeta::V4(m) = meta else {
        return false;
    };
    m.operations
        .iter()
        .flat_map(|o| o.changes.iter())
        .any(|c| matches!(c, LedgerEntryChange::Restored(e) if entry_key_matches(e, key)))
}

fn entry_key_matches(e: &stellar_xdr::LedgerEntry, key: &LedgerKey) -> bool {
    match (&e.data, key) {
        (stellar_xdr::LedgerEntryData::ContractData(d), LedgerKey::ContractData(k)) => {
            d.contract == k.contract && d.key == k.key && d.durability == k.durability
        }
        _ => false,
    }
}

impl Recipe for Archival {
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
        ctx.fund(&["alice", "bob", "carol"], BALANCE)?;
        let alice = ctx.account("alice");
        let bob = ctx.account("bob");
        let carol = ctx.account("carol");
        let code = echo_code();
        let mut ids = Vec::new();
        // instances are created under testnet's ttl, so only the values
        // stored after the upgrade expire quickly
        for name in ["restore", "auto", "evict"] {
            ids.push(ctx.driver().deploy(&alice, &code, &salt(FAMILY, name))?);
        }
        let [restore_id, auto_id, evict_id] = <[ContractId; 3]>::try_from(ids)
            .map_err(|_| anyhow::anyhow!("three instances were deployed"))?;

        let ConfigSettingEntry::StateArchival(mut archival) =
            live_setting(ctx, ConfigSettingId::StateArchival)?
        else {
            bail!("the state archival setting has another arm");
        };
        archival.min_persistent_ttl = MIN_PERSISTENT_TTL;
        archival.min_temporary_ttl = MIN_TEMPORARY_TTL;
        archival.starting_eviction_scan_level = STARTING_EVICTION_SCAN_LEVEL;
        apply_config_upgrade(ctx, vec![ConfigSettingEntry::StateArchival(archival)])?;

        let stored_after = ctx.driver().last_closed();
        for (i, id) in [&restore_id, &auto_id, &evict_id].into_iter().enumerate() {
            let value = ScVal::U32(u32::try_from(i)? + 1);
            ctx.driver().invoke(&alice, id, "echo", vec![value])?;
        }
        let temp_key = symbol("scratch")?;
        ctx.driver().invoke(
            &alice,
            &evict_id,
            "put_temp",
            vec![temp_key.clone(), ScVal::U32(7)],
        )?;
        let restore_key = stored_key(&restore_id)?;
        let auto_key = stored_key(&auto_id)?;
        let evict_key = stored_key(&evict_id)?;
        let temp_key = data_key(&evict_id, temp_key, ContractDataDurability::Temporary)?;

        let mut waited = 0;
        while state(ctx, &restore_key)? != EntryState::Archived
            || state(ctx, &auto_key)? != EntryState::Archived
        {
            ensure!(
                waited < MAX_WAIT_LEDGERS,
                "the stored values are still live after {MAX_WAIT_LEDGERS} ledgers"
            );
            ctx.driver().close_empty()?;
            waited += 1;
        }

        let node = ctx.driver().node();
        let sim = AnySimulator::new(node.snapshot()?, &node.ledger_context()?)?;
        let data = sim.simulate_restore(std::slice::from_ref(&restore_key))?;
        let tx = ctx
            .tx(&bob)?
            .operation(
                None,
                OperationBody::RestoreFootprint(RestoreFootprintOp {
                    ext: ExtensionPoint::V0,
                }),
            )
            .build()?;
        let tx = assemble_soroban(tx, data, None)?;
        let env = ctx.sign(tx, &[&bob])?;
        let applied = ctx.submit_and_close(&env)?;
        ensure!(
            restores(&applied.meta, &restore_key),
            "ledger {}: the restore's meta has no restored change for the archived value",
            applied.ledger
        );
        ctx.capture(
            "restore-footprint",
            Selector::ledger_of(&applied),
            Spec::new(
                "a ledger with a RestoreFootprint transaction bringing back an archived persistent contract data entry; its meta shows the entry and its ttl as restored changes",
            )
            .fields(&["op.restore_footprint", "tx.soroban.footprint"]),
        )?;

        let hf = call(&auto_id, "echo", vec![ScVal::U32(4)])?;
        let env = ctx.driver().soroban(&carol, hf)?;
        let data_ext = match &env {
            stellar_xdr::TransactionEnvelope::Tx(e) => match &e.tx.ext {
                stellar_xdr::TransactionExt::V1(d) => d.ext.clone(),
                stellar_xdr::TransactionExt::V0 => bail!("the call carries no soroban data"),
            },
            _ => bail!("the call is not a v1 envelope"),
        };
        match &data_ext {
            SorobanTransactionDataExt::V1(r) => ensure!(
                r.archived_soroban_entries.len() == 1,
                "simulation marked {:?} as archived, expected the one stored value",
                r.archived_soroban_entries
            ),
            SorobanTransactionDataExt::V0 => {
                bail!("simulation did not mark the archived value for automatic restore")
            }
        }
        let applied = ctx.submit_and_close(&env)?;
        ensure!(
            restores(&applied.meta, &auto_key),
            "ledger {}: the call's meta has no restored change for the archived value",
            applied.ledger
        );
        ctx.capture(
            "auto-restore-invoke",
            Selector::transaction(&applied),
            Spec::new(
                "an invocation whose read-write footprint holds an archived persistent entry, marked by index in SorobanResourcesExtV0, which core restores before the call writes it",
            )
            .fields(&[
                "tx.soroban.ext.archived_soroban_entries",
                "tx.soroban.footprint",
            ]),
        )?;

        // the scan may have run while the chain waited for archival, so
        // every ledger since the values were stored is searched
        let mut pending = vec![evict_key.clone(), temp_key.clone()];
        let mut found: Vec<(u32, Vec<LedgerKey>)> = Vec::new();
        let mut scanned = stored_after;
        let mut waited = 0;
        loop {
            for frame in ctx.driver().recorder().frames() {
                if frame.ledger_seq <= scanned {
                    continue;
                }
                let gone: Vec<LedgerKey> = evicted(&frame.meta)
                    .iter()
                    .filter(|k| pending.contains(k))
                    .cloned()
                    .collect();
                if !gone.is_empty() {
                    pending.retain(|k| !gone.contains(k));
                    found.push((frame.ledger_seq, gone));
                }
                scanned = frame.ledger_seq;
            }
            if pending.is_empty() {
                break;
            }
            ensure!(
                waited < MAX_WAIT_LEDGERS,
                "the eviction scan did not reach {pending:?} within {MAX_WAIT_LEDGERS} ledgers"
            );
            ctx.driver().close_empty()?;
            waited += 1;
        }
        for (seq, gone) in found {
            let (slug, what) = if gone.len() == 2 {
                (
                    "evicted-persistent-and-temporary",
                    "an expired persistent and an expired temporary contract data entry",
                )
            } else if gone[0] == evict_key {
                (
                    "evicted-persistent",
                    "an expired persistent contract data entry",
                )
            } else {
                (
                    "evicted-temporary",
                    "an expired temporary contract data entry",
                )
            };
            ctx.capture(
                slug,
                Selector::Ledger(seq),
                Spec::new(format!(
                    "a ledger whose eviction scan removed {what}, listed in evictedKeys"
                )),
            )
            .with_context(|| format!("capturing the eviction in ledger {seq}"))?;
        }
        Ok(())
    }
}
