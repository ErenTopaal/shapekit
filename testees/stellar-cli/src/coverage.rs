//! `coverage_snapshot` for a coverage build: the stellar binary is built from the crate source
//! with `-C instrument-coverage`, and every run of it adds its counters to the profile files of
//! this testee process (LLVM's `%Nm` pool, which merges runs into a fixed number of files).
//! A snapshot merges them with the toolchain's llvm-profdata and reads the regions that ran
//! with llvm-cov export, as the rust testee does for its own binary.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde_json::{Value, json};

/// The profile directory of this process when SHAPEKIT_COVERAGE=1, created on first use.
pub fn dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        if std::env::var("SHAPEKIT_COVERAGE").as_deref() != Ok("1") {
            return None;
        }
        let d = std::env::temp_dir().join(format!("shapekit-cli-cov-{}", std::process::id()));
        // a directory left by an earlier process with the same pid holds stale counters
        let _ = std::fs::remove_dir_all(&d);
        // coverage was asked for: running without it would answer every snapshot unsupported
        if let Err(e) = std::fs::create_dir_all(&d) {
            eprintln!(
                "SHAPEKIT_COVERAGE=1 but {} cannot be made: {e}",
                d.display()
            );
            std::process::exit(1);
        }
        Some(d)
    })
    .as_deref()
}

/// What LLVM_PROFILE_FILE is for each run of the binary: a pool of four files that the runs
/// merge into.
pub fn profile_file(dir: &Path) -> PathBuf {
    dir.join("stellar-%4m.profraw")
}

fn llvm_tool(name: &str) -> Result<PathBuf, String> {
    let out = |args: &[&str]| -> Result<String, String> {
        let o = Command::new("rustc")
            .args(args)
            .output()
            .map_err(|e| format!("running rustc {}: {e}", args.join(" ")))?;
        if !o.status.success() {
            return Err(format!("rustc {} exited with {}", args.join(" "), o.status));
        }
        String::from_utf8(o.stdout).map_err(|e| e.to_string())
    };
    let sysroot = out(&["--print", "sysroot"])?;
    let host = out(&["-vV"])?
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        .ok_or("rustc -vV names no host")?;
    let p = Path::new(sysroot.trim())
        .join("lib/rustlib")
        .join(host)
        .join("bin")
        .join(name);
    if p.is_file() {
        Ok(p)
    } else {
        Err(format!(
            "{} is missing; the llvm-tools component is needed",
            p.display()
        ))
    }
}

fn tool(tool: &Path, args: &[&std::ffi::OsStr]) -> Result<Vec<u8>, String> {
    let o = Command::new(tool)
        .args(args)
        .output()
        .map_err(|e| format!("running {}: {e}", tool.display()))?;
    if !o.status.success() {
        return Err(format!(
            "{} exited with {}: {}",
            tool.display(),
            o.status,
            String::from_utf8_lossy(&o.stderr).trim()
        ));
    }
    Ok(o.stdout)
}

/// The crates a range may belong to: the xdr commands are stellar-xdr's cli, the strkey
/// commands stellar-strkey's, and `tx hash` soroban-cli's.
const CRATES: &[&str] = &["stellar-xdr", "stellar-strkey", "soroban-cli"];

/// Every code region of those crates that ran in any run of the binary since this process
/// started, as `<crate>-<version>/<path inside the crate>:<line>:<col>-<line>:<col>`.
pub fn snapshot(dir: &Path, cli: &str) -> Result<Value, String> {
    let raws: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("reading {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "profraw"))
        .collect();
    let mut ranges = std::collections::BTreeSet::new();
    if !raws.is_empty() {
        let data = dir.join("merged.profdata");
        let mut args: Vec<&std::ffi::OsStr> = vec!["merge".as_ref(), "-sparse".as_ref()];
        args.extend(raws.iter().map(|p| p.as_os_str()));
        args.extend(["-o".as_ref(), data.as_os_str()]);
        tool(&llvm_tool("llvm-profdata")?, &args)?;
        let mut flag = std::ffi::OsString::from("-instr-profile=");
        flag.push(&data);
        let json = tool(
            &llvm_tool("llvm-cov")?,
            &[
                "export".as_ref(),
                "-format=text".as_ref(),
                flag.as_os_str(),
                cli.as_ref(),
            ],
        )?;
        let _ = std::fs::remove_file(&data);
        let export: Value =
            serde_json::from_slice(&json).map_err(|e| format!("parsing llvm-cov export: {e}"))?;
        for unit in export["data"].as_array().into_iter().flatten() {
            for f in unit["functions"].as_array().into_iter().flatten() {
                let files: Vec<&str> = f["filenames"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                for r in f["regions"].as_array().into_iter().flatten() {
                    // [line start, column start, line end, column end, count, file id,
                    // expanded file id, kind]; kind 0 is a code region
                    let n = |i: usize| r.get(i).and_then(Value::as_u64);
                    let (
                        Some(ls),
                        Some(cs),
                        Some(le),
                        Some(ce),
                        Some(count),
                        Some(file),
                        Some(kind),
                    ) = (n(0), n(1), n(2), n(3), n(4), n(5), n(7))
                    else {
                        return Err("llvm-cov export has a region of an unknown shape".into());
                    };
                    if count == 0 || kind != 0 {
                        continue;
                    }
                    let Some(path) = files.get(file as usize) else {
                        continue;
                    };
                    if let Some(rel) = crate_path(path) {
                        ranges.insert(format!("{rel}:{ls}:{cs}-{le}:{ce}"));
                    }
                }
            }
        }
    }
    Ok(json!({
        "tool": "llvm source-based region coverage",
        "ranges": ranges.into_iter().collect::<Vec<_>>(),
    }))
}

/// `<crate>-<version>/<path>` of a source file of one of CRATES in the cargo registry.
fn crate_path(path: &str) -> Option<&str> {
    CRATES.iter().find_map(|c| {
        let at = path.find(&format!("/{c}-"))?;
        let rel = &path[at + 1..];
        // the next path component after <crate>- must be a version
        let rest = &rel[c.len() + 1..];
        rest.chars()
            .next()
            .filter(char::is_ascii_digit)
            .map(|_| rel)
    })
}

#[cfg(test)]
mod tests {
    use super::crate_path;

    #[test]
    fn registry_paths_name_their_crate_and_version() {
        assert_eq!(
            crate_path(
                "/cargo/registry/src/index.crates.io-1949/stellar-xdr-30.0.0/src/cli/decode.rs"
            ),
            Some("stellar-xdr-30.0.0/src/cli/decode.rs")
        );
        assert_eq!(
            crate_path("/r/soroban-cli-30.0.1/src/commands/tx/hash.rs"),
            Some("soroban-cli-30.0.1/src/commands/tx/hash.rs")
        );
        assert_eq!(crate_path("/r/stellar-xdr-json/src/lib.rs"), None);
        assert_eq!(crate_path("/r/serde-1.0.0/src/lib.rs"), None);
    }
}
