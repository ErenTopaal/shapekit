//! A run stopped with SIGTERM removes its scratch directories, as one stopped with Ctrl-C
//! does: `serve` stops its server, every other command removes them before it exits.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

/// Names in `dir` that start with `prefix`.
fn named(dir: &Path, prefix: &str) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix))
        .collect()
}

fn terminate(child: &Child) {
    let ok = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap()
        .success();
    assert!(ok, "kill -TERM failed");
}

fn wait(child: &mut Child) -> Option<i32> {
    let start = Instant::now();
    loop {
        if let Some(s) = child.try_wait().unwrap() {
            return s.code();
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            panic!("the process did not end within a minute of SIGTERM");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn serve_removes_its_archive_copy_on_sigterm() {
    let tmp = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args([
            "serve",
            "--chain",
            "cap01.bump-sequence.p30",
            "--network",
            "testnet",
        ])
        .args(["--listen", "127.0.0.1:0", "--corpus"])
        .arg(repo("corpus"))
        .env("TMPDIR", tmp.path())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(line.starts_with("serving "), "{line}");
    assert_eq!(named(tmp.path(), "shapekit-serve-archive-").len(), 1);
    terminate(&child);
    assert_eq!(wait(&mut child), Some(0));
    assert!(named(tmp.path(), "shapekit-serve-archive-").is_empty());
}

#[test]
fn a_harness_run_removes_its_scratch_directories_on_sigterm() {
    let tmp = tempfile::tempdir().unwrap();
    let scratch = tmp.path().join("tmp");
    std::fs::create_dir(&scratch).unwrap();
    // an adapter that answers describe, then never answers start, so the run sits with its
    // replay server and adapter work directory alive
    let describe = serde_json::json!({
        "protocol": shapekit_harness::protocol::ADAPTER_PROTOCOL_VERSION,
        "adapter": "hang",
        "adapter_version": "0",
        "target": "hang",
        "target_version": "0",
        "delivery_modes": ["rpc"],
        "default_delivery": "rpc",
        "bootstrap": "ledger",
        "vocab_version": shapekit_oracle::ORACLE_VOCAB_VERSION,
        "protocols": {"min": 1, "max": 99},
        "claims": {"participants": {"address_kinds": [], "mux_ids": false}, "changes": []},
        "capabilities": [],
    });
    let script = tmp.path().join("adapter.sh");
    std::fs::write(
        &script,
        format!(
            "read l\nid=$(printf %s \"$l\" | sed 's/.*\"id\":\"\\([^\"]*\\)\".*/\\1/')\n\
             printf '%s\\n' \"{{\\\"id\\\":\\\"$id\\\",\\\"status\\\":\\\"ok\\\",\\\"result\\\":{}}}\"\n\
             read l\nsleep 20\n",
            describe.to_string().replace('"', "\\\"")
        ),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args([
            "harness",
            "run",
            "--no-docker",
            "--chain",
            "cap01.bump-sequence.p30",
        ])
        .arg("--adapter")
        .arg(format!("sh {}", script.display()))
        .arg("--out")
        .arg(tmp.path().join("out"))
        .arg("--corpus")
        .arg(repo("corpus"))
        .env("TMPDIR", &scratch)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while named(&scratch, "shapekit-harness-").is_empty() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the run never created its work directory"
        );
        if let Some(s) = child.try_wait().unwrap() {
            panic!("the run ended before the test stopped it: {s}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(named(&scratch, "shapekit-serve-archive-").len(), 1);
    terminate(&child);
    assert_eq!(wait(&mut child), Some(143));
    assert!(
        named(&scratch, "shapekit-").is_empty(),
        "{:?}",
        named(&scratch, "")
    );
}
