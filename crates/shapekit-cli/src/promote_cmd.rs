//! `shapekit corpus promote`: moves entries from the candidate lane into the corpus.
//!
//! An entry is promoted when the current versions of rust, go and js pass it, or when each of
//! their failures is taken by a finding in findings/triage.toml with a drafted issue (the
//! three current SDKs of the plan's promotion rule). Every other failing cell the matrices
//! hold for the entry, an older version of those three or any version of the other SDKs, must
//! be attributed too: to a main finding, to a candidate finding of
//! candidate/findings/triage.toml, which holds it, or, for an SDK outside rust, go and js, to
//! a record of candidate/finding-candidates.toml naming the SDK, the version and the entry (a
//! candidate defect or an explained exclusion). The failures of rust, go and js need a
//! triage, since their matrices are inputs of the main findings, where a failure with only a
//! record would be unmapped. One unattributed failure refuses the entry, naming the SDK.
//!
//! An entry cut from a chain needs that chain under corpus/chains, where the harness replays
//! it from. A chain of the candidate lane's recipes/chains.toml moves with the first of its
//! entries that is promoted: the chain directory, its recipe IR file and the other files of
//! the recipe's directories go to the same places under corpus/, the chain's row moves to
//! corpus/recipes/chains.toml, and every entry of either lane that names the recipe is
//! rewritten to name it under corpus/, as is every recipe witness of the gate list.
//!
//! The failing cells of a promoted entry are added to conformance/known-failures/ with the
//! finding's link, or its draft while nothing is filed, or the record file, so the corpus run
//! stays green.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use serde::Deserialize;
use shapekit_conformance::versions::VersionsFile;
use shapekit_contracts::GateList;
use shapekit_findings::collect::conformance_matches;
use shapekit_findings::inputs::{CellView, MatrixView};
use shapekit_findings::triage::{ConformanceMatch, FindingDef, Status, Triage};
use shapekit_gen::recipe::chains_index::{ChainsIndex, IndexedChain};
use shapekit_manifest::build::{ENTRIES_DIR, ENTRY_FILE};
use shapekit_manifest::xdr::render_json;
use shapekit_manifest::{Entry, Manifest, build};
use shapekit_registry::Registry;

/// The SDKs the plan's promotion rule gates on: their current versions pass, or a main
/// finding takes the failure.
pub const GATING_SDKS: [&str; 3] = ["rust", "go", "js"];

#[derive(Args)]
pub struct PromoteArgs {
    /// Ids of the candidate entries.
    #[arg(required = true)]
    entries: Vec<String>,
    /// divergence-matrix.json of a `shapekit conformance run --manifest
    /// candidate/manifest.json` that covers the entries; repeatable, one per run. Every
    /// version a matrix holds is checked, so pass `--all-versions` runs to get the
    /// known-failures lines of the older versions too.
    #[arg(long = "matrix", required = true)]
    matrices: Vec<PathBuf>,
    /// Report the decisions without moving anything.
    #[arg(long)]
    dry_run: bool,
    /// The candidate lane directory.
    #[arg(long, default_value = "candidate")]
    candidate: PathBuf,
    /// The corpus directory, holding manifest.json.
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
    /// Directory holding the registry files.
    #[arg(long, default_value = "registry")]
    registry: PathBuf,
    /// The SDK versions and how to build and start each testee.
    #[arg(long, default_value = "conformance/versions.toml")]
    versions_file: PathBuf,
    /// Directory of <sdk>.txt files listing the known failures.
    #[arg(long, default_value = "conformance/known-failures")]
    known_failures: PathBuf,
    /// The findings triage: the inputs and the finding every failure maps to.
    #[arg(long, default_value = "findings/triage.toml")]
    triage: PathBuf,
    /// The candidate lane's findings, which hold failures no main finding takes.
    #[arg(long, default_value = "candidate/findings/triage.toml")]
    candidate_triage: PathBuf,
    /// The candidate lane's attributions of SDK failures to findings, candidates and
    /// explained exclusions.
    #[arg(long, default_value = "candidate/finding-candidates.toml")]
    records: PathBuf,
    /// The gate list whose recipe witnesses follow a moved recipe; skipped when absent.
    #[arg(long, default_value = "registry/generated/gates.json")]
    gates: PathBuf,
}

/// What takes one failing cell.
enum Attribution<'a> {
    /// A finding of findings/triage.toml with a drafted issue.
    Finding { def: &'a FindingDef, issue: String },
    /// A finding of the candidate triage, which holds the failure.
    Held { def: &'a FindingDef, issue: String },
    /// A record of candidate/finding-candidates.toml.
    Record { record: &'a Record },
}

impl Attribution<'_> {
    fn issue(&self, records: &Path) -> String {
        match self {
            Attribution::Finding { issue, .. } | Attribution::Held { issue, .. } => issue.clone(),
            Attribution::Record { .. } => records.display().to_string(),
        }
    }

    /// The comment above the known-failures lines it accounts for.
    fn comment(&self) -> String {
        match self {
            Attribution::Finding { def, .. } => format!("{}: {}", def.id, def.title),
            Attribution::Held { def, .. } => {
                format!("{} (candidate finding): {}", def.id, def.title)
            }
            Attribution::Record { record } => {
                let finding = if record.finding == "new" {
                    "a candidate".to_owned()
                } else {
                    format!("as {}", record.finding)
                };
                format!("{finding}: {}", record.observed)
            }
        }
    }

    fn describe(&self) -> String {
        match self {
            Attribution::Finding { def, issue } => format!("is finding {} ({issue})", def.id),
            Attribution::Held { def, issue } => {
                format!("is held by candidate finding {} ({issue})", def.id)
            }
            Attribution::Record { record } => format!(
                "is attributed in the candidate records ({})",
                if record.finding == "new" {
                    "a candidate"
                } else {
                    &record.finding
                }
            ),
        }
    }
}

/// One `[[candidate]]` of candidate/finding-candidates.toml: an SDK's failures on entries,
/// with the finding they belong to or `new`.
#[derive(Debug, Deserialize)]
struct Record {
    finding: String,
    sdk: Option<String>,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    entries: Vec<String>,
    #[serde(default)]
    observed: String,
}

#[derive(Debug, Deserialize)]
struct RecordsFile {
    #[serde(default, rename = "candidate")]
    records: Vec<Record>,
}

impl Record {
    fn takes(&self, sdk: &str, version: &str, entry: &str) -> bool {
        let Some(own) = &self.sdk else {
            return false;
        };
        // an empty version list would take every version; a record names its versions
        !self.versions.is_empty()
            && conformance_matches(
                &ConformanceMatch {
                    sdk: own.clone(),
                    entries: self.entries.clone(),
                    versions: self.versions.clone(),
                    checks: Vec::new(),
                },
                sdk,
                version,
                entry,
            )
    }
}

/// A failing cell and what takes it.
struct Accepted<'a> {
    sdk: String,
    version: String,
    cell: &'a CellView,
    by: Attribution<'a>,
}

pub fn run(args: PromoteArgs) -> Result<bool> {
    let root = std::env::current_dir().context("reading the working directory")?;
    let candidate_file = args.candidate.join("manifest.json");
    let corpus_file = args.corpus.join("manifest.json");
    let candidate = build::load(&candidate_file)
        .with_context(|| format!("loading {}", candidate_file.display()))?;
    let corpus =
        build::load(&corpus_file).with_context(|| format!("loading {}", corpus_file.display()))?;
    let mut wanted = BTreeSet::new();
    for id in &args.entries {
        ensure!(wanted.insert(id.as_str()), "{id} is given twice");
        ensure!(
            candidate.entries.iter().any(|e| &e.id == id),
            "{} has no entry {id}",
            candidate_file.display()
        );
        ensure!(
            !corpus.entries.iter().any(|e| &e.id == id),
            "{} already has an entry {id}",
            corpus_file.display()
        );
    }
    let versions = VersionsFile::load(&args.versions_file)?;
    let triage = shapekit_findings::triage::load(&args.triage)?;
    let candidate_triage = shapekit_findings::triage::load(&args.candidate_triage)?;
    let records: RecordsFile = {
        let text = std::fs::read_to_string(&args.records)
            .with_context(|| format!("reading {}", args.records.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", args.records.display()))?
    };
    let mut matrices = Vec::new();
    for path in &args.matrices {
        let (bytes, _) = shapekit_findings::inputs::read(path)?;
        let path = path.clone();
        let view = shapekit_findings::parse_on_big_stack(move || {
            shapekit_findings::inputs::matrix(&bytes, &path)
        })?;
        matrices.push(view);
    }
    let candidate_index = ChainsIndex::load(&args.candidate)?;
    let corpus_index = ChainsIndex::load(&args.corpus)?;

    let ctx = Decide {
        versions: &versions,
        matrices: &matrices,
        triage: &triage,
        candidate_triage: &candidate_triage,
        records: &records.records,
        root: &root,
    };
    let mut promoted: Vec<(&Entry, Vec<Accepted<'_>>)> = Vec::new();
    let mut refused = 0usize;
    for id in &args.entries {
        let entry = candidate
            .entries
            .iter()
            .find(|e| &e.id == id)
            .expect("checked above");
        let (accepted, mut refusals) = ctx.decide(id);
        if let Some(chain) = &entry.source.chain
            && !args.corpus.join("chains").join(chain).is_dir()
            && !candidate_index.chains.iter().any(|c| &c.id == chain)
        {
            refusals.push(format!(
                "its chain {chain} is not under {} and {} does not list it, so the harness could not replay the entry",
                args.corpus.join("chains").display(),
                ChainsIndex::path(&args.candidate).display()
            ));
        }
        if let Some(why) = before_first_checkpoint(&args, entry)? {
            refusals.push(why);
        }
        for a in &accepted {
            println!(
                "{id}: {}@{}: {} {}",
                a.sdk,
                a.version,
                a.cell.result,
                a.by.describe()
            );
        }
        if refusals.is_empty() {
            println!("{id} can be promoted");
            promoted.push((entry, accepted));
        } else {
            for r in &refusals {
                println!("{id}: refused: {r}");
            }
            println!("{id} stays in the candidate lane");
            refused += 1;
        }
    }
    if args.dry_run || promoted.is_empty() {
        println!(
            "{} of {} can be promoted, {refused} refused",
            promoted.len(),
            args.entries.len()
        );
        return Ok(refused == 0);
    }

    let registry = Registry::load(&args.registry)
        .with_context(|| format!("loading the registry in {}", args.registry.display()))?;
    let entries: Vec<&Entry> = promoted.iter().map(|(e, _)| *e).collect();
    let moves = chain_moves(&args, &entries, &candidate_index, &corpus_index)?;
    let moved_files = promote(&args, &registry, &candidate, &corpus, &entries, &moves)?;
    let mut groups: BTreeMap<(String, String), Vec<(&str, &Accepted<'_>)>> = BTreeMap::new();
    for (e, accepted) in &promoted {
        for a in accepted {
            groups
                .entry((a.sdk.clone(), a.by.comment()))
                .or_default()
                .push((e.id.as_str(), a));
        }
    }
    for ((sdk, comment), lines) in &groups {
        add_known_failures(&args.known_failures, &args.records, sdk, comment, lines)?;
    }
    for m in &moves.chains {
        println!(
            "moved chain {} and recipe {} into {}",
            m.id,
            m.recipe,
            args.corpus.display()
        );
    }
    println!(
        "promoted {} entries ({moved_files} entry files), {refused} refused; rerun `shapekit oracle run`, `shapekit coverage` and the conformance run for the corpus",
        entries.len()
    );
    Ok(refused == 0)
}

/// Why an entry cut from a chain at or before the chain's first history checkpoint cannot
/// move: an indexer that bootstraps from a checkpoint (wallet-backend, stellar-rpc) starts the
/// chain at the checkpoint before its first entry, and there is none, so the harness could
/// evaluate no entry of the chain.
fn before_first_checkpoint(args: &PromoteArgs, entry: &Entry) -> Result<Option<String>> {
    let (Some(chain), Some(seq)) = (&entry.source.chain, entry.source.ledger_seq) else {
        return Ok(None);
    };
    let dir = [&args.corpus, &args.candidate]
        .into_iter()
        .map(|lane| lane.join("chains").join(chain))
        .find(|d| d.is_dir());
    let Some(dir) = dir else {
        return Ok(None);
    };
    let file = dir.join("testnet").join("chain.json");
    let file = if file.is_file() {
        file
    } else {
        dir.join("mainnet").join("chain.json")
    };
    let doc: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?,
    )
    .with_context(|| format!("parsing {}", file.display()))?;
    let first = doc
        .get("checkpoints")
        .and_then(serde_json::Value::as_array)
        .and_then(|c| c.iter().filter_map(serde_json::Value::as_u64).min());
    Ok(match first {
        Some(c) if u64::from(seq) <= c => Some(format!(
            "its ledger {seq} is not after its chain's first history checkpoint {c}, so an indexer that bootstraps from a checkpoint could replay no entry of chain {chain}"
        )),
        _ => None,
    })
}

struct Decide<'a> {
    versions: &'a VersionsFile,
    matrices: &'a [MatrixView],
    triage: &'a Triage,
    candidate_triage: &'a Triage,
    records: &'a [Record],
    root: &'a Path,
}

impl<'a> Decide<'a> {
    /// Every failing cell of the entry in the matrices, with what takes it, and the reasons
    /// to refuse the entry.
    fn decide(&self, entry: &str) -> (Vec<Accepted<'a>>, Vec<String>) {
        let mut accepted = Vec::new();
        let mut refusals = Vec::new();
        let rows: Vec<_> = self
            .matrices
            .iter()
            .filter_map(|m| m.entries.iter().find(|e| e.id == entry))
            .collect();
        for (sdk, def) in &self.versions.sdk {
            let current = format!("{sdk}@{}", def.current);
            if !rows.iter().any(|r| r.cells.contains_key(&current)) {
                refusals.push(format!(
                    "no matrix has a result for {entry} on {current}; run `shapekit conformance run --manifest candidate/manifest.json --entry {entry} --sdk {sdk}`"
                ));
            }
            for v in &def.versions {
                let key = format!("{sdk}@{}", v.version);
                let Some(cell) = rows.iter().find_map(|r| r.cells.get(&key)) else {
                    continue;
                };
                if !cell.is_failure() {
                    continue;
                }
                let strict = GATING_SDKS.contains(&sdk.as_str()) && v.version == def.current;
                match self.attribute(sdk, &v.version, entry, strict) {
                    Ok(by) => accepted.push(Accepted {
                        sdk: sdk.clone(),
                        version: v.version.clone(),
                        cell,
                        by,
                    }),
                    Err(why) => refusals.push(format!("{key} answers {}; {why}", cell.result)),
                }
            }
        }
        (accepted, refusals)
    }

    /// What takes the failure of `sdk` at `version` on `entry`. `strict`: only a main finding
    /// may (the current version of a gating SDK).
    fn attribute(
        &self,
        sdk: &str,
        version: &str,
        entry: &str,
        strict: bool,
    ) -> std::result::Result<Attribution<'a>, String> {
        // the first finding whose matcher takes the failure owns it, as in findings collect
        if let Some(def) = first_taker(self.triage, sdk, version, entry) {
            let issue = main_issue(def, self.root)?;
            return Ok(Attribution::Finding { def, issue });
        }
        if strict {
            return Err(format!(
                "{sdk} is one of rust, go and js, whose current version must pass or have its failure taken by a finding in findings/triage.toml, and none takes it"
            ));
        }
        if let Some(def) = first_taker(self.candidate_triage, sdk, version, entry) {
            let issue = match (&def.link, &def.draft) {
                (Some(link), _) if matches!(def.status, Status::Filed | Status::DuplicateOf) => {
                    link.clone()
                }
                (_, Some(draft)) => draft.clone(),
                _ => format!("candidate/findings/triage.toml#{}", def.id),
            };
            return Ok(Attribution::Held { def, issue });
        }
        if GATING_SDKS.contains(&sdk) {
            return Err(format!(
                "no finding in findings/triage.toml or the candidate triage takes it; every version of {sdk} is a main findings input, so a record alone would leave it unmapped"
            ));
        }
        if let Some(record) = self.records.iter().find(|r| r.takes(sdk, version, entry)) {
            return Ok(Attribution::Record { record });
        }
        Err(
            "no finding in findings/triage.toml or the candidate triage takes it and no record in candidate/finding-candidates.toml names it"
                .to_owned(),
        )
    }
}

fn first_taker<'t>(
    triage: &'t Triage,
    sdk: &str,
    version: &str,
    entry: &str,
) -> Option<&'t FindingDef> {
    triage.findings.iter().find(|f| {
        f.conformance
            .iter()
            .any(|m| conformance_matches(m, sdk, version, entry))
    })
}

/// The issue a main finding's known-failures lines point at: its link once filed, its draft
/// otherwise; the draft must be a file under findings/issues/.
fn main_issue(finding: &FindingDef, root: &Path) -> std::result::Result<String, String> {
    let Some(draft) = finding.draft.as_deref() else {
        return Err(format!(
            "finding {} takes it but names no draft under findings/issues/",
            finding.id
        ));
    };
    if !draft.starts_with("findings/issues/") || !root.join(draft).is_file() {
        return Err(format!(
            "finding {} names draft {draft}, which is not a file under findings/issues/",
            finding.id
        ));
    }
    Ok(match (finding.status, &finding.link) {
        (Status::Filed | Status::DuplicateOf, Some(link)) => link.clone(),
        _ => draft.to_owned(),
    })
}

/// The candidate chains that move into the corpus with the entries.
#[derive(Default)]
struct Moves {
    chains: Vec<IndexedChain>,
    /// Files beside the recipes, relative to the lanes' `recipes/`: run notes, pair
    /// comparisons, a family's README. Copied into the corpus when not there yet, and removed
    /// from the candidate lane once no listed recipe is left below their directory.
    companions: BTreeSet<String>,
}

fn chain_moves(
    args: &PromoteArgs,
    entries: &[&Entry],
    candidate_index: &ChainsIndex,
    corpus_index: &ChainsIndex,
) -> Result<Moves> {
    let mut moves = Moves::default();
    let mut seen = BTreeSet::new();
    for e in entries {
        let Some(chain) = &e.source.chain else {
            continue;
        };
        if args.corpus.join("chains").join(chain).is_dir() || !seen.insert(chain.clone()) {
            continue;
        }
        let row = candidate_index
            .chains
            .iter()
            .find(|c| &c.id == chain)
            .expect("refused above unless listed");
        ensure!(
            !corpus_index.chains.iter().any(|c| c.id == row.id),
            "{} already lists chain {}",
            ChainsIndex::path(&args.corpus).display(),
            row.id
        );
        let from = args.candidate.join("chains").join(&row.id);
        ensure!(from.is_dir(), "{} is not a directory", from.display());
        let recipe = args.candidate.join("recipes").join(&row.recipe);
        ensure!(recipe.is_file(), "{} is not a file", recipe.display());
        let to = args.corpus.join("recipes").join(&row.recipe);
        ensure!(!to.exists(), "{} already exists", to.display());
        // the files beside the recipe and in each directory above it up to recipes/
        let mut dir = Path::new(&row.recipe).parent();
        while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
            let abs = args.candidate.join("recipes").join(d);
            for item in
                std::fs::read_dir(&abs).with_context(|| format!("reading {}", abs.display()))?
            {
                let item = item?;
                if !item.file_type()?.is_file() {
                    continue;
                }
                let rel = d.join(item.file_name());
                let rel = rel
                    .to_str()
                    .context("a recipe path is not utf-8")?
                    .to_owned();
                if !candidate_index.chains.iter().any(|c| c.recipe == rel) {
                    moves.companions.insert(rel);
                }
            }
            dir = d.parent();
        }
        moves.chains.push(row.clone());
    }
    Ok(moves)
}

/// The entry's files relative to the manifest directory: its entry directory for an xdr entry,
/// its vector file otherwise.
fn entry_files(entry: &Entry) -> Result<Vec<String>> {
    if let Some(v) = &entry.vector {
        return Ok(vec![v.path.clone()]);
    }
    let first = entry
        .variants
        .iter()
        .next()
        .with_context(|| format!("entry {} has neither variants nor a vector", entry.id))?
        .1;
    let dir = first
        .xdr_file
        .rsplit_once('/')
        .map(|(d, _)| d.to_owned())
        .with_context(|| format!("{} is not inside an entry directory", first.xdr_file))?;
    Ok(vec![dir])
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if from.is_file() {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::copy(from, to)
            .with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
        return Ok(());
    }
    for item in walkdir::WalkDir::new(from).sort_by_file_name() {
        let item = item.with_context(|| format!("walking {}", from.display()))?;
        let target = to.join(
            item.path()
                .strip_prefix(from)
                .expect("inside the walked tree"),
        );
        if item.file_type().is_dir() {
            std::fs::create_dir_all(&target)
                .with_context(|| format!("creating {}", target.display()))?;
        } else {
            std::fs::copy(item.path(), &target)
                .with_context(|| format!("copying {}", item.path().display()))?;
        }
    }
    Ok(())
}

fn remove(path: &Path) -> Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
    .with_context(|| format!("removing {}", path.display()))
}

/// Removes `dir` and its parents below `stop` while they are empty; one already gone, as the
/// directory two moved recipes share, is skipped.
fn prune_empty(mut dir: PathBuf, stop: &Path) -> Result<()> {
    while dir != stop && dir.starts_with(stop) {
        if !dir.exists() {
            dir = match dir.parent() {
                Some(p) => p.to_path_buf(),
                None => break,
            };
            continue;
        }
        let empty = std::fs::read_dir(&dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .next()
            .is_none();
        if !empty {
            break;
        }
        std::fs::remove_dir(&dir).with_context(|| format!("removing {}", dir.display()))?;
        dir = match dir.parent() {
            Some(p) => p.to_path_buf(),
            None => break,
        };
    }
    Ok(())
}

/// The repository paths a moved recipe or companion file had and has.
fn renames(args: &PromoteArgs, moves: &Moves) -> Vec<(String, String)> {
    let lane = |dir: &Path, rel: &str| format!("{}/recipes/{rel}", dir.display());
    moves
        .chains
        .iter()
        .map(|c| c.recipe.clone())
        .chain(moves.companions.iter().cloned())
        .map(|rel| (lane(&args.candidate, &rel), lane(&args.corpus, &rel)))
        .collect()
}

/// Rewrites an entry's recipe references after its recipe moved: `source.recipe`,
/// `source.recipe_ir` and the paths in its notes.
fn rewrite_entry(file: &Path, renames: &[(String, String)]) -> Result<()> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let mut e: Entry =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", file.display()))?;
    let Some(recipe) = e.source.recipe.clone() else {
        return Ok(());
    };
    let Some((_, to)) = renames.iter().find(|(from, _)| *from == recipe) else {
        return Ok(());
    };
    e.source.recipe = Some(to.clone());
    e.source.recipe_ir = Some(to.clone());
    if let Some(notes) = &mut e.notes {
        for (from, to) in renames {
            *notes = notes.replace(from.as_str(), to);
        }
    }
    std::fs::write(file, render_json(&serde_json::to_value(&e)?))
        .with_context(|| format!("writing {}", file.display()))
}

/// Every entry.json under the lane's entries/.
fn entry_jsons(lane: &Path) -> Result<Vec<PathBuf>> {
    let dir = lane.join(ENTRIES_DIR);
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for item in walkdir::WalkDir::new(&dir)
        .min_depth(2)
        .max_depth(2)
        .sort_by_file_name()
    {
        let item = item.with_context(|| format!("walking {}", dir.display()))?;
        let file = item.path().join(ENTRY_FILE);
        if item.file_type().is_dir() && file.is_file() {
            out.push(file);
        }
    }
    Ok(out)
}

/// Rewrites the recipe witnesses of the gate list that name a moved recipe; each side's
/// witnesses stay sorted. Returns how many changed.
fn rewrite_gates(list: &mut GateList, renames: &[(String, String)]) -> usize {
    let mut changed = 0;
    for side in list.gates.iter_mut().flat_map(|g| g.sides.iter_mut()) {
        let mut touched = false;
        for w in &mut side.witnesses {
            let Some(rest) = w.strip_prefix("recipe:") else {
                continue;
            };
            let (file, at) = rest.split_once('@').map_or((rest, ""), |(f, a)| (f, a));
            if let Some((_, to)) = renames.iter().find(|(from, _)| from == file) {
                *w = if at.is_empty() {
                    format!("recipe:{to}")
                } else {
                    format!("recipe:{to}@{at}")
                };
                touched = true;
                changed += 1;
            }
        }
        if touched {
            side.witnesses.sort();
            side.witnesses.dedup();
        }
    }
    changed
}

/// The gate list at `path`, if there is one. It is loaded as the contract type, before
/// anything moves, so a broken list stops the promotion early and a rewritten one keeps the
/// field order `twin gate-witnesses` writes.
fn load_gates(path: &Path) -> Result<Option<GateList>> {
    if !path.is_file() {
        return Ok(None);
    }
    shapekit_contracts::load(path)
        .map(Some)
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Copies the entries and the chains moving with them into the corpus, rewrites recipe
/// references, rebuilds and validates the corpus manifest, and only then removes everything
/// from the candidate lane; a corpus manifest with problems undoes the copy. Returns the
/// number of entry files moved.
fn promote(
    args: &PromoteArgs,
    registry: &Registry,
    candidate: &Manifest,
    corpus: &Manifest,
    entries: &[&Entry],
    moves: &Moves,
) -> Result<usize> {
    let mut files = Vec::new();
    for e in entries {
        files.extend(entry_files(e)?);
    }
    for rel in &files {
        let to = args.corpus.join(rel);
        ensure!(!to.exists(), "{} already exists", to.display());
    }
    for c in &moves.chains {
        let to = args.corpus.join("chains").join(&c.id);
        ensure!(!to.exists(), "{} already exists", to.display());
    }
    let mut gates = if moves.chains.is_empty() {
        None
    } else {
        load_gates(&args.gates)?
    };
    let renames = renames(args, moves);
    // everything this run creates in the corpus, to undo on failure
    let mut created: Vec<PathBuf> = Vec::new();
    let copied = (|| -> Result<Manifest> {
        for rel in &files {
            let to = args.corpus.join(rel);
            copy_tree(&args.candidate.join(rel), &to)?;
            created.push(to);
        }
        for c in &moves.chains {
            let to = args.corpus.join("chains").join(&c.id);
            copy_tree(&args.candidate.join("chains").join(&c.id), &to)?;
            created.push(to);
            let to = args.corpus.join("recipes").join(&c.recipe);
            copy_tree(&args.candidate.join("recipes").join(&c.recipe), &to)?;
            created.push(to);
        }
        for rel in &moves.companions {
            let to = args.corpus.join("recipes").join(rel);
            if !to.exists() {
                copy_tree(&args.candidate.join("recipes").join(rel), &to)?;
                created.push(to);
            }
        }
        for rel in &files {
            let file = args.corpus.join(rel).join(ENTRY_FILE);
            if file.is_file() {
                rewrite_entry(&file, &renames)?;
            }
        }
        let rebuilt = build::build(
            &args.corpus,
            &corpus.corpus_version,
            corpus.generated_with.clone(),
        )?;
        let problems: Vec<String> = shapekit_manifest::validate(&rebuilt, registry, &args.corpus)
            .iter()
            .map(ToString::to_string)
            .collect();
        if !problems.is_empty() {
            bail!(
                "the corpus manifest would not validate, nothing moved:\n{}",
                problems.join("\n")
            );
        }
        Ok(rebuilt)
    })();
    let rebuilt = match copied {
        Ok(m) => m,
        Err(e) => {
            for path in created.iter().rev() {
                if path.exists() {
                    remove(path)?;
                }
            }
            return Err(e);
        }
    };
    write_manifest(&args.corpus, &rebuilt)?;
    for rel in &files {
        let path = args.candidate.join(rel);
        remove(&path)?;
        if let Some(parent) = path.parent() {
            prune_empty(parent.to_path_buf(), &args.candidate)?;
        }
    }
    if !moves.chains.is_empty() {
        let mut candidate_index = ChainsIndex::load(&args.candidate)?;
        let mut corpus_index = ChainsIndex::load(&args.corpus)?;
        for c in &moves.chains {
            remove(&args.candidate.join("chains").join(&c.id))?;
            remove(&args.candidate.join("recipes").join(&c.recipe))?;
            candidate_index.chains.retain(|x| x.id != c.id);
            corpus_index.chains.push(c.clone());
        }
        for rel in &moves.companions {
            let dir = Path::new(rel).parent().unwrap_or(Path::new(""));
            let still = candidate_index
                .chains
                .iter()
                .any(|c| Path::new(&c.recipe).starts_with(dir));
            let path = args.candidate.join("recipes").join(rel);
            if !still && path.exists() {
                remove(&path)?;
            }
        }
        for c in &moves.chains {
            let recipe = args.candidate.join("recipes").join(&c.recipe);
            if let Some(parent) = recipe.parent() {
                prune_empty(parent.to_path_buf(), &args.candidate.join("recipes"))?;
            }
        }
        candidate_index.write(&args.candidate)?;
        corpus_index.write(&args.corpus)?;
        for file in entry_jsons(&args.candidate)? {
            rewrite_entry(&file, &renames)?;
        }
        let n = gates
            .as_mut()
            .map_or(0, |list| rewrite_gates(list, &renames));
        if let Some(list) = gates.as_ref().filter(|_| n > 0) {
            shapekit_contracts::write(&args.gates, list).map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "{n} recipe witnesses of {} follow their recipe",
                args.gates.display()
            );
        }
    }
    let rest = build::build(
        &args.candidate,
        &candidate.corpus_version,
        candidate.generated_with.clone(),
    )
    .with_context(|| format!("rebuilding {}", args.candidate.display()))?;
    write_manifest(&args.candidate, &rest)?;
    Ok(files.len())
}

fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<()> {
    let path = dir.join("manifest.json");
    std::fs::write(&path, build::render(manifest))
        .with_context(|| format!("writing {}", path.display()))
}

/// Appends `<entry> <version> <result> <issue> <checks>` for each accepted failure, one line
/// per result its failing checks give, under one comment naming what takes them.
fn add_known_failures(
    dir: &Path,
    records: &Path,
    sdk: &str,
    comment: &str,
    lines: &[(&str, &Accepted<'_>)],
) -> Result<()> {
    let path = shapekit_conformance::known::path(dir, sdk);
    let mut text = if path.is_file() {
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?
    } else {
        String::new()
    };
    if !text.ends_with('\n') && !text.is_empty() {
        text.push('\n');
    }
    let _ = writeln!(text, "# {comment}");
    for (entry, a) in lines {
        let mut by_result: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for c in &a.cell.checks {
            if matches!(c.result.as_str(), "pass" | "unsupported") {
                continue;
            }
            // with the kind of failure, which the line has to name to match
            by_result
                .entry(c.outcome())
                .or_default()
                .insert(c.check.as_str());
        }
        ensure!(
            !by_result.is_empty(),
            "{}@{} fails {entry} but the matrix lists no failing check",
            a.sdk,
            a.version
        );
        let issue = a.by.issue(records);
        for (result, checks) in by_result {
            let _ = writeln!(
                text,
                "{entry} {} {result} {issue} {}",
                a.version,
                checks.into_iter().collect::<Vec<_>>().join(", ")
            );
        }
    }
    shapekit_conformance::known::parse(&text)
        .with_context(|| format!("the new lines would break {}", path.display()))?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}
