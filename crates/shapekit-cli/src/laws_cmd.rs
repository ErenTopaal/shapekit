//! `shapekit laws`: the law book over ledger files or the corpus.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use shapekit_laws::{Checker, Input, Report};

#[derive(clap::Subcommand)]
pub enum LawsCommand {
    /// Check every law on LedgerCloseMeta files, or on every LedgerCloseMeta entry of the
    /// corpus. Exits 1 when a law fails.
    Check {
        /// LedgerCloseMeta files, checked in the order given; consecutive ledgers are also
        /// checked against each other, and a ledger that does not follow the one before it
        /// (a gap, a repeat or a step back) counts as undecided for header-chain.
        /// zstd-compressed files are decompressed first.
        #[arg(long, num_args = 1.., required_unless_present = "corpus", conflicts_with = "corpus")]
        lcm: Vec<PathBuf>,
        /// The network passphrase the ledgers were closed under.
        #[arg(long, required_unless_present = "corpus")]
        passphrase: Option<String>,
        /// The files hold a LedgerCloseMetaBatch, as SEP-54 data lake objects do (implied for
        /// files ending in .zst).
        #[arg(long)]
        batch: bool,
        /// A corpus directory: check each variant of each LedgerCloseMeta entry under the
        /// passphrase the manifest gives for it.
        #[arg(long)]
        corpus: Option<PathBuf>,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
    },
}

pub fn run(command: LawsCommand) -> Result<bool> {
    match command {
        LawsCommand::Check {
            lcm,
            passphrase,
            batch,
            corpus,
            json,
        } => match corpus {
            Some(dir) => check_corpus(&dir, json),
            None => {
                let passphrase = passphrase.context("--passphrase is required with --lcm")?;
                check_files(&lcm, &passphrase, batch, json)
            }
        },
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    let raw = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    shapekit_laws::read_input(raw).with_context(|| format!("decompressing {}", path.display()))
}

fn check_files(files: &[PathBuf], passphrase: &str, batch: bool, json: bool) -> Result<bool> {
    let mut checker = Checker::new(passphrase);
    for f in files {
        let input = if batch || f.extension().is_some_and(|e| e == "zst") {
            Input::Batch
        } else {
            Input::Ledger
        };
        let bytes = read(f)?;
        checker
            .bytes(&bytes, input)
            .with_context(|| format!("{} does not decode as {input:?}", f.display()))?;
    }
    let report = checker.report;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(&report);
    }
    Ok(report.holds())
}

fn print_report(r: &Report) {
    let envelopes: Vec<String> = r
        .envelopes
        .iter()
        .map(|(k, n)| format!("{n} {k}"))
        .collect();
    println!(
        "{} ledgers, {} protocol versions, {} transactions, envelopes: {}",
        r.ledgers,
        r.protocols.len(),
        r.transactions,
        if envelopes.is_empty() {
            "none".to_string()
        } else {
            envelopes.join(", ")
        }
    );
    for (law, t) in &r.laws {
        let undecided = r
            .undecided
            .get(law)
            .map(|n| format!(", {n} undecided"))
            .unwrap_or_default();
        println!(
            "  {:<22} {} checked, {} violated{undecided}",
            law.as_str(),
            t.checked,
            t.violated
        );
    }
    for v in r.violations.iter().take(50) {
        let at = v
            .ledger
            .map(|s| format!("ledger {s}: "))
            .unwrap_or_default();
        println!("  violation {}: {at}{}", v.law.as_str(), v.detail);
    }
    if r.violations.len() > 50 {
        println!("  ... {} more violations", r.violations.len() - 50);
    }
}

#[derive(Serialize)]
struct EntryViolation {
    entry: String,
    variant: String,
    synthetic: bool,
    law: &'static str,
    ledger: Option<u32>,
    detail: String,
}

fn check_corpus(dir: &Path, json: bool) -> Result<bool> {
    let manifest_path = dir.join("manifest.json");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parsing {}", manifest_path.display()))?;
    let Some(entries) = manifest["entries"].as_array() else {
        bail!("{} has no entries array", manifest_path.display());
    };
    let mut total = Report::new();
    let mut found = Vec::new();
    let mut checked_entries = 0;
    let mut checked_variants = 0;
    for e in entries {
        if e["xdr_type"] != "LedgerCloseMeta" {
            continue;
        }
        let id = e["id"].as_str().context("an entry without an id")?;
        let synthetic = e["synthetic"].as_bool().unwrap_or(false);
        let Some(variants) = e["variants"].as_object() else {
            bail!("entry {id} has no variants");
        };
        checked_entries += 1;
        for (name, v) in variants {
            let file = v["xdr_file"]
                .as_str()
                .with_context(|| format!("{id} variant {name} has no xdr_file"))?;
            let bytes = read(&dir.join(file))?;
            // a passphrase-independent entry has no network; its network-hashed laws stay
            // undecided
            let mut checker = match v["network_passphrase"].as_str() {
                Some(p) => Checker::new(p),
                None if e["passphrase_dependent"] == false => Checker::without_network(),
                None => bail!("{id} variant {name} has no network_passphrase"),
            };
            checker
                .bytes(&bytes, Input::Ledger)
                .with_context(|| format!("{id} variant {name} does not decode"))?;
            checked_variants += 1;
            for viol in &checker.report.violations {
                found.push(EntryViolation {
                    entry: id.to_string(),
                    variant: name.clone(),
                    synthetic,
                    law: viol.law.as_str(),
                    ledger: viol.ledger,
                    detail: viol.detail.clone(),
                });
            }
            total.merge(checker.report);
        }
    }
    let unexplained = found.iter().filter(|v| !v.synthetic).count();
    if json {
        let mut by_law: BTreeMap<&str, usize> = BTreeMap::new();
        for v in &found {
            *by_law.entry(v.law).or_default() += 1;
        }
        let out = serde_json::json!({
            "entries": checked_entries,
            "variants": checked_variants,
            "report": total,
            "violations_by_law": by_law,
            "entry_violations": found,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("{checked_entries} LedgerCloseMeta entries, {checked_variants} variants");
        let mut shown = total.clone();
        shown.violations.clear();
        print_report(&shown);
        for v in &found {
            let kind = if v.synthetic {
                "synthetic entry"
            } else {
                "entry"
            };
            println!(
                "  violation {} in {kind} {} ({}): {}",
                v.law, v.entry, v.variant, v.detail
            );
        }
        println!(
            "{} violations, {} in synthetic entries, {unexplained} in recorded entries",
            found.len(),
            found.len() - unexplained
        );
    }
    Ok(unexplained == 0)
}
