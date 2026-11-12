//! `shapekit oracle run`: writes `<variant>.oracle.json` next to every eligible corpus entry
//! and records it in the entry's `entry.json` (and in `manifest.json` when one exists).
//!
//! Eligible entries:
//! - `xdr_type = LedgerCloseMeta`: the oracle file holds that ledger's facts;
//! - transaction-level entries (`TransactionEnvelope`, `TransactionMeta`,
//!   `TransactionResultMeta`, `TransactionResultMetaV1`, `TransactionResultPair`) whose
//!   source names a chain and a ledger: the ledger is read from
//!   `chains/<chain>/<variant>/ledgers.xdr`, and the transaction is the one whose XDR in that
//!   ledger is byte-equal to the variant's bytes, which needs no per-variant hash.
//!
//! Every ledger read must also pass the conservation check before its facts are written.
//! Both need `expected.validity = valid`: an entry core would reject or never emit has no
//! applied ledger to describe. Everything else is reported as not eligible, with the reason.

use std::path::{Path, PathBuf};

use shapekit_manifest::build::{self, ENTRIES_DIR, ENTRY_FILE};
use shapekit_manifest::xdr::render_json;
use shapekit_manifest::{
    Entry, EntryKind, MAINNET_PASSPHRASE, TESTNET_PASSPHRASE, Variant, sha256_hex,
};
use stellar_xdr::{LedgerCloseMeta, Limits, WriteXdr};

use crate::conservation::check_ledger;
use crate::decode::{LcmStream, decode_lcm};
use crate::interpret::interpret_ledger;
use crate::network::Network;
use crate::view::LedgerView;
use crate::vocab::{LedgerFacts, ORACLE_VOCAB_VERSION, OracleFile};

pub const CHAINS_DIR: &str = "chains";
pub const CHAIN_STREAM: &str = "ledgers.xdr";
pub const CHAIN_FILE: &str = "chain.json";

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("{path}: {message}")]
    Entry { path: String, message: String },
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error(transparent)]
    Build(#[from] build::BuildError),
}

#[derive(Debug, Default)]
pub struct RunSummary {
    /// (entry id, variant, oracle file path relative to the corpus).
    pub written: Vec<(String, String, String)>,
    /// (entry id, reason).
    pub not_eligible: Vec<(String, String)>,
}

pub fn run(corpus: &Path) -> Result<RunSummary, RunError> {
    let mut summary = RunSummary::default();
    let mut files = Vec::new();
    let root = corpus.join(ENTRIES_DIR);
    if root.exists() {
        collect_entry_files(&root, &mut files)?;
    }
    files.sort();
    for file in files {
        let rel_dir = file
            .parent()
            .and_then(|p| p.strip_prefix(corpus).ok())
            .map(|p| {
                p.components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .ok_or_else(|| entry_err(&file, "entry is outside the corpus".into()))?;
        let bytes = read(&file)?;
        let mut entry: Entry = serde_json::from_slice(&bytes)
            .map_err(|e| entry_err(&file, format!("parsing: {e}")))?;
        let level = match eligibility(&entry) {
            Ok(l) => l,
            Err(reason) => {
                summary.not_eligible.push((entry.id.clone(), reason));
                continue;
            }
        };
        let mut changed = false;
        let names: Vec<&'static str> = entry.variants.iter().map(|(n, _)| n).collect();
        for name in names {
            let variant = variant_mut(&mut entry, name);
            let passphrase = passphrase_for(name, variant, &level, corpus, &file)?;
            let network = Network::new(&passphrase);
            let xdr = read(&corpus.join(&variant.xdr_file))?;
            let ledger = match &level {
                Level::Ledger => {
                    let lcm = decode_lcm(&xdr).map_err(|e| {
                        entry_err(&file, format!("{name}: decoding LedgerCloseMeta: {e}"))
                    })?;
                    interpret_ledger(&lcm, &network)
                        .map_err(|e| entry_err(&file, format!("{name}: {e}")))?
                }
                Level::Transaction {
                    chain,
                    ledger_seq,
                    xdr_type,
                } => {
                    let lcm = chain_ledger(corpus, chain, name, *ledger_seq)
                        .map_err(|m| entry_err(&file, format!("{name}: {m}")))?;
                    check_ledger(&lcm, &network, None)
                        .map_err(|e| entry_err(&file, format!("{name}: {e}")))?;
                    let index = find_tx(&lcm, &network, xdr_type, &xdr)
                        .map_err(|m| entry_err(&file, format!("{name}: {m}")))?;
                    let mut facts = interpret_ledger(&lcm, &network)
                        .map_err(|e| entry_err(&file, format!("{name}: {e}")))?;
                    let tx = facts.transactions.swap_remove(index);
                    LedgerFacts {
                        transactions: vec![tx],
                        upgrades: vec![],
                        evicted: vec![],
                        ..facts
                    }
                }
            };
            let out = OracleFile {
                oracle_vocab_version: ORACLE_VOCAB_VERSION,
                network_passphrase: passphrase,
                ledgers: vec![ledger],
            };
            let text =
                render_json(&serde_json::to_value(&out).expect("oracle output is plain json"));
            let rel = format!("{rel_dir}/{name}.oracle.json");
            write(&corpus.join(&rel), text.as_bytes())?;
            let sha = sha256_hex(text.as_bytes());
            let variant = variant_mut(&mut entry, name);
            if variant.oracle_file.as_deref() != Some(rel.as_str())
                || variant.oracle_sha256.as_deref() != Some(sha.as_str())
            {
                variant.oracle_file = Some(rel.clone());
                variant.oracle_sha256 = Some(sha);
                changed = true;
            }
            summary
                .written
                .push((entry.id.clone(), name.to_string(), rel));
        }
        if changed {
            let text = render_json(&serde_json::to_value(&entry).expect("an entry is plain json"));
            write(&file, text.as_bytes())?;
        }
    }
    let manifest_path = corpus.join("manifest.json");
    if manifest_path.exists() {
        let mut manifest = build::load(&manifest_path)?;
        for e in &mut manifest.entries {
            for (id, name, rel) in &summary.written {
                if &e.id != id {
                    continue;
                }
                let sha = sha256_hex(&read(&corpus.join(rel))?);
                let v = variant_mut(e, name);
                v.oracle_file = Some(rel.clone());
                v.oracle_sha256 = Some(sha);
            }
        }
        write(&manifest_path, build::render(&manifest).as_bytes())?;
    }
    Ok(summary)
}

enum Level {
    Ledger,
    Transaction {
        chain: String,
        ledger_seq: u32,
        xdr_type: String,
    },
}

const TX_TYPES: [&str; 5] = [
    "TransactionEnvelope",
    "TransactionMeta",
    "TransactionResultMeta",
    "TransactionResultMetaV1",
    "TransactionResultPair",
];

fn eligibility(e: &Entry) -> Result<Level, String> {
    if e.kind != EntryKind::Xdr {
        return Err(format!("kind {:?} has no ledger", e.kind));
    }
    if e.variants.is_empty() {
        return Err("no variants".into());
    }
    if e.expected.validity != shapekit_manifest::Validity::Valid {
        return Err(format!(
            "validity is {:?}: core never applied it, so there is nothing to interpret",
            e.expected.validity
        ));
    }
    if e.expected.decode != shapekit_manifest::DecodeExpectation::MustAccept {
        return Err("not a must-accept entry, so there is no applied ledger".into());
    }
    let t = e.xdr_type.as_deref().unwrap_or_default();
    if t == "LedgerCloseMeta" {
        return Ok(Level::Ledger);
    }
    if TX_TYPES.contains(&t) {
        return match (&e.source.chain, e.source.ledger_seq) {
            (Some(chain), Some(ledger_seq)) => Ok(Level::Transaction {
                chain: chain.clone(),
                ledger_seq,
                xdr_type: t.to_string(),
            }),
            _ => Err(format!(
                "{t} entry names no chain holding its ledger, so there is no applied ledger to read"
            )),
        };
    }
    Err(format!("xdr type {t} is not ledger or transaction level"))
}

fn variant_mut<'a>(e: &'a mut Entry, name: &str) -> &'a mut Variant {
    let slot = match name {
        "testnet" => &mut e.variants.testnet,
        "mainnet" => &mut e.variants.mainnet,
        _ => &mut e.variants.any,
    };
    slot.as_mut()
        .expect("the variant was listed by Variants::iter")
}

fn passphrase_for(
    name: &str,
    v: &Variant,
    level: &Level,
    corpus: &Path,
    file: &Path,
) -> Result<String, RunError> {
    if let Some(p) = &v.network_passphrase {
        return Ok(p.clone());
    }
    if let Level::Transaction { chain, .. } = level {
        let path = corpus
            .join(CHAINS_DIR)
            .join(chain)
            .join(name)
            .join(CHAIN_FILE);
        let json: serde_json::Value = serde_json::from_slice(&read(&path)?)
            .map_err(|e| entry_err(&path, format!("parsing: {e}")))?;
        return json
            .get("passphrase")
            .and_then(|p| p.as_str())
            .map(str::to_string)
            .ok_or_else(|| entry_err(&path, "no passphrase field".into()));
    }
    match name {
        "testnet" => Ok(TESTNET_PASSPHRASE.into()),
        "mainnet" => Ok(MAINNET_PASSPHRASE.into()),
        _ => Err(entry_err(
            file,
            "the any variant names no network_passphrase, so its transactions cannot be hashed"
                .into(),
        )),
    }
}

fn chain_ledger(
    corpus: &Path,
    chain: &str,
    variant: &str,
    seq: u32,
) -> Result<LedgerCloseMeta, String> {
    let path = corpus
        .join(CHAINS_DIR)
        .join(chain)
        .join(variant)
        .join(CHAIN_STREAM);
    for item in LcmStream::open(&path).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| format!("{}: {e}", path.display()))?;
        if crate::view::ledger_seq(&item.meta) == seq {
            return Ok(item.meta);
        }
    }
    Err(format!("{} has no ledger {seq}", path.display()))
}

fn find_tx(
    lcm: &LedgerCloseMeta,
    network: &Network,
    xdr_type: &str,
    bytes: &[u8],
) -> Result<usize, String> {
    let view = LedgerView::new(lcm, network).map_err(|e| e.to_string())?;
    let enc = |v: &dyn Fn() -> Result<Vec<u8>, stellar_xdr::Error>| v().map_err(|e| e.to_string());
    let mut found = Vec::new();
    for (i, tx) in view.txs.iter().enumerate() {
        let mine = match xdr_type {
            "TransactionEnvelope" => enc(&|| tx.envelope.to_xdr(Limits::none()))?,
            "TransactionMeta" => enc(&|| tx.meta.to_xdr(Limits::none()))?,
            "TransactionResultPair" => enc(&|| tx.result.to_xdr(Limits::none()))?,
            "TransactionResultMeta" => match lcm {
                LedgerCloseMeta::V0(m) => enc(&|| m.tx_processing[i].to_xdr(Limits::none()))?,
                LedgerCloseMeta::V1(m) => enc(&|| m.tx_processing[i].to_xdr(Limits::none()))?,
                LedgerCloseMeta::V2(_) => continue,
            },
            "TransactionResultMetaV1" => match lcm {
                LedgerCloseMeta::V2(m) => enc(&|| m.tx_processing[i].to_xdr(Limits::none()))?,
                _ => continue,
            },
            other => return Err(format!("{other} is not transaction level")),
        };
        if mine == bytes {
            found.push(i);
        }
    }
    match found.as_slice() {
        [i] => Ok(*i),
        [] => Err(format!(
            "no transaction in ledger {} matches the entry bytes",
            view.seq()
        )),
        _ => Err(format!(
            "{} transactions match the entry bytes",
            found.len()
        )),
    }
}

fn collect_entry_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), RunError> {
    let rd = std::fs::read_dir(dir).map_err(|source| RunError::Io {
        path: dir.display().to_string(),
        source,
    })?;
    for item in rd {
        let item = item.map_err(|source| RunError::Io {
            path: dir.display().to_string(),
            source,
        })?;
        let p = item.path();
        if p.is_dir() {
            collect_entry_files(&p, out)?;
        } else if p.file_name().and_then(|n| n.to_str()) == Some(ENTRY_FILE) {
            out.push(p);
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, RunError> {
    std::fs::read(path).map_err(|source| RunError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), RunError> {
    std::fs::write(path, bytes).map_err(|source| RunError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn entry_err(path: &Path, message: String) -> RunError {
    RunError::Entry {
        path: path.display().to_string(),
        message,
    }
}
