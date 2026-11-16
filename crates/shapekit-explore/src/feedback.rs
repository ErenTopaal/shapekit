//! What counts as new. Two sources feed the search:
//!
//! - the ledgers themselves: every path of a closed ledger's signature (struct fields, union
//!   arms, enum values, length buckets), checked after every close, so a single step can be
//!   credited;
//! - the coverage twin: the code regions of core's transaction and ledger code an episode
//!   ran, read from the profile the twin writes when it stops, so only a whole episode can be
//!   credited.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

/// A code region of core: repository path and span.
pub type RegionKey = (String, u32, u32, u32, u32);

/// Everything the run has seen so far, shared by the workers.
#[derive(Debug, Default)]
pub struct Seen {
    inner: Mutex<SeenInner>,
}

#[derive(Debug, Default)]
struct SeenInner {
    paths: BTreeSet<String>,
    regions: BTreeSet<RegionKey>,
}

impl Seen {
    /// Adds a ledger's signature; returns the paths not seen before.
    pub fn paths(&self, signature: &BTreeSet<String>) -> BTreeSet<String> {
        let mut g = self
            .inner
            .lock()
            .expect("no worker panics holding the lock");
        let new: BTreeSet<String> = signature.difference(&g.paths).cloned().collect();
        g.paths.extend(new.iter().cloned());
        new
    }

    /// Adds the regions an episode reached; returns how many were new.
    pub fn regions(&self, reached: impl IntoIterator<Item = RegionKey>) -> usize {
        let mut g = self
            .inner
            .lock()
            .expect("no worker panics holding the lock");
        reached
            .into_iter()
            .filter(|r| g.regions.insert(r.clone()))
            .count()
    }

    pub fn counts(&self) -> (usize, usize) {
        let g = self
            .inner
            .lock()
            .expect("no worker panics holding the lock");
        (g.paths.len(), g.regions.len())
    }

    /// The regions reached so far, per file.
    pub fn regions_by_file(&self) -> BTreeMap<String, usize> {
        let g = self
            .inner
            .lock()
            .expect("no worker panics holding the lock");
        let mut out = BTreeMap::new();
        for r in &g.regions {
            *out.entry(r.0.clone()).or_insert(0) += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unseen_paths_and_regions_count() {
        let s = Seen::default();
        let a: BTreeSet<String> = ["A", "A.b"].iter().map(|x| x.to_string()).collect();
        assert_eq!(s.paths(&a).len(), 2);
        assert_eq!(s.paths(&a).len(), 0);
        let r = ("src/x.cpp".to_owned(), 1, 1, 2, 2);
        assert_eq!(s.regions([r.clone()]), 1);
        assert_eq!(s.regions([r]), 0);
        assert_eq!(s.counts(), (2, 1));
    }
}
