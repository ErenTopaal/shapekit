//! Scratch directories under the system temporary directory that a signal removes. A
//! `TempDir` removes its directory when it is dropped, but a process ended by a signal drops
//! nothing; each [`Scratch`] is recorded in [`crate::cleanup`] while it lives, so the signal
//! thread removes it before the process exits.

use std::path::Path;

/// A temporary directory, removed when dropped or closed, or by [`crate::cleanup::stop`].
#[derive(Debug)]
pub struct Scratch {
    dir: Option<tempfile::TempDir>,
}

/// Creates a scratch directory named `<prefix><random>` in the system temporary directory.
pub fn scratch_dir(prefix: &str) -> std::io::Result<Scratch> {
    scratch_in(&std::env::temp_dir(), prefix)
}

/// Creates a scratch directory named `<prefix><random>` in `parent`.
pub fn scratch_in(parent: &Path, prefix: &str) -> std::io::Result<Scratch> {
    // created and recorded under one lock: a signal taking the record in between would
    // leave the directory behind, and one created after the record was taken would never be
    // removed
    crate::cleanup::with(|s| {
        if s.stopping {
            return Err(std::io::Error::other("the process is stopping"));
        }
        let dir = tempfile::Builder::new().prefix(prefix).tempdir_in(parent)?;
        s.dirs.insert(dir.path().to_path_buf());
        Ok(Scratch { dir: Some(dir) })
    })
}

impl Scratch {
    pub fn path(&self) -> &Path {
        self.dir
            .as_ref()
            .expect("a scratch directory until it is closed or kept")
            .path()
    }

    /// Removes the directory now, reporting a failure that a drop could only ignore.
    pub fn close(mut self) -> std::io::Result<()> {
        let dir = self.dir.take().expect("a scratch directory is closed once");
        forget(dir.path());
        dir.close()
    }

    /// Keeps the directory: neither a drop nor a signal removes it any more.
    pub fn keep(mut self) -> std::path::PathBuf {
        let dir = self.dir.take().expect("a scratch directory is kept once");
        forget(dir.path());
        dir.keep()
    }
}

fn forget(path: &Path) {
    crate::cleanup::with(|s| s.dirs.remove(path));
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Some(dir) = self.dir.take() {
            forget(dir.path());
            drop(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // one test: the record is process-wide and stop() closes it for good
    #[test]
    fn live_directories_are_removed_by_stop_and_none_are_created_after() {
        let a = scratch_dir("shapekit-scratch-test-").unwrap();
        let b = scratch_dir("shapekit-scratch-test-").unwrap();
        let kept = scratch_dir("shapekit-scratch-test-").unwrap().keep();
        let (pa, pb) = (a.path().to_path_buf(), b.path().to_path_buf());
        b.close().unwrap();
        assert!(!pb.exists());
        assert!(pa.is_dir());
        assert!(crate::cleanup::stop().is_empty());
        assert!(!pa.exists());
        assert!(kept.is_dir(), "a kept directory is not removed");
        std::fs::remove_dir(&kept).unwrap();
        // dropping one stop() already took is quiet
        drop(a);
        assert!(scratch_dir("shapekit-scratch-test-").is_err());
    }
}
