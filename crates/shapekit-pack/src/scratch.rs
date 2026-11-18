//! Scratch directories under the system temporary directory that a signal handler can
//! remove. A `TempDir` removes its directory when it is dropped, but a process ended by a
//! signal drops nothing, so every run stopped with SIGTERM (or SIGINT without a handler)
//! left its node workdirs and archive copies behind. Each [`Scratch`] is recorded here while
//! it lives, and [`remove_all`] removes whatever is recorded; `shapekit` calls it from its
//! signal handler before it exits.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LIVE: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

fn live() -> std::sync::MutexGuard<'static, BTreeSet<PathBuf>> {
    // the set stays consistent whatever panicked while holding it: each step is one insert
    // or one remove
    LIVE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A temporary directory, removed when dropped or closed, or by [`remove_all`].
#[derive(Debug)]
pub struct Scratch {
    dir: Option<tempfile::TempDir>,
}

/// Creates a scratch directory named `<prefix><random>` in the system temporary directory.
pub fn scratch_dir(prefix: &str) -> std::io::Result<Scratch> {
    let dir = tempfile::Builder::new().prefix(prefix).tempdir()?;
    live().insert(dir.path().to_path_buf());
    Ok(Scratch { dir: Some(dir) })
}

impl Scratch {
    pub fn path(&self) -> &Path {
        self.dir
            .as_ref()
            .expect("a scratch directory until it is closed")
            .path()
    }

    /// Removes the directory now, reporting a failure that a drop could only ignore.
    pub fn close(mut self) -> std::io::Result<()> {
        let dir = self.dir.take().expect("a scratch directory is closed once");
        live().remove(dir.path());
        dir.close()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Some(dir) = self.dir.take() {
            live().remove(dir.path());
            drop(dir);
        }
    }
}

/// Removes every scratch directory still alive, for a process about to exit without
/// unwinding. Returns the ones that could not be removed, with why.
pub fn remove_all() -> Vec<String> {
    let dirs = std::mem::take(&mut *live());
    let mut failed = Vec::new();
    for d in dirs {
        if let Err(e) = std::fs::remove_dir_all(&d)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            failed.push(format!("{}: {e}", d.display()));
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_directories_are_removed_by_remove_all() {
        let a = scratch_dir("shapekit-scratch-test-").unwrap();
        let b = scratch_dir("shapekit-scratch-test-").unwrap();
        let (pa, pb) = (a.path().to_path_buf(), b.path().to_path_buf());
        b.close().unwrap();
        assert!(!pb.exists());
        assert!(pa.is_dir());
        assert!(remove_all().is_empty());
        assert!(!pa.exists());
        // dropping one remove_all already took is quiet
        drop(a);
    }
}
