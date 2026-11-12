//! Which chains and entries a harness run replays, from the corpus manifest.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// One corpus entry the harness gives a verdict: it sits in a recorded chain and has an
/// oracle file for the replayed network.
#[derive(Debug, Clone)]
pub struct EntryRef {
    pub id: String,
    pub family: String,
    pub ledger: u32,
    pub oracle_file: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ChainJob {
    pub chain: String,
    pub network: String,
    pub dir: PathBuf,
    /// Sorted by ledger, then id.
    pub entries: Vec<EntryRef>,
}

#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub jobs: Vec<ChainJob>,
    /// Selected entries the harness cannot replay: no chain (history captures, goldens,
    /// synthetic entries, vectors) or no oracle file for the network (ledger entries and
    /// other values that are not a transaction or a ledger).
    pub without_chain: usize,
    pub without_oracle: usize,
    pub manifest_sha256: String,
}

pub struct Filter<'a> {
    pub families: &'a [String],
    pub chains: &'a [String],
    pub networks: &'a [String],
}

pub fn select(corpus: &Path, filter: &Filter<'_>) -> Result<Selection> {
    let path = corpus.join("manifest.json");
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let manifest = shapekit_manifest::build::load(&path)
        .with_context(|| format!("loading {}", path.display()))?;
    let mut sel = Selection {
        manifest_sha256: shapekit_manifest::sha256_hex(&bytes),
        ..Selection::default()
    };
    for f in filter.families {
        if !manifest.entries.iter().any(|e| &e.family == f) {
            bail!("no corpus entry belongs to family {f}");
        }
    }
    let mut jobs: BTreeMap<(String, String), Vec<EntryRef>> = BTreeMap::new();
    for e in &manifest.entries {
        if !filter.families.is_empty() && !filter.families.contains(&e.family) {
            continue;
        }
        let chain = match (&e.source.chain, e.source.ledger_seq) {
            (Some(c), Some(l)) => (c.clone(), l),
            _ => {
                if filter.chains.is_empty() {
                    sel.without_chain += 1;
                }
                continue;
            }
        };
        if !filter.chains.is_empty() && !filter.chains.contains(&chain.0) {
            continue;
        }
        for network in filter.networks {
            let variant = match network.as_str() {
                "testnet" => e.variants.testnet.as_ref(),
                "mainnet" => e.variants.mainnet.as_ref(),
                other => bail!("unknown network {other:?}"),
            };
            match variant.and_then(|v| v.oracle_file.as_ref()) {
                Some(f) => jobs
                    .entry((chain.0.clone(), network.clone()))
                    .or_default()
                    .push(EntryRef {
                        id: e.id.clone(),
                        family: e.family.clone(),
                        ledger: chain.1,
                        oracle_file: corpus.join(f),
                    }),
                None => sel.without_oracle += 1,
            }
        }
    }
    for c in filter.chains {
        if !jobs.keys().any(|(chain, _)| chain == c) {
            bail!("chain {c} has no entry with an oracle file for the selected networks");
        }
    }
    for ((chain, network), mut entries) in jobs {
        entries.sort_by(|a, b| (a.ledger, &a.id).cmp(&(b.ledger, &b.id)));
        let dir = corpus.join("chains").join(&chain).join(&network);
        sel.jobs.push(ChainJob {
            chain,
            network,
            dir,
            entries,
        });
    }
    Ok(sel)
}
