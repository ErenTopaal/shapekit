//! `shapekit doctor`: checks that the machine has what `check sdk`, `check indexer` and `lint`
//! need, and says what is missing and how to install it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::harness_cmd::split_command;
use crate::pins::{self, Toolchain};

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum For {
    /// `shapekit check sdk`
    Sdk,
    /// `shapekit check indexer`
    Indexer,
    /// `shapekit lint`
    Lint,
}

#[derive(Args)]
pub struct DoctorArgs {
    /// The commands to check for; every one when absent.
    #[arg(value_enum)]
    what: Vec<For>,
    /// With sdk: only the toolchains of these SDKs (keys of the versions file); repeatable.
    #[arg(long = "sdk")]
    sdks: Vec<String>,
    /// With indexer: the adapter command line you will pass to `check indexer`.
    #[arg(long)]
    adapter: Option<String>,
    /// With lint: the indexerlint binary, a path or a name looked up on PATH.
    #[arg(long, default_value = "indexerlint")]
    linter: String,
    /// Skip the checks that contact package registries and GitHub.
    #[arg(long)]
    offline: bool,
    #[arg(long, default_value = "conformance/versions.toml")]
    versions_file: PathBuf,
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
    #[arg(long, default_value = "pins.toml")]
    pins: PathBuf,
}

#[derive(Default)]
struct Report {
    failed: usize,
    warned: usize,
}

impl Report {
    fn ok(&self, what: &str) {
        println!("ok    {what}");
    }
    fn warn(&mut self, what: &str, fix: &str) {
        self.warned += 1;
        println!("warn  {what}");
        if !fix.is_empty() {
            println!("      {fix}");
        }
    }
    fn fail(&mut self, what: &str, fix: &str) {
        self.failed += 1;
        println!("FAIL  {what}");
        if !fix.is_empty() {
            println!("      {fix}");
        }
    }
}

/// The first line a command prints, or None when it cannot be started or fails.
fn first_line(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Some(text.lines().next().unwrap_or("").trim().to_owned())
}

/// Whether `program` can be started: a path that exists, or a name found on PATH.
fn on_path(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

fn install_hint(tool: &str, pinned: Option<&str>) -> String {
    let v = pinned.map(|v| format!(" {v}")).unwrap_or_default();
    match tool {
        "cargo" | "rustc" => format!(
            "install Rust{v} with rustup (https://rustup.rs); in a checkout, rustup reads the version from rust-toolchain.toml"
        ),
        "go" => format!("install Go{v} from https://go.dev/dl/"),
        "node" | "npm" | "npx" => format!("install Node.js{v}, which ships npm, from https://nodejs.org/en/download"),
        "docker" => "install Docker Engine (https://docs.docker.com/engine/install/) and add your user to the docker group".to_owned(),
        "indexerlint" => "download indexerlint-<os>-<arch> from a shapekit release, or build it with `(cd lint && go build -o indexerlint ./cmd/indexerlint)`, and put it on PATH".to_owned(),
        "git" => "install git from your system's package manager".to_owned(),
        other => format!("install {other} and put it on PATH"),
    }
}

fn pinned_version(pins: Option<&pins::Pins>, t: Toolchain) -> Option<String> {
    pins?
        .toolchains
        .iter()
        .find(|p| p.name == t)
        .map(|p| p.version.clone())
}

/// Checks one tool: present on PATH, and for the pinned toolchains its version against the pin.
fn tool(r: &mut Report, name: &str, pins: Option<&pins::Pins>, why: &str) {
    let toolchain = match name {
        "cargo" | "rustc" => Some(Toolchain::Rust),
        "go" => Some(Toolchain::Go),
        "node" | "npm" | "npx" => Some(Toolchain::Node),
        _ => None,
    };
    let pinned = toolchain.and_then(|t| pinned_version(pins, t));
    if !on_path(name) {
        r.fail(
            &format!("{name}: not found ({why})"),
            &install_hint(name, pinned.as_deref()),
        );
        return;
    }
    // the version the toolchain itself reports, compared with pins.toml
    let found = match toolchain {
        Some(Toolchain::Rust) => first_line("rustc", &["--version"])
            .and_then(|l| l.split_whitespace().nth(1).map(str::to_owned)),
        Some(Toolchain::Go) => {
            first_line("go", &["env", "GOVERSION"]).map(|v| v.trim_start_matches("go").to_owned())
        }
        // npm and npx come with node, whose version is the one pinned and checked under node
        Some(Toolchain::Node) if name != "node" => None,
        Some(Toolchain::Node) => {
            first_line("node", &["--version"]).map(|v| v.trim_start_matches('v').to_owned())
        }
        // the testee toolchains are not tools doctor checks; no name above maps to them
        Some(Toolchain::Flutter | Toolchain::Php | Toolchain::Composer | Toolchain::Dotnet)
        | None => None,
    };
    match (found, pinned) {
        (Some(f), Some(p)) if f == p => r.ok(&format!("{name}: {f}, the pinned version ({why})")),
        (Some(f), Some(p)) => r.warn(
            &format!("{name}: {f}, pins.toml pins {p} ({why})"),
            &format!(
                "results and builds are checked on {p}; {}",
                install_hint(name, Some(&p))
            ),
        ),
        (Some(f), None) => r.ok(&format!("{name}: {f} ({why})")),
        (None, _) if toolchain.is_some() && name != "npm" && name != "npx" => r.fail(
            &format!("{name}: found but does not report its version ({why})"),
            &install_hint(name, None),
        ),
        (None, _) => r.ok(&format!("{name}: found ({why})")),
    }
}

/// Any HTTP answer means the name resolved and TLS worked; the status is not looked at.
fn reachable(r: &mut Report, client: &reqwest::blocking::Client, url: &str, why: &str) {
    match client.head(url).send() {
        Ok(resp) => r.ok(&format!(
            "{url}: reachable, HTTP {} ({why})",
            resp.status().as_u16()
        )),
        Err(e) => r.warn(
            &format!("{url}: not reachable: {e} ({why})"),
            "a first run needs it; once the builds and images exist, later runs work offline",
        ),
    }
}

fn disk(r: &mut Report, dir: &Path) {
    let out = Command::new("df").args(["-Pk", "--"]).arg(dir).output();
    let free_kib = out.ok().and_then(|o| {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .nth(1)
            .and_then(|l| l.split_whitespace().nth(3).map(str::to_owned))
            .and_then(|v| v.parse::<u64>().ok())
    });
    // measured: the testee builds of every version in conformance/versions.toml take 1.3 GiB
    // (276 MiB for rust 30.0.0 alone), the wallet-backend and timescaledb images 0.5 GB; the
    // rest leaves room for the toolchains' own caches
    const WARN_GIB: u64 = 5;
    match free_kib {
        Some(k) if k / (1024 * 1024) >= WARN_GIB => r.ok(&format!(
            "disk: {} GiB free in {}",
            k / (1024 * 1024),
            dir.display()
        )),
        Some(k) => r.warn(
            &format!(
                "disk: {:.1} GiB free in {}, under {WARN_GIB} GiB",
                k as f64 / (1024.0 * 1024.0),
                dir.display()
            ),
            "free space before building testees or a target's images",
        ),
        None => r.warn(
            &format!(
                "disk: could not read free space with df for {}",
                dir.display()
            ),
            "",
        ),
    }
}

fn sdk(
    r: &mut Report,
    a: &DoctorArgs,
    pins: Option<&pins::Pins>,
    client: Option<&reqwest::blocking::Client>,
) {
    let file = match shapekit_conformance::versions::VersionsFile::load(&a.versions_file) {
        Ok(f) => f,
        Err(e) => {
            r.fail(
                &format!("{}: {e:#}", a.versions_file.display()),
                "run from the root of a shapekit checkout, or pass --versions-file",
            );
            return;
        }
    };
    let names: Vec<String> = if a.sdks.is_empty() {
        file.sdk.keys().cloned().collect()
    } else {
        a.sdks.clone()
    };
    let mut tools: Vec<(String, String)> = Vec::new();
    for name in &names {
        let sdk = match file.sdk(name) {
            Ok(s) => s,
            Err(e) => {
                r.fail(&format!("sdk {name}: {e:#}"), "");
                continue;
            }
        };
        let target = match file.target(name, &sdk.current) {
            Ok(t) => t,
            Err(e) => {
                r.fail(&format!("sdk {name}: {e:#}"), "");
                continue;
            }
        };
        // every program the build steps and the testee command start, except files the build
        // itself writes (paths inside the checkout)
        let mut programs: Vec<&str> = target
            .build
            .iter()
            .filter_map(|s| s.argv.first())
            .map(String::as_str)
            .collect();
        if let Some(c) = target.command.first() {
            programs.push(c);
        }
        for p in programs {
            if p.contains('/') {
                continue;
            }
            match tools.iter_mut().find(|(t, _)| t == p) {
                Some((_, users)) => {
                    if !users.split(", ").any(|u| u == name) {
                        users.push_str(&format!(", {name}"));
                    }
                }
                None => tools.push((p.to_owned(), name.clone())),
            }
        }
    }
    tools.sort();
    for (t, users) in &tools {
        tool(r, t, pins, &format!("check sdk: {users}"));
    }
    if let Some(client) = client {
        let used = |t: &str| tools.iter().any(|(x, _)| x == t);
        if used("cargo") {
            reachable(
                r,
                client,
                "https://index.crates.io/config.json",
                "cargo fetches the sdk crates",
            );
        }
        if used("go") {
            reachable(
                r,
                client,
                "https://proxy.golang.org/",
                "go fetches the sdk modules",
            );
        }
        if used("npm") {
            reachable(
                r,
                client,
                "https://registry.npmjs.org/",
                "npm installs the sdk packages",
            );
        }
    }
}

fn indexer(r: &mut Report, a: &DoctorArgs) {
    if !on_path("docker") {
        r.fail(
            "docker: not found (check indexer runs the target and its replay network in docker)",
            &install_hint("docker", None),
        );
    } else {
        match Command::new("docker")
            .args(["version", "--format", "{{.Server.Version}}"])
            .output()
        {
            Ok(o) if o.status.success() => r.ok(&format!(
                "docker: daemon {} answers (check indexer)",
                String::from_utf8_lossy(&o.stdout).trim()
            )),
            Ok(o) => r.fail(
                &format!(
                    "docker: the daemon does not answer: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ),
                "start the docker daemon, and add your user to the docker group to use it without sudo",
            ),
            Err(e) => r.fail(&format!("docker: {e}"), &install_hint("docker", None)),
        }
    }
    if let Some(cmd) = &a.adapter {
        match split_command(cmd) {
            Ok(argv) if on_path(&argv[0]) => r.ok(&format!("adapter: {} found", argv[0])),
            Ok(argv) => r.fail(
                &format!("adapter: {} not found", argv[0]),
                "build the adapter first; for the wallet-backend reference adapter, `(cd adapters && go build -o bin/wb-adapter ./walletbackend/cmd/wb-adapter)`",
            ),
            Err(e) => r.fail(&format!("adapter: {e:#}"), ""),
        }
    }
}

fn lint(r: &mut Report, a: &DoctorArgs, pins: Option<&pins::Pins>) {
    tool(
        r,
        "go",
        pins,
        "lint: indexerlint loads packages with the go toolchain",
    );
    if on_path(&a.linter) {
        r.ok(&format!("indexerlint: {} found (lint)", a.linter));
    } else {
        r.fail(
            &format!("indexerlint: {} not found (lint)", a.linter),
            &install_hint("indexerlint", None),
        );
    }
}

pub fn run(a: DoctorArgs) -> Result<bool> {
    let what = if a.what.is_empty() {
        vec![For::Sdk, For::Indexer, For::Lint]
    } else {
        a.what.clone()
    };
    let mut r = Report::default();
    let needs_checkout = what.iter().any(|w| matches!(w, For::Sdk | For::Indexer));
    let pins = if a.pins.is_file() {
        match pins::load(&a.pins) {
            Ok(p) => Some(p),
            Err(e) => {
                r.fail(&format!("{}: {e:#}", a.pins.display()), "");
                None
            }
        }
    } else {
        if needs_checkout {
            r.warn(
                &format!(
                    "{}: not found, so toolchain versions are not compared with the pins",
                    a.pins.display()
                ),
                "run from the root of a shapekit checkout",
            );
        }
        None
    };
    if needs_checkout {
        let manifest = a.corpus.join("manifest.json");
        if manifest.is_file() {
            r.ok(&format!("corpus: {} present", manifest.display()));
        } else {
            r.fail(
                &format!("corpus: {} not found", manifest.display()),
                "run from the root of a shapekit checkout at a release tag, or pass --corpus",
            );
        }
    }
    let client = if a.offline || !what.contains(&For::Sdk) {
        None
    } else {
        Some(
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()?,
        )
    };
    if what.contains(&For::Sdk) {
        sdk(&mut r, &a, pins.as_ref(), client.as_ref());
    }
    if what.contains(&For::Indexer) {
        indexer(&mut r, &a);
    }
    if what.contains(&For::Lint) {
        lint(&mut r, &a, pins.as_ref());
    }
    disk(&mut r, Path::new("."));
    println!("doctor: {} missing, {} warning(s)", r.failed, r.warned);
    Ok(r.failed == 0)
}
