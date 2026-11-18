//! Entries for recorded explorer chains: the ledgers of a chain that hold its union arms,
//! written as `LedgerCloseMeta` entries of a candidate lane.
//!
//! A chain recorded from an explorer recipe holds the arms the run credited to it and usually
//! more. A greedy cover picks the fewest ledgers that together hold every arm the chain holds:
//! the ledger with the most arms not yet held by a picked one first, the lower sequence number
//! on a tie. Each picked ledger is one entry, with both passphrases' canonical bytes from the
//! chain's lakes and its notes naming the arms it was picked for. Both passphrases' ledgers
//! must hold the same arms; a chain where they differ is refused.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use shapekit_gen::golden::{decode_lcm, lcm_version, ledger_version, tx_meta_version};
use shapekit_gen::recipe::Network;
use shapekit_manifest::arm_caps::ArmCaps;
use shapekit_manifest::build::{ENTRIES_DIR, ENTRY_FILE};
use shapekit_manifest::xdr::render_json;
use shapekit_manifest::{
    DecodeExpectation, Entry, EntryKind, EntryProtocol, Expected, Normalization, Source, Validity,
    Variant, Variants, decode_limits, sha256_hex, to_xdr_json,
};
use shapekit_pack::Lake;
use shapekit_pack::chain::LEDGERS_DIR;
use shapekit_registry::SourceClass;

use crate::arms::{arms_in, ledger_signature};

const LCM: &str = "LedgerCloseMeta";

/// One ledger of a chain under both passphrases.
#[derive(Debug, Clone)]
pub struct Ledger {
    pub seq: u32,
    pub protocol: u32,
    pub lcm_version: u32,
    pub tx_meta_version: Option<u32>,
    pub arms: BTreeSet<String>,
    pub bytes: BTreeMap<Network, Vec<u8>>,
}

fn read_lake(dir: &Path) -> Result<Vec<(u32, Vec<u8>)>> {
    let lake = Lake::open(dir).map_err(|e| anyhow::anyhow!("{}: {e}", dir.display()))?;
    lake.ledgers()
        .map(|l| {
            l.map(|l| (l.seq, l.xdr))
                .map_err(|e| anyhow::anyhow!("{}: {e}", dir.display()))
        })
        .collect()
}

/// Every ledger of the committed chain at `chain`, with the reachable arms it holds. The
/// testnet and mainnet lakes must cover the same ledgers and hold the same arms in each.
pub fn read_chain(chain: &Path, reachable: &BTreeSet<String>) -> Result<Vec<Ledger>> {
    let mut by_network = BTreeMap::new();
    for n in Network::BOTH {
        by_network.insert(n, read_lake(&chain.join(n.name()).join(LEDGERS_DIR))?);
    }
    let testnet = &by_network[&Network::Testnet];
    let mainnet = &by_network[&Network::Mainnet];
    ensure!(
        testnet.iter().map(|l| l.0).eq(mainnet.iter().map(|l| l.0)),
        "{}: the testnet and mainnet lakes hold different ledgers",
        chain.display()
    );
    let mut out = Vec::new();
    for ((seq, t), (_, m)) in testnet.iter().zip(mainnet) {
        let mut arms_by = BTreeMap::new();
        let mut first = None;
        for (n, bytes) in [(Network::Testnet, t), (Network::Mainnet, m)] {
            let lcm = decode_lcm(bytes)
                .with_context(|| format!("{} ledger {seq} ({})", chain.display(), n.name()))?;
            arms_by.insert(n, arms_in(&ledger_signature(&lcm)?, reachable));
            if first.is_none() {
                first = Some((
                    ledger_version(&lcm),
                    lcm_version(&lcm),
                    tx_meta_version(&lcm),
                ));
            }
        }
        let (tn, mn) = (&arms_by[&Network::Testnet], &arms_by[&Network::Mainnet]);
        if tn != mn {
            bail!(
                "{} ledger {seq}: the passphrases' ledgers hold different arms: testnet only {:?}, mainnet only {:?}",
                chain.display(),
                tn.difference(mn).collect::<Vec<_>>(),
                mn.difference(tn).collect::<Vec<_>>()
            );
        }
        let (protocol, lcm_version, tx_meta_version) = first.expect("two networks were read");
        out.push(Ledger {
            seq: *seq,
            protocol,
            lcm_version,
            tx_meta_version,
            arms: arms_by.remove(&Network::Testnet).expect("read above"),
            bytes: BTreeMap::from([(Network::Testnet, t.clone()), (Network::Mainnet, m.clone())]),
        });
    }
    ensure!(!out.is_empty(), "{} holds no ledgers", chain.display());
    Ok(out)
}

/// A picked ledger and the arms it was picked for; empty for a ledger kept for another
/// reason (the last ledger of a pair).
#[derive(Debug, Clone)]
pub struct Pick<'a> {
    pub ledger: &'a Ledger,
    pub credited: BTreeSet<String>,
}

/// The greedy cover, in ledger order, of `arms` (every arm the ledgers hold when `None`),
/// plus the last ledger when `keep_last` is set. An arm of `arms` no ledger holds is an error:
/// the chain does not show what its run credited to it.
pub fn cover<'a>(
    ledgers: &'a [Ledger],
    arms: Option<&BTreeSet<String>>,
    keep_last: bool,
) -> Result<Vec<Pick<'a>>> {
    let held: BTreeSet<String> = ledgers.iter().flat_map(|l| l.arms.clone()).collect();
    let mut left = match arms {
        None => held,
        Some(want) => {
            let missing: Vec<&String> = want.difference(&held).collect();
            ensure!(
                missing.is_empty(),
                "no ledger of the chain holds {missing:?}"
            );
            want.clone()
        }
    };
    let mut picks: BTreeMap<u32, Pick<'_>> = BTreeMap::new();
    while !left.is_empty() {
        // most new arms first; max_by_key keeps the last maximum, so iterate in reverse to
        // keep the lowest sequence number on a tie
        let best = ledgers
            .iter()
            .rev()
            .max_by_key(|l| l.arms.intersection(&left).count())
            .expect("left is not empty, so some ledger holds an arm");
        let credited: BTreeSet<String> = best.arms.intersection(&left).cloned().collect();
        for a in &credited {
            left.remove(a);
        }
        picks.insert(
            best.seq,
            Pick {
                ledger: best,
                credited,
            },
        );
    }
    if keep_last && let Some(last) = ledgers.last() {
        picks.entry(last.seq).or_insert(Pick {
            ledger: last,
            credited: BTreeSet::new(),
        });
    }
    Ok(picks.into_values().collect())
}

/// Where the entries of one chain come from.
#[derive(Debug, Clone)]
pub struct ChainSource {
    /// The chain id, the name of its directory.
    pub chain_id: String,
    /// The recipe the chain was recorded from, relative to the repository root.
    pub recipe: String,
    pub family: String,
    /// What the entry ids of this chain start with, after the family.
    pub slug: String,
    /// One sentence naming where the recipe came from, the start of every entry's notes.
    pub about: String,
}

fn variants() -> Variants {
    let v = |p: &str| Variant {
        network_passphrase: Some(p.to_owned()),
        xdr_file: String::new(),
        xdr_sha256: String::new(),
        expected_json_file: None,
        expected_json_sha256: None,
        oracle_file: None,
        oracle_sha256: None,
    };
    Variants {
        testnet: Some(v(shapekit_manifest::TESTNET_PASSPHRASE)),
        mainnet: Some(v(shapekit_manifest::MAINNET_PASSPHRASE)),
        ..Default::default()
    }
}

/// The entry record of one picked ledger, before its files are written. Its `arms` are the
/// arms it was picked for (every arm the ledger holds for one kept for another reason), and
/// its caps the CAPs that introduced them.
pub fn entry(src: &ChainSource, pick: &Pick<'_>, arm_caps: &ArmCaps) -> Result<Entry> {
    let l = pick.ledger;
    let arms: Vec<String> = if pick.credited.is_empty() {
        l.arms.iter().cloned().collect()
    } else {
        pick.credited.iter().cloned().collect()
    };
    let caps = arm_caps.for_arms(&arms).map_err(|unknown| {
        anyhow::anyhow!(
            "ledger {} of chain {} holds arms the xdr definitions do not name: {}",
            l.seq,
            src.chain_id,
            unknown.join(", ")
        )
    })?;
    let held = if pick.credited.is_empty() {
        "kept as the last ledger, the one the pair compares".to_owned()
    } else {
        format!(
            "picked for the arms it holds that no earlier pick holds: {}",
            pick.credited.iter().cloned().collect::<Vec<_>>().join(", ")
        )
    };
    Ok(Entry {
        id: format!("{}.{}-l{}", src.family, src.slug, l.seq),
        kind: EntryKind::Xdr,
        family: src.family.clone(),
        caps,
        protocol: EntryProtocol {
            introduced: 0,
            generated_at: Some(l.protocol),
            valid_min: l.protocol,
            valid_max: None,
        },
        xdr_type: Some(LCM.to_owned()),
        lcm_version: Some(l.lcm_version),
        tx_meta_version: l.tx_meta_version,
        source: Source {
            class: SourceClass::LocalCore,
            recipe: Some(src.recipe.clone()),
            chain: Some(src.chain_id.clone()),
            ledger_seq: Some(l.seq),
            tx_hash: None,
            tx_index: None,
            upstream: None,
            provenance_url: None,
            normalization: Some(Normalization::KeyStable),
            core_rejection: None,
            recipe_ir: None,
        },
        synthetic: false,
        passphrase_dependent: l.bytes[&Network::Testnet] != l.bytes[&Network::Mainnet],
        variants: variants(),
        vector: None,
        expected: Expected {
            validity: Validity::Valid,
            decode: DecodeExpectation::MustAccept,
            roundtrip: true,
        },
        flags: Vec::new(),
        fields_exercised: Vec::new(),
        arms,
        linked_issues: Vec::new(),
        notes: Some(format!(
            "{} ledger {} of chain {} at protocol {}, recorded from {}; {}",
            src.about, l.seq, src.chain_id, l.protocol, src.recipe, held
        )),
        obligations: Vec::new(),
    })
}

fn read_entry(path: &Path) -> Result<Entry> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

/// Removes the entries of `family` under `lane` that name `chain_id`, so a second run writes
/// the same set rather than adding to it. Returns how many were removed.
pub fn remove_chain_entries(lane: &Path, family: &str, chain_id: &str) -> Result<usize> {
    let dir = lane.join(ENTRIES_DIR).join(family);
    if !dir.exists() {
        return Ok(0);
    }
    let mut removed = 0;
    for item in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = item?.path();
        let file = path.join(ENTRY_FILE);
        if !file.is_file() {
            continue;
        }
        if read_entry(&file)?.source.chain.as_deref() == Some(chain_id) {
            std::fs::remove_dir_all(&path)
                .with_context(|| format!("removing {}", path.display()))?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Writes one entry directory under `lane`: per passphrase `<network>.xdr` and
/// `<network>.expected.json`, then `entry.json`.
pub fn write(lane: &Path, mut entry: Entry, pick: &Pick<'_>) -> Result<PathBuf> {
    shapekit_gen::entry_io::check_entry_path(&entry.family, &entry.id)?;
    let rel_dir = format!("{ENTRIES_DIR}/{}/{}", entry.family, entry.id);
    let dir = lane.join(&rel_dir);
    ensure!(!dir.exists(), "{} already exists", dir.display());
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    for (n, bytes) in &pick.ledger.bytes {
        let xdr_name = format!("{}.xdr", n.name());
        let json_name = format!("{}.expected.json", n.name());
        let json = to_xdr_json(LCM, bytes, decode_limits(LCM, bytes.len()))
            .with_context(|| format!("{}: rendering XDR-JSON", entry.id))?;
        std::fs::write(dir.join(&xdr_name), bytes)?;
        std::fs::write(dir.join(&json_name), json.as_bytes())?;
        let slot = match n {
            Network::Testnet => &mut entry.variants.testnet,
            Network::Mainnet => &mut entry.variants.mainnet,
        };
        let v = slot.as_mut().expect("both variants are set");
        v.xdr_file = format!("{rel_dir}/{xdr_name}");
        v.xdr_sha256 = sha256_hex(bytes);
        v.expected_json_file = Some(format!("{rel_dir}/{json_name}"));
        v.expected_json_sha256 = Some(sha256_hex(json.as_bytes()));
    }
    let path = dir.join(ENTRY_FILE);
    std::fs::write(&path, render_json(&serde_json::to_value(&entry)?))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger(seq: u32, arms: &[&str]) -> Ledger {
        Ledger {
            seq,
            protocol: 30,
            lcm_version: 2,
            tx_meta_version: None,
            arms: arms.iter().map(|a| (*a).to_owned()).collect(),
            bytes: BTreeMap::from([
                (Network::Testnet, vec![seq as u8]),
                (Network::Mainnet, vec![seq as u8]),
            ]),
        }
    }

    #[test]
    fn the_cover_takes_the_fewest_ledgers_and_the_lower_one_on_a_tie() {
        let ledgers = vec![
            ledger(2, &["A"]),
            ledger(3, &["A", "B"]),
            ledger(4, &["A", "B"]),
            ledger(5, &["C"]),
            ledger(6, &[]),
        ];
        let picks = cover(&ledgers, None, false).unwrap();
        let got: Vec<(u32, Vec<&str>)> = picks
            .iter()
            .map(|p| {
                (
                    p.ledger.seq,
                    p.credited.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(got, vec![(3, vec!["A", "B"]), (5, vec!["C"])]);
        let with_last = cover(&ledgers, None, true).unwrap();
        assert_eq!(with_last.last().unwrap().ledger.seq, 6);
        assert!(with_last.last().unwrap().credited.is_empty());
        let only_c = cover(&ledgers, Some(&BTreeSet::from(["C".to_owned()])), false).unwrap();
        assert_eq!(only_c.len(), 1);
        assert_eq!(only_c[0].ledger.seq, 5);
        assert!(cover(&ledgers, Some(&BTreeSet::from(["D".to_owned()])), false).is_err());
    }
}
