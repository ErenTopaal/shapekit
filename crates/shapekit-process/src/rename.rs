//! Renames that leave no instant where the destination is missing or silently replaced: an
//! exchange of two paths, and a rename that refuses an existing destination. `std::fs::rename`
//! offers neither: it replaces an existing file or empty directory at the destination, and
//! swapping two directories with it takes two renames with a gap between them.

use std::io;
use std::path::Path;

/// Swaps `a` and `b` in one step: afterwards `a` holds what `b` held and `b` what `a` held.
/// Both must exist and be on one filesystem.
pub fn exchange(a: &Path, b: &Path) -> io::Result<()> {
    imp::exchange(a, b)
}

/// Moves `from` to `to`, failing with [`io::ErrorKind::AlreadyExists`] if `to` exists, even
/// as an empty directory that a plain rename would replace.
pub fn no_replace(from: &Path, to: &Path) -> io::Result<()> {
    imp::no_replace(from, to)
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    fn c(p: &Path) -> io::Result<CString> {
        CString::new(p.as_os_str().as_bytes()).map_err(io::Error::other)
    }

    fn renameat2(from: &Path, to: &Path, flags: libc::c_uint) -> io::Result<()> {
        let (f, t) = (c(from)?, c(to)?);
        // SAFETY: both pointers are nul-terminated strings that live across the call
        let rc = unsafe {
            libc::renameat2(libc::AT_FDCWD, f.as_ptr(), libc::AT_FDCWD, t.as_ptr(), flags)
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn exchange(a: &Path, b: &Path) -> io::Result<()> {
        renameat2(a, b, libc::RENAME_EXCHANGE)
    }

    pub fn no_replace(from: &Path, to: &Path) -> io::Result<()> {
        renameat2(from, to, libc::RENAME_NOREPLACE)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    fn c(p: &Path) -> io::Result<CString> {
        CString::new(p.as_os_str().as_bytes()).map_err(io::Error::other)
    }

    fn renamex_np(from: &Path, to: &Path, flags: libc::c_uint) -> io::Result<()> {
        let (f, t) = (c(from)?, c(to)?);
        // SAFETY: both pointers are nul-terminated strings that live across the call
        let rc = unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), flags) };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn exchange(a: &Path, b: &Path) -> io::Result<()> {
        renamex_np(a, b, libc::RENAME_SWAP)
    }

    pub fn no_replace(from: &Path, to: &Path) -> io::Result<()> {
        renamex_np(from, to, libc::RENAME_EXCL)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    use super::*;

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "an atomic exchange or no-replace rename is implemented for linux and macos only",
        )
    }

    pub fn exchange(_: &Path, _: &Path) -> io::Result<()> {
        Err(unsupported())
    }

    pub fn no_replace(_: &Path, _: &Path) -> io::Result<()> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchange_swaps_two_directories() {
        let t = tempfile::tempdir().unwrap();
        let (a, b) = (t.path().join("a"), t.path().join("b"));
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        std::fs::write(a.join("x"), "old").unwrap();
        std::fs::write(b.join("x"), "new").unwrap();
        exchange(&b, &a).unwrap();
        assert_eq!(std::fs::read_to_string(a.join("x")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(b.join("x")).unwrap(), "old");
        assert!(exchange(&a, &t.path().join("missing")).is_err());
    }

    #[test]
    fn no_replace_refuses_even_an_empty_directory() {
        let t = tempfile::tempdir().unwrap();
        let (from, to) = (t.path().join("from"), t.path().join("to"));
        std::fs::create_dir(&from).unwrap();
        std::fs::create_dir(&to).unwrap();
        let err = no_replace(&from, &to).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(from.is_dir());
        std::fs::remove_dir(&to).unwrap();
        no_replace(&from, &to).unwrap();
        assert!(to.is_dir() && !from.exists());
    }
}
