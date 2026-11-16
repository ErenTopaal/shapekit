//! The audit through an SDK testee: every ledger goes through the SDK's decoder and encoder,
//! and every envelope through its transaction hash, as the conformance runner sends them
//! (docs/testee-protocol.md). The expected answers are the network's: the ledger's own bytes,
//! and the hashes and ids core wrote into it (`shapekit_conformance::anchors`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Map, Value};
use shapekit_conformance::compare::ResultClass;
use shapekit_conformance::plan::{Check, Expect};
use shapekit_conformance::runner::run_check;
use shapekit_conformance::testee::Testee;
use shapekit_conformance::versions::{self, Target, VersionsFile};
use shapekit_contracts::audit::{CoverageDelta, LawResult, NewBranch, SdkRef, Violation};
use stellar_xdr::LedgerCloseMeta;

use crate::check::EXAMPLES_PER_LAW;

/// The law each op checks, by the name `shapekit laws` gives it.
fn law_of(op: &str) -> Result<&'static str> {
    Ok(match op {
        "decode_encode" => "roundtrip",
        "tx_hash" => "tx-hash",
        "balance_id" => "balance-id",
        "contract_id" => "contract-id",
        other => bail!("no law is checked with the {other} op"),
    })
}

#[derive(Default)]
struct Tally {
    checked: u64,
    violations: u64,
    examples: Vec<Violation>,
}

/// One SDK testee and what it answered so far.
pub struct SdkAudit {
    testee: Testee,
    passphrase: String,
    laws: BTreeMap<&'static str, Tally>,
    /// Checks the testee answered `unsupported`, by op.
    pub unsupported: BTreeMap<String, u64>,
    /// Checks that ended in an `error`: the testee failed, or the ledger did not confirm what
    /// the law library derived. Never counted as a pass or as an SDK violation.
    pub errors: Vec<(u32, String)>,
    coverage: Option<Coverage>,
}

/// Consumer code the ledgers reach beyond a baseline, measured in the testee's own process:
/// the baseline is a snapshot taken before the first ledger, then a snapshot follows every
/// ledger, so each range the baseline lacks is credited to the first ledger that reached it.
struct Coverage {
    /// What the baseline is: `corpus` for the committed corpus, `corpus+candidate` with the
    /// candidate lane added.
    name: String,
    baseline: BTreeSet<String>,
    /// Ranges beyond the baseline, with the first ledger that reached each.
    new: BTreeMap<String, u32>,
    /// The process the counters live in; a restart loses them, which ends the run.
    process: u32,
}

/// Loads the target from the versions file (`current` names the sdk's current version), turns
/// it into its coverage variant when `coverage` is set, and builds it unless `build` is false.
/// `root` is the repository the versions file's commands are relative to.
pub fn start(
    root: &Path,
    versions_file: &Path,
    sdk: &str,
    version: &str,
    coverage: bool,
    build: bool,
) -> Result<Target> {
    let file = VersionsFile::load(versions_file)
        .with_context(|| format!("loading {}", versions_file.display()))?;
    let version = match version {
        "current" => file.sdk(sdk)?.current.clone(),
        v => v.to_string(),
    };
    let mut target = file.target(sdk, &version)?;
    if coverage {
        target = versions::coverage_target(&target)?;
    }
    if build {
        versions::build(root, &target)?;
    }
    Ok(target)
}

impl SdkAudit {
    pub fn new(root: &Path, target: &Target, passphrase: &str) -> Result<SdkAudit> {
        let testee = Testee::start(root, target)?;
        for op in ["decode_encode", "tx_hash"] {
            if !testee.supports(op) {
                bail!("{} does not implement {op}", target.key());
            }
        }
        Ok(SdkAudit {
            testee,
            passphrase: passphrase.to_string(),
            // every law the sdk is checked against gets a row, also when no ledger held an
            // instance of it
            laws: ["roundtrip", "tx-hash", "balance-id", "contract-id"]
                .into_iter()
                .map(|l| (l, Tally::default()))
                .collect(),
            unsupported: BTreeMap::new(),
            errors: Vec::new(),
            coverage: None,
        })
    }

    pub fn sdk_ref(&self) -> SdkRef {
        let t = self.testee.target();
        SdkRef {
            sdk: t.sdk.clone(),
            version: t.version.clone(),
        }
    }

    /// Process starts of the testee: 1 when it never had to be restarted.
    pub fn starts(&self) -> u32 {
        self.testee.starts
    }

    /// Sends one ledger: `bytes` is its XDR exactly as the lake holds it, `lcm` the same
    /// ledger decoded. `object` names the lake object, for check sources.
    pub fn ledger(
        &mut self,
        lcm: &LedgerCloseMeta,
        bytes: &[u8],
        seq: u32,
        object: &str,
    ) -> Result<()> {
        let b64 = B64.encode(bytes);
        let mut request = Map::new();
        request.insert("op".into(), Value::from("decode_encode"));
        request.insert("type".into(), Value::from("LedgerCloseMeta"));
        request.insert("xdr".into(), Value::from(b64.clone()));
        request.insert(
            "depth_limit".into(),
            Value::from(shapekit_manifest::decode_depth("LedgerCloseMeta")),
        );
        request.insert("len_limit".into(), Value::from(bytes.len()));
        let mut checks = vec![Check {
            name: format!("ledger {seq}/decode_encode"),
            op: "decode_encode".into(),
            request,
            expect: Expect::Bytes(b64),
            source: format!("{object}: the ledger's bytes"),
        }];
        checks.extend(shapekit_conformance::anchors::ledger_checks(
            &format!("ledger {seq}"),
            &self.passphrase,
            lcm,
            object,
        ));
        for check in &checks {
            let law = law_of(&check.op)?;
            let verdict = run_check(check, std::slice::from_mut(&mut self.testee))?
                .pop()
                .context("run_check returned no verdict for the one testee")?;
            match verdict.result {
                ResultClass::Pass | ResultClass::Reference => {
                    self.laws.entry(law).or_default().checked += 1;
                }
                ResultClass::DivergesFromNetwork | ResultClass::DivergesFromPeers => {
                    let t = self.laws.entry(law).or_default();
                    t.checked += 1;
                    t.violations += 1;
                    if t.examples.len() < EXAMPLES_PER_LAW {
                        t.examples.push(Violation {
                            ledger: seq,
                            detail: format!(
                                "{}: {} (answered {})",
                                check.name,
                                verdict.detail.unwrap_or_default(),
                                verdict.actual
                            ),
                        });
                    }
                }
                ResultClass::Unsupported => {
                    *self.unsupported.entry(check.op.clone()).or_default() += 1;
                }
                ResultClass::Error => self.errors.push((
                    seq,
                    format!("{}: {}", check.name, verdict.detail.unwrap_or_default()),
                )),
            }
        }
        if let Some(process) = self.coverage.as_ref().map(|c| c.process) {
            if self.testee.starts != process {
                bail!(
                    "{} was restarted at ledger {seq}, so its coverage counters were lost",
                    self.testee.target().key()
                );
            }
            let now = self.snapshot()?;
            let c = self.coverage.as_mut().expect("checked above");
            for r in now {
                if !c.baseline.contains(&r) {
                    c.new.entry(r).or_insert(seq);
                }
            }
        }
        Ok(())
    }

    /// The ranges the testee's coverage counters show as run. Only tools whose set of ranges
    /// is fixed by the code (llvm regions, go statement blocks) are accepted: there a range
    /// either ran or did not, so two snapshots compare as sets. V8's precise coverage leaves
    /// out a nested range whose count equals the range around it and lists it once the counts
    /// differ, so a block the baseline ran can first appear in a later snapshot; comparing its
    /// ranges as strings would report branches that are not new.
    fn snapshot(&mut self) -> Result<BTreeSet<String>> {
        let key = self.testee.target().key();
        let mut request = Map::new();
        request.insert("op".into(), Value::from("coverage_snapshot"));
        let reply = self
            .testee
            .request(request)?
            .map_err(|f| anyhow::anyhow!("{key}: coverage_snapshot failed: {f}"))?;
        let shapekit_conformance::testee::Reply::Ok(result) = reply else {
            bail!("{key}: coverage_snapshot was answered with {reply:?}");
        };
        let tool = result["tool"].as_str().unwrap_or_default();
        if tool.starts_with("v8") {
            bail!(
                "{key} measures with {tool:?}, whose ranges are not a fixed set: V8 omits a nested range while its count equals the enclosing one, so a set difference of snapshots reports blocks the baseline already ran. New branches need the ranges with count 0 as well, which this testee does not report"
            );
        }
        let ranges: Vec<String> = serde_json::from_value(result["ranges"].clone())
            .with_context(|| format!("{key}: the ranges of coverage_snapshot"))?;
        Ok(ranges.into_iter().collect())
    }

    /// Sends every check of every entry in `manifests` (the corpus, and optionally the
    /// candidate lane) through the testee, so its coverage counters hold what those entries
    /// reach, and takes that as the baseline. The verdicts are not kept: judging them is
    /// `shapekit conformance run`'s job. Returns how many checks were sent.
    pub fn corpus_baseline(&mut self, manifests: &[std::path::PathBuf]) -> Result<usize> {
        let key = self.testee.target().key();
        if !self.testee.supports("coverage_snapshot") {
            bail!("{key} does not collect coverage; start its coverage variant");
        }
        // one snapshot first, so a tool whose ranges cannot be compared is refused before the
        // corpus runs
        self.snapshot()?;
        let process = self.testee.starts;
        let mut sent = 0;
        let mut names = Vec::new();
        for path in manifests {
            let (manifest, corpus) = shapekit_conformance::runner::load_manifest(path)?;
            names.push(
                corpus
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| corpus.display().to_string()),
            );
            for entry in &manifest.entries {
                let checks = shapekit_conformance::plan::checks(entry, &corpus)
                    .with_context(|| format!("the checks of {}", entry.id))?;
                for check in &checks {
                    run_check(check, std::slice::from_mut(&mut self.testee))?;
                    sent += 1;
                }
            }
        }
        if self.testee.starts != process {
            bail!(
                "{key} was restarted while the corpus ran, so its counters lost part of the baseline"
            );
        }
        let baseline = self.snapshot()?;
        self.coverage = Some(Coverage {
            name: names.join("+"),
            baseline,
            new: BTreeMap::new(),
            process,
        });
        Ok(sent)
    }

    /// The coverage part of the report, when a baseline was taken.
    pub fn coverage_delta(&self) -> Option<CoverageDelta> {
        let c = self.coverage.as_ref()?;
        Some(CoverageDelta {
            baseline: c.name.clone(),
            baseline_ranges: c.baseline.len() as u64,
            ranges: (c.baseline.len() + c.new.len()) as u64,
            new: c
                .new
                .iter()
                .map(|(location, ledger)| NewBranch {
                    location: location.clone(),
                    ledger: *ledger,
                })
                .collect(),
        })
    }

    /// One row per law the SDK was checked against, sorted by name.
    pub fn law_results(&self) -> Vec<LawResult> {
        self.laws
            .iter()
            .map(|(law, t)| LawResult {
                law: law.to_string(),
                checked: t.checked,
                violations: t.violations,
                examples: t.examples.clone(),
            })
            .collect()
    }
}
