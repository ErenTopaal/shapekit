//! docs/sdk-ci.md against the code: every SDK of conformance/versions.toml has a section with a
//! versions file that loads, describes the same package as shapekit's own file and builds one
//! version, a workflow file for that SDK, and every file its build steps copy from the SDK
//! checkout; the reusable workflow accepts exactly those SDKs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use shapekit_conformance::versions::VersionsFile;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One fenced block with a `title="..."` attribute.
struct Block {
    title: String,
    body: String,
}

/// The `### ` sections of the page's "Versions files per SDK" part, each with its titled
/// blocks.
fn sections(text: &str) -> Vec<(String, Vec<Block>)> {
    let mut out: Vec<(String, Vec<Block>)> = Vec::new();
    let mut lines = text.lines();
    let mut inside = false;
    while let Some(line) = lines.next() {
        if line.starts_with("## ") {
            inside = line == "## Versions files per SDK";
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(h) = line.strip_prefix("### ") {
            out.push((h.to_string(), Vec::new()));
            continue;
        }
        if let Some(info) = line.strip_prefix("```")
            && let Some((_, rest)) = info.split_once("title=\"")
        {
            let title = rest.trim_end_matches('"').to_string();
            let mut body = String::new();
            for l in lines.by_ref() {
                if l == "```" {
                    break;
                }
                body.push_str(l);
                body.push('\n');
            }
            let (_, blocks) = out
                .last_mut()
                .expect("a titled block before the first section");
            blocks.push(Block { title, body });
        }
    }
    out
}

/// The sdk keys the workflow's first step accepts.
fn workflow_sdks() -> BTreeSet<String> {
    let text =
        std::fs::read_to_string(root().join(".github/workflows/sdk-conformance.yml")).unwrap();
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| l.ends_with(") ;;") && l.contains('|'))
        .expect("the case line of the sdk check");
    line.trim_end_matches(") ;;")
        .split('|')
        .map(str::to_string)
        .collect()
}

#[test]
fn every_sdk_has_a_tested_versions_file_and_workflow_in_sdk_ci_docs() {
    let shapekit = VersionsFile::load(&root().join("conformance/versions.toml")).unwrap();
    let text = std::fs::read_to_string(root().join("docs/sdk-ci.md")).unwrap();
    let tmp = std::env::temp_dir().join(format!("sdk-ci-docs-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let mut seen = BTreeMap::new();
    for (i, (heading, blocks)) in sections(&text).into_iter().enumerate() {
        let versions: Vec<&Block> = blocks
            .iter()
            .filter(|b| b.title == "ci/shapekit/versions.toml")
            .collect();
        assert_eq!(versions.len(), 1, "{heading}: one versions file");
        let path = tmp.join(format!("{i}.toml"));
        std::fs::write(&path, &versions[0].body).unwrap();
        let file = VersionsFile::load(&path).unwrap_or_else(|e| panic!("{heading}: {e:#}"));
        assert_eq!(file.sdk.len(), 1, "{heading}: one sdk");
        let (name, sdk) = file.sdk.iter().next().unwrap();
        let ours = shapekit
            .sdk
            .get(name)
            .unwrap_or_else(|| panic!("{heading}: {name} is no sdk of conformance/versions.toml"));
        assert_eq!(sdk.package, ours.package, "{heading}: package");
        assert_eq!(sdk.issues_repo, ours.issues_repo, "{heading}: issues_repo");
        assert_eq!(sdk.timeout_ms, ours.timeout_ms, "{heading}: timeout_ms");
        assert_eq!(sdk.memory, ours.memory, "{heading}: memory");
        assert_eq!(sdk.kind, ours.kind, "{heading}: kind");
        assert_eq!(sdk.versions.len(), 1, "{heading}: one version");
        let v = &sdk.versions[0];
        assert_eq!(v.version, sdk.current, "{heading}: the version is current");
        assert!(v.max_protocol.is_some(), "{heading}: max_protocol");

        // a type table written for the build names the build's version
        for step in &v.build {
            if let Some(at) = step.argv.iter().position(|a| a == "--version") {
                assert_eq!(step.argv[at + 1], v.version, "{heading}: {:?}", step.argv);
            }
        }
        // every file a build step takes from the checkout is shown in the section
        for arg in v.build.iter().flat_map(|s| &s.argv) {
            if let Some(file) = arg.strip_prefix("../sdk/ci/shapekit/") {
                let title = format!("ci/shapekit/{file}");
                assert!(
                    blocks.iter().any(|b| b.title == title),
                    "{heading}: the build copies {arg}, which the section does not show"
                );
            }
        }
        let workflows: Vec<&Block> = blocks
            .iter()
            .filter(|b| b.title == ".github/workflows/shapekit.yml")
            .collect();
        assert_eq!(workflows.len(), 1, "{heading}: one workflow file");
        let wf = &workflows[0].body;
        assert!(
            wf.lines().any(|l| l.trim() == format!("sdk: {name}")),
            "{heading}: the workflow runs sdk {name}"
        );
        assert!(wf.contains("versions-file: ci/shapekit/versions.toml"));
        assert!(wf.contains("known-failures: ci/shapekit/known-failures"));
        assert!(
            seen.insert(name.clone(), heading.clone()).is_none(),
            "{name} twice"
        );
    }
    std::fs::remove_dir_all(&tmp).unwrap();

    let documented: BTreeSet<String> = seen.into_keys().collect();
    let all: BTreeSet<String> = shapekit.sdk.keys().cloned().collect();
    assert_eq!(documented, all, "docs/sdk-ci.md has a section per sdk");
    assert_eq!(
        workflow_sdks(),
        all,
        "sdk-conformance.yml accepts every sdk"
    );
}
