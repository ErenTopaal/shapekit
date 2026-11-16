//! `coverage_snapshot`: llvm source-based region coverage of stellar-xdr, for a testee built
//! with `-C instrument-coverage --cfg shapekit_coverage` (shapekit builds that variant for
//! coverage runs). The profiler runtime writes this process's counters on request; the
//! toolchain's own llvm-profdata and llvm-cov (the llvm-tools component) turn them into regions.

use std::ffi::{CString, c_char, c_int};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

unsafe extern "C" {
    // compiler-rt's profiler runtime, linked into every instrument-coverage build
    fn __llvm_profile_write_file() -> c_int;
    fn __llvm_profile_set_filename(name: *const c_char);
    // the bounds of this process's counter section: one u64 per counter of every instrumented
    // function (instrument-coverage does not use single-byte counters)
    fn __llvm_profile_begin_counters() -> *const u8;
    fn __llvm_profile_end_counters() -> *const u8;
}

fn set_filename(path: &Path) -> Result<(), String> {
    let c = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|e| format!("profile path: {e}"))?;
    // the runtime keeps the pointer, so the string lives for the rest of the process
    let leaked: &'static CString = Box::leak(Box::new(c));
    unsafe { __llvm_profile_set_filename(leaked.as_ptr()) };
    Ok(())
}

/// Called first thing: without it the runtime writes `default_<n>.profraw` into the working
/// directory when the process exits.
pub fn init() -> Result<(), String> {
    set_filename(Path::new("/dev/null"))
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
        Err(format!("{} is missing; the llvm-tools component is needed", p.display()))
    }
}

fn run(tool: &Path, args: &[&std::ffi::OsStr]) -> Result<Vec<u8>, String> {
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

/// Every code region of stellar-xdr that ran at least once since the process started, as
/// `<path inside the crate>:<line>:<col>-<line>:<col>`.
pub fn snapshot() -> Result<Value, String> {
    static N: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "shapekit-cov-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let result = collect(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    init()?;
    result
}

fn collect(dir: &Path) -> Result<Value, String> {
    let raw = dir.join("testee.profraw");
    set_filename(&raw)?;
    if unsafe { __llvm_profile_write_file() } != 0 {
        return Err("the profiler runtime could not write its counters".into());
    }
    let data = dir.join("testee.profdata");
    run(
        &llvm_tool("llvm-profdata")?,
        &["merge".as_ref(), "-sparse".as_ref(), raw.as_os_str(), "-o".as_ref(), data.as_os_str()],
    )?;
    let exe = std::env::current_exe().map_err(|e| format!("locating the testee: {e}"))?;
    let mut flag = std::ffi::OsString::from("-instr-profile=");
    flag.push(&data);
    let json = run(
        &llvm_tool("llvm-cov")?,
        &["export".as_ref(), "-format=text".as_ref(), flag.as_os_str(), exe.as_os_str()],
    )?;
    let export: Value =
        serde_json::from_slice(&json).map_err(|e| format!("parsing llvm-cov export: {e}"))?;
    let marker = format!("/stellar-xdr-{}/", stellar_xdr::VERSION.pkg);
    let mut ranges = std::collections::BTreeSet::new();
    for unit in export["data"].as_array().into_iter().flatten() {
        for f in unit["functions"].as_array().into_iter().flatten() {
            let files: Vec<&str> = f["filenames"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            for r in f["regions"].as_array().into_iter().flatten() {
                // [line start, column start, line end, column end, count, file id, expanded
                // file id, kind]; kind 0 is a code region
                let n = |i: usize| r.get(i).and_then(Value::as_u64);
                let (Some(ls), Some(cs), Some(le), Some(ce), Some(count), Some(file), Some(kind)) =
                    (n(0), n(1), n(2), n(3), n(4), n(5), n(7))
                else {
                    return Err("llvm-cov export has a region of an unknown shape".into());
                };
                if count == 0 || kind != 0 {
                    continue;
                }
                let Some(path) = files.get(file as usize) else { continue };
                let Some(at) = path.find(&marker) else { continue };
                let rel = &path[at + marker.len()..];
                ranges.insert(format!("{rel}:{ls}:{cs}-{le}:{ce}"));
            }
        }
    }
    Ok(json!({
        "tool": "llvm source-based region coverage",
        "ranges": ranges.into_iter().collect::<Vec<_>>(),
    }))
}

/// `coverage_new`: how many of the process's llvm counters (every instrumented function of the
/// testee binary, stellar-xdr and the crates around it) have become nonzero since the previous
/// call, and how many are nonzero. Read from the counter section in place, with no file and no
/// llvm tool, so it is cheap enough to ask after every input.
pub fn fresh() -> Result<Value, String> {
    static SEEN: Mutex<Vec<bool>> = Mutex::new(Vec::new());
    let (begin, end) = unsafe { (__llvm_profile_begin_counters(), __llvm_profile_end_counters()) };
    let bytes = (end as usize)
        .checked_sub(begin as usize)
        .ok_or("the counter section ends before it begins")?;
    if bytes % 8 != 0 || (begin as usize) % std::mem::align_of::<u64>() != 0 {
        return Err("the counter section is not an array of u64 counters".into());
    }
    let counters = unsafe { std::slice::from_raw_parts(begin.cast::<u64>(), bytes / 8) };
    let mut seen = SEEN.lock().map_err(|_| "coverage state poisoned")?;
    seen.resize(counters.len(), false);
    let mut new = 0u64;
    let mut total = 0u64;
    for (c, s) in counters.iter().zip(seen.iter_mut()) {
        // read once: the counters are plain memory the instrumented code increments
        let v = unsafe { std::ptr::read_volatile(c) };
        if v != 0 && !*s {
            *s = true;
            new += 1;
        }
        total += u64::from(*s);
    }
    Ok(json!({ "new": new, "total": total }))
}
