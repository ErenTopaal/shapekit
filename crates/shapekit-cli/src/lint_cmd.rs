//! `shapekit lint`: runs indexerlint, the go linter in lint/, over a go module. indexerlint is a
//! separate go binary because it loads packages with the go toolchain; this command starts it.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;

#[derive(Args)]
pub struct LintArgs {
    /// Root of the go module to check.
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// Package patterns, relative to the module root.
    #[arg(long = "packages", default_value = "./...")]
    packages: Vec<String>,
    /// Baseline file of accepted, issue-linked findings (docs/linter.md, "Baseline").
    #[arg(long)]
    baseline: Option<PathBuf>,
    /// Print the findings as JSON.
    #[arg(long)]
    json: bool,
    /// The indexerlint binary: a path, or a name looked up on PATH.
    #[arg(long, default_value = "indexerlint")]
    linter: PathBuf,
}

pub fn run(a: LintArgs) -> Result<bool> {
    // a missing module would otherwise surface as the linter failing to start
    if !a.dir.is_dir() {
        bail!("module directory {} does not exist", a.dir.display());
    }
    // indexerlint runs inside the module, so a path (anything with a /) is made absolute
    // against the directory you ran from, as `shapekit doctor lint` checks it; a bare name
    // is still looked up on PATH
    let linter = if a.linter.components().count() > 1 {
        std::path::absolute(&a.linter)
            .with_context(|| format!("resolving {}", a.linter.display()))?
    } else {
        a.linter.clone()
    };
    let mut cmd = Command::new(&linter);
    if a.json {
        cmd.args(["-format", "json"]);
    }
    if let Some(b) = &a.baseline {
        // indexerlint runs inside the module, so a relative baseline is made absolute here
        let abs = std::path::absolute(b).with_context(|| format!("resolving {}", b.display()))?;
        if !abs.is_file() {
            bail!("baseline {} does not exist", abs.display());
        }
        cmd.arg("-baseline").arg(abs);
    }
    cmd.args(&a.packages).current_dir(&a.dir);
    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
            "{} not found. Download indexerlint-<os>-<arch> from a shapekit release, or build \
             it from a checkout with `(cd lint && go build -o indexerlint ./cmd/indexerlint)`, \
             then put it on PATH or pass --linter <path>",
            linter.display()
        ),
        Err(e) => return Err(e).with_context(|| format!("starting {}", linter.display())),
    };
    // indexerlint exits 0 without findings, 1 with findings, 2 on a usage or load error
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        Some(c) => bail!("{} exited with status {c}", linter.display()),
        None => bail!("{} was ended by a signal", linter.display()),
    }
}
