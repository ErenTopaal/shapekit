//! Seed check of go-stellar-sdk with go-mutesting (the maintained avito-tech fork).
//!
//! The module source is copied to `<work>/sdk` and the testee is built against it through a
//! `replace` directive. go-mutesting writes each mutation of a file to a separate file and calls
//! an exec command with `MUTATE_ORIGINAL` and `MUTATE_CHANGED`; that command is
//! `shapekit conformance seed-go-exec`. A first pass only lists the mutations (by the sha-256 of
//! the mutated file) and skips them all;
//! the second pass builds and probes the seeded sample and skips the rest.
//!
//! Exit codes of the exec command, as go-mutesting reads them: 0 the mutation was caught, 1
//! it was missed, 2 it was skipped (not sampled, or it does not build).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use shapekit_process::error_text::chain;

use super::{
    Context, Paths, ProbeOutcome, Report, Status, Tool, args, copy_tree, counts, fault_from,
    output, probe, read_json, run, sample, write_baseline, write_json,
};
use crate::versions::VersionsFile;

/// The packages of the module that encode, decode and convert: xdr and strkey, without their
/// tests and without the two generated files of xdr (`xdr_generated.go`,
/// `xdr_views_generated.go`, 1.9 and 2.6 MB). go-mutesting writes the whole file out for every
/// mutation, so listing the mutations of those two files alone takes hours; the hand-written
/// encoding helpers around them are in scope.
pub const SCOPE: [&str; 2] = ["xdr", "strkey"];
const GENERATED: [&str; 2] = ["xdr_generated.go", "xdr_views_generated.go"];

/// The files go-mutesting is given, relative to the module root, sorted.
fn scope_files(sdk_dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for dir in SCOPE {
        let rd = std::fs::read_dir(sdk_dir.join(dir))
            .with_context(|| format!("reading {}/{dir}", sdk_dir.display()))?;
        for e in rd {
            let name = e?.file_name().to_string_lossy().into_owned();
            if name.ends_with(".go")
                && !name.ends_with("_test.go")
                && !GENERATED.contains(&name.as_str())
            {
                out.push(format!("./{dir}/{name}"));
            }
        }
    }
    out.sort();
    ensure!(!out.is_empty(), "no go files in the scope");
    Ok(out)
}

/// go-mutesting v2.3.1. The tag has no /v2 module path, so the module proxy serves it only as
/// this pseudo-version of the tagged commit.
pub const TOOL_MODULE: &str = "github.com/avito-tech/go-mutesting/cmd/go-mutesting";
pub const TOOL_VERSION: &str = "v0.0.0-20251226130216-48d0401f00fb";
pub const TOOL_RELEASE: &str = "v2.3.1";

/// One mutation as the list pass saw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listed {
    pub file: String,
    pub digest: String,
    pub line: u32,
    pub change: String,
}

/// What `seed-go-exec` is told through its environment and arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecSetup {
    pub work: PathBuf,
    pub sdk_dir: PathBuf,
    pub version: String,
    pub module: String,
    pub baseline: PathBuf,
    pub testee_dir: PathBuf,
    pub modfile: PathBuf,
    pub list: PathBuf,
    /// Empty in the list pass.
    pub sampled: BTreeSet<String>,
    pub records: PathBuf,
}

fn digest_hex(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(b))
}

/// The first and last line that differ, and what the first became.
fn describe(original: &str, changed: &str) -> (u32, String) {
    let a: Vec<&str> = original.lines().collect();
    let b: Vec<&str> = changed.lines().collect();
    let first = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[first..]
        .iter()
        .rev()
        .zip(b[first..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let before = a[first..a.len() - tail].join(" ").trim().to_string();
    let after = b[first..b.len() - tail].join(" ").trim().to_string();
    let cut = |s: String| {
        let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if s.chars().count() > 160 {
            format!("{}...", s.chars().take(160).collect::<String>())
        } else {
            s
        }
    };
    (
        u32::try_from(first + 1).unwrap_or(u32::MAX),
        format!("{} -> {}", cut(before), cut(after)),
    )
}

fn build_testee(setup: &ExecSetup, out: &Path) -> Result<()> {
    run(
        &[
            "go".to_string(),
            "build".to_string(),
            "-tags".to_string(),
            crate::versions::go_build_tag(&setup.version),
            "-modfile".to_string(),
            setup.modfile.display().to_string(),
            // a directory replace has no module version; hello must still report it
            "-ldflags".to_string(),
            format!("-X main.sdkVersionOverride={}", setup.version),
            "-o".to_string(),
            out.display().to_string(),
            ".".to_string(),
        ],
        &setup.testee_dir,
        &[
            ("GOWORK".to_string(), "off".to_string()),
            ("GOFLAGS".to_string(), "-mod=mod".to_string()),
        ],
    )
}

/// The exec command go-mutesting calls for every mutation; returns its exit code.
pub fn exec(paths: &Paths, setup_file: &Path) -> Result<i32> {
    let setup: ExecSetup = read_json(setup_file)?;
    let original =
        PathBuf::from(std::env::var("MUTATE_ORIGINAL").context("MUTATE_ORIGINAL is not set")?);
    let changed =
        PathBuf::from(std::env::var("MUTATE_CHANGED").context("MUTATE_CHANGED is not set")?);
    let orig_text = std::fs::read_to_string(&original)
        .with_context(|| format!("reading {}", original.display()))?;
    let changed_bytes =
        std::fs::read(&changed).with_context(|| format!("reading {}", changed.display()))?;
    let digest = digest_hex(&changed_bytes);
    // go-mutesting keeps every mutated copy of a file until it exits, and the generated xdr
    // files are megabytes each, so a run over them fills the disk. it reads the copy once more
    // after this command and only needs it to exist, so the copy is emptied now that its bytes
    // are in memory
    std::fs::write(&changed, b"").with_context(|| format!("emptying {}", changed.display()))?;
    // go-mutesting names files as it was given them, ./xdr/asset.go
    let rel = original
        .strip_prefix(&setup.sdk_dir)
        .unwrap_or(&original)
        .display()
        .to_string();
    let rel = rel.strip_prefix("./").unwrap_or(&rel).to_string();

    if setup.sampled.is_empty() {
        let (line, change) = describe(&orig_text, &String::from_utf8_lossy(&changed_bytes));
        let row = Listed {
            file: rel,
            digest,
            line,
            change,
        };
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&setup.list)
            .with_context(|| format!("opening {}", setup.list.display()))?;
        writeln!(f, "{}", serde_json::to_string(&row)?)?;
        return Ok(2);
    }
    if !setup.sampled.contains(&digest) {
        return Ok(2);
    }

    let record = setup.records.join(format!("{digest}.json"));
    std::fs::write(&original, &changed_bytes)
        .with_context(|| format!("writing {}", original.display()))?;
    let bin = setup.work.join(format!("testee-{digest}"));
    let result = (|| -> Result<i32> {
        if let Err(e) = build_testee(&setup, &bin) {
            write_json(
                &record,
                &ProbeOutcome::Failed {
                    message: format!("does not build: {}", chain(e.as_ref())),
                },
            )?;
            std::fs::write(record.with_extension("unviable"), b"")?;
            return Ok(2);
        }
        let outcome = probe(
            paths,
            "go",
            &setup.version,
            &[bin.display().to_string()],
            &setup.baseline,
            &record,
        )?;
        Ok(if outcome.caught() { 0 } else { 1 })
    })();
    // the original goes back whatever happened, or every later mutation builds on this one
    std::fs::write(&original, orig_text.as_bytes())
        .with_context(|| format!("restoring {}", original.display()))?;
    let _ = std::fs::remove_file(&bin);
    result
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Downloaded {
    dir: String,
}

pub fn check(ctx: &Context) -> Result<Report> {
    let root = &ctx.paths.root;
    let file = VersionsFile::load(&ctx.paths.versions)?;
    let sdk = file.sdk("go")?;
    let module = sdk.package.clone();
    ctx.fresh_work()?;
    let work = &ctx.work;
    let go_env = [("GOWORK".to_string(), "off".to_string())];

    // a writable copy of the module at the version under test
    let dl: Downloaded = serde_json::from_str(&output(
        &[
            "go".to_string(),
            "mod".to_string(),
            "download".to_string(),
            "-json".to_string(),
            format!("{module}@{}", ctx.version),
        ],
        work,
        &go_env,
    )?)
    .context("parsing go mod download")?;
    let sdk_dir = work.join("sdk");
    copy_tree(Path::new(&dl.dir), &sdk_dir)?;
    run(&args(&["chmod", "-R", "u+w", "."]), &sdk_dir, &[])?;

    // the testee's module file for this version, pointed at the copy
    let testee_dir = root.join("testees/go");
    let pinned = testee_dir.join(format!("versions/{}.mod", ctx.version));
    let mut modtext = std::fs::read_to_string(&pinned)
        .with_context(|| format!("reading {}", pinned.display()))?;
    modtext.push_str(&format!("\nreplace {module} => {}\n", sdk_dir.display()));
    let modfile = work.join("testee.mod");
    std::fs::write(&modfile, modtext)?;
    std::fs::copy(pinned.with_extension("sum"), work.join("testee.sum"))?;

    // go-mutesting, installed into the work directory
    let tool_bin = ctx.tools().join("bin");
    run(
        &[
            "go".to_string(),
            "install".to_string(),
            format!("{TOOL_MODULE}@{TOOL_VERSION}"),
        ],
        work,
        &[
            ("GOWORK".to_string(), "off".to_string()),
            ("GOBIN".to_string(), tool_bin.display().to_string()),
        ],
    )?;
    let mutesting = tool_bin.join("go-mutesting");

    let mut setup = ExecSetup {
        work: work.clone(),
        sdk_dir: sdk_dir.clone(),
        version: ctx.version.clone(),
        module: module.clone(),
        baseline: work.join("baseline.json"),
        testee_dir: testee_dir.clone(),
        modfile: modfile.clone(),
        list: work.join("mutations.jsonl"),
        sampled: BTreeSet::new(),
        records: work.join("records"),
    };
    std::fs::create_dir_all(&setup.records)?;
    let base_bin = work.join("testee-baseline");
    build_testee(&setup, &base_bin)?;
    write_baseline(
        &ctx.paths,
        "go",
        &ctx.version,
        &[base_bin.display().to_string()],
        &setup.baseline,
    )?;

    let setup_file = work.join("exec-setup.json");
    let hook = work.join("exec.sh");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nexec '{}' conformance seed-go-exec --root '{}' --manifest '{}' --versions-file '{}' --known-failures '{}'{} --setup '{}'\n",
            ctx.exe.display(),
            ctx.paths.root.display(),
            ctx.paths.manifest.display(),
            ctx.paths.versions.display(),
            ctx.paths.known_dir.display(),
            ctx.lane_args()
                .iter()
                .map(|a| format!(" '{a}'"))
                .collect::<String>(),
            setup_file.display()
        ),
    )?;
    run(&args(&["chmod", "+x", "exec.sh"]), work, &[])?;
    let files = scope_files(&sdk_dir)?;
    let mutesting_argv = |timeout: &str| -> Vec<String> {
        let mut v = vec![
            mutesting.display().to_string(),
            "--exec".to_string(),
            hook.display().to_string(),
            "--exec-timeout".to_string(),
            timeout.to_string(),
        ];
        v.extend(files.iter().cloned());
        v
    };

    // pass 1: the population
    write_json(&setup_file, &setup)?;
    run(&mutesting_argv("60"), &sdk_dir, &go_env)?;
    let listed: Vec<Listed> = std::fs::read_to_string(&setup.list)
        .with_context(|| format!("reading {}", setup.list.display()))?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()
        .context("parsing the mutation list")?;
    // go-mutesting already drops a mutation whose file equals an earlier one; the same file
    // reached twice is one fault
    let mut unique: BTreeMap<String, Listed> = BTreeMap::new();
    for l in listed {
        unique.entry(l.digest.clone()).or_insert(l);
    }
    let population: Vec<Listed> = unique.into_values().collect();
    ensure!(
        !population.is_empty(),
        "go-mutesting generates no mutation in the scope"
    );
    let chosen: Vec<&Listed> = sample(population.len(), ctx.sample, ctx.seed)
        .into_iter()
        .map(|i| &population[i])
        .collect();

    // pass 2: the sample
    setup.sampled = chosen.iter().map(|l| l.digest.clone()).collect();
    write_json(&setup_file, &setup)?;
    run(&mutesting_argv("1800"), &sdk_dir, &go_env)?;

    let capture = ctx.capture()?;
    let mut faults = Vec::new();
    for l in &chosen {
        let record = setup.records.join(format!("{}.json", l.digest));
        if !record.is_file() {
            bail!(
                "go-mutesting did not run the sampled mutation {} of {}",
                l.digest,
                l.file
            );
        }
        let outcome: ProbeOutcome = read_json(&record)?;
        let status = if record.with_extension("unviable").is_file() {
            Status::Unviable
        } else if outcome.caught() {
            Status::Caught
        } else {
            Status::Missed
        };
        faults.push(fault_from(
            format!("{}:{}:{}", l.file, l.line, &l.digest[..12]),
            l.file.clone(),
            l.line,
            l.change.clone(),
            status,
            if status == Status::Unviable {
                None
            } else {
                Some(&outcome)
            },
            &capture,
        ));
    }
    faults.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(Report {
        schema: super::REPORT_SCHEMA,
        sdk: "go".to_string(),
        package: module,
        version: ctx.version.clone(),
        tool: Tool {
            name: "go-mutesting".to_string(),
            version: TOOL_RELEASE.to_string(),
        },
        scope: vec![
            format!("xdr/*.go, without tests, {}", GENERATED.join(" and ")),
            "strkey/*.go, without tests".to_string(),
        ],
        population: population.len(),
        seed: ctx.seed,
        sample: faults.len(),
        counts: counts(&faults),
        faults,
        real_faults: Vec::new(),
        lane: None,
    })
}

#[cfg(test)]
mod tests {
    use super::describe;

    #[test]
    fn describes_the_changed_lines() {
        let a = "package x\nfunc f() int {\n\treturn a + b\n}\n";
        let b = "package x\nfunc f() int {\n\treturn a - b\n}\n";
        assert_eq!(
            describe(a, b),
            (3, "return a + b -> return a - b".to_string())
        );
        let c = "package x\nfunc f() int {\n}\n";
        assert_eq!(describe(a, c), (3, "return a + b -> ".to_string()));
    }
}
