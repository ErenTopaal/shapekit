//! Reading files a manifest or chain file names. Those files are edited by hand and by
//! tools, so a path is accepted only when it stays inside the directory it is relative to,
//! and the bytes only when they still hash to what was recorded.

use std::path::Path;

pub use crate::validate::is_safe_relative;

/// Reads `rel` under `root`, refusing absolute paths, `..` and backslashes, and bytes whose
/// sha256 is not `sha256` when one is given. The error says which check failed.
pub fn read_verified(root: &Path, rel: &str, sha256: Option<&str>) -> Result<Vec<u8>, String> {
    if !is_safe_relative(rel) {
        return Err(format!(
            "{rel:?} is not a relative path inside {}",
            root.display()
        ));
    }
    let path = root.join(rel);
    let bytes = std::fs::read(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    if let Some(want) = sha256 {
        let got = crate::sha256_hex(&bytes);
        if got != want {
            return Err(format!(
                "{} has sha256 {got}, the record says {want}",
                path.display()
            ));
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_escapes_and_stale_bytes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), b"abc").unwrap();
        let sha = crate::sha256_hex(b"abc");
        assert_eq!(
            read_verified(dir.path(), "a.bin", Some(&sha)).unwrap(),
            b"abc"
        );
        assert!(read_verified(dir.path(), "a.bin", None).is_ok());
        let other = crate::sha256_hex(b"abd");
        assert!(read_verified(dir.path(), "a.bin", Some(&other)).is_err());
        for bad in ["../a.bin", "/etc/passwd", "x/../a.bin", "", "a\\b"] {
            assert!(read_verified(dir.path(), bad, None).is_err(), "{bad}");
        }
    }
}
