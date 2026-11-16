//! The replay server keeps its memory bounded while serving the largest committed chain end
//! to end, repeatedly: every ledger in both formats, every transaction, the ledger entries at
//! the head, and the head moved back and forward.
//!
//! Memory is counted, not sampled from the kernel: this test binary's global allocator adds
//! up the bytes live at every moment, and the server runs in this process. Resident memory
//! (the peak `VmHWM`) depends on which threads the runtime hands each request to: glibc gives
//! every thread its own malloc arena, and a request thread that served only small responses
//! in the first rounds grows its arena the first time it serves the largest one, in any later
//! round, with no new thread and no leak. Under load that hand-off varies from run to run, so
//! a resident-memory check of warm rounds either flaps or must allow one arena per thread,
//! which hides a leak of that size. Live bytes do not depend on arenas or scheduling: a
//! request frees what it allocated, so warm rounds end with the same live bytes, and a leak
//! is a rise of exactly what was not freed.
//!
//! Each round uses its own connection and is read once the count stops moving, so a round
//! ends with only what the server keeps between requests: the cached lake and the ledger
//! state. A connection kept across rounds holds buffers sized by its last bodies, which made
//! round ends swing by up to 350 KiB. What still varies is timing within a round, how much of
//! a response the server holds while the client already holds its copy, so the peak is
//! checked against a bound and leaks are read from the round ends.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::json;
use shapekit_serve::{DEFAULT_CACHE_BYTES, ServeConfig};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(by: usize) {
    let now = LIVE.fetch_add(by, Ordering::Relaxed) + by;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

// SAFETY: every method forwards to the system allocator unchanged and only counts sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: the caller's contract for `alloc` is passed on as is
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            grew(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        // SAFETY: as for `alloc`
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            grew(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: as for `alloc`
        unsafe { System.dealloc(p, l) };
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        // SAFETY: as for `alloc`
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            if new >= l.size() {
                grew(new - l.size());
            } else {
                LIVE.fetch_sub(l.size() - new, Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// The counters are process wide, so the tests of this file run one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

/// Live bytes at the peak of a round, beyond what was live before the first request and the
/// cached lake, per byte of the largest response: the ledger state, the server's text of the
/// response and the client's copy of the body. Measured over ten rounds, five runs in debug
/// and release on a host at load average 28: 3.3 to 3.8 on classic.failed-tx.p30 (3,756,669
/// bytes of getTransactions json) and 4.4 to 4.9 on the checkpoints fixture (395,811 bytes),
/// where the runtime's fixed allocations weigh more; with a connection per round, 2.9 to 4.1
/// and 3.7 to 5.3. 8 is above all of them.
const LIVE_PER_RESPONSE_BYTE: u64 = 8;

/// Live bytes of the ledger state per decompressed lake byte.
const STATE_PER_LAKE_BYTE: u64 = 3;

/// How far any warm round's end may sit above the first round's end. Measured over eight runs
/// in debug and release at load average 24: at most 475 bytes on classic.failed-tx.p30 and
/// 434 bytes on the fixture. A leak of 512 bytes a round passes it by the ninth round.
const WARM_SLACK: u64 = 4 * 1024;

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

/// Only the members the sweep reads; the rest of a response is skipped, not built.
#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    error: Option<IgnoredAny>,
    #[serde(default)]
    result: Option<ReplyResult>,
}

#[derive(Deserialize)]
struct ReplyResult {
    #[serde(default)]
    ledgers: Vec<IgnoredAny>,
    #[serde(default, rename = "oldestLedger")]
    oldest: Option<u64>,
    #[serde(default, rename = "latestLedger")]
    latest: Option<u64>,
}

#[derive(Default)]
struct Seen {
    largest_response: u64,
    lake_bytes: u64,
}

fn call(
    client: &reqwest::blocking::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
    seen: &mut Seen,
) -> (ReplyResult, String) {
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
    let r: Reply = serde_json::from_str(&text).unwrap();
    assert!(
        r.error.is_none(),
        "{method}: {}",
        &text[..text.len().min(300)]
    );
    (r.result.unwrap(), text)
}

/// Decoded byte length of every base64 `metadataXdr` in a getLedgers body, read from the
/// text so no copy of the ledgers is kept.
fn base64_bytes(text: &str) -> u64 {
    let mut total = 0;
    for part in text.split("\"metadataXdr\":\"").skip(1) {
        let s = &part[..part.find('"').unwrap()];
        let pad = s.bytes().rev().take_while(|&b| b == b'=').count();
        total += (s.len() / 4 * 3 - pad) as u64;
    }
    total
}

fn sweep(client: &reqwest::blocking::Client, url: &str, first: u64, last: u64, seen: &mut Seen) {
    let mut served = 0;
    let mut lake = 0;
    for format in ["base64", "json"] {
        let mut start = first;
        while start <= last {
            let (r, text) = call(
                client,
                url,
                "getLedgers",
                json!({"startLedger": start, "pagination": {"limit": 200}, "xdrFormat": format}),
                seen,
            );
            if format == "base64" {
                lake += base64_bytes(&text);
            }
            served += r.ledgers.len() as u64;
            start += r.ledgers.len() as u64;
        }
        call(
            client,
            url,
            "getTransactions",
            json!({"startLedger": first, "pagination": {"limit": 200}, "xdrFormat": format}),
            seen,
        );
    }
    assert_eq!(served, 2 * (last - first + 1));
    seen.lake_bytes = lake;
}

/// One round: live bytes at its end and at its peak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sample {
    end: u64,
    peak: u64,
}

struct Inputs {
    /// Live bytes once the server is up, before any request.
    start: u64,
    largest_response: u64,
    lake_bytes: u64,
}

/// The peak may hold what was live at startup, the cached lake, the ledger state and the
/// working set of the largest response. The first round fills the cache and the state; no
/// later round may end above it. Growth is reported before the bound, so a leak reads as a
/// leak even once it also pushes the peak over.
fn check(samples: &[Sample], i: &Inputs) -> Result<(), String> {
    if samples.len() < 2 {
        return Err(format!("{} rounds, the check needs 2", samples.len()));
    }
    let first = samples[0].end;
    let rises: Vec<u64> = samples[1..]
        .iter()
        .map(|s| s.end.saturating_sub(first))
        .collect();
    if rises.iter().any(|r| *r > WARM_SLACK) {
        return Err(format!(
            "live bytes grew across warm rounds: round ends above the first round's by {rises:?} (allowed {WARM_SLACK}): {samples:?}"
        ));
    }
    let bound = i.start
        + i.lake_bytes.min(DEFAULT_CACHE_BYTES as u64)
        + STATE_PER_LAKE_BYTE * i.lake_bytes
        + LIVE_PER_RESPONSE_BYTE * i.largest_response;
    let peak = samples.iter().map(|s| s.peak).max().unwrap_or(0);
    if peak > bound {
        return Err(format!(
            "peak of {peak} live bytes over {bound}: {} at startup, {} lake bytes, largest response {} bytes",
            i.start, i.lake_bytes, i.largest_response
        ));
    }
    Ok(())
}

/// Live bytes once nothing allocates or frees any more: the server's request threads can
/// still be dropping a response or closing the round's connection after the client has its
/// answer, so a round's end is read only when the count has stayed the same for 100 ms. A
/// count that does not settle within 10 s fails the test: a reading taken while something
/// still runs would measure that work, not what the server keeps.
fn settled() -> usize {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut last = LIVE.load(Ordering::Relaxed);
    let mut same = 0;
    while same < 10 {
        assert!(
            std::time::Instant::now() < deadline,
            "live bytes did not stay the same for 100 ms within 10 s (last {last}); something in the process keeps allocating"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        let now = LIVE.load(Ordering::Relaxed);
        if now == last {
            same += 1;
        } else {
            same = 0;
            last = now;
        }
    }
    last
}

/// Serves `dir` in this process for [`ROUNDS`] rounds; `leak` runs at the end of each round.
fn measure(dir: &Path, leak: &mut dyn FnMut()) -> (Vec<Sample>, Inputs) {
    let server = shapekit_serve::start(ServeConfig {
        chain_dir: dir.to_path_buf(),
        listen: "127.0.0.1:0".parse().unwrap(),
        head: None,
        cache_bytes: DEFAULT_CACHE_BYTES,
        log_requests: false,
    })
    .unwrap();
    let url = format!("{}/rpc", server.url());
    let mut seen = Seen::default();
    let (health, _) = call(
        &reqwest::blocking::Client::new(),
        &url,
        "getHealth",
        json!(null),
        &mut seen,
    );
    let (first, last) = (health.oldest.unwrap(), health.latest.unwrap());
    let start = settled() as u64;
    PEAK.store(start as usize, Ordering::Relaxed);
    let mut samples = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        // a client, and so a connection, per round: the connection's buffers on both sides
        // keep whatever size the last bodies gave them, which is not what this test measures
        let client = reqwest::blocking::Client::new();
        sweep(&client, &url, first, last, &mut seen);
        // move the head back and forward so the ledger state is rebuilt
        for head in [first + (last - first) / 2, last] {
            client
                .post(format!("{}/control/head", server.url()))
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
        drop(client);
        leak();
        let end = settled();
        samples.push(Sample {
            end: end as u64,
            peak: PEAK.swap(end, Ordering::Relaxed) as u64,
        });
    }
    server.stop().unwrap();
    let peak = samples.iter().map(|s| s.peak).max().unwrap();
    let cached = seen.lake_bytes.min(DEFAULT_CACHE_BYTES as u64);
    eprintln!(
        "{}: ledgers {first} to {last}, lake {} bytes, largest response {} bytes, live at startup {start}, per round {samples:?}, beyond startup and cache per largest response byte {:.2}",
        dir.display(),
        seen.lake_bytes,
        seen.largest_response,
        peak.saturating_sub(start + cached) as f64 / seen.largest_response as f64
    );
    (
        samples,
        Inputs {
            start,
            largest_response: seen.largest_response,
            lake_bytes: seen.lake_bytes,
        },
    )
}

fn fixture() -> PathBuf {
    repo().join("crates/shapekit-pack/tests/fixtures/checkpoints.p30/testnet")
}

#[test]
fn serve_memory_stays_bounded() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for dir in [largest_chain(), fixture()] {
        let (samples, inputs) = measure(&dir, &mut || {});
        if let Err(e) = check(&samples, &inputs) {
            panic!("{}: {e}", dir.display());
        }
    }
}

/// A real leak in the measured process, 64 KiB a round kept past the round's end, is caught
/// by the same measurement and check, so a pass of the test above means no leak, not a check
/// that cannot fail.
#[test]
fn a_leak_in_the_process_is_caught() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut kept: Vec<Vec<u8>> = Vec::with_capacity(ROUNDS);
    let (samples, inputs) = measure(&fixture(), &mut || kept.push(vec![1u8; 64 * 1024]));
    let err = check(&samples, &inputs).unwrap_err();
    assert!(err.contains("grew across warm rounds"), "{err}");
    drop(kept);
}

/// The check itself, on made-up rounds.
#[test]
fn the_check_catches_growth_and_excess() {
    let inputs = Inputs {
        start: 1 << 20,
        largest_response: 4 << 20,
        lake_bytes: 4 << 20,
    };
    let at = |end: u64, peak: u64| Sample { end, peak };
    let base = 4 << 20;
    let flat: Vec<Sample> = (0..10).map(|_| at(base, base + (8 << 20))).collect();
    assert_eq!(check(&flat, &inputs), Ok(()));
    // ends that move by a few hundred bytes, as measured, are no growth
    let jitter: Vec<Sample> = (0..10u64)
        .map(|r| at(base + (r % 2) * 475, base + (8 << 20)))
        .collect();
    assert_eq!(check(&jitter, &inputs), Ok(()));
    // a leak of 512 bytes a round
    let leak: Vec<Sample> = (0..10u64)
        .map(|r| at(base + r * 512, base + (8 << 20)))
        .collect();
    assert!(check(&leak, &inputs).unwrap_err().contains("grew"));
    // a peak over the bound
    let over: Vec<Sample> = (0..10).map(|_| at(base, 200 << 20)).collect();
    assert!(check(&over, &inputs).unwrap_err().contains("over"));
}
