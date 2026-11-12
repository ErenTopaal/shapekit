//! `shapekit oracle run`, `check` and `interpret`.

use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use shapekit_manifest::xdr::render_json;
use shapekit_oracle::conservation::check_ledger;
use shapekit_oracle::corpus;
use shapekit_oracle::{
    LcmStream, LedgerState, Network, ORACLE_VOCAB_VERSION, OracleFile, decode_lcm, interpret_ledger,
};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum StartState {
    /// A fresh network: the root account holds every lumen.
    Genesis,
    /// Genesis as a core test application creates it (core goldens).
    CoreTestGenesis,
    /// Nothing known before the first ledger.
    Unknown,
}

pub fn run(corpus_dir: &Path) -> Result<bool> {
    let summary = corpus::run(corpus_dir)
        .with_context(|| format!("running the oracle over {}", corpus_dir.display()))?;
    for (id, variant, path) in &summary.written {
        println!("wrote {path} ({id} {variant})");
    }
    for (id, reason) in &summary.not_eligible {
        println!("not eligible: {id}: {reason}");
    }
    println!(
        "oracle vocabulary v{ORACLE_VOCAB_VERSION}: {} files written, {} entries not eligible",
        summary.written.len(),
        summary.not_eligible.len()
    );
    Ok(true)
}

/// Applies a framed stream to a ledger state and checks conservation and interpretation
/// on every ledger.
pub fn check(file: &Path, passphrase: &str, start: StartState) -> Result<bool> {
    let network = Network::new(passphrase);
    let mut state = match start {
        StartState::Genesis => LedgerState::genesis(&network),
        StartState::CoreTestGenesis => LedgerState::core_test_genesis(&network),
        StartState::Unknown => LedgerState::partial(),
    };
    let (mut ledgers, mut checked, mut unchecked, mut txs) = (0u64, 0u64, 0u64, 0usize);
    for item in LcmStream::open(file)? {
        let item = item?;
        let prev = state.header().cloned();
        let report = state.apply_ledger(&item.meta, &network)?;
        let cons = check_ledger(&item.meta, &network, prev.as_ref())?;
        let facts = interpret_ledger(&item.meta, &network)?;
        ledgers += 1;
        checked += report.checked;
        unchecked += report.unchecked;
        txs += cons.transactions_checked;
        if facts.transactions.len() != cons.transactions_checked + cons.inflation_txs {
            bail!(
                "ledger {}: interpretation and conservation disagree on the transaction count",
                report.ledger_seq
            );
        }
    }
    println!(
        "{}: {ledgers} ledgers, {txs} transactions conserve lumens, {checked} changes checked against state, {unchecked} unchecked",
        file.display()
    );
    Ok(true)
}

/// Prints the oracle file for one ledger (`--framed` for a record-marked stream).
pub fn interpret(file: &Path, passphrase: &str, framed: bool) -> Result<bool> {
    let network = Network::new(passphrase);
    let mut ledgers = Vec::new();
    if framed {
        for item in LcmStream::open(file)? {
            ledgers.push(interpret_ledger(&item?.meta, &network)?);
        }
    } else {
        let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
        let lcm = decode_lcm(&bytes).context("decoding LedgerCloseMeta")?;
        ledgers.push(interpret_ledger(&lcm, &network)?);
    }
    let out = OracleFile {
        oracle_vocab_version: ORACLE_VOCAB_VERSION,
        network_passphrase: passphrase.to_string(),
        ledgers,
    };
    print!("{}", render_json(&serde_json::to_value(&out)?));
    Ok(true)
}
