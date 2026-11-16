//! The time axis: the law book over the public SEP-54 ledger lake.
//!
//! - [`random_audit`] draws ledgers from the whole history with a seed and checks each.
//! - [`watch`] checks every ledger after a cursor up to the lake's newest, so consecutive
//!   runs cover the chain without a gap or a repeat, header chain included.
//!
//! Ledgers are streamed: objects are fetched a few at a time ahead of the checker, and only
//! one decompressed object is held while it is checked. Nothing is written to disk.

pub mod check;
pub mod lake;
pub mod sample;
pub mod summary;

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, bail, ensure};

pub use check::{Audit, Tip};
pub use lake::{Lake, Network, Object};

/// Fetches `ranges` (each a batch's first and last ledger, ascending) with `jobs` concurrent
/// requests and checks them in order. At most two groups of `4 * jobs` compressed objects
/// are in memory: the one being checked and the next one being fetched.
pub fn check_ranges(
    lake: &Lake,
    ranges: &[(u32, u32)],
    jobs: usize,
    audit: &mut Audit,
) -> Result<()> {
    ensure!(jobs > 0, "jobs must be at least 1");
    let group = jobs * 4;
    let groups: Vec<&[(u32, u32)]> = ranges.chunks(group).collect();
    std::thread::scope(|s| -> Result<()> {
        let fetch = |g: &'_ [(u32, u32)]| fetch_group(lake, g, jobs);
        let mut next = groups.first().map(|g| s.spawn(move || fetch(g)));
        for (i, g) in groups.iter().enumerate() {
            let objects = match next.take() {
                Some(h) => match h.join() {
                    Ok(r) => r?,
                    Err(_) => bail!("a fetch thread panicked"),
                },
                None => unreachable!("one fetch is started per group"),
            };
            next = groups.get(i + 1).map(|g| s.spawn(move || fetch(g)));
            for (object, range) in objects.into_iter().zip(g.iter()) {
                audit.object(&object, *range)?;
            }
        }
        Ok(())
    })
}

fn fetch_group(lake: &Lake, ranges: &[(u32, u32)], jobs: usize) -> Result<Vec<Object>> {
    let slots: Vec<Mutex<Option<Result<Object>>>> =
        ranges.iter().map(|_| Mutex::new(None)).collect();
    let cursor = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..jobs.min(ranges.len()) {
            s.spawn(|| {
                loop {
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some((start, _)) = ranges.get(i) else {
                        break;
                    };
                    let r = lake.fetch(*start);
                    let failed = r.is_err();
                    *slots[i].lock().expect("no thread panics holding a slot") = Some(r);
                    if failed {
                        break;
                    }
                }
            });
        }
    });
    let mut out = Vec::with_capacity(ranges.len());
    for (slot, range) in slots.into_iter().zip(ranges) {
        match slot.into_inner().expect("no thread panics holding a slot") {
            Some(r) => out.push(r?),
            // slots are claimed in order and a thread stops only after a failed fetch, so an
            // unclaimed slot always comes after a failed one, whose error returned above
            None => bail!(
                "ledger {} was not fetched because an earlier fetch failed",
                range.0
            ),
        }
    }
    Ok(out)
}

/// The batch ranges holding `ledgers`, ascending, each once.
pub fn ranges_of(lake: &Lake, ledgers: &[u32]) -> Vec<(u32, u32)> {
    let mut r: Vec<(u32, u32)> = ledgers
        .iter()
        .map(|l| lake.layout.batch_range(*l))
        .collect();
    r.sort_unstable();
    r.dedup();
    r
}

/// The random audit: `n` distinct ledgers below `below` drawn with `seed`, every ledger of
/// the batches holding them checked.
pub fn random_audit(lake: &Lake, n: u32, seed: u32, below: u32, jobs: usize) -> Result<Audit> {
    let drawn = sample::ledgers(n, seed, below);
    let ranges = ranges_of(lake, &drawn);
    let mut audit = Audit::new(lake.network);
    check_ranges(lake, &ranges, jobs, &mut audit)?;
    Ok(audit)
}

/// What one watch run covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub first: u32,
    pub last: u32,
}

/// The ledgers a watch run checks: from the one after `tip` (or `start` when there is no tip)
/// up to `newest`, at most `max` of them. `None` when nothing new has closed.
pub fn window(
    tip: Option<&Tip>,
    start: Option<u32>,
    newest: u32,
    max: u32,
) -> Result<Option<Window>> {
    ensure!(max > 0, "a window of 0 ledgers checks nothing");
    let first = match (tip, start) {
        (Some(t), None) => t
            .ledger
            .checked_add(1)
            .context("the cursor is at the last possible ledger")?,
        (None, Some(s)) => s,
        (Some(_), Some(_)) => bail!("a cursor and a start ledger were both given"),
        (None, None) => bail!("there is no cursor; give the ledger to start at"),
    };
    if first > newest {
        return Ok(None);
    }
    let last = newest.min(first.saturating_add(max - 1));
    Ok(Some(Window { first, last }))
}

/// Checks every ledger of `window`, continuing the header chain from `tip`.
pub fn watch(lake: &Lake, tip: Option<Tip>, window: Window, jobs: usize) -> Result<Audit> {
    let mut ranges = Vec::new();
    let mut at = window.first;
    loop {
        let (start, end) = lake.layout.batch_range(at);
        // a batch partly before the window is fetched whole; its leading ledgers were
        // checked before, so this only happens with batches of more than one ledger
        ensure!(
            start == at,
            "ledger {at} is inside the batch {start} to {end}; a window has to start at a batch boundary"
        );
        ranges.push((start, end));
        if end >= window.last {
            break;
        }
        at = end + 1;
    }
    let mut audit = match tip {
        Some(t) => Audit::after(lake.network, t),
        None => Audit::new(lake.network),
    };
    check_ranges(lake, &ranges, jobs, &mut audit)?;
    Ok(audit)
}
