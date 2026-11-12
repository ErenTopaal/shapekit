//! `shapekit serve` keeps its resident memory under a fixed bound while serving the largest
//! committed chain end to end, repeatedly: every ledger in both formats, every
//! transaction, the ledger entries at the head, and the head moved back and forward.

#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

/// Peak resident memory allowed for the server process. Chains are read one batch at a
/// time through a cache bounded in bytes, so the peak is the binary, the runtime, the
/// largest single response and the ledger state, none of which grows with repetition.
const RSS_BOUND_KIB: u64 = 128 * 1024;

const ROUNDS: usize = 10;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn dir_bytes(p: &Path) -> u64 {
    walkdir::WalkDir::new(p)
        .into_iter()
        .map(|e| e.unwrap())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().unwrap().len())
        .sum()
}

/// The committed chain whose lake is largest in bytes.
fn largest_chain() -> PathBuf {
    let mut best = (0, PathBuf::new());
    for c in std::fs::read_dir(repo().join("corpus/chains")).unwrap() {
        for n in std::fs::read_dir(c.unwrap().path()).unwrap() {
            let n = n.unwrap().path();
            let size = dir_bytes(&n.join("ledgers"));
            if size > best.0 {
                best = (size, n);
            }
        }
    }
    best.1
}

fn status_kib(pid: u32, field: &str) -> u64 {
    let s = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let line = s
        .lines()
        .find(|l| l.starts_with(field))
        .unwrap_or_else(|| panic!("no {field} in /proc/{pid}/status"));
    line.split_whitespace().nth(1).unwrap().parse().unwrap()
}

fn call(client: &reqwest::blocking::Client, url: &str, method: &str, params: Value) -> Value {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let text = client
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .unwrap()
        .text()
        .unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert!(v.get("error").is_none(), "{method}: {v}");
    v
}

fn sweep(client: &reqwest::blocking::Client, url: &str, first: u64, last: u64) -> usize {
    let mut served = 0;
    for format in ["base64", "json"] {
        let mut start = first;
        while start <= last {
            let r = call(
                client,
                url,
                "getLedgers",
                json!({"startLedger": start, "pagination": {"limit": 200}, "xdrFormat": format}),
            );
            let n = r["result"]["ledgers"].as_array().unwrap().len() as u64;
            served += n as usize;
            start += n;
        }
        call(
            client,
            url,
            "getTransactions",
            json!({"startLedger": first, "pagination": {"limit": 200}, "xdrFormat": format}),
        );
    }
    served
}

#[test]
fn serve_memory_stays_bounded() {
    let mut chains = vec![largest_chain()];
    chains.push(repo().join("crates/shapekit-pack/tests/fixtures/checkpoints.p30/testnet"));
    for dir in chains {
        let mut child = Command::new(env!("CARGO_BIN_EXE_shapekit"))
            .args(["serve", "--listen", "127.0.0.1:0", "--chain"])
            .arg(&dir)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let url = line.trim().rsplit(" at ").next().unwrap().to_string();
        let client = reqwest::blocking::Client::new();
        let health = call(&client, &url, "getHealth", json!(null));
        let first = health["result"]["oldestLedger"].as_u64().unwrap();
        let last = health["result"]["latestLedger"].as_u64().unwrap();

        let mut peaks = Vec::new();
        for _ in 0..ROUNDS {
            let served = sweep(&client, &url, first, last);
            assert_eq!(served as u64, 2 * (last - first + 1));
            // move the head back and forward so the ledger state is rebuilt
            for head in [first + (last - first) / 2, last] {
                client
                    .post(format!("{url}/control/head"))
                    .body(json!({"ledger": head}).to_string())
                    .send()
                    .unwrap();
                call(&client, &url, "getLedgerEntries", json!({"keys": []}));
            }
            peaks.push(status_kib(pid, "VmHWM:"));
        }
        let rss = status_kib(pid, "VmRSS:");
        child.kill().unwrap();
        child.wait().unwrap();
        eprintln!(
            "{}: ledgers {first} to {last}, lake {} bytes, peak rss per round {peaks:?} KiB, rss at end {rss} KiB",
            dir.display(),
            dir_bytes(&dir.join("ledgers"))
        );
        let peak = *peaks.last().unwrap();
        assert!(
            peak < RSS_BOUND_KIB,
            "{}: peak rss {peak} KiB over {RSS_BOUND_KIB} KiB",
            dir.display()
        );
        // repeating the same work must not keep raising the peak once it is warm; a little
        // allocator slack is allowed, growth with every round is not
        assert!(
            peaks[ROUNDS - 1] - peaks[1] <= 4 * 1024,
            "{}: peak grew {peaks:?}",
            dir.display()
        );
    }
}
