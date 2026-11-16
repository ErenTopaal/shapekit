//! Recipes: deterministic programs that drive a private standalone core and
//! capture core-emitted corpus entries from the chain it records.
//!
//! A [`Recipe`] runs once per network passphrase on its own node, through a
//! [`ChainCtx`]. Every capture becomes one entry under
//! `corpus/entries/<family>/<family>.p<protocol>-<slug>/` with a testnet and a
//! mainnet variant, and the chain itself is kept under
//! `corpus/chains/<chain-id>/<network>/`. See `docs/adding-a-shape.md`.

pub mod auth_trees;
pub mod cap01;
pub mod cap03;
pub mod cap15;
pub mod cap18;
pub mod cap21;
pub mod cap23;
pub mod cap27;
pub mod cap33;
pub mod cap35;
pub mod cap38;
pub mod cap40;
pub mod cap46;
pub mod cap63;
pub mod cap66;
pub mod cap67;
pub mod cap70;
pub mod cap71;
pub mod cap73;
pub mod cap77;
pub mod cap84;
pub mod cap85;
mod ctx;
pub mod events_max;
pub mod failed_tx;
pub mod invoke_basics;
pub mod ir;
pub mod ir_run;
pub mod max_length;
pub mod memo_arms;
pub mod memo_bytes;
pub mod muxed_source;
pub mod old_protocols;
mod order;
mod output;
mod run;
pub mod scval;
mod select;
pub mod sep41;
pub mod soroban_steps;
mod state_check;

pub use ctx::{Captured, ChainCtx, Spec};
pub use output::{CHAINS_DIR, ChainCheck, EntryCheck};
pub use run::{
    ChainRun, GenOptions, GenReport, Repro, generate, start_chain, verify_repro, verify_repro_on,
};
pub use select::{Selected, Selector, select};

use shapekit_manifest::{MAINNET_PASSPHRASE, TESTNET_PASSPHRASE};

/// One program that records a chain for a shape family at one protocol.
///
/// `run` must be deterministic: the same steps, keys and values on every
/// run, so the canonical chain and every entry are byte-identical across
/// runs. Keys come from [`ChainCtx::account`], never from randomness.
pub trait Recipe: Send + Sync {
    /// Derives every key of the recipe. Shared by the recipe's chains at
    /// different protocols, so their accounts are the same.
    fn id(&self) -> &str;
    /// The `registry/families.toml` id every capture belongs to.
    fn family(&self) -> &str;
    /// Ledger protocol the chain is upgraded to after genesis.
    fn protocol(&self) -> u32;
    /// Whether the chain needs testnet's Soroban settings before `run`.
    /// They were captured at protocol 30 and only apply there.
    fn soroban_settings(&self) -> bool {
        false
    }
    /// Whether the node starts with core's high Soroban limit override,
    /// without which a fresh network cannot even upload a contract. Implied
    /// by `soroban_settings`; below protocol 30 it is all a Soroban chain
    /// gets, since the captured settings only validate at 30.
    fn soroban_limits(&self) -> bool {
        self.soroban_settings()
    }
    /// Minimum persistent TTL for the state archival settings core creates
    /// at genesis (`TESTING_MINIMUM_PERSISTENT_ENTRY_LIFETIME`, at least 10),
    /// so a persistent entry created before any settings upgrade archives
    /// within that many ledgers. `None` keeps core's 4096.
    fn min_persistent_ttl(&self) -> Option<u32> {
        None
    }
    /// `EMIT_CLASSIC_EVENTS`: CAP-67 events for classic operations from
    /// protocol 23 on. On by default, as the public networks emit them.
    fn emit_classic_events(&self) -> bool {
        true
    }
    /// `BACKFILL_STELLAR_ASSET_EVENTS`: the same events and TransactionMeta
    /// v4 for ledgers below protocol 23 (needs `emit_classic_events`). No
    /// effect from protocol 23 on. Turn both off to record the meta core
    /// emitted for a pre-23 ledger in its day.
    fn backfill_stellar_asset_events(&self) -> bool {
        true
    }
    /// Whether the chain runs below its family's `protocol_valid.min` on
    /// purpose, to record contrast entries (what an older ledger looks like
    /// without the shape). Each of its entries is flagged
    /// `below-protocol-valid`.
    fn below_protocol_valid(&self) -> bool {
        false
    }
    fn run(&self, ctx: &mut ChainCtx) -> anyhow::Result<()>;
}

/// `<recipe id>.p<protocol>`, the directory under `corpus/chains/`.
pub fn chain_id(recipe: &dyn Recipe) -> String {
    format!("{}.p{}", recipe.id(), recipe.protocol())
}

/// Every recipe this build knows, one per chain.
pub fn all() -> Vec<Box<dyn Recipe>> {
    vec![
        Box::new(cap40::SignedPayloadSigner { protocol: 30 }),
        Box::new(cap40::SignedPayloadSigner { protocol: 28 }),
        Box::new(cap46::ConfigUpgrades { protocol: 30 }),
        Box::new(cap63::ParallelTxSet { protocol: 30 }),
        Box::new(cap66::Archival { protocol: 30 }),
        Box::new(cap70::ScpTiming { protocol: 30 }),
        Box::new(cap77::FrozenKeys { protocol: 30 }),
        Box::new(cap73::SacTrust { protocol: 30 }),
        Box::new(cap73::SacTrust { protocol: 28 }),
        Box::new(muxed_source::MuxedSource { protocol: 30 }),
        Box::new(muxed_source::MuxedSource { protocol: 28 }),
        Box::new(sep41::MuxedAddresses { protocol: 30 }),
        Box::new(sep41::MuxedAddresses { protocol: 28 }),
        Box::new(cap38::LiquidityPools { protocol: 30 }),
        Box::new(cap03::OfferLiabilities { protocol: 30 }),
        Box::new(max_length::MaxLength { protocol: 30 }),
        Box::new(memo_bytes::MemoBytes { protocol: 30 }),
        Box::new(memo_arms::MemoArms { protocol: 30 }),
        Box::new(failed_tx::FailedTx { protocol: 30 }),
        Box::new(cap15::FeeBump { protocol: 30 }),
        Box::new(cap27::MuxedAccounts { protocol: 30 }),
        Box::new(cap27::MuxedAccounts { protocol: 28 }),
        Box::new(cap21::Preconds { protocol: 30 }),
        Box::new(cap01::BumpSequence { protocol: 30 }),
        Box::new(cap23::ClaimableBalances { protocol: 30 }),
        Box::new(cap33::Sponsorship { protocol: 30 }),
        Box::new(cap35::Clawback { protocol: 30 }),
        Box::new(cap18::MaintainLiabilities { protocol: 30 }),
        Box::new(scval::ScvalShapes { protocol: 30 }),
        Box::new(events_max::EventsMax { protocol: 30 }),
        Box::new(cap84::MuxedContractDestination { protocol: 30 }),
        Box::new(cap67::UnifiedEvents {
            protocol: 30,
            events: cap67::Events::Live,
        }),
        Box::new(cap67::UnifiedEvents {
            protocol: 22,
            events: cap67::Events::PreCap67,
        }),
        Box::new(cap67::UnifiedEvents {
            protocol: 22,
            events: cap67::Events::Backfill,
        }),
        Box::new(invoke_basics::InvokeBasics { protocol: 30 }),
        Box::new(auth_trees::AuthTrees { protocol: 30 }),
        Box::new(cap71::Credentials { protocol: 30 }),
        Box::new(cap85::ExecutableTag { protocol: 30 }),
        Box::new(old_protocols::IssuerChecks { protocol: 12 }),
        Box::new(old_protocols::ManageDataProtocol1 { protocol: 1 }),
        Box::new(old_protocols::ChangeTrustNative { protocol: 9 }),
        Box::new(old_protocols::Inflation { protocol: 11 }),
        Box::new(old_protocols::Inflation { protocol: 12 }),
        Box::new(old_protocols::ClaimAtomV0 { protocol: 17 }),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Network {
    Testnet,
    Mainnet,
}

impl Network {
    pub const BOTH: [Network; 2] = [Network::Testnet, Network::Mainnet];

    pub fn passphrase(self) -> &'static str {
        match self {
            Network::Testnet => TESTNET_PASSPHRASE,
            Network::Mainnet => MAINNET_PASSPHRASE,
        }
    }

    /// The variant name and chain subdirectory.
    pub fn name(self) -> &'static str {
        match self {
            Network::Testnet => "testnet",
            Network::Mainnet => "mainnet",
        }
    }
}
