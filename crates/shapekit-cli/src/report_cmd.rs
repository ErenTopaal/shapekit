//! `shapekit report`: one harness or conformance run as an HTML page and a pull request
//! summary. What the report shows is documented on the shapekit-report crate.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use shapekit_report::{DEFAULT_SITE, Options};

#[derive(Args)]
pub struct ReportArgs {
    /// The run: a `harness run --out` or `conformance run --out` directory, or the
    /// harness-report.json or divergence-matrix.json in one.
    pub input: PathBuf,
    /// Where the HTML page goes. Default: report.html next to the run's json.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Also write a short markdown summary for a pull request comment here.
    #[arg(long)]
    pub summary: Option<PathBuf>,
    /// Also write the report's data as json here.
    #[arg(long)]
    pub json: Option<PathBuf>,
    /// An earlier run of the same kind (directory or json), or for a harness run an
    /// expectations file (`harness expect --write`): failures are then classed as new,
    /// changed, fixed or unchanged.
    #[arg(long, conflicts_with = "known_failures")]
    pub baseline: Option<PathBuf>,
    /// For a conformance run: the known-failures directory it ran with, as its baseline.
    #[arg(long)]
    pub known_failures: Option<PathBuf>,
    /// The corpus manifest the run used. Default: the triage's manifest.
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    /// The shapekit checkout the triage, the candidate triage and the registry are read from.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// The findings triage, relative to --root.
    #[arg(long, default_value = "findings/triage.toml")]
    pub triage: PathBuf,
    /// The site findings and guide sections are linked on.
    #[arg(long, default_value = DEFAULT_SITE)]
    pub site: String,
}

pub fn run(a: ReportArgs) -> Result<bool> {
    let (_, json) = shapekit_report::resolve_input(&a.input)?;
    let report = shapekit_report::build(&Options {
        root: a.root,
        input: a.input,
        triage: a.triage,
        manifest: a.manifest,
        baseline: a.baseline,
        known_failures: a.known_failures,
        site: a.site,
    })?;
    let out = a.out.unwrap_or_else(|| {
        json.parent()
            .map(|p| p.join("report.html"))
            .unwrap_or_else(|| PathBuf::from("report.html"))
    });
    shapekit_report::write_html(&report, &out)?;
    println!("wrote {}", out.display());
    if let Some(p) = &a.summary {
        let name = out.file_name().map(|n| n.to_string_lossy().into_owned());
        shapekit_report::write_summary(&report, p, name.as_deref())?;
        println!("wrote {}", p.display());
    }
    if let Some(p) = &a.json {
        let mut text = serde_json::to_string_pretty(&report)?;
        text.push('\n');
        std::fs::write(p, text).map_err(|e| anyhow::anyhow!("writing {}: {e}", p.display()))?;
        println!("wrote {}", p.display());
    }
    println!("{}", shapekit_report::grouping_line(&report));
    if let Some(c) = &report.comparison {
        println!(
            "against the {}: {} new, {} changed, {} fixed, {} unchanged, {} not rerun",
            c.kind,
            c.new.len(),
            c.changed.len(),
            c.fixed.len(),
            c.unchanged,
            c.not_rerun
        );
    }
    // the report is written whatever the run found; a failing run fails in its own command
    Ok(true)
}
