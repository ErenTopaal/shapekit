//! `cap70.scp-timing`: CAP-70 moved SCP timeouts and the ledger close time
//! target into the CONFIG_SETTING_SCP_TIMING entry. A config upgrade set
//! holding only that entry changes the close time target, and its ledger
//! and the resulting entry are captured.
//!
//! | ledger | content                                              |
//! |--------|------------------------------------------------------|
//! | 2      | protocol upgrade                                     |
//! | 3 to 6 | testnet Soroban settings                             |
//! | 7      | root stores the scp timing upgrade set               |
//! | 8      | config upgrade of the scp timing setting             |
//!
//! The chain closes manually, so the new timing changes nothing about when
//! ledgers close here; it only shows up in the meta.

use anyhow::{Result, bail, ensure};
use stellar_xdr::{ConfigSettingEntry, ConfigSettingId, ConfigSettingScpTiming};

use super::cap46::{apply_config_upgrade, config_key, live_setting, upgrade_wrote};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap70.scp-timing";

/// The new ledger close time target, the smallest core accepts
/// (`MinimumSorobanNetworkConfig::LEDGER_TARGET_CLOSE_TIME_MILLISECONDS`,
/// v30.0.0). Testnet runs 5000.
const TARGET_CLOSE_MS: u32 = 4000;

pub struct ScpTiming {
    pub protocol: u32,
}

impl Recipe for ScpTiming {
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
        let ConfigSettingEntry::ScpTiming(before) = live_setting(ctx, ConfigSettingId::ScpTiming)?
        else {
            bail!("the scp timing setting has another arm");
        };
        ensure!(
            before.ledger_target_close_time_milliseconds != TARGET_CLOSE_MS,
            "the live close time target is already {TARGET_CLOSE_MS} ms, the upgrade would change nothing"
        );
        let want = ConfigSettingScpTiming {
            ledger_target_close_time_milliseconds: TARGET_CLOSE_MS,
            ..before
        };
        let (_, seq) =
            apply_config_upgrade(ctx, vec![ConfigSettingEntry::ScpTiming(want.clone())])?;
        ensure!(
            upgrade_wrote(ctx, ConfigSettingId::ScpTiming),
            "the upgrade meta of ledger {seq} does not write the scp timing setting"
        );
        let live = live_setting(ctx, ConfigSettingId::ScpTiming)?;
        ensure!(
            live == ConfigSettingEntry::ScpTiming(want),
            "scp timing after the upgrade is {live:?}"
        );
        ctx.capture(
            "scp-timing-upgrade",
            Selector::Ledger(seq),
            Spec::new(
                "a ledger whose config upgrade set holds only the scp timing setting, lowering the ledger close time target to 4000 ms, so the upgrade meta rewrites that entry",
            ),
        )?;
        ctx.capture(
            "scp-timing-entry",
            Selector::LedgerEntry {
                ledger: seq,
                key: config_key(ConfigSettingId::ScpTiming),
            },
            Spec::new(
                "the scp timing config setting entry after the upgrade: close time target, nomination and ballot timeouts and their increments in milliseconds",
            ),
        )
    }
}
