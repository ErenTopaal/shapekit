//! `cap46.config-upgrades`: Soroban network config upgrades (CAP-46-09) as
//! they appear in ledger meta. Applying testnet's settings to the fresh
//! network is itself a config upgrade, so its ledger is captured, with two
//! of the cost parameter entries it rewrote; then a separate
//! `maxsorobantxsetsize` ledger upgrade, which core turns into a write of
//! the execution lanes setting.
//!
//! | ledger | content                                                  |
//! |--------|----------------------------------------------------------|
//! | 2      | protocol upgrade                                         |
//! | 3 to 6 | testnet Soroban settings, ledger 6 is the config upgrade |
//! | 7      | `maxsorobantxsetsize` upgrade                            |
//!
//! Also holds the config upgrade helpers the other settings-driven recipes
//! (cap63, cap66, cap70, cap77) share.

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::settings::{NetworkSettings, PreparedUpgrade};
use stellar_xdr::{
    ConfigSettingEntry, ConfigSettingId, ConfigUpgradeSet, LedgerCloseMeta, LedgerEntryChange,
    LedgerEntryData, LedgerKey, LedgerKeyConfigSetting, LedgerUpgrade,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap46.config-upgrades";

/// The new Soroban transaction set size. Testnet's execution lanes setting
/// allows 2000 transactions per ledger; any other value makes core apply it.
const MAX_SOROBAN_TX_SET_SIZE: u32 = 1000;

pub struct ConfigUpgrades {
    pub protocol: u32,
}

/// Ledger key of one CONFIG_SETTING entry.
pub(crate) fn config_key(id: ConfigSettingId) -> LedgerKey {
    LedgerKey::ConfigSetting(LedgerKeyConfigSetting {
        config_setting_id: id,
    })
}

/// The live value of one setting, from the node's query server.
pub(crate) fn live_setting(ctx: &mut ChainCtx, id: ConfigSettingId) -> Result<ConfigSettingEntry> {
    let got = ctx.driver().node().ledger_entries(&[config_key(id)])?;
    match got.entries[0].entry.as_ref().map(|e| &e.data) {
        Some(LedgerEntryData::ConfigSetting(s)) if s.discriminant() == id => Ok(s.clone()),
        other => bail!("config setting {id:?} is not live: {other:?}"),
    }
}

/// Applies `entries` as one config upgrade set, stored through the settings
/// writer the chain's Soroban bootstrap deployed from root. The entries are
/// put in increasing id order, which core requires. Returns the upgrade and
/// the ledger it closed in.
pub(crate) fn apply_config_upgrade(
    ctx: &mut ChainCtx,
    mut entries: Vec<ConfigSettingEntry>,
) -> Result<(PreparedUpgrade, u32)> {
    entries.sort_by_key(|e| e.discriminant() as i32);
    let root: SigningKey = ctx.root().clone();
    let writer = ctx.driver().settings_writer(&root)?;
    let set = ConfigUpgradeSet {
        updated_entry: entries.try_into()?,
    };
    Ok(ctx.driver().apply_upgrade_set(set, &writer, &root)?)
}

/// The changes the upgrades of a ledger's meta made, in order.
pub(crate) fn upgrade_changes(lcm: &LedgerCloseMeta) -> Vec<&LedgerEntryChange> {
    let list = match lcm {
        LedgerCloseMeta::V0(m) => &m.upgrades_processing,
        LedgerCloseMeta::V1(m) => &m.upgrades_processing,
        LedgerCloseMeta::V2(m) => &m.upgrades_processing,
    };
    list.iter().flat_map(|u| u.changes.iter()).collect()
}

/// Whether an upgrade in the last closed ledger wrote setting `id`.
pub(crate) fn upgrade_wrote(ctx: &mut ChainCtx, id: ConfigSettingId) -> bool {
    upgrade_changes(ctx.driver().last_meta())
        .into_iter()
        .any(|c| match c {
            LedgerEntryChange::Updated(e) | LedgerEntryChange::Created(e) => {
                matches!(&e.data, LedgerEntryData::ConfigSetting(s) if s.discriminant() == id)
            }
            _ => false,
        })
}

impl Recipe for ConfigUpgrades {
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
        // the bootstrap's last ledger is the testnet settings upgrade
        let settings_ledger = ctx.driver().last_closed();
        let applied = shapekit_core_driver::chain::upgrades(ctx.driver().last_meta());
        ensure!(
            matches!(applied.as_slice(), [LedgerUpgrade::Config(_)]),
            "ledger {settings_ledger} should hold the settings config upgrade, has {applied:?}"
        );
        for id in [
            ConfigSettingId::ContractCostParamsCpuInstructions,
            ConfigSettingId::ContractCostParamsMemoryBytes,
        ] {
            ensure!(
                upgrade_wrote(ctx, id),
                "the settings upgrade in ledger {settings_ledger} did not write {id:?}"
            );
        }
        let testnet = NetworkSettings::testnet()?;
        ctx.capture(
            "testnet-settings-upgrade",
            Selector::Ledger(settings_ledger),
            Spec::new(
                "a ledger whose only upgrade is a config upgrade set carrying every upgradeable Soroban setting of testnet, so its upgrade meta rewrites the config setting entries, cost parameters included",
            ),
        )?;
        for (slug, id, what) in [
            (
                "cost-params-cpu",
                ConfigSettingId::ContractCostParamsCpuInstructions,
                "cpu instruction",
            ),
            (
                "cost-params-memory",
                ConfigSettingId::ContractCostParamsMemoryBytes,
                "memory byte",
            ),
        ] {
            let want = testnet
                .get(id)
                .with_context(|| format!("testnet settings have no {id:?}"))?;
            let live = live_setting(ctx, id)?;
            ensure!(
                &live == want,
                "{id:?} after the upgrade differs from testnet's value"
            );
            ctx.capture(
                slug,
                Selector::LedgerEntry {
                    ledger: settings_ledger,
                    key: config_key(id),
                },
                Spec::new(format!(
                    "the {what} cost parameter config setting entry as the testnet settings upgrade wrote it, one cost model per cost type"
                )),
            )?;
        }

        let size = MAX_SOROBAN_TX_SET_SIZE.to_string();
        let seq = ctx
            .driver()
            .close_with_upgrades(&[("maxsorobantxsetsize", &size)])?;
        let applied = shapekit_core_driver::chain::upgrades(ctx.driver().last_meta());
        ensure!(
            applied == [LedgerUpgrade::MaxSorobanTxSetSize(MAX_SOROBAN_TX_SET_SIZE)],
            "ledger {seq} should hold only the max soroban tx set size upgrade, has {applied:?}"
        );
        match live_setting(ctx, ConfigSettingId::ContractExecutionLanes)? {
            ConfigSettingEntry::ContractExecutionLanes(l) => ensure!(
                l.ledger_max_tx_count == MAX_SOROBAN_TX_SET_SIZE,
                "execution lanes allow {} transactions after the upgrade, expected {MAX_SOROBAN_TX_SET_SIZE}",
                l.ledger_max_tx_count
            ),
            other => bail!("execution lanes setting is {other:?}"),
        }
        ensure!(
            upgrade_wrote(ctx, ConfigSettingId::ContractExecutionLanes),
            "the upgrade meta of ledger {seq} does not write the execution lanes setting"
        );
        ctx.capture(
            "max-soroban-tx-set-size-upgrade",
            Selector::Ledger(seq),
            Spec::new(
                "a ledger upgrade of the maximum Soroban transaction set size, which core applies by rewriting the execution lanes config setting entry in the upgrade meta",
            ),
        )
    }
}
