//! `sdk-version` obligations: one per version of each sdk in `conformance/versions.toml`, met
//! when `conformance run --all-versions` ran the whole corpus against that version and the
//! known-failures list holds for it. The witnesses are the entries every check of which passed
//! on that version (or agreed with it as the reference rendering). The fragment of an sdk is
//! `conformance/sdk-versions/<sdk>.obligations.json`; `check` keeps it in step with the
//! versions file.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context as _, Result, ensure};
use shapekit_contracts::obligations::{
    Obligation, ObligationKind, ObligationLedger, ObligationStatus,
};

use crate::compare::ResultClass;
use crate::runner::RunOutcome;
use crate::versions::VersionsFile;

/// Where the fragment of `sdk` lives, relative to the repository root.
pub fn fragment_path(sdk: &str) -> String {
    format!("conformance/sdk-versions/{sdk}.obligations.json")
}

fn id(key: &str) -> String {
    Obligation::id_for(ObligationKind::SdkVersion, key)
}

/// The ledger of one sdk from a run of all its versions over the whole corpus.
pub fn obligations(
    outcome: &RunOutcome,
    file: &VersionsFile,
    sdk: &str,
) -> Result<ObligationLedger> {
    let versions = &file.sdk(sdk)?.versions;
    let mut out = Vec::new();
    for v in versions {
        let key = format!("{sdk}@{}", v.version);
        let ran = outcome.matrix.testees.iter().any(|t| t.key == key);
        ensure!(
            ran,
            "the run did not cover {key}; run --all-versions for {sdk}"
        );
        let mut witnesses = Vec::new();
        let mut counts = std::collections::BTreeMap::<ResultClass, usize>::new();
        for row in &outcome.matrix.entries {
            let Some(cell) = row.cells.get(&key) else {
                continue;
            };
            let class = cell.summary_class();
            *counts.entry(class).or_default() += 1;
            if matches!(class, ResultClass::Pass | ResultClass::Reference) {
                witnesses.push(format!("entry:{}", row.id));
            }
        }
        let tally = counts
            .iter()
            .map(|(c, n)| format!("{n} {}", c.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let failing = outcome.failing.contains(&key);
        let (status, witnesses, evidence) = if failing || witnesses.is_empty() {
            (
                ObligationStatus::Open,
                Vec::new(),
                format!(
                    "entries: {tally}; {}",
                    if failing {
                        "the known-failures list does not hold for this version"
                    } else {
                        "no entry passes on this version"
                    }
                ),
            )
        } else {
            (
                ObligationStatus::Met,
                witnesses,
                format!("entries: {tally}; the known-failures list holds"),
            )
        };
        out.push(Obligation {
            id: id(&key),
            kind: ObligationKind::SdkVersion,
            status,
            witnesses,
            evidence: Some(evidence),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(ObligationLedger {
        schema_version: 1,
        obligations: out,
    })
}

/// The committed fragment of `sdk` names exactly the versions the versions file lists.
pub fn check(root: &Path, file: &VersionsFile, sdk: &str) -> Result<()> {
    let path = root.join(fragment_path(sdk));
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let ledger: ObligationLedger =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let have: BTreeSet<String> = ledger.obligations.iter().map(|o| o.id.clone()).collect();
    let want: BTreeSet<String> = file
        .sdk(sdk)?
        .versions
        .iter()
        .map(|v| id(&format!("{sdk}@{}", v.version)))
        .collect();
    ensure!(
        have == want,
        "{} is stale: it lists {:?} that conformance/versions.toml does not and lacks {:?}; rerun `shapekit conformance run --sdk {sdk} --all-versions --obligations`",
        path.display(),
        have.difference(&want).collect::<Vec<_>>(),
        want.difference(&have).collect::<Vec<_>>()
    );
    Ok(())
}
