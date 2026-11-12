//! `shapekit pack`: chains in their committed form (SEP-54 lake, archive tar).

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use shapekit_core_driver::canonical::{canonicalize_lcm, permutation_equal};
use shapekit_manifest::framing::FramedReader;
use shapekit_pack::chain::{CHAIN_FILE, ChainFile, MAX_FRAME_LEN, open_lake};
use shapekit_pack::{Layout, Packed, archive_at, pack_chain, verify_chain};
use stellar_xdr::{Limits, WriteXdr};

#[derive(Subcommand)]
pub enum PackCommand {
    /// Convert recorded chains to the committed form: the canonical ledgers as a SEP-54
    /// lake in ledgers/, the history archive as archive.tar, core's raw stream kept.
    /// Chains already in that form are left alone.
    Sep54 {
        /// A chain directory (corpus/chains/<chain-id>/<network>); repeatable. Every chain
        /// in the corpus when absent.
        #[arg(long)]
        chain: Vec<PathBuf>,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
        #[arg(long, default_value_t = shapekit_pack::layout::DEFAULT_LEDGERS_PER_BATCH)]
        ledgers_per_batch: u32,
        #[arg(long, default_value_t = shapekit_pack::layout::DEFAULT_BATCHES_PER_PARTITION)]
        batches_per_partition: u32,
    },
    /// Check committed chains against their chain.json, and that the lake holds exactly the
    /// canonical form of core's raw stream.
    Verify {
        #[arg(long)]
        chain: Vec<PathBuf>,
        #[arg(long, default_value = "corpus")]
        corpus: PathBuf,
    },
    /// Write the history archive as it stood at one checkpoint into a new directory.
    ArchiveAt {
        #[arg(long)]
        chain: PathBuf,
        #[arg(long)]
        checkpoint: u32,
        #[arg(long)]
        out: PathBuf,
    },
}

pub fn run(cmd: PackCommand) -> Result<bool> {
    match cmd {
        PackCommand::Sep54 {
            chain,
            corpus,
            ledgers_per_batch,
            batches_per_partition,
        } => {
            let layout = Layout::new(ledgers_per_batch, batches_per_partition)?;
            for dir in chains(&chain, &corpus)? {
                match pack_chain(&dir, layout)
                    .with_context(|| format!("packing {}", dir.display()))?
                {
                    Packed::Converted { ledgers, batches } => println!(
                        "{}: {ledgers} ledgers in {batches} batches, archive bundled",
                        dir.display()
                    ),
                    Packed::AlreadyPacked => println!("{}: already packed", dir.display()),
                }
            }
            Ok(true)
        }
        PackCommand::Verify { chain, corpus } => {
            let mut clean = true;
            let dirs = chains(&chain, &corpus)?;
            for dir in &dirs {
                let mut problems =
                    verify_chain(dir).with_context(|| format!("verifying {}", dir.display()))?;
                problems.extend(canonical_matches_raw(dir)?);
                if problems.is_empty() {
                    println!("{}: ok", dir.display());
                } else {
                    clean = false;
                    for p in problems {
                        println!("{}: {p}", dir.display());
                    }
                }
            }
            println!("{} chains checked", dirs.len());
            Ok(clean)
        }
        PackCommand::ArchiveAt {
            chain,
            checkpoint,
            out,
        } => {
            let c = ChainFile::read(&chain)?;
            let files = archive_at(&chain.join(&c.archive.file), checkpoint, &out)?;
            println!(
                "{}: {} files of checkpoint {checkpoint}",
                out.display(),
                files.len()
            );
            Ok(true)
        }
    }
}

/// The given chain directories, or every `chains/<id>/<network>` with a chain.json.
fn chains(given: &[PathBuf], corpus: &Path) -> Result<Vec<PathBuf>> {
    if !given.is_empty() {
        return Ok(given.to_vec());
    }
    let root = corpus.join("chains");
    let mut out = Vec::new();
    for c in std::fs::read_dir(&root).with_context(|| format!("reading {}", root.display()))? {
        let c = c?.path();
        if !c.is_dir() {
            continue;
        }
        for n in std::fs::read_dir(&c).with_context(|| format!("reading {}", c.display()))? {
            let n = n?.path();
            if n.join(CHAIN_FILE).is_file() {
                out.push(n);
            }
        }
    }
    out.sort();
    if out.is_empty() {
        bail!("no chains under {}", root.display());
    }
    Ok(out)
}

/// Re-derives the canonical form from core's raw stream, ledger by ledger, and compares it
/// with the lake: the lake must be exactly that form, and each canonical ledger a
/// permutation of the raw one.
fn canonical_matches_raw(dir: &Path) -> Result<Vec<String>> {
    let chain = ChainFile::read(dir)?;
    let lake = open_lake(dir, &chain)?;
    let raw_path = dir.join(&chain.stream_file);
    let raw = FramedReader::new(
        BufReader::new(File::open(&raw_path).with_context(|| raw_path.display().to_string())?),
        MAX_FRAME_LEN,
    );
    let mut problems = Vec::new();
    let mut lake_ledgers = lake.ledgers();
    for frame in raw {
        let frame = frame.with_context(|| raw_path.display().to_string())?;
        let original = shapekit_oracle::decode_lcm(&frame)
            .with_context(|| format!("decoding {}", raw_path.display()))?;
        let mut canonical = original.clone();
        canonicalize_lcm(&mut canonical)?;
        let seq = shapekit_pack::lake::seq_of(&canonical);
        if !permutation_equal(&original, &canonical)? {
            problems.push(format!(
                "ledger {seq}: the canonical form is not a permutation of core's"
            ));
        }
        let bytes = canonical.to_xdr(Limits::none())?;
        match lake_ledgers.next() {
            Some(l) => {
                let l = l?;
                if l.seq != seq || l.xdr != bytes {
                    problems.push(format!(
                        "lake ledger {} is not the canonical form of raw ledger {seq}",
                        l.seq
                    ));
                }
            }
            None => problems.push(format!("raw ledger {seq} has no lake ledger")),
        }
    }
    if lake_ledgers.next().is_some() {
        problems.push("the lake holds more ledgers than the raw stream".into());
    }
    Ok(problems)
}
