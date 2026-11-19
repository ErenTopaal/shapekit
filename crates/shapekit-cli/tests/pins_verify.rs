use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn shapekit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args(args)
        .output()
        .expect("running shapekit")
}

fn repo_pins() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml")
}

#[test]
fn missing_file_is_an_error() {
    let out = shapekit(&["pins", "verify", "--file", "/nonexistent/pins.toml"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("reading /nonexistent/pins.toml"));
}

#[test]
fn invalid_file_fails_before_any_network_call() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pins.toml");
    std::fs::write(&path, "schema = 2\n").unwrap();
    let out = shapekit(&["pins", "verify", "--file", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unsupported schema 2"));
}

#[test]
fn a_file_without_pins_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pins.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let out = shapekit(&["pins", "verify", "--file", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("lists no pins"));
}

#[test]
fn registered_commands() {
    let out = shapekit(&["--help"]);
    let help = String::from_utf8_lossy(&out.stdout);
    let commands: Vec<&str> = help
        .split("Commands:")
        .nth(1)
        .unwrap()
        .split("Options:")
        .next()
        .unwrap()
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(
        commands,
        [
            "check", "lint", "report", "init", "doctor", "dev", "serve", "help"
        ]
    );
    // the internal commands are left out of --help but keep their paths; `shapekit dev`
    // lists them, sorted
    let out = shapekit(&["dev"]);
    assert!(out.status.success());
    let listing = String::from_utf8_lossy(&out.stdout);
    let internal: Vec<&str> = listing
        .lines()
        .filter(|l| l.starts_with("  "))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(
        internal,
        [
            "audit",
            "capture",
            "conformance",
            "contracts",
            "coretest",
            "corpus",
            "coverage",
            "explore",
            "findings",
            "fuzz",
            "gen",
            "goldens",
            "guide",
            "guided-fuzz",
            "harness",
            "import",
            "laws",
            "manifest",
            "native-fuzz",
            "oracle",
            "pack",
            "pins",
            "probe",
            "registry",
            "schema",
            "site",
            "synth",
            "twin",
            "vectors",
        ]
    );
    for name in internal {
        let out = shapekit(&[name, "--help"]);
        assert!(out.status.success(), "{name} --help failed");
    }
}

#[test]
#[ignore = "needs network access to github, docker hub and the package registries; run with --include-ignored"]
fn online_repo_pins_pass() {
    let out = shapekit(&["pins", "verify", "--file", repo_pins().to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "pins verify failed:\n{stdout}");
    assert!(stdout.contains(" 0 failed"));
}

#[test]
#[ignore = "needs network access to github, docker hub and the package registries; run with --include-ignored"]
fn online_altered_sha_fails() {
    let text = std::fs::read_to_string(repo_pins()).unwrap();
    let real = "0e49751aa9ddf892302cca02cea7f3f5d97df9f1";
    assert!(text.contains(real));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pins.toml");
    std::fs::write(
        &path,
        text.replacen(real, "0e49751aa9ddf892302cca02cea7f3f5d97df9f0", 1),
    )
    .unwrap();
    let out = shapekit(&["pins", "verify", "--file", path.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected a pin failure:\n{stdout}"
    );
    assert!(stdout.contains("FAIL  git       stellar-core  refs/tags/v30.0.0 resolves to 0e49751aa9ddf892302cca02cea7f3f5d97df9f1"));
    assert!(stdout.contains(" 1 failed"));
}

/// Every action a workflow uses is named by a commit sha that pins.toml pins, with the release
/// tag that names that commit as the comment after it, so a moved tag cannot change what runs
/// and pins verify checks the tag still names the commit. No git pin of the repository floats.
#[test]
fn workflow_actions_are_the_pinned_commits() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pins: toml::Table = std::fs::read_to_string(repo_pins())
        .unwrap()
        .parse()
        .unwrap();
    let git = pins["git"].as_array().unwrap();
    for g in git {
        assert!(
            g.get("sha").is_some(),
            "pins.toml: git pin {} has no sha",
            g["name"]
        );
    }
    let mut seen = 0;
    let mut dir: Vec<_> = std::fs::read_dir(root.join(".github/workflows"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    dir.sort();
    for wf in dir {
        let text = std::fs::read_to_string(&wf).unwrap();
        for (n, line) in text.lines().enumerate() {
            let rest = line.trim_start().trim_start_matches("- ");
            let Some(rest) = rest.strip_prefix("uses:") else {
                continue;
            };
            let at = format!("{}:{}", wf.display(), n + 1);
            let (action, comment) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
            // a reusable workflow of this repository runs at the caller's own commit
            if action.starts_with("./") {
                continue;
            }
            let (path, sha) = action
                .split_once('@')
                .unwrap_or_else(|| panic!("{at}: {action} names no version"));
            assert!(
                sha.len() == 40 && sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
                "{at}: {action} is not named by a commit sha"
            );
            // owner/repo, then an optional path inside it (actions/cache/restore)
            let repo: Vec<&str> = path.splitn(3, '/').take(2).collect();
            let url = format!("https://github.com/{}", repo.join("/"));
            let pin = git
                .iter()
                .find(|g| g["url"].as_str() == Some(&url) && g["sha"].as_str() == Some(sha))
                .unwrap_or_else(|| panic!("{at}: no git pin in pins.toml for {url} at {sha}"));
            let comment = comment.trim();
            let version = comment
                .strip_prefix("# ")
                .unwrap_or_else(|| panic!("{at}: {action} has no `# <version>` comment"));
            if let Some(r) = pin.get("ref") {
                assert_eq!(
                    r.as_str().unwrap(),
                    format!("refs/tags/{version}"),
                    "{at}: the comment names another release than pins.toml"
                );
            }
            seen += 1;
        }
    }
    assert!(seen > 100, "read only {seen} action uses");
}
