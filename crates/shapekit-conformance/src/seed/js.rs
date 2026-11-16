//! Seed check of @stellar/stellar-sdk with Stryker.
//!
//! The installed package is copied to `<work>/sdk` and linked back into `node_modules`, so
//! Stryker can mutate its compiled files in place while the testee loads them. Stryker's own
//! instrumenter lists every mutant of the scope (`testees/js/seed/list-mutants.mjs`); a seeded
//! sample of them is then given to Stryker as exact source ranges. Stryker's command runner
//! activates one mutant at a time through `__STRYKER_ACTIVE_MUTANT__`, which the testee
//! process inherits, and calls `shapekit conformance seed-probe` for it.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, Result, bail, ensure};
use serde::Deserialize;
use serde_json::json;

use super::{
    Context, Fault, ProbeOutcome, Report, Status, Tool, args, copy_tree, counts, fault_from,
    output, read_json, real_faults, run, sample, write_baseline, write_json,
};
use crate::versions::{self, VersionsFile};

/// The compiled code the testee loads for encoding and helpers: the base library (keys,
/// strkey, assets, scval and native conversions, operations) and the generated xdr.
pub const SCOPE: [&str; 2] = ["lib/esm/base", "lib/esm/xdr"];

const STRYKER_PACKAGE: &str = "@stryker-mutator/core";

#[derive(Debug, Clone, Deserialize)]
struct Pos {
    line: u32,
    column: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct Loc {
    start: Pos,
    end: Pos,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listed {
    file_name: String,
    location: Loc,
    mutator_name: String,
    replacement: String,
}

#[derive(Debug, Deserialize)]
struct StrykerReport {
    files: BTreeMap<String, StrykerFile>,
}

#[derive(Debug, Deserialize)]
struct StrykerFile {
    mutants: Vec<StrykerMutant>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StrykerMutant {
    id: String,
    mutator_name: String,
    replacement: Option<String>,
    location: Loc,
    status: String,
}

// the instrumenter counts lines and columns from 0, stryker's report from 1; keys count
// both from 1 like an editor
fn key(file: &str, l: &Loc, mutator: &str, replacement: &str) -> String {
    let digest = hex::encode(&sha2_256(replacement.as_bytes())[..4]);
    format!(
        "{file}:{}:{}-{}:{}:{mutator}:{digest}",
        l.start.line + 1,
        l.start.column + 1,
        l.end.line + 1,
        l.end.column + 1
    )
}

fn sha2_256(b: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(b).into()
}

fn tool_version(tool: &Path) -> Result<String> {
    #[derive(Deserialize)]
    struct Pkg {
        version: String,
    }
    let p: Pkg = read_json(
        &tool
            .join("node_modules")
            .join(STRYKER_PACKAGE)
            .join("package.json"),
    )?;
    Ok(p.version)
}

fn scope_files(work: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for dir in SCOPE {
        let base = work.join("sdk").join(dir);
        for e in walkdir::WalkDir::new(&base).sort_by_file_name() {
            let e = e.with_context(|| format!("walking {}", base.display()))?;
            if e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "js") {
                let rel = e.path().strip_prefix(work)?.to_string_lossy().into_owned();
                out.push(rel);
            }
        }
    }
    ensure!(!out.is_empty(), "no javascript files under the scope");
    Ok(out)
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn check(ctx: &Context) -> Result<Report> {
    let root = &ctx.paths.root;
    let file = VersionsFile::load(&ctx.paths.versions)?;
    let target = file.target("js", &ctx.version)?;
    if ctx.build {
        versions::build(root, &target)?;
    }
    ctx.fresh_work()?;
    let work = &ctx.work;
    let pins = root.join(format!("testees/js/versions/{}", ctx.version));

    // the sdk install, then the package moved out of node_modules so stryker sees it as source
    for f in ["package.json", "package-lock.json"] {
        std::fs::copy(pins.join(f), work.join(f))
            .with_context(|| format!("copying {}", pins.join(f).display()))?;
    }
    run(
        &args(&["npm", "ci", "--no-audit", "--no-fund", "--ignore-scripts"]),
        work,
        &[],
    )?;
    let installed = work.join("node_modules/@stellar/stellar-sdk");
    std::fs::rename(&installed, work.join("sdk")).context("moving the sdk out of node_modules")?;
    std::os::unix::fs::symlink("../../sdk", &installed).context("linking the sdk")?;

    // the compiled testee next to that install, so node resolves the sdk through the link
    let dist = root.join(format!("target/testees/js/{}/dist", ctx.version));
    ensure!(
        dist.is_dir(),
        "{} is missing; build the js {} testee first",
        dist.display(),
        ctx.version
    );
    copy_tree(&dist, &work.join("dist"))?;
    let command = vec![
        "node".to_string(),
        work.join("dist/main.js").display().to_string(),
    ];

    // stryker, at the versions committed under testees/js/seed
    let tool = work.join("tool");
    std::fs::create_dir_all(&tool)?;
    let seed_dir = root.join("testees/js/seed");
    for f in ["package.json", "package-lock.json", "list-mutants.mjs"] {
        std::fs::copy(seed_dir.join(f), tool.join(f))
            .with_context(|| format!("copying {}", seed_dir.join(f).display()))?;
    }
    run(
        &args(&["npm", "ci", "--no-audit", "--no-fund", "--ignore-scripts"]),
        &tool,
        &[],
    )?;
    let tool_version = tool_version(&tool)?;

    // every mutant stryker generates in the scope, then the sample
    let files = scope_files(work)?;
    let mut list_argv = vec![
        "node".to_string(),
        tool.join("list-mutants.mjs").display().to_string(),
    ];
    list_argv.extend(files.iter().cloned());
    let listed: Vec<Listed> =
        serde_json::from_str(&output(&list_argv, work, &[])?).context("parsing the mutant list")?;
    let population = listed.len();
    ensure!(population > 0, "stryker generates no mutant in the scope");
    let chosen: Vec<&Listed> = sample(population, ctx.sample, ctx.seed)
        .into_iter()
        .map(|i| &listed[i])
        .collect();

    let baseline = work.join("baseline.json");
    write_baseline(&ctx.paths, "js", &ctx.version, &command, &baseline)?;

    let records = work.join("records");
    let mut probe = ctx.probe_argv(&command, &baseline);
    probe.extend([
        "--record-dir".to_string(),
        records.display().to_string(),
        "--label-env".to_string(),
        "__STRYKER_ACTIVE_MUTANT__".to_string(),
    ]);
    let probe_line = probe
        .iter()
        .map(|a| shell_quote(a))
        .collect::<Vec<_>>()
        .join(" ");
    let mutate: Vec<String> = chosen
        .iter()
        .map(|m| {
            // stryker reads mutate ranges with lines from 1 and columns from 0
            let l = &m.location;
            format!(
                "{}:{}:{}-{}:{}",
                m.file_name,
                l.start.line + 1,
                l.start.column,
                l.end.line + 1,
                l.end.column
            )
        })
        .collect();
    let report_path = work.join("stryker-report.json");
    let config = json!({
        "testRunner": "command",
        "commandRunner": { "command": probe_line },
        "inPlace": true,
        "mutate": mutate,
        "coverageAnalysis": "off",
        "concurrency": ctx.jobs,
        "reporters": ["json", "clear-text"],
        "jsonReporter": { "fileName": report_path.display().to_string() },
        "tempDirName": work.join(".stryker-tmp").display().to_string(),
        "cleanTempDir": true,
    });
    let config_path = work.join("stryker.config.json");
    write_json(&config_path, &config)?;
    run(
        &[
            tool.join("node_modules/.bin/stryker").display().to_string(),
            "run".to_string(),
            config_path.display().to_string(),
        ],
        work,
        &[],
    )?;

    // stryker also runs mutants nested inside a sampled range; only the sampled ones count
    let report: StrykerReport = read_json(&report_path)?;
    let mut by_key: BTreeMap<String, &StrykerMutant> = BTreeMap::new();
    for (f, sf) in &report.files {
        for m in &sf.mutants {
            let zero = Loc {
                start: Pos {
                    line: m.location.start.line - 1,
                    column: m.location.start.column - 1,
                },
                end: Pos {
                    line: m.location.end.line - 1,
                    column: m.location.end.column - 1,
                },
            };
            let k = key(
                f,
                &zero,
                &m.mutator_name,
                m.replacement.as_deref().unwrap_or(""),
            );
            by_key.insert(k, m);
        }
    }
    let mut faults = Vec::new();
    for m in &chosen {
        let k = key(&m.file_name, &m.location, &m.mutator_name, &m.replacement);
        let Some(r) = by_key.get(&k) else {
            bail!("stryker did not run the sampled mutant {k}");
        };
        let status = match r.status.as_str() {
            "Killed" => Status::Caught,
            "Timeout" => Status::Timeout,
            "Survived" => Status::Missed,
            "CompileError" | "RuntimeError" => Status::Unviable,
            other => bail!("stryker reports {k} as {other}"),
        };
        let record = records.join(format!("{}.json", r.id));
        let outcome: Option<ProbeOutcome> = if record.is_file() {
            Some(read_json(&record)?)
        } else {
            None
        };
        if status == Status::Caught {
            ensure!(
                outcome.as_ref().is_some_and(ProbeOutcome::caught),
                "stryker reports {k} killed but the probe recorded no change"
            );
        }
        let file_rel = m
            .file_name
            .strip_prefix("sdk/")
            .unwrap_or(&m.file_name)
            .to_string();
        faults.push(fault_from(
            k.strip_prefix("sdk/").unwrap_or(&k).to_string(),
            file_rel,
            m.location.start.line + 1,
            format!("{} -> {}", m.mutator_name, m.replacement),
            status,
            outcome.as_ref(),
        ));
    }

    let real = real_faults(&ctx.paths, "js", ctx.build)?;
    Ok(Report {
        schema: super::REPORT_SCHEMA,
        sdk: "js".to_string(),
        package: ctx.package.clone(),
        version: ctx.version.clone(),
        tool: Tool {
            name: "stryker".to_string(),
            version: tool_version,
        },
        scope: SCOPE.iter().map(|s| format!("{s}/**/*.js")).collect(),
        population,
        seed: ctx.seed,
        sample: faults.len(),
        counts: counts(&faults),
        faults: faults_sorted(faults),
        real_faults: real,
    })
}

fn faults_sorted(mut f: Vec<Fault>) -> Vec<Fault> {
    f.sort_by(|a, b| a.key.cmp(&b.key));
    f
}
