//! The audit through an SDK testee: every ledger goes through the SDK's decoder and encoder,
//! and every envelope through its transaction hash, as the conformance runner sends them
//! (docs/testee-protocol.md). The expected answers are the network's: the ledger's own bytes,
//! and the hashes and ids core wrote into it (`shapekit_conformance::anchors`).

use std::collections::BTreeMap;
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
use shapekit_contracts::audit::{LawResult, SdkRef, Violation};
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
}

/// Loads the target from the versions file (`current` names the sdk's current version), builds it unless `build` is false, and starts
/// its testee from `root`, the repository the versions file's commands are relative to.
pub fn start(
    root: &Path,
    versions_file: &Path,
    sdk: &str,
    version: &str,
    build: bool,
) -> Result<Target> {
    let file = VersionsFile::load(versions_file)
        .with_context(|| format!("loading {}", versions_file.display()))?;
    let version = match version {
        "current" => file.sdk(sdk)?.current.clone(),
        v => v.to_string(),
    };
    let target = file.target(sdk, &version)?;
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
        Ok(())
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
