//! `shapekit serve` keeps its resident memory bounded while serving the largest committed
//! chain end to end, repeatedly: every ledger in both formats, every transaction, the
//! ledger entries at the head, and the head moved back and forward.

#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use shapekit_serve::{BLOCKING_THREADS, DEFAULT_CACHE_BYTES};

/// Resident bytes per byte of the largest JSON-RPC response, for one thread that built it.
/// A response is written as text a ledger or transaction at a time, so on a large chain this
/// is little more than the text itself; on a small one it is mostly the fixed cost of the
/// first request thread (its malloc arena and stack, about 3 MiB) over a small response.
/// Measured as peak minus the rss at startup, over the largest response, every committed
/// chain swept three times: 2.8 to 3.1 on classic.failed-tx.p30 (3,756,669 bytes of
/// getTransactions json; was 33.9 when responses were built as a `serde_json::Value`), 9.5
/// to 10.7 on the checkpoints fixture below (395,811 bytes), and at most 16.6 over all chains
/// (cap26.inflation.p12, 178,265 bytes, nearly all of it the fixed cost). 17 is above all of
/// them.
const RESIDENT_PER_RESPONSE_BYTE: u64 = 17;

/// Resident bytes of the ledger state per decompressed lake byte: the state holds only what
/// the metas created, measured at 1.6 times the lake on classic.failed-tx.p30 (head 2 to
/// 216, 1,371,252 lake bytes). The batch cache, the lake once up to its budget, is apart.
const STATE_PER_LAKE_BYTE: u64 = 3;

/// Growth of the peak allowed across warm rounds that started no new request thread: the
/// allocator settles by a few hundred KiB per round, a leak keeps adding.
const WARM_SLACK_KIB: u64 = 4 * 1024;

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

/// What one round of requests saw: the largest response body, and the decompressed lake
/// bytes as the base64 ledgers carry them.
#[derive(Default)]
struct Seen {
    largest_response: u64,
    lake_bytes: u64,
}

fn call(
    client: &reqwest::blocking::Client,
    url: &str,
    method: &str,
    params: Value,
    seen: &mut Seen,
) -> Value {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let text = client
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .unwrap()
        .text()
        .unwrap();
    seen.largest_response = seen.largest_response.max(text.len() as u64);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert!(v.get("error").is_none(), "{method}: {v}");
    v
}

fn base64_len(s: &str) -> u64 {
    let pad = s.bytes().rev().take_while(|&b| b == b'=').count();
    (s.len() / 4 * 3 - pad) as u64
}

fn sweep(client: &reqwest::blocking::Client, url: &str, first: u64, last: u64) -> (usize, Seen) {
    let mut seen = Seen::default();
    let mut served = 0;
    for format in ["base64", "json"] {
        let mut start = first;
        while start <= last {
            let r = call(
                client,
                url,
                "getLedgers",
                json!({"startLedger": start, "pagination": {"limit": 200}, "xdrFormat": format}),
                &mut seen,
            );
            let ledgers = r["result"]["ledgers"].as_array().unwrap();
            if format == "base64" {
                for l in ledgers {
                    seen.lake_bytes += base64_len(l["metadataXdr"].as_str().unwrap());
                }
            }
            served += ledgers.len();
            start += ledgers.len() as u64;
        }
        call(
            client,
            url,
            "getTransactions",
            json!({"startLedger": first, "pagination": {"limit": 200}, "xdrFormat": format}),
            &mut seen,
        );
    }
    (served, seen)
}

/// The end of one round: the peak so far and the threads alive.
#[derive(Debug, Clone, Copy)]
struct Sample {
    peak_kib: u64,
    threads: u64,
}

/// What was measured besides the rounds: rss and threads once the server is up, before any
/// request, and the largest response and lake size the rounds saw.
struct Inputs {
    start_rss_kib: u64,
    start_threads: u64,
    largest_response: u64,
    lake_bytes: u64,
}

impl Inputs {
    /// One request thread's arena at its high-water mark.
    fn arena_kib(&self) -> u64 {
        RESIDENT_PER_RESPONSE_BYTE * self.largest_response / 1024
    }
}

/// The peak may hold the process at startup, the cached lake and the ledger state, and one
/// response's working set per request thread the server started, which it caps. Between
/// warm rounds the peak may rise by allocator slack, or by one arena per thread started in
/// that round; any other rise is growth with repetition.
fn check(samples: &[Sample], i: &Inputs) -> Result<(), String> {
    let last = samples.last().ok_or("no rounds")?;
    let request_threads = last.threads.saturating_sub(i.start_threads);
    if request_threads > BLOCKING_THREADS as u64 {
        return Err(format!(
            "{request_threads} request threads, the server caps them at {BLOCKING_THREADS}"
        ));
    }
    let bound = i.start_rss_kib
        + i.lake_bytes.min(DEFAULT_CACHE_BYTES as u64) / 1024
        + STATE_PER_LAKE_BYTE * i.lake_bytes / 1024
        + request_threads * i.arena_kib();
    if last.peak_kib > bound {
        return Err(format!(
            "peak rss {} KiB over {bound} KiB: {} KiB at startup, {} lake bytes, {request_threads} request thread(s) of at most {} KiB for a largest response of {} bytes",
            last.peak_kib,
            i.start_rss_kib,
            i.lake_bytes,
            i.arena_kib(),
            i.largest_response
        ));
    }
    let mut grew = 0;
    for w in samples[1..].windows(2) {
        let rise = w[1].peak_kib.saturating_sub(w[0].peak_kib);
        let new_threads = w[1].threads.saturating_sub(w[0].threads);
        grew += rise.saturating_sub(new_threads * i.arena_kib());
    }
    if grew > WARM_SLACK_KIB {
        return Err(format!(
            "peak grew {grew} KiB across warm rounds beyond new threads: {samples:?}"
        ));
    }
    Ok(())
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
        let start_rss_kib = status_kib(pid, "VmRSS:");
        let start_threads = status_kib(pid, "Threads:");
        let url = line.trim().rsplit(" at ").next().unwrap().to_string();
        let client = reqwest::blocking::Client::new();
        let mut seen = Seen::default();
        let health = call(&client, &url, "getHealth", json!(null), &mut seen);
        let first = health["result"]["oldestLedger"].as_u64().unwrap();
        let last = health["result"]["latestLedger"].as_u64().unwrap();

        let mut samples = Vec::new();
        for _ in 0..ROUNDS {
            let (served, round) = sweep(&client, &url, first, last);
            assert_eq!(served as u64, 2 * (last - first + 1));
            seen.largest_response = seen.largest_response.max(round.largest_response);
            seen.lake_bytes = round.lake_bytes;
            // move the head back and forward so the ledger state is rebuilt
            for head in [first + (last - first) / 2, last] {
                client
                    .post(format!("{url}/control/head"))
                    .body(json!({"ledger": head}).to_string())
                    .send()
                    .unwrap();
                call(
                    &client,
                    &url,
                    "getLedgerEntries",
                    json!({"keys": []}),
                    &mut seen,
                );
            }
            samples.push(Sample {
                peak_kib: status_kib(pid, "VmHWM:"),
                threads: status_kib(pid, "Threads:"),
            });
        }
        let rss = status_kib(pid, "VmRSS:");
        child.kill().unwrap();
        child.wait().unwrap();
        eprintln!(
            "{}: ledgers {first} to {last}, lake {} bytes, largest response {} bytes, rss at startup {start_rss_kib} KiB with {start_threads} threads, per round {samples:?}, rss at end {rss} KiB",
            dir.display(),
            seen.lake_bytes,
            seen.largest_response
        );
        let inputs = Inputs {
            start_rss_kib,
            start_threads,
            largest_response: seen.largest_response,
            lake_bytes: seen.lake_bytes,
        };
        if let Err(e) = check(&samples, &inputs) {
            panic!("{}: {e}", dir.display());
        }
    }
}

/// The check itself, on made-up rounds, so a failure of the test above means the server and
/// not a check that cannot fail.
#[test]
fn the_check_catches_growth_and_excess() {
    let inputs = Inputs {
        start_rss_kib: 32 * 1024,
        start_threads: 4,
        largest_response: 4 << 20,
        lake_bytes: 4 << 20,
    };
    let arena = inputs.arena_kib();
    let rounds = |peaks: &[u64], threads: &[u64]| -> Vec<Sample> {
        peaks
            .iter()
            .zip(threads)
            .map(|(&p, &t)| Sample {
                peak_kib: p,
                threads: t,
            })
            .collect()
    };
    let warm = 32 * 1024 + arena;

    // flat after the first round, then a second request thread arrives with its own arena
    let flat = rounds(&[warm; 10], &[5; 10]);
    assert_eq!(check(&flat, &inputs), Ok(()));
    let mut peaks = [warm; 10];
    let mut threads = [5; 10];
    for r in 4..10 {
        peaks[r] += arena;
        threads[r] = 6;
    }
    assert_eq!(check(&rounds(&peaks, &threads), &inputs), Ok(()));

    // a leak of 1 MiB a round with no new thread
    let leak: Vec<u64> = (0..10).map(|r| warm + r * 1024).collect();
    let err = check(&rounds(&leak, &[5; 10]), &inputs).unwrap_err();
    assert!(err.contains("grew"), "{err}");

    // the same leak, with request threads arriving to hide it behind their arenas
    let mut peaks: Vec<u64> = leak.clone();
    let mut threads = [5; 10];
    for (r, t) in threads.iter_mut().enumerate().skip(2) {
        *t = 5 + (r as u64 - 1).min(3);
    }
    for r in 2..5 {
        for p in peaks.iter_mut().skip(r) {
            *p += arena;
        }
    }
    let err = check(&rounds(&peaks, &threads), &inputs).unwrap_err();
    assert!(err.contains("grew"), "{err}");

    // flat, but more than one arena per request thread
    let over = rounds(&[warm + arena; 10], &[5; 10]);
    let err = check(&over, &inputs).unwrap_err();
    assert!(err.contains("over"), "{err}");

    // more request threads than the server allows
    let many = rounds(&[warm; 10], &[5, 5, 5, 5, 5, 5, 5, 5, 5, 9]);
    let err = check(&many, &inputs).unwrap_err();
    assert!(err.contains("caps"), "{err}");
}
