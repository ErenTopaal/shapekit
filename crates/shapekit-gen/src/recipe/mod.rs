//! Recipes: deterministic programs that drive a private standalone core and
//! capture core-emitted corpus entries from the chain it records.
//!
//! A [`Recipe`] runs once per network passphrase on its own node, through a
//! [`ChainCtx`]. Every capture becomes one entry under
//! `corpus/entries/<family>/<family>.p<protocol>-<slug>/` with a testnet and a
//! mainnet variant, and the chain itself is kept under
//! `corpus/chains/<chain-id>/<network>/`. See `docs/adding-a-shape.md`.

pub mod cap40;
mod ctx;
mod output;
mod run;
mod select;

pub use ctx::{Captured, ChainCtx, Spec};
pub use output::{CHAINS_DIR, ChainCheck, EntryCheck};
pub use run::{ChainRun, GenOptions, GenReport, Repro, generate, verify_repro};
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
