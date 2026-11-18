//! A harness run stopped with SIGTERM (a cancelled CI job) leaves no container, network or
//! adapter process behind.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sh(cmd: &str) -> String {
    let out = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(root())
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

// answers describe, starts a container on the chain's network the way the adapters do, and
// then never answers start, so the run is still waiting when the signal comes
const ADAPTER: &str = r#"
read -r line
echo '{"id":"0","status":"ok","result":{"protocol":2,"adapter":"a","adapter_version":"1","target":"t","target_version":"v","delivery_modes":["rpc"],"default_delivery":"rpc","bootstrap":"ledger","vocab_version":2,"protocols":{"min":1,"max":30},"claims":{"changes":[]}}}'
read -r line
net=$(printf '%s' "$line" | sed 's/.*"docker_network":"\([^"]*\)".*/\1/')
name=shapekit-interrupt-test-$$
docker run -d --name "$name" --network "$net" --entrypoint sleep "$IMAGE" 600 >/dev/null
echo "$name $$ $net" > "$MARKER"
sleep 600
"#;

#[test]
#[ignore = "needs docker and the pinned timescaledb image; run with --include-ignored"]
fn sigterm_during_a_run_leaves_nothing_behind() {
    let tmp = tempfile::tempdir().unwrap();
    let script = tmp.path().join("adapter.sh");
    std::fs::write(&script, ADAPTER).unwrap();
    let marker = tmp.path().join("started");
    let image = sh("scripts/pin.sh image timescaledb");
    assert!(!image.is_empty(), "no timescaledb pin");
    let mut run = Command::new(env!("CARGO_BIN_EXE_shapekit"))
        .args([
            "harness",
            "run",
            "--chain",
            "cap01.bump-sequence.p30",
            "--network",
            "testnet",
        ])
        .arg("--adapter")
        .arg(format!("sh {}", script.display()))
        .arg("--out")
        .arg(tmp.path().join("out"))
        .env("IMAGE", &image)
        .env("MARKER", &marker)
        .current_dir(root())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !marker.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(300),
            "the adapter never started its container"
        );
        assert!(
            run.try_wait().unwrap().is_none(),
            "the run ended before the adapter started"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    let text = std::fs::read_to_string(&marker).unwrap();
    let [container, adapter, network] = text.split_whitespace().collect::<Vec<_>>()[..] else {
        panic!("marker {text:?}");
    };
    assert_eq!(
        sh(&format!("docker ps -aq --filter name=^{container}$"))
            .lines()
            .count(),
        1
    );

    sh(&format!("kill -TERM {}", run.id()));
    let started = Instant::now();
    let status = loop {
        if let Some(s) = run.try_wait().unwrap() {
            break s;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "the run did not stop"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    assert_eq!(status.code(), Some(128 + 15), "{status}");
    assert_eq!(
        sh(&format!("docker ps -aq --filter name=^{container}$")),
        "",
        "the container is left"
    );
    assert_eq!(
        sh(&format!("docker network ls -q --filter name=^{network}$")),
        "",
        "the network is left"
    );
    assert_eq!(
        sh(&format!("kill -0 {adapter} 2>/dev/null && echo alive")),
        "",
        "the adapter is left"
    );
}
