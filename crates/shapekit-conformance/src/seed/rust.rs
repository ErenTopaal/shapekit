//! Seed check of the stellar-xdr crate with cargo-mutants.
//!
//! The published crate is unpacked into a scratch workspace next to a harness package that
//! builds the rust testee against it and holds one test: run the corpus through `seed-probe`.
//! cargo-mutants lists every mutant of the crate (`--list --json`); each sampled mutant is then
//! applied in place and tested by its own `cargo mutants --re '^<name>$'` run, so its probe
//! record is known. The crate is built with every feature on, so every listed mutant is in
//! compiled code; code the testee never reaches counts as missed. The crate's command line
//! (`src/bin`, `src/cli`) is not sdk code and is left out.

use std::path::Path;

use anyhow::{Context as _, Result, bail, ensure};
use serde::Deserialize;

use super::{
    Context, ProbeOutcome, Report, Status, Tool, counts, fault_from, output, read_json, run,
    sample, write_baseline,
};
use crate::upstream::Http;
use crate::versions::{self, VersionsFile};

pub const TOOL_VERSION: &str = "27.1.0";
pub const EXCLUDE: [&str; 2] = ["src/bin/**", "src/cli/**"];
const CRATE: &str = "stellar-xdr";
const HARNESS: &str = "shapekit-testee-rust";

#[derive(Debug, Clone, Deserialize)]
struct Span {
    start: Pos,
}

#[derive(Debug, Clone, Deserialize)]
struct Pos {
    line: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct Listed {
    name: String,
    file: String,
    span: Span,
}

#[derive(Debug, Deserialize)]
struct Outcomes {
    outcomes: Vec<Outcome>,
}

#[derive(Debug, Deserialize)]
struct Outcome {
    scenario: serde_json::Value,
    summary: String,
}

const TEST: &str = r#"// runs the corpus through the testee this package builds against the seeded crate;
// written by `shapekit conformance seed-check --sdk rust`
use std::process::Command;

#[test]
fn corpus_run_matches_the_unmodified_build() {
    let probe = std::env::var("SHAPEKIT_SEED_PROBE").expect("SHAPEKIT_SEED_PROBE is not set");
    let mut argv: Vec<String> = serde_json::from_str(&probe).expect("SHAPEKIT_SEED_PROBE is a json array");
    argv.push("--command".to_string());
    argv.push(env!("CARGO_BIN_EXE_shapekit-testee-rust").to_string());
    let status = Command::new(&argv[0]).args(&argv[1..]).status().expect("starting seed-probe");
    assert!(status.success(), "the corpus run changed ({status})");
}
"#;

fn cargo_mutants(ctx: &Context) -> String {
    ctx.tools().join("bin/cargo-mutants").display().to_string()
}

/// Unpacks the published crate into `dir`.
fn unpack_crate(version: &str, dir: &Path) -> Result<()> {
    let http = Http::new()?;
    let url = format!("https://static.crates.io/crates/{CRATE}/{CRATE}-{version}.crate");
    let bytes = http.get_bytes(&url)?;
    let tmp = dir.with_extension("unpack");
    std::fs::create_dir_all(&tmp)?;
    tar::Archive::new(flate2::read::GzDecoder::new(&bytes[..]))
        .unpack(&tmp)
        .with_context(|| format!("unpacking {url}"))?;
    std::fs::rename(tmp.join(format!("{CRATE}-{version}")), dir)
        .context("moving the unpacked crate")?;
    std::fs::remove_dir_all(&tmp)?;
    Ok(())
}

fn write_workspace(ctx: &Context, ws: &Path) -> Result<()> {
    let root = &ctx.paths.root;
    std::fs::create_dir_all(ws.join("harness/tests"))?;
    std::fs::write(
        ws.join("Cargo.toml"),
        "[workspace]\nmembers = [\"stellar-xdr\", \"harness\"]\nresolver = \"2\"\n",
    )?;
    unpack_crate(&ctx.version, &ws.join(CRATE))?;
    let pins = root.join(format!("testees/rust/versions/{}", ctx.version));
    let manifest = std::fs::read_to_string(pins.join("Cargo.toml"))?;
    let main = root.join("testees/rust/src/main.rs");
    let dep_old = format!("stellar-xdr = {{ version = \"={}\",", ctx.version);
    ensure!(
        manifest.contains(&dep_old) && manifest.contains("path = \"../../src/main.rs\""),
        "{} no longer has the shape seed-check rewrites",
        pins.join("Cargo.toml").display()
    );
    let mut harness = manifest
        .replace(
            "path = \"../../src/main.rs\"",
            &format!("path = {:?}", main.display().to_string()),
        )
        .replace(&dep_old, "stellar-xdr = { path = \"../stellar-xdr\",")
        .replace("\n[workspace]\n", "\n");
    harness.push_str("\n[[test]]\nname = \"corpus\"\npath = \"tests/corpus.rs\"\n");
    std::fs::write(ws.join("harness/Cargo.toml"), harness)?;
    std::fs::write(ws.join("harness/tests/corpus.rs"), TEST)?;
    // the testee's pinned lock file; cargo swaps the registry crate for the path one
    std::fs::copy(pins.join("Cargo.lock"), ws.join("Cargo.lock"))?;
    Ok(())
}

pub fn check(ctx: &Context) -> Result<Report> {
    let file = VersionsFile::load(&ctx.paths.versions)?;
    let target = file.target("rust", &ctx.version)?;
    if ctx.build {
        versions::build(&ctx.paths.root, &target)?;
    }
    ctx.fresh_work()?;
    let work = &ctx.work;
    let ws = work.join("ws");
    write_workspace(ctx, &ws)?;
    let target_dir = work.join("target");
    let cargo_env = vec![(
        "CARGO_TARGET_DIR".to_string(),
        target_dir.display().to_string(),
    )];

    run(
        &[
            "cargo".to_string(),
            "install".to_string(),
            "--locked".to_string(),
            "--root".to_string(),
            ctx.tools().display().to_string(),
            format!("cargo-mutants@{TOOL_VERSION}"),
        ],
        work,
        &cargo_env,
    )?;

    // the unmodified build and its baseline
    run(
        &[
            "cargo".to_string(),
            "build".to_string(),
            "--all-features".to_string(),
            "-p".to_string(),
            HARNESS.to_string(),
        ],
        &ws,
        &cargo_env,
    )?;
    let baseline = work.join("baseline.json");
    let base_bin = target_dir.join(format!("debug/{HARNESS}"));
    write_baseline(
        &ctx.paths,
        "rust",
        &ctx.version,
        &[base_bin.display().to_string()],
        &baseline,
    )?;

    let mut list_argv = vec![
        cargo_mutants(ctx),
        "mutants".to_string(),
        "--list".to_string(),
        "--json".to_string(),
        "--package".to_string(),
        CRATE.to_string(),
        "--all-features".to_string(),
    ];
    for e in EXCLUDE {
        list_argv.push("--exclude".to_string());
        list_argv.push(format!("{CRATE}/{e}"));
    }
    let listed: Vec<Listed> =
        serde_json::from_str(&output(&list_argv, &ws, &cargo_env)?).context("parsing the mutant list")?;
    let population = listed.len();
    ensure!(population > 0, "cargo-mutants lists no mutant");
    let chosen: Vec<&Listed> = sample(population, ctx.sample, ctx.seed)
        .into_iter()
        .map(|i| &listed[i])
        .collect();

    let records = work.join("records");
    std::fs::create_dir_all(&records)?;
    let mut faults = Vec::new();
    for (i, m) in chosen.iter().enumerate() {
        let record = records.join(format!("{i}.json"));
        let mut probe = ctx.probe_argv(&[], &baseline);
        probe.extend(["--record".to_string(), record.display().to_string()]);
        let out_dir = work.join(format!("out/{i}"));
        std::fs::create_dir_all(&out_dir)?;
        let argv = vec![
            cargo_mutants(ctx),
            "mutants".to_string(),
            "--in-place".to_string(),
            "--baseline".to_string(),
            "skip".to_string(),
            "--package".to_string(),
            CRATE.to_string(),
            "--test-package".to_string(),
            HARNESS.to_string(),
            "--all-features".to_string(),
            "--re".to_string(),
            format!("^{}$", regex_escape(&m.name)),
            "--timeout".to_string(),
            "900".to_string(),
            "--build-timeout".to_string(),
            "1800".to_string(),
            "--output".to_string(),
            out_dir.display().to_string(),
        ];
        let mut env = cargo_env.clone();
        env.push((
            "SHAPEKIT_SEED_PROBE".to_string(),
            serde_json::to_string(&probe)?,
        ));
        eprintln!("seed {}/{}: {}", i + 1, chosen.len(), m.name);
        // cargo-mutants exits 2 for a missed mutant, 3 for a timeout and 4 for an unviable one;
        // the outcome file says which
        let status = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(&ws)
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdout(std::io::stderr())
            .status()
            .with_context(|| format!("starting {}", argv.join(" ")))?;
        let outcomes: Outcomes = read_json(&out_dir.join("mutants.out/outcomes.json"))
            .with_context(|| format!("cargo-mutants exited with {status}"))?;
        let mine: Vec<&Outcome> = outcomes
            .outcomes
            .iter()
            .filter(|o| o.scenario.get("Mutant").is_some())
            .collect();
        ensure!(
            mine.len() == 1,
            "cargo-mutants ran {} mutants for {}",
            mine.len(),
            m.name
        );
        let st = match mine[0].summary.as_str() {
            "CaughtMutant" => Status::Caught,
            "MissedMutant" => Status::Missed,
            "Timeout" => Status::Timeout,
            "Unviable" => Status::Unviable,
            other => bail!("cargo-mutants reports {} as {other}", m.name),
        };
        let outcome: Option<ProbeOutcome> = if record.is_file() {
            Some(read_json(&record)?)
        } else {
            None
        };
        if st == Status::Caught {
            ensure!(
                outcome.as_ref().is_some_and(ProbeOutcome::caught),
                "cargo-mutants reports {} caught but the probe recorded no change",
                m.name
            );
        }
        let file = m
            .file
            .strip_prefix(&format!("{CRATE}/"))
            .unwrap_or(&m.file)
            .to_string();
        let name = m
            .name
            .strip_prefix(&format!("{CRATE}/"))
            .unwrap_or(&m.name)
            .to_string();
        let mutation = name
            .splitn(4, ':')
            .nth(3)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| name.clone());
        faults.push(fault_from(
            name,
            file,
            m.span.start.line,
            mutation,
            st,
            outcome.as_ref(),
        ));
    }
    faults.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(Report {
        schema: super::REPORT_SCHEMA,
        sdk: "rust".to_string(),
        package: ctx.package.clone(),
        version: ctx.version.clone(),
        tool: Tool {
            name: "cargo-mutants".to_string(),
            version: TOOL_VERSION.to_string(),
        },
        scope: vec!["src/**/*.rs, without src/bin and src/cli".to_string()],
        population,
        seed: ctx.seed,
        sample: faults.len(),
        counts: counts(&faults),
        faults,
        real_faults: Vec::new(),
    })
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::regex_escape;

    #[test]
    fn names_become_literal_patterns() {
        assert_eq!(
            regex_escape("src/a.rs:6:5: replace main with ()"),
            "src/a\\.rs:6:5: replace main with \\(\\)"
        );
        assert_eq!(regex_escape("<impl Foo>::x -> Vec<u8>"), "<impl Foo>::x -> Vec<u8>");
    }
}
