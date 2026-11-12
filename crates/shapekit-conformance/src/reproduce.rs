//! `shapekit conformance reproduce`: each upstream sighting in `conformance/reproductions.toml`
//! must fail on the versions before its fix and pass on the versions with it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Map, Value};
use shapekit_manifest::EntryKind;

use crate::compare::ResultClass;
use crate::plan::{self, Check, Expect};
use crate::runner::{self, Selection};
use crate::versions::VersionsFile;

pub const REPRODUCTIONS_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReproductionsFile {
    pub schema: u32,
    #[serde(rename = "reproduction")]
    pub reproductions: Vec<Reproduction>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reproduction {
    pub id: String,
    pub sighting: String,
    pub link: String,
    pub sdk: String,
    /// Versions without the fix: the row must fail on each.
    pub fail: Vec<String>,
    /// Versions with the fix: the row must pass on each.
    pub pass: Vec<String>,
    /// Entries named by id.
    #[serde(default)]
    pub entries: Vec<String>,
    /// Entries named by what they are; combined with `entries`.
    #[serde(default)]
    pub select: Option<Select>,
    /// Only checks of these ops count; all when empty.
    #[serde(default)]
    pub ops: Vec<String>,
    /// Literal requests for flaws no corpus entry kind can carry (see the protocol doc on
    /// encode_json).
    #[serde(default, rename = "probe")]
    pub probes: Vec<Probe>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Select {
    #[serde(default)]
    pub kinds: Vec<EntryKind>,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub linked_issues: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    pub name: String,
    pub op: String,
    /// The request members other than `op` and `id`.
    pub request: toml::Table,
    /// `"rejected"`, or a table with the expected `xdr`.
    pub expect: toml::Value,
    /// Where the expectation comes from.
    pub source: String,
}

impl ReproductionsFile {
    pub fn load(path: &Path) -> Result<ReproductionsFile> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let file: ReproductionsFile =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        ensure!(
            file.schema == REPRODUCTIONS_SCHEMA,
            "{}: schema {} is not {REPRODUCTIONS_SCHEMA}",
            path.display(),
            file.schema
        );
        let mut ids = BTreeSet::new();
        for r in &file.reproductions {
            ensure!(ids.insert(&r.id), "reproduction {} is listed twice", r.id);
            ensure!(
                !r.fail.is_empty() && !r.pass.is_empty(),
                "reproduction {} needs fail and pass versions",
                r.id
            );
            ensure!(
                !r.entries.is_empty() || r.select.is_some() || !r.probes.is_empty(),
                "reproduction {} names no entries and no probes",
                r.id
            );
        }
        Ok(file)
    }
}

/// What a row did on one version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    Fail,
    Pass,
    /// Some counted check errored or was unsupported, so neither verdict is established.
    Inconclusive,
}

impl Observed {
    fn as_str(self) -> &'static str {
        match self {
            Observed::Fail => "fail",
            Observed::Pass => "pass",
            Observed::Inconclusive => "inconclusive",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub reproduction: String,
    pub version: String,
    pub expected: Observed,
    pub observed: Observed,
    pub entries: Vec<String>,
    /// Non-pass checks, as `entry check: result, detail`.
    pub notes: Vec<String>,
}

impl Cell {
    pub fn holds(&self) -> bool {
        self.expected == self.observed
    }
}

pub struct ReproduceOptions {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub versions: PathBuf,
    pub reproductions: PathBuf,
    pub sdks: Vec<String>,
    pub build: bool,
}

pub fn run(opts: &ReproduceOptions) -> Result<Vec<Cell>> {
    let file = ReproductionsFile::load(&opts.reproductions)?;
    let versions = VersionsFile::load(&opts.versions)?;
    let (manifest, corpus) = runner::load_manifest(&opts.manifest)?;
    let rows: Vec<&Reproduction> = file
        .reproductions
        .iter()
        .filter(|r| opts.sdks.is_empty() || opts.sdks.contains(&r.sdk))
        .collect();
    ensure!(
        !rows.is_empty(),
        "no reproduction matches the selected sdks"
    );

    // the checks of every row, resolved once
    let mut row_checks: BTreeMap<&str, RowChecks> = BTreeMap::new();
    for r in &rows {
        let mut ids: BTreeSet<String> = BTreeSet::new();
        for id in &r.entries {
            ensure!(
                manifest.entries.iter().any(|e| &e.id == id),
                "reproduction {}: entry {id} is not in the manifest",
                r.id
            );
            ids.insert(id.clone());
        }
        if let Some(s) = &r.select {
            let sel = Selection {
                ids: Vec::new(),
                kinds: s.kinds.clone(),
                flags: s.flags.clone(),
                linked_issues: s.linked_issues.clone(),
            };
            let found = sel.select(&manifest)?;
            ensure!(
                !found.is_empty(),
                "reproduction {}: the selector matches no entry",
                r.id
            );
            ids.extend(found.into_iter().map(|e| e.id.clone()));
        }
        let mut checks = Vec::new();
        for id in &ids {
            let entry = manifest
                .entries
                .iter()
                .find(|e| &e.id == id)
                .expect("ids come from the manifest");
            for c in plan::checks(entry, &corpus)? {
                if r.ops.is_empty() || r.ops.contains(&c.op) {
                    checks.push((id.clone(), c));
                }
            }
        }
        for p in &r.probes {
            checks.push((format!("probe {}", p.name), probe_check(p)?));
        }
        ensure!(
            !checks.is_empty(),
            "reproduction {}: no check survives the ops filter",
            r.id
        );
        row_checks.insert(r.id.as_str(), (ids.into_iter().collect(), checks));
    }

    // every (sdk, version) the rows need, each started once
    let mut needed: BTreeSet<(String, String)> = BTreeSet::new();
    for r in &rows {
        for v in r.fail.iter().chain(&r.pass) {
            needed.insert((r.sdk.clone(), v.clone()));
        }
    }
    let mut cells = Vec::new();
    for (sdk, version) in needed {
        let target = versions.target(&sdk, &version)?;
        let mut testees = runner::start_all(&opts.root, std::slice::from_ref(&target), opts.build)?;
        for r in rows.iter().filter(|r| r.sdk == sdk) {
            let expected = if r.fail.contains(&version) {
                Observed::Fail
            } else if r.pass.contains(&version) {
                Observed::Pass
            } else {
                continue;
            };
            let (entries, checks) = &row_checks[r.id.as_str()];
            let mut notes = Vec::new();
            let mut worst = ResultClass::Pass;
            for (owner, check) in checks {
                let v = runner::run_check(check, &mut testees)?
                    .into_iter()
                    .next()
                    .expect("one testee gives one verdict");
                if v.result != ResultClass::Pass {
                    notes.push(format!(
                        "{owner} {}: {}{}",
                        check.name,
                        v.result,
                        v.detail.map(|d| format!(", {d}")).unwrap_or_default()
                    ));
                }
                worst = worst.max(v.result);
            }
            let observed = match worst {
                ResultClass::Pass => Observed::Pass,
                ResultClass::DivergesFromNetwork | ResultClass::DivergesFromPeers => Observed::Fail,
                ResultClass::Unsupported | ResultClass::Error => Observed::Inconclusive,
            };
            cells.push(Cell {
                reproduction: r.id.clone(),
                version: version.clone(),
                expected,
                observed,
                entries: entries.clone(),
                notes,
            });
        }
    }
    cells.sort_by(|a, b| {
        (a.reproduction.as_str(), a.version.as_str())
            .cmp(&(b.reproduction.as_str(), b.version.as_str()))
    });
    Ok(cells)
}

/// A row's resolved entry ids and its checks, each with the entry or probe it came from.
type RowChecks = (Vec<String>, Vec<(String, Check)>);

fn probe_check(p: &Probe) -> Result<Check> {
    let mut request: Map<String, Value> = serde_json::to_value(&p.request)
        .context("probe request as json")?
        .as_object()
        .cloned()
        .context("a probe request is a table")?;
    ensure!(
        !request.contains_key("op") && !request.contains_key("id"),
        "probe {}: op and id are set by the runner",
        p.name
    );
    request.insert("op".into(), Value::String(p.op.clone()));
    let expect = match &p.expect {
        toml::Value::String(s) if s == "rejected" => Expect::Rejected,
        toml::Value::Table(t) => match t.get("xdr") {
            Some(toml::Value::String(x)) if t.len() == 1 => Expect::Bytes(x.clone()),
            _ => bail!("probe {}: expect table must hold exactly xdr", p.name),
        },
        other => bail!("probe {}: unknown expect {other}", p.name),
    };
    Ok(Check {
        name: format!("probe/{}", p.op),
        op: p.op.clone(),
        request,
        expect,
        source: p.source.clone(),
    })
}

/// The reproduction table printed by the command.
pub fn render(cells: &[Cell]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "| reproduction | version | expected | observed | holds |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|");
    for c in cells {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            c.reproduction,
            c.version,
            c.expected.as_str(),
            c.observed.as_str(),
            if c.holds() { "yes" } else { "NO" }
        );
    }
    for c in cells {
        let _ = writeln!(
            out,
            "\n{} on {}: entries {}",
            c.reproduction,
            c.version,
            if c.entries.is_empty() {
                "(probes only)".to_string()
            } else {
                c.entries.join(", ")
            }
        );
        for n in &c.notes {
            let _ = writeln!(out, "  - {n}");
        }
    }
    out
}
