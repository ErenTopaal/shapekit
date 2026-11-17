//! `shapekit probe`: which envelope fields core validates, asked of core itself.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use shapekit_contracts::validation::{ProbeAttempt, ValidationTable};
use shapekit_contracts::{Contract, CoreRef, ImageRef};
use shapekit_core_driver::config::CoreImage;
use shapekit_core_driver::smoke::{self, MAINNET_PASSPHRASE, TESTNET_PASSPHRASE};
use shapekit_probe::{Probe, variants};

#[derive(Subcommand)]
pub enum ProbeCommand {
    /// Submit single-field changes of one valid payment to a standalone core and print core's
    /// answer to each: refused at submission, applied and failed, or applied and successful.
    Run {
        /// Only the changes of this registry/fields.toml field (repeatable); all when absent.
        #[arg(long)]
        field: Vec<String>,
        /// testnet or mainnet passphrase.
        #[arg(long, default_value = "testnet")]
        network: String,
        /// Also write the attempts as JSON.
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Ask core about every envelope field the schema defines: each field's struct sits in a
    /// valid carrier transaction, the field takes every replacement its type gives, and core's
    /// answers become a verdict per field.
    All {
        /// Only these structs or `Struct.field` paths (repeatable). A partial run prints its
        /// verdicts and writes nothing under the repository.
        #[arg(long)]
        only: Vec<String>,
        #[arg(long, default_value = "registry/generated/validation.json")]
        out: PathBuf,
        /// The candidate lane the witness entries are written to.
        #[arg(long, default_value = "candidate")]
        candidate: PathBuf,
        /// Write the attempts of a partial run as JSON to this file.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// With --only: write the table and the witness entries under this directory instead
        /// of leaving them out.
        #[arg(long)]
        scratch: Option<PathBuf>,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Find the largest value core accepts for each bounded quantity and the first it refuses,
    /// by bisection, and compare each with registry/caps-limits.toml.
    Limits {
        /// Only these caps-limits.toml rows (repeatable). A partial run writes only under
        /// --scratch.
        #[arg(long)]
        only: Vec<String>,
        #[arg(long, default_value = "registry/generated/limits.json")]
        out: PathBuf,
        #[arg(long, default_value = "candidate")]
        candidate: PathBuf,
        #[arg(long, default_value = "registry/caps-limits.toml")]
        caps: PathBuf,
        #[arg(long)]
        scratch: Option<PathBuf>,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Run the probe again in a scratch directory and compare its recorded chains (canonical
    /// hashes, ledger range, lake), the table and the witness entries with the committed ones.
    VerifyRepro {
        #[arg(long, default_value = "registry/generated/validation.json")]
        out: PathBuf,
        #[arg(long, default_value = "candidate")]
        candidate: PathBuf,
        #[arg(long, default_value = "pins.toml")]
        pins: PathBuf,
    },
    /// Write the `field` obligations of a validation table beside it, as
    /// <table stem>.obligations.json; probe all writes them too.
    Obligations {
        #[arg(long, default_value = "registry/generated/validation.json")]
        table: PathBuf,
    },
    /// Compare registry/fields.toml with the measured verdicts: every row against the struct
    /// fields and carriers it speaks about. Exits 1 on a disagreement no source reading
    /// settles, or on a settlement for a row that agrees.
    Compare {
        #[arg(long, default_value = "registry/generated/validation.json")]
        table: PathBuf,
        #[arg(long, default_value = "registry/fields.toml")]
        fields: PathBuf,
        /// The per-row verdict file: one record per fields.toml row with the measured verdict,
        /// its sites and the source reading that settles it, if any.
        #[arg(long, default_value = "registry/generated/field-verdicts.json")]
        verdicts: PathBuf,
        /// Write the verdict file; without it the committed file is checked to be current.
        #[arg(long)]
        write: bool,
    },
}

pub fn run(command: ProbeCommand) -> Result<bool> {
    match command {
        ProbeCommand::Run {
            field,
            network,
            out,
            pins,
        } => probe_run(&field, &network, out.as_deref(), &pins),
        ProbeCommand::All {
            only,
            out,
            candidate,
            dump,
            scratch,
            pins,
        } => probe_all(
            &only,
            &out,
            &candidate,
            dump.as_deref(),
            scratch.as_deref(),
            &pins,
        ),
        ProbeCommand::Compare {
            table,
            fields,
            verdicts,
            write,
        } => probe_compare(&table, &fields, &verdicts, write),
        ProbeCommand::VerifyRepro {
            out,
            candidate,
            pins,
        } => probe_verify_repro(&out, &candidate, &pins),
        ProbeCommand::Obligations { table } => {
            let t: ValidationTable = shapekit_contracts::load(&table)
                .with_context(|| format!("reading {}", table.display()))?;
            write_obligations(&t, &table)?;
            Ok(true)
        }
        ProbeCommand::Limits {
            only,
            out,
            candidate,
            caps,
            scratch,
            pins,
        } => probe_limits(&only, &out, &candidate, &caps, scratch.as_deref(), &pins),
    }
}

#[derive(serde::Serialize)]
struct Row<'a> {
    field: &'a str,
    #[serde(flatten)]
    attempt: &'a ProbeAttempt,
}

fn probe_run(fields: &[String], network: &str, out: Option<&Path>, pins: &Path) -> Result<bool> {
    let passphrase = match network {
        "testnet" => TESTNET_PASSPHRASE,
        "mainnet" => MAINNET_PASSPHRASE,
        other => bail!("unknown network {other:?}, expected testnet or mainnet"),
    };
    let all = variants::payment();
    for f in fields {
        if !all.iter().any(|v| v.field == f) {
            bail!("no probe changes field {f}");
        }
    }
    let chosen: Vec<_> = all
        .iter()
        .filter(|v| fields.is_empty() || fields.iter().any(|f| f == v.field))
        .collect();
    let image = CoreImage::from_pins(pins)
        .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
    let mut cfg = smoke::node_config(passphrase, image);
    cfg.soroban_high_limits = false;
    // whatever core applies must pass every invariant, or the node stops
    cfg.invariant_checks = true;
    let work = tempfile::Builder::new()
        .prefix("shapekit-probe-")
        .tempdir()
        .context("creating the node workdir")?;
    let mut probe = Probe::start(cfg, work.path(), chosen.len())?;
    let started = std::time::Instant::now();
    let mut attempts = Vec::with_capacity(chosen.len());
    for v in &chosen {
        let (a, _) = probe.attempt(v)?;
        attempts.push((v.field, a));
    }
    let seconds = started.elapsed().as_secs_f64();
    drop(probe);
    println!("{} attempts in {seconds:.2} s", attempts.len());
    for (field, a) in &attempts {
        println!(
            "{field:<32} {:<34} {:<10} {:<24} {:?}",
            a.variant,
            format!("{:?}", a.submission).to_lowercase(),
            a.result_code.as_deref().unwrap_or("-"),
            a.applied
        );
    }
    if let Some(out) = out {
        let rows: Vec<Row> = attempts
            .iter()
            .map(|(field, attempt)| Row { field, attempt })
            .collect();
        let mut text = serde_json::to_string_pretty(&rows)?;
        text.push('\n');
        std::fs::write(out, text).with_context(|| format!("writing {}", out.display()))?;
    }
    Ok(true)
}

fn probe_all(
    only: &[String],
    out: &Path,
    candidate: &Path,
    dump: Option<&Path>,
    scratch: Option<&Path>,
    pins: &Path,
) -> Result<bool> {
    // where the table, the witnesses and the chains go, decided before the run: each world's
    // chain is recorded when the probe is done with it
    let target = match (only.is_empty(), scratch) {
        (true, None) => Some((out.to_owned(), candidate.to_owned())),
        (_, Some(dir)) => {
            std::fs::create_dir_all(dir)?;
            Some((dir.join("validation.json"), dir.join("candidate")))
        }
        (false, None) => None,
    };
    let chains = target.as_ref().map(|(_, c)| c.join("chains"));
    if let Some(dir) = &chains {
        clear_chains(dir)?;
    }
    let schema = shapekit_schema::Schema::pinned();
    let fields = shapekit_probe::run::selected(&schema, only)?;
    let image = CoreImage::from_pins(pins)
        .with_context(|| format!("reading the core image pin from {}", pins.display()))?;
    let work = tempfile::Builder::new()
        .prefix("shapekit-probe-all-")
        .tempdir()
        .context("creating the node workdir")?;
    let started = std::time::Instant::now();
    let runs = shapekit_probe::run::run(
        &fields,
        |protocol| shapekit_probe::node_config(TESTNET_PASSPHRASE, image.clone(), protocol),
        work.path(),
        chains.as_deref(),
        |r| {
            println!(
                "{:<56} {:<14} {} attempts",
                r.verdict.path,
                format!("{:?}", r.verdict.verdict),
                r.verdict.attempts.len()
            );
            for a in &r.verdict.attempts {
                println!(
                    "    {:<70} {:<8} {:<28} {:?}",
                    a.variant,
                    format!("{:?}", a.submission).to_lowercase(),
                    a.result_code.as_deref().unwrap_or("-"),
                    a.applied
                );
            }
        },
    )?;
    println!(
        "{} fields, {} attempts in {:.1} s",
        runs.len(),
        runs.iter().map(|r| r.verdict.attempts.len()).sum::<usize>(),
        started.elapsed().as_secs_f64()
    );
    if let Some(d) = dump {
        let v: Vec<_> = runs.iter().map(|r| &r.verdict).collect();
        std::fs::write(d, serde_json::to_string_pretty(&v)? + "\n")?;
    }
    let Some((out, candidate)) = target else {
        println!(
            "partial run: {} and {} left as they are",
            out.display(),
            candidate.display()
        );
        return Ok(true);
    };
    let mut runs = runs;
    shapekit_probe::witness::clear(&candidate, shapekit_probe::witness::FAMILY)?;
    for r in &mut runs {
        for (e, a) in r.evidence.iter().zip(r.verdict.attempts.iter_mut()) {
            a.witnesses = shapekit_probe::witness::write(&candidate, e)?;
        }
    }
    let table = ValidationTable {
        schema_version: ValidationTable::VERSION,
        core: core_ref(pins, &image)?,
        fields: runs.into_iter().map(|r| r.verdict).collect(),
    };
    let problems = table.problems();
    if !problems.is_empty() {
        bail!("the table has {} problems: {problems:#?}", problems.len());
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out, serde_json::to_string_pretty(&table)? + "\n")
        .with_context(|| format!("writing {}", out.display()))?;
    println!("wrote {} with {} fields", out.display(), table.fields.len());
    write_obligations(&table, &out)?;
    let version = match shapekit_manifest::build::load(&candidate.join("manifest.json")) {
        Ok(m) => m.corpus_version,
        Err(_) => env!("CARGO_PKG_VERSION").to_owned(),
    };
    crate::manifest_cmd::build_file(&candidate, pins, &version)
}

/// Removes the probe's recorded chains under `chains`, which a run records again.
fn clear_chains(chains: &Path) -> Result<()> {
    if !chains.is_dir() {
        return Ok(());
    }
    let prefix = format!("{}.", shapekit_probe::witness::FAMILY);
    for e in std::fs::read_dir(chains)? {
        let p = e?.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name.starts_with(&prefix) {
            std::fs::remove_dir_all(&p).with_context(|| format!("removing {}", p.display()))?;
        }
    }
    Ok(())
}

/// The probe's chains under `chains`, by directory name relative to it.
fn probe_chains(chains: &Path) -> Result<Vec<PathBuf>> {
    let prefix = format!("{}.", shapekit_probe::witness::FAMILY);
    let mut out = Vec::new();
    for e in std::fs::read_dir(chains).with_context(|| format!("reading {}", chains.display()))? {
        let p = e?.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if !name.starts_with(&prefix) {
            continue;
        }
        for n in std::fs::read_dir(&p)? {
            out.push(n?.path().strip_prefix(chains)?.to_path_buf());
        }
    }
    out.sort();
    Ok(out)
}

/// chain.json without what records core's raw output, whose change order differs between
/// runs (the same rule as `gen verify-repro`).
fn reproducible_chain_json(path: &Path) -> Result<serde_json::Value> {
    let mut v: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )?;
    let obj = v.as_object_mut().context("chain.json is not an object")?;
    obj.remove("stream_sha256");
    if let Some(serde_json::Value::Array(ledgers)) = obj.get_mut("ledgers") {
        for l in ledgers {
            if let Some(o) = l.as_object_mut() {
                o.remove("raw_sha256");
            }
        }
    }
    Ok(v)
}

/// Runs the probe again into a scratch directory and compares its chains, table and witness
/// manifest entries with the committed ones.
fn probe_verify_repro(out: &Path, candidate: &Path, pins: &Path) -> Result<bool> {
    let fresh = tempfile::Builder::new()
        .prefix("shapekit-probe-repro-")
        .tempdir()
        .context("creating the regeneration directory")?;
    // an empty scratch lane: only the probe's own entries are compared
    let all: Vec<String> = Vec::new();
    let fresh_out = fresh.path().join("validation.json");
    let fresh_candidate = fresh.path().join("candidate");
    std::fs::create_dir_all(&fresh_candidate)?;
    if !probe_all(&all, &fresh_out, &fresh_candidate, None, None, pins)? {
        bail!("the regenerating run failed");
    }
    let mut diffs = Vec::new();
    let committed_chains = candidate.join("chains");
    let fresh_chains = fresh_candidate.join("chains");
    let a = probe_chains(&committed_chains)?;
    let b = probe_chains(&fresh_chains)?;
    if a != b {
        diffs.push(format!("chains differ: committed {a:?}, regenerated {b:?}"));
    }
    for rel in &a {
        if !b.contains(rel) {
            continue;
        }
        let (x, y) = (committed_chains.join(rel), fresh_chains.join(rel));
        if reproducible_chain_json(&x.join("chain.json"))?
            != reproducible_chain_json(&y.join("chain.json"))?
        {
            diffs.push(format!(
                "{}: chain.json differs in canonical hashes or ledger range",
                rel.display()
            ));
        }
        if shapekit_pack::lake_digest(&x.join("ledgers"))?
            != shapekit_pack::lake_digest(&y.join("ledgers"))?
        {
            diffs.push(format!("{}: ledger lake differs", rel.display()));
        }
        println!("compared {}", rel.display());
    }
    if std::fs::read(out)? != std::fs::read(&fresh_out)? {
        diffs.push(format!(
            "{} differs from the regenerated table",
            out.display()
        ));
    }
    let entries = |root: &Path| -> Result<Vec<serde_json::Value>> {
        let m = shapekit_manifest::build::load(&root.join("manifest.json"))?;
        Ok(m.entries
            .iter()
            .filter(|e| e.family == shapekit_probe::witness::FAMILY)
            .map(serde_json::to_value)
            .collect::<std::result::Result<_, _>>()?)
    };
    if entries(candidate)? != entries(&fresh_candidate)? {
        diffs.push("the witness entries differ from the regenerated ones".into());
    }
    for d in &diffs {
        println!("DIFF {d}");
    }
    println!("{} chains compared, {} differences", a.len(), diffs.len());
    Ok(diffs.is_empty())
}

/// The pinned core release and the image the probe ran.
fn core_ref(pins_path: &Path, image: &CoreImage) -> Result<CoreRef> {
    let p = crate::pins::load(pins_path)?;
    let git = p
        .git
        .iter()
        .find(|g| g.name == "stellar-core")
        .with_context(|| format!("{} has no [[git]] stellar-core", pins_path.display()))?;
    Ok(CoreRef {
        version: git
            .git_ref
            .as_deref()
            .and_then(|r| r.strip_prefix("refs/tags/"))
            .context("the stellar-core pin names no tag")?
            .to_owned(),
        commit: git
            .sha
            .clone()
            .context("the stellar-core pin has no commit")?,
        image: Some(ImageRef {
            repository: image.repository.clone(),
            tag: image.tag.clone(),
            digest: image.digest.clone(),
        }),
    })
}

fn probe_compare(table: &Path, fields: &Path, verdicts: &Path, write: bool) -> Result<bool> {
    let t: ValidationTable =
        shapekit_contracts::load(table).with_context(|| format!("reading {}", table.display()))?;
    let f: toml::Table = std::fs::read_to_string(fields)
        .with_context(|| format!("reading {}", fields.display()))?
        .parse()
        .with_context(|| format!("parsing {}", fields.display()))?;
    let rows = shapekit_probe::compare::compare(&t, &f)?;
    let (mut open, mut settled, mut stale) = (0, 0, 0);
    for r in &rows {
        let state = match (r.agrees, &r.settled_by) {
            (true, None) => "agrees",
            (false, Some(_)) => {
                settled += 1;
                "settled"
            }
            (false, None) => {
                open += 1;
                "OPEN"
            }
            (true, Some(_)) => {
                stale += 1;
                "STALE settlement"
            }
        };
        println!("{:<60} {:<8} {state}", r.row, r.registry);
        if !r.agrees || r.settled_by.is_some() {
            for site in &r.sites {
                let v = if site.validated {
                    "validated"
                } else {
                    "not-validated"
                };
                let name = if site.carrier.is_empty() {
                    site.field.clone()
                } else {
                    format!("{} [{}]", site.field, site.carrier)
                };
                println!(
                    "    {name:<64} {v}, {} ({} attempts)",
                    site.values, site.attempts
                );
            }
            if let Some(x) = &r.settled_by {
                println!("    {}", x.reading);
            }
        }
    }
    println!(
        "{} rows: {} agree, {settled} settled by the source, {open} open, {stale} stale settlements",
        rows.len(),
        rows.len() - settled - open - stale
    );
    let file = shapekit_probe::compare::field_verdicts(&rows, &table.display().to_string());
    let text = serde_json::to_string_pretty(&file)? + "\n";
    let current = if write {
        std::fs::write(verdicts, &text)
            .with_context(|| format!("writing {}", verdicts.display()))?;
        println!("wrote {} with {} rows", verdicts.display(), file.rows.len());
        true
    } else {
        let committed = std::fs::read_to_string(verdicts).unwrap_or_default();
        let same = committed == text;
        if !same {
            println!(
                "{} is not current; run shapekit probe compare --write",
                verdicts.display()
            );
        }
        same
    };
    Ok(open == 0 && stale == 0 && current)
}

fn probe_limits(
    only: &[String],
    out: &Path,
    candidate: &Path,
    caps: &Path,
    scratch: Option<&Path>,
    pins: &Path,
) -> Result<bool> {
    use shapekit_contracts::limits::LimitsTable;
    use shapekit_probe::limits::{Check, checks, measure, probes};
    let caps_table: toml::Table = std::fs::read_to_string(caps)
        .with_context(|| format!("reading {}", caps.display()))?
        .parse()
        .with_context(|| format!("parsing {}", caps.display()))?;
    let rows: Vec<(String, u64)> = caps_table
        .get("limit")
        .and_then(|l| l.as_array())
        .context("caps-limits.toml has no [[limit]] rows")?
        .iter()
        .map(|r| {
            let id = r
                .get("id")
                .and_then(|v| v.as_str())
                .context("a row without id")?;
            let rec = r
                .get("recommended")
                .and_then(|v| v.as_integer())
                .with_context(|| format!("{id} has no recommended value"))?;
            Ok((id.to_owned(), u64::try_from(rec)?))
        })
        .collect::<Result<_>>()?;
    let all_checks = checks();
    for (id, _) in &rows {
        if !all_checks.iter().any(|(r, _)| r == id) {
            bail!("caps-limits.toml row {id} has no check");
        }
    }
    let probes: Vec<_> = probes()
        .into_iter()
        .filter(|p| only.is_empty() || only.iter().any(|o| o == p.row))
        .collect();
    for (r, c) in &all_checks {
        if !rows.iter().any(|(id, _)| id == r) {
            bail!("a check names {r}, which caps-limits.toml does not have");
        }
        let measured = shapekit_probe::limits::probes().iter().any(|p| p.row == *r);
        if matches!(c, Check::Unmeasured(_)) == measured {
            bail!("{r}: a row is measured exactly when its check is not unmeasured");
        }
    }
    let image = CoreImage::from_pins(pins)?;
    let work = tempfile::Builder::new()
        .prefix("shapekit-probe-limits-")
        .tempdir()?;
    let schema = shapekit_schema::Schema::pinned();
    let typer = shapekit_probe::edit::Typer::new(&schema);
    let mut worlds = std::collections::BTreeMap::new();
    let mut measured = Vec::new();
    for p in &probes {
        let w = match worlds.entry(p.world) {
            std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::btree_map::Entry::Vacant(e) => {
                let dir = work
                    .path()
                    .join(format!("{:?}", p.world).to_ascii_lowercase());
                std::fs::create_dir_all(&dir)?;
                let cfg = shapekit_probe::node_config(
                    TESTNET_PASSPHRASE,
                    image.clone(),
                    p.world.protocol(),
                );
                e.insert(shapekit_probe::world::World::start(p.world, cfg, &dir)?)
            }
        };
        let m = measure(w, &typer, p).with_context(|| format!("measuring {}", p.row))?;
        println!(
            "{:<36} {} = {} accepted, {} refused ({:?} {})",
            m.row,
            p.measure,
            m.limit.max_accepted,
            m.limit.first_rejected,
            m.limit.rejection.stage,
            m.limit
                .rejection
                .result_code
                .as_deref()
                .unwrap_or("no result")
        );
        for (n, o) in &m.tried {
            println!("    {n:>8}  {o}");
        }
        measured.push(m);
    }
    drop(worlds);

    let mut open = 0;
    for (id, rec) in &rows {
        let check = &all_checks
            .iter()
            .find(|(r, _)| r == id)
            .expect("checked above")
            .1;
        let m = measured.iter().find(|m| m.row == id.as_str());
        let verdict = match (check, m) {
            (_, None) if !only.is_empty() => continue,
            (Check::Unmeasured(why), None) => format!("unmeasured: {why}"),
            (Check::Equal, Some(m)) => {
                let a: u64 = m.limit.max_accepted.parse()?;
                if a == *rec {
                    format!("agrees, {a}")
                } else {
                    open += 1;
                    format!("OPEN: measured {a}, the row recommends {rec}")
                }
            }
            (Check::Size(f), Some(m)) => {
                let (a, r) = f(m)?;
                if a <= *rec && r > *rec {
                    format!("agrees, {a} bytes accepted and {r} refused around {rec}")
                } else {
                    open += 1;
                    format!("OPEN: {a} bytes accepted and {r} refused, the row recommends {rec}")
                }
            }
            (Check::Below(why), Some(m)) => {
                let a: u64 = m.limit.max_accepted.parse()?;
                if a <= *rec {
                    format!("agrees, measured {a} within {rec}: {why}")
                } else {
                    open += 1;
                    format!("OPEN: measured {a} past {rec}")
                }
            }
            (_, None) => bail!("{id} is measured but has no measurement"),
            (Check::Unmeasured(_), Some(_)) => bail!("{id} is unmeasured but was measured"),
        };
        println!("{id:<36} {verdict}");
    }
    println!(
        "{} limits measured, {open} open disagreements",
        measured.len()
    );

    let (out, candidate) = match (only.is_empty(), scratch) {
        (true, None) => (out.to_owned(), candidate.to_owned()),
        (false, Some(dir)) => {
            std::fs::create_dir_all(dir)?;
            (dir.join("limits.json"), dir.join("candidate"))
        }
        (false, None) => return Ok(open == 0),
        (true, Some(_)) => bail!("--scratch is for runs with --only"),
    };
    let family = shapekit_probe::witness::LIMITS_FAMILY;
    shapekit_probe::witness::clear(&candidate, family)?;
    let mut limits = Vec::new();
    for mut m in measured {
        let mut w = shapekit_probe::witness::write_in(&candidate, family, &m.accepted)?;
        w.extend(shapekit_probe::witness::write_in(
            &candidate, family, &m.refused,
        )?);
        m.limit.witnesses = w;
        limits.push(m.limit);
    }
    limits.sort_by(|a, b| (&a.path, &a.measure).cmp(&(&b.path, &b.measure)));
    let table = LimitsTable {
        schema_version: LimitsTable::VERSION,
        core: core_ref(pins, &image)?,
        limits,
    };
    let problems = table.problems();
    if !problems.is_empty() {
        bail!("the limits table has problems: {problems:#?}");
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out, serde_json::to_string_pretty(&table)? + "\n")?;
    println!("wrote {} with {} limits", out.display(), table.limits.len());
    let version = match shapekit_manifest::build::load(&candidate.join("manifest.json")) {
        Ok(m) => m.corpus_version,
        Err(_) => env!("CARGO_PKG_VERSION").to_owned(),
    };
    let built = crate::manifest_cmd::build_file(&candidate, pins, &version)?;
    Ok(built && open == 0)
}

/// The `field` obligations of a table, next to it as `<stem>.obligations.json`.
fn write_obligations(table: &ValidationTable, out: &Path) -> Result<()> {
    let ledger = shapekit_probe::compare::obligations(table);
    let problems = ledger.problems();
    if !problems.is_empty() {
        bail!("the field obligations have problems: {problems:#?}");
    }
    let path = out.with_extension("obligations.json");
    std::fs::write(&path, serde_json::to_string_pretty(&ledger)? + "\n")
        .with_context(|| format!("writing {}", path.display()))?;
    println!(
        "wrote {} with {} obligations",
        path.display(),
        ledger.obligations.len()
    );
    Ok(())
}
