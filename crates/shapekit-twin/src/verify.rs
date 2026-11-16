//! Byte equality of the coverage twin with the release image: the same chains
//! recorded on both, compared by their canonical stream hashes.
//!
//! The twin only measures what the release build does if both write the same
//! ledgers. Each chain is recorded under both passphrases on each image, and
//! the SHA-256 of the canonical stream (the recorder's reproducible form of
//! the meta, `chain.json` `canonical_stream_sha256`) is compared. For chains
//! that are in the corpus, the twin's hash is also compared with the one
//! committed in `corpus/chains/<chain>/<network>/chain.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use shapekit_core_driver::config::CoreImage;
use shapekit_core_driver::recorder::CHAIN_FILE;
use shapekit_core_driver::smoke;
use shapekit_gen::recipe::{self, GenOptions, Network, chain_id};
use shapekit_manifest::GeneratedWith;
use shapekit_registry::Registry;

/// The chain id `verify` uses for the smoke chain, which has no recipe.
pub const SMOKE: &str = "smoke";

/// The chains of the byte equality check: the smoke chain and four corpus
/// chains spanning protocol 9 to 30, classic and contract transactions,
/// failed transactions and the unified events backfill.
pub const DEFAULT_CHAINS: [&str; 5] = [
    SMOKE,
    "cap40.signed-payload-signer.p30",
    "cap73.sac-trust.p30",
    "classic.failed-tx.change-trust-native.p9",
    "cap67.unified-events.p22",
];

/// One chain under one passphrase on one image.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub canonical_stream_sha256: String,
    pub first_ledger: u32,
    pub last_ledger: u32,
    pub seconds: f64,
}

/// One chain under one passphrase on both images.
#[derive(Debug, Clone)]
pub struct Comparison {
    pub chain: String,
    pub network: Network,
    pub release: Recorded,
    pub twin: Recorded,
    /// The hash committed in the corpus, for corpus chains.
    pub committed: Option<String>,
}

impl Comparison {
    pub fn equal(&self) -> bool {
        self.release.canonical_stream_sha256 == self.twin.canonical_stream_sha256
    }
    /// `None` for chains outside the corpus.
    pub fn matches_corpus(&self) -> Option<bool> {
        self.committed
            .as_ref()
            .map(|c| *c == self.twin.canonical_stream_sha256)
    }
}

pub struct VerifyOptions<'a> {
    pub release: CoreImage,
    pub twin: CoreImage,
    pub chains: Vec<String>,
    pub corpus: PathBuf,
    pub registry: &'a Registry,
    pub generated_with: GeneratedWith,
    /// Chains recorded at once per image.
    pub jobs: usize,
    /// Where the recorded chains are kept, `release/` and `twin/` below it.
    pub out: PathBuf,
}

/// Records every selected chain under both passphrases on both images and
/// compares them.
pub fn verify(opts: &VerifyOptions<'_>) -> Result<Vec<Comparison>> {
    let all = recipe::all();
    let mut recipes = Vec::new();
    for c in &opts.chains {
        if c == SMOKE {
            continue;
        }
        let r = all
            .iter()
            .find(|r| chain_id(r.as_ref()) == *c)
            .with_context(|| format!("no recipe records chain {c}"))?;
        recipes.push(r.as_ref());
    }
    let smoke = opts.chains.iter().any(|c| c == SMOKE);

    let setup = RecordSetup {
        registry: opts.registry,
        generated_with: opts.generated_with.clone(),
        jobs: opts.jobs,
        networks: Network::BOTH.to_vec(),
    };
    let mut by_image = Vec::new();
    for (name, image) in [("release", &opts.release), ("twin", &opts.twin)] {
        let dir = opts.out.join(name);
        by_image.push(record_all(image, &recipes, smoke, &dir, &setup)?);
    }
    let (release, twin) = (&by_image[0], &by_image[1]);

    let mut out = Vec::new();
    for c in &opts.chains {
        for n in Network::BOTH {
            let key = (c.clone(), n.name());
            let (Some(r), Some(t)) = (release.get(&key), twin.get(&key)) else {
                bail!("chain {c} ({}) was not recorded on both images", n.name());
            };
            let committed_file = opts
                .corpus
                .join("chains")
                .join(c)
                .join(n.name())
                .join(CHAIN_FILE);
            let committed = if c == SMOKE {
                None
            } else {
                Some(committed_hash(&committed_file)?)
            };
            out.push(Comparison {
                chain: c.clone(),
                network: n,
                release: r.clone(),
                twin: t.clone(),
                committed,
            });
        }
    }
    Ok(out)
}

fn committed_hash(path: &Path) -> Result<String> {
    let v: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("parsing {}", path.display()))?;
    v["canonical_stream_sha256"]
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("{} has no canonical_stream_sha256", path.display()))
}

pub type ByChain = BTreeMap<(String, &'static str), Recorded>;

/// What [`record_all`] needs besides the image and chains.
pub struct RecordSetup<'a> {
    pub registry: &'a Registry,
    pub generated_with: GeneratedWith,
    pub jobs: usize,
    pub networks: Vec<Network>,
}

/// Records the smoke chain (if asked) and `recipes` on `image` into `dir`,
/// returning each chain's canonical hash by (chain id, network).
pub fn record_all(
    image: &CoreImage,
    recipes: &[&dyn recipe::Recipe],
    smoke_chain: bool,
    dir: &Path,
    opts: &RecordSetup<'_>,
) -> Result<ByChain> {
    let mut out = ByChain::new();
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    if smoke_chain {
        for &n in &opts.networks {
            let target = dir.join(SMOKE).join(n.name());
            if target.exists() {
                bail!("{} already exists", target.display());
            }
            let work = tempfile::Builder::new()
                .prefix("shapekit-twin-smoke-")
                .tempdir()
                .context("creating the node workdir")?;
            let started = Instant::now();
            let chain = smoke::record(
                smoke::node_config(n.passphrase(), image.clone()),
                work.path(),
                &target,
            )
            .with_context(|| {
                format!(
                    "recording the smoke chain ({}) on {}",
                    n.name(),
                    image.reference()
                )
            })?;
            out.insert(
                (SMOKE.to_owned(), n.name()),
                Recorded {
                    canonical_stream_sha256: chain.summary.canonical_stream_sha256,
                    first_ledger: chain.summary.ledger_range.first,
                    last_ledger: chain.summary.ledger_range.last,
                    seconds: started.elapsed().as_secs_f64(),
                },
            );
        }
    }
    if !recipes.is_empty() {
        let corpus = dir.join("corpus");
        let gen_opts = GenOptions {
            corpus,
            registry: opts.registry,
            image: image.clone(),
            networks: opts.networks.clone(),
            jobs: opts.jobs,
            generated_with: opts.generated_with.clone(),
        };
        let report = recipe::generate(recipes, &gen_opts)
            .with_context(|| format!("recording the recipe chains on {}", image.reference()))?;
        for run in report.runs {
            out.insert(
                (run.chain_id.clone(), run.network.name()),
                Recorded {
                    canonical_stream_sha256: run.canonical_stream_sha256,
                    first_ledger: run.first_ledger,
                    last_ledger: run.last_ledger,
                    seconds: run.seconds,
                },
            );
        }
    }
    Ok(out)
}
