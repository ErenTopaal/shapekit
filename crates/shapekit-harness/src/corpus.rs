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
    /// What the manifest records for the oracle file; the run refuses other bytes.
    pub oracle_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChainJob {
    pub chain: String,
    pub network: String,
    pub dir: PathBuf,
    /// Sorted by ledger, then id.
    pub entries: Vec<EntryRef>,
    /// A chain outside the corpus, named on the command line and run for its dump alone.
    pub external: bool,
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
    /// The manifest's replay digest ([`shapekit_manifest::replay_digest`]).
    pub manifest_replay_digest: Option<String>,
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
        manifest_replay_digest: Some(shapekit_manifest::replay_digest(&manifest)),
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
            // the chain name is one directory under chains/, never a path out of it
            (Some(c), Some(_)) if !shapekit_manifest::is_safe_relative(c) || c.contains('/') => {
                bail!("{}: chain {c:?} is not a directory name", e.id)
            }
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
            match variant.and_then(|v| v.oracle_file.as_ref().map(|f| (f, v))) {
                Some((f, v)) => {
                    if !shapekit_manifest::is_safe_relative(f) {
                        bail!(
                            "{}: oracle_file {f:?} is not a path inside the corpus",
                            e.id
                        );
                    }
                    jobs.entry((chain.0.clone(), network.clone()))
                        .or_default()
                        .push(EntryRef {
                            id: e.id.clone(),
                            family: e.family.clone(),
                            ledger: chain.1,
                            oracle_file: corpus.join(f),
                            oracle_sha256: v.oracle_sha256.clone(),
                        })
                }
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
            external: false,
        });
    }
    Ok(sel)
}

/// A `--shard` of a selection: `index` of `count`, 1-based. The selection's chains, sorted by
/// name, are dealt out in turn, so a chain's networks stay in one shard, every chain is in
/// exactly one shard, and the shards of one corpus hold chain counts that differ by one at most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shard {
    pub index: u32,
    pub count: u32,
}

impl std::str::FromStr for Shard {
    type Err = String;

    fn from_str(s: &str) -> Result<Shard, String> {
        let (i, n) = s
            .split_once('/')
            .ok_or_else(|| format!("{s:?} is not <index>/<count>, for example 2/4"))?;
        let index: u32 = i
            .parse()
            .map_err(|_| format!("{i:?} is not a shard index"))?;
        let count: u32 = n
            .parse()
            .map_err(|_| format!("{n:?} is not a shard count"))?;
        if count == 0 || index == 0 || index > count {
            return Err(format!(
                "shard {index}/{count}: the index runs from 1 to the count"
            ));
        }
        Ok(Shard { index, count })
    }
}

/// Keeps the jobs of `shard`. The counts of entries without a chain or an oracle file stay
/// those of the whole selection; `report::merge` takes them from the whole selection too.
pub fn shard(sel: &mut Selection, shard: Shard) {
    let mut chains: Vec<&str> = sel.jobs.iter().map(|j| j.chain.as_str()).collect();
    chains.dedup();
    let keep: std::collections::BTreeSet<String> = chains
        .iter()
        .enumerate()
        .filter(|(i, _)| *i as u32 % shard.count == shard.index - 1)
        .map(|(_, c)| (*c).to_owned())
        .collect();
    sel.jobs.retain(|j| keep.contains(&j.chain));
}

#[cfg(test)]
mod shard_tests {
    use super::*;

    fn job(chain: &str, network: &str) -> ChainJob {
        ChainJob {
            chain: chain.into(),
            network: network.into(),
            dir: PathBuf::new(),
            entries: Vec::new(),
            external: false,
        }
    }

    #[test]
    fn shards_partition_the_chains_and_keep_networks_together() {
        let all = Selection {
            jobs: ["a", "b", "c", "d", "e"]
                .iter()
                .flat_map(|c| [job(c, "mainnet"), job(c, "testnet")])
                .collect(),
            ..Selection::default()
        };
        let mut seen = Vec::new();
        for index in 1..=3 {
            let mut s = all.clone();
            shard(&mut s, Shard { index, count: 3 });
            let chains: std::collections::BTreeSet<_> =
                s.jobs.iter().map(|j| j.chain.clone()).collect();
            assert_eq!(
                s.jobs.len(),
                chains.len() * 2,
                "both networks of a chain in one shard"
            );
            seen.extend(chains);
        }
        seen.sort();
        assert_eq!(seen, ["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn a_shard_is_index_over_count() {
        assert_eq!("2/4".parse::<Shard>(), Ok(Shard { index: 2, count: 4 }));
        for bad in ["0/4", "5/4", "1/0", "4", "a/b"] {
            assert!(bad.parse::<Shard>().is_err(), "{bad}");
        }
    }
}
