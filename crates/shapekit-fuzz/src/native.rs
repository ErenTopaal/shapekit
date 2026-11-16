//! `shapekit native-fuzz`: each sdk's own coverage-guided fuzzer on its decoder, with the
//! encoding laws of the law library as the check. Rust runs cargo-fuzz (libFuzzer) on
//! `testees/rust/fuzz`, go runs `go test -fuzz` on `testees/go/fuzz_test.go`, javascript runs
//! jazzer.js on `testees/js/fuzz/xdr-laws.cjs`. The three targets read one input format (a type
//! selector byte in `FuzzType::ALL` order, then the xdr) and name a broken law the same way,
//! as a class (`<type>:roundtrip:<in>-><out>`, `<type>:noncanonical:longer|shorter`,
//! `<type>:panic`).
//!
//! A fuzzer stops at the first failing input. The runner records it with its class, adds the
//! class to the target's skip list and starts the fuzzer again on the time that is left, so one
//! run finds several classes. Classes in the known list are skipped from the start. Every class
//! found is minimised by its own tool, checked once more on its own, then judged by stellar-xdr
//! under the corpus limits and written to the candidate lane as a synthetic xdr entry.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use shapekit_conformance::versions::VersionsFile;
use shapekit_manifest::{DecodeExpectation, Entry, Validity, sha256_hex};

use crate::types::FuzzType;

pub const CARGO_FUZZ_VERSION: &str = "0.13.2";
const RUST_TARGET: &str = "xdr_laws";
const GO_TARGET: &str = "FuzzXdrLaws";
const MAX_LEN: usize = 256 * 1024;
// a restart costs a build check and the seed run; less time than this is not worth one
const MIN_SLICE: Duration = Duration::from_secs(30);
// predicate calls when shrinking a rust input, as for the differential fuzzer
const MINIMIZE_BUDGET: usize = 6000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Rust,
    Go,
    Js,
}

impl Target {
    pub fn parse(s: &str) -> Result<Target> {
        Ok(match s {
            "rust" => Target::Rust,
            "go" => Target::Go,
            "js" => Target::Js,
            other => bail!("no native fuzz target {other}; rust, go or js"),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Target::Rust => "rust",
            Target::Go => "go",
            Target::Js => "js",
        }
    }
}

pub struct NativeConfig {
    pub root: PathBuf,
    pub target: Target,
    pub seconds: u64,
    pub out: PathBuf,
    /// The candidate lane new classes are written to.
    pub candidate: PathBuf,
    /// `<target> <class> <issue or reason>` per line.
    pub known: PathBuf,
    pub build: bool,
    pub max_seed_bytes: u64,
}

/// One class a fuzzer found.
#[derive(Debug, Clone, Serialize)]
pub struct Found {
    pub class: String,
    /// The minimised input, selector byte first.
    #[serde(skip)]
    pub input: Vec<u8>,
    pub input_len: usize,
    pub original_len: usize,
    pub detail: String,
    /// The candidate entry it was written as, or the entry that already holds it.
    pub entry: String,
    pub new_entry: bool,
}

#[derive(Debug, Serialize)]
pub struct NativeReport {
    pub target: String,
    pub tool: String,
    pub sdk: String,
    pub sdk_version: String,
    pub seconds: u64,
    pub restarts: usize,
    pub seeds: usize,
    pub known_skipped: Vec<String>,
    pub found: Vec<Found>,
}

/// The known list: target, class, why.
pub fn load_known(path: &Path) -> Result<BTreeMap<(String, String), String>> {
    let mut out = BTreeMap::new();
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, char::is_whitespace);
        let (Some(t), Some(c), Some(why)) = (parts.next(), parts.next(), parts.next()) else {
            bail!(
                "{}:{}: expected `<target> <class> <issue or reason>`",
                path.display(),
                n + 1
            );
        };
        Target::parse(t).with_context(|| format!("{}:{}", path.display(), n + 1))?;
        ensure!(
            out.insert((t.to_string(), c.to_string()), why.trim().to_string())
                .is_none(),
            "{}:{}: {t} {c} is listed twice",
            path.display(),
            n + 1
        );
    }
    Ok(out)
}

fn run_ok(cmd: &mut Command, what: &str) -> Result<()> {
    let status = cmd
        .stdout(std::io::stderr())
        .status()
        .with_context(|| format!("starting {what}"))?;
    ensure!(status.success(), "{what} exited with {status}");
    Ok(())
}

/// What one fuzzer process left behind.
struct Stop {
    success: bool,
    class: Option<String>,
    detail: String,
    artifact: Option<PathBuf>,
    log_tail: String,
}

fn run_capture(cmd: &mut Command, log: &Path) -> Result<(bool, String)> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .context("starting the fuzzer")?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    std::fs::write(log, &text).with_context(|| format!("writing {}", log.display()))?;
    Ok((out.status.success(), text))
}

/// The first class line and the artifact path in a fuzzer's output.
fn parse_stop(success: bool, text: &str, dir: &Path) -> Stop {
    let mut class = None;
    let mut detail = String::new();
    let mut artifact = None;
    for line in text.lines() {
        if class.is_none()
            && let Some(at) = line.find("shapekit-class ")
        {
            let rest = &line[at + "shapekit-class ".len()..];
            let (c, d) = rest.split_once(": ").unwrap_or((rest, ""));
            class = Some(c.trim().to_string());
            detail = d.trim().to_string();
        }
        // libFuzzer and jazzer.js: "Test unit written to <path>"; go: "Failing input written
        // to testdata/fuzz/<target>/<hash>"
        for marker in ["Test unit written to ", "Failing input written to "] {
            if let Some(at) = line.find(marker) {
                let p = line[at + marker.len()..].trim();
                let p = PathBuf::from(p);
                artifact = Some(if p.is_absolute() { p } else { dir.join(p) });
            }
        }
    }
    let lines: Vec<&str> = text.lines().collect();
    let tail = lines[lines.len().saturating_sub(40)..].join("\n");
    Stop {
        success,
        class,
        detail,
        artifact,
        log_tail: tail,
    }
}

/// Go writes failing inputs in its corpus file format: a version line, then one
/// `[]byte("...")` line with Go's string escapes.
fn read_go_corpus_file(path: &Path) -> Result<Vec<u8>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut lines = text.lines();
    ensure!(
        lines.next() == Some("go test fuzz v1"),
        "{} is not a go fuzz corpus file",
        path.display()
    );
    let body = lines
        .next()
        .and_then(|l| l.strip_prefix("[]byte(\""))
        .and_then(|l| l.strip_suffix("\")"))
        .with_context(|| format!("{} holds no []byte value", path.display()))?;
    unquote_go(body).with_context(|| format!("decoding {}", path.display()))
}

fn unquote_go(s: &str) -> Result<Vec<u8>> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let c = *b.get(i + 1).context("a trailing backslash")?;
        i += 2;
        match c {
            b'x' => {
                let h = s.get(i..i + 2).context("a short \\x escape")?;
                out.push(u8::from_str_radix(h, 16)?);
                i += 2;
            }
            b'0'..=b'7' => {
                let o = s.get(i - 1..i + 2).context("a short octal escape")?;
                out.push(u8::from_str_radix(o, 8)?);
                i += 2;
            }
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(11),
            b'\\' => out.push(b'\\'),
            b'"' => out.push(b'"'),
            b'\'' => out.push(b'\''),
            b'u' | b'U' => {
                let n = if c == b'u' { 4 } else { 8 };
                let h = s.get(i..i + n).context("a short unicode escape")?;
                let ch = char::from_u32(u32::from_str_radix(h, 16)?)
                    .context("an invalid unicode escape")?;
                let mut buf = [0; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                i += n;
            }
            other => bail!("unknown escape \\{}", other as char),
        }
    }
    Ok(out)
}

/// Seeds in the shared input format, one file each.
fn write_seeds(cfg: &NativeConfig, dir: &Path) -> Result<usize> {
    std::fs::create_dir_all(dir)?;
    let seeds = crate::load_seeds(&crate::Config {
        root: cfg.root.clone(),
        corpus: cfg.root.join("corpus"),
        versions: cfg.root.join("conformance/versions.toml"),
        go_version: None,
        build: false,
        seconds: 0,
        max_inputs: None,
        seed: 0,
        jobs: 1,
        types: FuzzType::ALL.to_vec(),
        max_seed_bytes: cfg.max_seed_bytes,
        max_arbitrary_bytes: 0,
    })?;
    let mut n = 0;
    for (ty, list) in seeds {
        let sel = FuzzType::ALL
            .iter()
            .position(|t| *t == ty)
            .context("a seed of an unknown type")?;
        for bytes in list {
            let mut input = vec![u8::try_from(sel)?];
            input.extend_from_slice(&bytes);
            std::fs::write(dir.join(sha256_hex(&input)), &input)?;
            n += 1;
        }
    }
    Ok(n)
}

fn types_json() -> Value {
    Value::Array(
        FuzzType::ALL
            .iter()
            .map(|t| json!({"name": t.name(), "depth": t.depth()}))
            .collect(),
    )
}

struct Engine {
    target: Target,
    root: PathBuf,
    work: PathBuf,
    tool: String,
    sdk: String,
    sdk_version: String,
    /// The go module file of the current version, or the js install directory.
    sdk_dir: PathBuf,
}

impl Engine {
    fn prepare(cfg: &NativeConfig, work: &Path) -> Result<Engine> {
        let file = VersionsFile::load(&cfg.root.join("conformance/versions.toml"))?;
        let sdk = file.sdk(cfg.target.name())?;
        let target = file.target(cfg.target.name(), &sdk.current)?;
        let tools = cfg.root.join("target/native-fuzz/tools");
        let (tool, sdk_dir) = match cfg.target {
            Target::Rust => {
                run_ok(
                    Command::new("cargo")
                        .args(["install", "--locked", "--root"])
                        .arg(&tools)
                        .arg(format!("cargo-fuzz@{CARGO_FUZZ_VERSION}"))
                        .current_dir(&cfg.root),
                    "cargo install cargo-fuzz",
                )?;
                (format!("cargo-fuzz {CARGO_FUZZ_VERSION}"), PathBuf::new())
            }
            Target::Go => (
                "go test -fuzz".to_string(),
                cfg.root
                    .join(format!("testees/go/versions/{}.mod", sdk.current)),
            ),
            Target::Js => {
                if cfg.build {
                    shapekit_conformance::versions::build(&cfg.root, &target)?;
                }
                let fuzz = cfg.root.join("testees/js/fuzz");
                run_ok(
                    Command::new("npm")
                        .args(["ci", "--no-audit", "--no-fund", "--ignore-scripts"])
                        .current_dir(&fuzz),
                    "npm ci in testees/js/fuzz",
                )?;
                let p: Value = serde_json::from_str(&std::fs::read_to_string(
                    fuzz.join("node_modules/@jazzer.js/core/package.json"),
                )?)?;
                (
                    format!(
                        "jazzer.js {}",
                        p["version"].as_str().context("jazzer.js has no version")?
                    ),
                    cfg.root.join(format!("target/testees/js/{}", sdk.current)),
                )
            }
        };
        Ok(Engine {
            target: cfg.target,
            root: cfg.root.clone(),
            work: work.to_path_buf(),
            tool,
            sdk: sdk.package.clone(),
            sdk_version: sdk.current.clone(),
            sdk_dir,
        })
    }

    fn config_file(&self, seeds: &Path, skip: &Path) -> Result<PathBuf> {
        let path = self.work.join("config.json");
        let v = json!({
            "types": types_json(),
            "seeds": seeds,
            "skip": skip,
            "sdk": self.sdk_dir,
        });
        std::fs::write(&path, serde_json::to_string_pretty(&v)?)?;
        Ok(path)
    }

    /// One fuzzer process for at most `slice`.
    fn fuzz(&self, seeds: &Path, skip: &Path, slice: Duration, log: &Path) -> Result<Stop> {
        let secs = slice.as_secs().max(1).to_string();
        let corpus = self.work.join("corpus");
        let artifacts = self.work.join("artifacts");
        std::fs::create_dir_all(&corpus)?;
        std::fs::create_dir_all(&artifacts)?;
        let prefix = format!("{}/", artifacts.display());
        let config = self.config_file(seeds, skip)?;
        let (success, text, dir) = match self.target {
            Target::Rust => {
                let mut cmd =
                    Command::new(self.root.join("target/native-fuzz/tools/bin/cargo-fuzz"));
                cmd.args([
                    "fuzz",
                    "run",
                    "-s",
                    "none",
                    "--fuzz-dir",
                    "testees/rust/fuzz",
                ])
                .arg("--target-dir")
                .arg(self.root.join("target/native-fuzz/rust-build"))
                .arg(RUST_TARGET)
                .arg(&corpus)
                .arg(seeds)
                .arg("--")
                .arg(format!("-max_total_time={secs}"))
                .arg(format!("-artifact_prefix={prefix}"))
                .arg(format!("-max_len={MAX_LEN}"))
                .args(["-timeout=60", "-rss_limit_mb=2048"])
                .env("SHAPEKIT_FUZZ_SKIP", skip)
                .current_dir(&self.root);
                let (ok, t) = run_capture(&mut cmd, log)?;
                (ok, t, self.root.clone())
            }
            Target::Go => {
                let dir = self.root.join("testees/go");
                let mut cmd = Command::new("go");
                cmd.args(["test", "-run", "^$", "-fuzz"])
                    .arg(format!("^{GO_TARGET}$"))
                    .arg(format!("-fuzztime={secs}s"))
                    .args(["-parallel", "2", "-modfile"])
                    .arg(&self.sdk_dir)
                    .arg(".")
                    .env("GOWORK", "off")
                    .env("SHAPEKIT_FUZZ_CONFIG", &config)
                    .current_dir(&dir);
                let (ok, t) = run_capture(&mut cmd, log)?;
                (ok, t, dir)
            }
            Target::Js => {
                let dir = self.root.join("testees/js/fuzz");
                let mut cmd = Command::new(dir.join("node_modules/.bin/jazzer"));
                cmd.arg("xdr-laws.cjs")
                    .arg(&corpus)
                    .arg(seeds)
                    .args(["--sync", "-i", "stellar-sdk", "-e", "jazzer.js", "--"])
                    .arg(format!("-max_total_time={secs}"))
                    .arg(format!("-artifact_prefix={prefix}"))
                    .arg(format!("-max_len={MAX_LEN}"))
                    .args(["-timeout=60", "-rss_limit_mb=4096"])
                    .env("SHAPEKIT_FUZZ_CONFIG", &config)
                    .current_dir(&dir);
                let (ok, t) = run_capture(&mut cmd, log)?;
                (ok, t, dir)
            }
        };
        Ok(parse_stop(success, &text, &dir))
    }

    /// The input of a stop, selector byte first.
    fn read_artifact(&self, path: &Path) -> Result<Vec<u8>> {
        match self.target {
            Target::Go => read_go_corpus_file(path),
            _ => std::fs::read(path).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// The class the target names for one input, run on its own.
    fn classify(&self, input: &[u8], skip: &Path, log: &Path) -> Result<Option<(String, String)>> {
        let file = self.work.join("probe.bin");
        std::fs::write(&file, input)?;
        let text = match self.target {
            Target::Rust => {
                let mut cmd =
                    Command::new(self.root.join("target/native-fuzz/tools/bin/cargo-fuzz"));
                cmd.args([
                    "fuzz",
                    "run",
                    "-s",
                    "none",
                    "--fuzz-dir",
                    "testees/rust/fuzz",
                ])
                .arg("--target-dir")
                .arg(self.root.join("target/native-fuzz/rust-build"))
                .arg(RUST_TARGET)
                .arg(&file)
                .env("SHAPEKIT_FUZZ_SKIP", skip)
                .current_dir(&self.root);
                run_capture(&mut cmd, log)?.1
            }
            Target::Js => {
                let dir = self.root.join("testees/js/fuzz");
                let config = self.config_file(&self.work.join("empty-seeds"), skip)?;
                std::fs::create_dir_all(self.work.join("empty-seeds"))?;
                let mut cmd = Command::new(dir.join("node_modules/.bin/jazzer"));
                cmd.arg("xdr-laws.cjs")
                    .arg(&file)
                    .args(["--sync", "-i", "stellar-sdk", "-e", "jazzer.js"])
                    .env("SHAPEKIT_FUZZ_CONFIG", &config)
                    .current_dir(&dir);
                run_capture(&mut cmd, log)?.1
            }
            Target::Go => {
                // go runs a named corpus file of the target as an ordinary test
                let dir = self.root.join("testees/go");
                let data = dir.join(format!("testdata/fuzz/{GO_TARGET}"));
                std::fs::create_dir_all(&data)?;
                let name = format!("shapekit-probe-{}", &sha256_hex(input)[..16]);
                let mut body = String::from("go test fuzz v1\n[]byte(\"");
                for b in input {
                    body.push_str(&format!("\\x{b:02x}"));
                }
                body.push_str("\")\n");
                std::fs::write(data.join(&name), body)?;
                std::fs::create_dir_all(self.work.join("empty-seeds"))?;
                let config = self.config_file(&self.work.join("empty-seeds"), skip)?;
                let mut cmd = Command::new("go");
                cmd.args(["test", "-run"])
                    .arg(format!("^{GO_TARGET}$/^{name}$"))
                    .arg("-modfile")
                    .arg(&self.sdk_dir)
                    .arg(".")
                    .env("GOWORK", "off")
                    .env("SHAPEKIT_FUZZ_CONFIG", &config)
                    .current_dir(&dir);
                let r = run_capture(&mut cmd, log);
                std::fs::remove_file(data.join(&name))?;
                r?.1
            }
        };
        let stop = parse_stop(false, &text, &self.root);
        Ok(stop.class.map(|c| (c, stop.detail)))
    }

    /// Go's engine minimised the input already. Rust inputs are shrunk in process by the
    /// word-aware minimiser of the differential fuzzer, with the rust target's own law check
    /// as the predicate; javascript ones by libFuzzer's crash minimisation inside jazzer.js.
    /// Either way the result is checked against the target again before it counts.
    fn minimize(&self, artifact: &Path, input: &[u8], skip: &Path, class: &str) -> Result<Vec<u8>> {
        let min = self.work.join("minimized.bin");
        let log = self.work.join("minimize.log");
        let ran = match self.target {
            Target::Go => return Ok(input.to_vec()),
            Target::Rust => {
                let (sel, body) = input.split_first().context("an empty input")?;
                let ty = FuzzType::ALL[usize::from(*sel) % FuzzType::ALL.len()];
                let (small, _) = crate::minimize::minimize(body, MINIMIZE_BUDGET, |cand| {
                    Ok(rust_class(ty, cand).as_deref() == Some(class))
                })?;
                let mut out = vec![*sel];
                out.extend_from_slice(&small);
                if out.len() < input.len()
                    && self
                        .classify(&out, skip, &self.work.join("minimize-check.log"))?
                        .is_some_and(|(c, _)| c == class)
                {
                    return Ok(out);
                }
                return Ok(input.to_vec());
            }
            Target::Js => {
                let dir = self.root.join("testees/js/fuzz");
                let config = self.config_file(&self.work.join("empty-seeds"), skip)?;
                std::fs::create_dir_all(self.work.join("empty-seeds"))?;
                let mut cmd = Command::new(dir.join("node_modules/.bin/jazzer"));
                cmd.arg("xdr-laws.cjs")
                    .arg(artifact)
                    .args(["--sync", "-i", "stellar-sdk", "-e", "jazzer.js", "--"])
                    .args(["-minimize_crash=1", "-runs=20000"])
                    .arg(format!("-exact_artifact_path={}", min.display()))
                    .env("SHAPEKIT_FUZZ_CONFIG", &config)
                    .current_dir(&dir);
                run_capture(&mut cmd, &log)?
            }
        };
        let _ = ran;
        // minimisation keeps any crash; it counts only when the class stayed the same
        if let Ok(small) = std::fs::read(&min)
            && small.len() < input.len()
            && self
                .classify(&small, skip, &self.work.join("minimize-check.log"))?
                .is_some_and(|(c, _)| c == class)
        {
            return Ok(small);
        }
        Ok(input.to_vec())
    }
}

/// The class the rust target names for `bytes` of `ty`, computed in process the same way:
/// decode under the corpus limits, then the encoding laws of the law library.
pub fn rust_class(ty: FuzzType, bytes: &[u8]) -> Option<String> {
    let limits = ty.limits(bytes.len());
    let again = ty.roundtrip(bytes, limits).ok()?;
    if again != bytes {
        return Some(format!(
            "{}:roundtrip:{}",
            ty.name(),
            first_word_change(bytes, &again)
        ));
    }
    let mut longer = bytes.to_vec();
    longer.extend_from_slice(&[0; 4]);
    if ty.roundtrip(&longer, stellar_xdr::Limits::none()).is_ok() {
        return Some(format!("{}:noncanonical:longer", ty.name()));
    }
    if !bytes.is_empty()
        && ty
            .roundtrip(&bytes[..bytes.len() - 1], stellar_xdr::Limits::none())
            .is_ok()
    {
        return Some(format!("{}:noncanonical:shorter", ty.name()));
    }
    None
}

fn word_kind(w: Option<&[u8]>) -> &'static str {
    match w {
        Some([0, 0, 0, 0]) => "0",
        Some([0, 0, 0, 1]) => "1",
        _ => "x",
    }
}

/// `<in>-><out>` of the first 4-byte word in which the two encodings differ, as the targets
/// name it.
fn first_word_change(input: &[u8], again: &[u8]) -> String {
    let words = input.len().max(again.len()).div_ceil(4);
    for i in 0..words {
        let a = input.get(i * 4..(i * 4 + 4).min(input.len()));
        let b = again.get(i * 4..(i * 4 + 4).min(again.len()));
        if a != b {
            return format!("{}->{}", word_kind(a), word_kind(b));
        }
    }
    "x->x".to_string()
}

/// The class's law as an entry flag and id part.
fn law_slug(class: &str) -> String {
    let mut parts = class.split(':').skip(1);
    let law = parts.next().unwrap_or("unknown");
    match parts.next() {
        Some(rest) => format!(
            "{law}-{}",
            rest.replace("->", "-to-")
                .replace(|c: char| !c.is_ascii_alphanumeric() && c != '-', "")
        ),
        None => law.to_string(),
    }
}

/// The family a class's entry belongs to: the first family (by priority) that takes synthetic
/// entries and one of whose fragments the value's shape signature matches; for bytes
/// stellar-xdr does not decode, the first such family with a fragment anchored at the type.
fn family_for<'a>(
    families: &'a [shapekit_registry::Family],
    ty: FuzzType,
    bytes: &[u8],
) -> Option<&'a shapekit_registry::Family> {
    let mut ordered: Vec<&shapekit_registry::Family> = families
        .iter()
        .filter(|f| {
            f.source_classes
                .contains(&shapekit_registry::SourceClass::Synthetic)
        })
        .collect();
    ordered.sort_by(|a, b| (a.priority, &a.id).cmp(&(b.priority, &b.id)));
    if let Ok(sig) = shapekit_manifest::shape_signature(ty.name(), bytes, ty.limits(bytes.len()))
        && let Some(f) = ordered.iter().find(|f| {
            f.signature
                .iter()
                .any(|frag| shapekit_manifest::fragment_matches(&sig, frag))
        })
    {
        return Some(f);
    }
    let colon = format!("{}:", ty.name());
    let eq = format!("{}=", ty.name());
    ordered.into_iter().find(|f| {
        f.signature
            .iter()
            .any(|frag| frag.starts_with(&colon) || frag.starts_with(&eq))
    })
}

/// The candidate entry for one class: stellar-xdr's verdict on the bytes under the corpus
/// limits decides the expectation, as for the differential fuzzer's reproducers.
pub fn entry_for(
    target: Target,
    sdk: &str,
    sdk_version: &str,
    found: &Found,
    families: &[shapekit_registry::Family],
) -> Result<Option<(String, Value, Vec<u8>)>> {
    let sel = *found.input.first().context("an empty input")?;
    let ty = FuzzType::ALL[usize::from(sel) % FuzzType::ALL.len()];
    let bytes = found.input[1..].to_vec();
    let Some(family) = family_for(families, ty, &bytes) else {
        return Ok(None);
    };
    let id = format!(
        "{}.native-fuzz-{}-{}-{}-{}",
        family.id,
        target.name(),
        crate::kebab(ty.name()),
        law_slug(&found.class),
        &sha256_hex(found.class.as_bytes())[..8]
    );
    let rust = ty.roundtrip(&bytes, ty.limits(bytes.len()));
    let (decode, validity, why) = match &rust {
        Ok(again) if again != &bytes => (
            DecodeExpectation::ImplementationDefined,
            Validity::NotEmittable,
            "implementation-defined: stellar-xdr reads these bytes but writes different ones back, so they are not canonical and no conforming encoder emits them",
        ),
        Ok(_) if ty.core_decodes() => (
            DecodeExpectation::MustAccept,
            Validity::Valid,
            "core's verdict: stellar-xdr TransactionEnvelope::from_xdr at depth 1000 reads and round-trips them; validity here is decode validity only",
        ),
        Ok(_) => (
            DecodeExpectation::ImplementationDefined,
            Validity::Valid,
            "implementation-defined: core produces this type and never decodes untrusted bytes of it",
        ),
        Err(_) if ty.core_decodes() => (
            DecodeExpectation::MustReject,
            Validity::InvalidByCore,
            "core's verdict: from protocol 28 core refuses an envelope stellar-xdr cannot read at depth 1000 (can_parse_transaction)",
        ),
        Err(_) => (
            DecodeExpectation::ImplementationDefined,
            Validity::NotEmittable,
            "implementation-defined: core produces this type and never decodes untrusted bytes of it",
        ),
    };
    let rust_said = match &rust {
        Ok(again) if again != &bytes => {
            format!("accepts and re-encodes {} other bytes", again.len())
        }
        Ok(_) => "accepts and round-trips".to_string(),
        Err(e) => format!("rejects ({e})"),
    };
    let notes = format!(
        "native fuzz class {} of {sdk} {sdk_version} ({}), minimised to {} bytes from {}: {}. \
         stellar-xdr {} under the corpus limits {rust_said}. expected: {why}.",
        found.class,
        target.name(),
        bytes.len(),
        found.original_len.saturating_sub(1),
        found.detail,
        stellar_xdr::VERSION.pkg,
    );
    let value = json!({
        "caps": family.caps,
        "expected": {"decode": decode, "roundtrip": decode == DecodeExpectation::MustAccept, "validity": validity},
        "family": family.id,
        "fields_exercised": [],
        "flags": ["native-fuzz", target.name(), law_slug(&found.class)],
        "id": id,
        "kind": "xdr",
        "linked_issues": [],
        "notes": notes,
        "passphrase_dependent": false,
        "protocol": {"introduced": family.protocol_introduced, "valid_min": family.protocol_valid.min},
        "source": {"class": "synthetic"},
        "synthetic": true,
        "variants": {"any": {
            "xdr_file": format!("entries/{}/{id}/any.xdr", family.id),
            "xdr_sha256": sha256_hex(&bytes),
        }},
        "xdr_type": ty.name(),
    });
    let entry: Entry = serde_json::from_value(value)
        .with_context(|| format!("{id}: the record is not a manifest entry"))?;
    Ok(Some((id, serde_json::to_value(entry)?, bytes)))
}

pub fn run(cfg: &NativeConfig) -> Result<NativeReport> {
    let work = cfg.out.clone();
    if work.exists() {
        std::fs::remove_dir_all(&work).with_context(|| format!("removing {}", work.display()))?;
    }
    std::fs::create_dir_all(&work)?;
    let engine = Engine::prepare(cfg, &work)?;
    let seeds = work.join("seeds");
    let n_seeds = write_seeds(cfg, &seeds)?;

    let known = load_known(&cfg.known)?;
    let mut skip: BTreeSet<String> = known
        .keys()
        .filter(|(t, _)| t == cfg.target.name())
        .map(|(_, c)| c.clone())
        .collect();
    let known_skipped: Vec<String> = skip.iter().cloned().collect();
    let skip_file = work.join("skip.txt");

    let deadline = Instant::now() + Duration::from_secs(cfg.seconds);
    let mut found: Vec<Found> = Vec::new();
    let mut restarts = 0;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left < MIN_SLICE && restarts > 0 {
            break;
        }
        std::fs::write(
            &skip_file,
            skip.iter().map(|c| format!("{c}\n")).collect::<String>(),
        )?;
        let log = work.join(format!("fuzz-{restarts}.log"));
        let stop = engine.fuzz(&seeds, &skip_file, left.max(MIN_SLICE), &log)?;
        restarts += 1;
        if stop.success {
            break;
        }
        let Some(class) = stop.class else {
            bail!(
                "the {} fuzzer stopped without naming a class; {}:\n{}",
                cfg.target.name(),
                log.display(),
                stop.log_tail
            );
        };
        ensure!(
            !skip.contains(&class),
            "the {} fuzzer reported {class} again although it is skipped; {}",
            cfg.target.name(),
            log.display()
        );
        let artifact = stop.artifact.with_context(|| {
            format!(
                "the fuzzer named {class} but wrote no input; {}",
                log.display()
            )
        })?;
        let original = engine.read_artifact(&artifact)?;
        if cfg.target == Target::Go {
            // go wrote the input into the testee's source tree
            std::fs::rename(
                &artifact,
                work.join(format!("go-{}", &sha256_hex(&original)[..16])),
            )?;
        }
        let input = engine.minimize(&artifact, &original, &skip_file, &class)?;
        match engine.classify(&input, &skip_file, &work.join("check.log"))? {
            Some((c, _)) if c == class => {}
            other => {
                bail!("the input the fuzzer stopped on for {class} gives {other:?} on its own run")
            }
        }
        skip.insert(class.clone());
        found.push(Found {
            class,
            input_len: input.len(),
            original_len: original.len(),
            input,
            detail: stop.detail,
            entry: String::new(),
            new_entry: false,
        });
    }

    // the classes as candidate entries
    let registry = shapekit_registry::Registry::load(&cfg.root.join("registry"))
        .map_err(|e| anyhow::anyhow!("loading registry/: {e}"))?;
    let families = registry
        .families
        .map(|f| f.families)
        .context("registry/families.toml is missing")?;
    let mut wrote = false;
    for f in &mut found {
        let Some((id, entry, bytes)) =
            entry_for(cfg.target, &engine.sdk, &engine.sdk_version, f, &families)?
        else {
            f.entry = "no family takes it; not written".to_string();
            continue;
        };
        f.entry = id.clone();
        let family = entry["family"].as_str().unwrap_or_default().to_string();
        let in_corpus = cfg
            .root
            .join("corpus/entries")
            .join(&family)
            .join(&id)
            .exists();
        let dir = cfg.candidate.join("entries").join(&family).join(&id);
        if in_corpus || dir.exists() {
            continue;
        }
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("any.xdr"), &bytes)?;
        let mut text = serde_json::to_string_pretty(&entry)?;
        text.push('\n');
        std::fs::write(dir.join("entry.json"), text)?;
        f.new_entry = true;
        wrote = true;
    }
    if wrote {
        run_ok(
            Command::new(std::env::current_exe()?)
                .args(["manifest", "build", "--corpus"])
                .arg(&cfg.candidate)
                .current_dir(&cfg.root),
            "shapekit manifest build --corpus candidate",
        )?;
    }

    let report = NativeReport {
        target: cfg.target.name().to_string(),
        tool: engine.tool.clone(),
        sdk: engine.sdk.clone(),
        sdk_version: engine.sdk_version.clone(),
        seconds: cfg.seconds,
        restarts,
        seeds: n_seeds,
        known_skipped,
        found,
    };
    let mut text = serde_json::to_string_pretty(&report)?;
    text.push('\n');
    std::fs::write(work.join("report.json"), text)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_corpus_files_decode() {
        assert_eq!(
            unquote_go(r#"\x06\x00A\"\\\n\001"#).unwrap(),
            vec![6, 0, b'A', b'"', b'\\', b'\n', 1]
        );
    }

    #[test]
    fn stops_name_the_first_class_and_the_artifact() {
        let text = "panicked at x\nshapekit-class ScVal:roundtrip:x->0: re-encoding gives 8 bytes\n\
                    shapekit-class ScVal:panic\nartifact_prefix='/a/'; Test unit written to /a/crash-1\n";
        let s = parse_stop(false, text, Path::new("/r"));
        assert_eq!(s.class.as_deref(), Some("ScVal:roundtrip:x->0"));
        assert_eq!(s.detail, "re-encoding gives 8 bytes");
        assert_eq!(s.artifact, Some(PathBuf::from("/a/crash-1")));
        let go = "    fuzz_test.go:140: shapekit-class Asset:panic: boom\n    Failing input written to testdata/fuzz/FuzzXdrLaws/abc\n";
        let s = parse_stop(false, go, Path::new("/r/testees/go"));
        assert_eq!(s.class.as_deref(), Some("Asset:panic"));
        assert_eq!(
            s.artifact,
            Some(PathBuf::from("/r/testees/go/testdata/fuzz/FuzzXdrLaws/abc"))
        );
    }

    #[test]
    fn law_slugs_are_id_safe() {
        assert_eq!(law_slug("ScVal:roundtrip:x->0"), "roundtrip-x-to-0");
        assert_eq!(law_slug("Asset:panic"), "panic");
        assert_eq!(law_slug("ScVal:noncanonical:longer"), "noncanonical-longer");
    }
}
