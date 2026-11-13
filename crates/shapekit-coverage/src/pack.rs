//! `corpus-<version>.tar.zst` and `SHA256SUMS`: the committed corpus as one release asset that
//! SDKs pin by checksum. The same corpus always gives the same bytes.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use shapekit_manifest::{Manifest, sha256_hex};

use crate::CoverageError;

/// Fixed so a rebuild on another machine or zstd build gives the same archive. 19 is the
/// highest level that stays out of zstd's ultra range and its larger decoder windows.
pub const ZSTD_LEVEL: i32 = 19;

/// Every member's modification time. Any constant works; zero is what `--mtime=@0` gives.
pub const MTIME: u64 = 0;

pub const SUMS_FILE: &str = "SHA256SUMS";

/// Members an SDK's CI cannot run without; packing fails when one is missing.
const REQUIRED: [&str; 3] = [
    "manifest.json",
    "coverage/coverage.json",
    "coverage/COVERAGE.md",
];

/// Directories that must hold at least one file.
const REQUIRED_DIRS: [&str; 3] = ["entries/", "vectors/", "chains/"];

#[derive(Debug, Clone)]
pub struct PackReport {
    pub archive: PathBuf,
    pub sums: PathBuf,
    pub sha256: String,
    pub files: usize,
    pub uncompressed_bytes: u64,
    pub compressed_bytes: u64,
}

/// The archive name for a corpus version.
pub fn archive_name(corpus_version: &str) -> String {
    format!("corpus-{corpus_version}.tar.zst")
}

/// Packs every regular file under `corpus` into `<out>/corpus-<version>.tar.zst`, inside a
/// top-level `corpus-<version>/` directory, and writes `<out>/SHA256SUMS`.
///
/// Before packing it checks that every file the manifest names exists with its recorded hash,
/// so a release can never ship bytes the manifest does not vouch for.
pub fn pack(corpus: &Path, manifest: &Manifest, out: &Path) -> Result<PackReport, CoverageError> {
    let version = manifest.corpus_version.trim();
    if version.is_empty() || version.contains(['/', '\\']) || version.starts_with('.') {
        return Err(CoverageError::Pack(format!(
            "corpus_version {:?} cannot name an archive",
            manifest.corpus_version
        )));
    }
    verify_manifest_files(corpus, manifest)?;
    let files = collect(corpus)?;
    no_strays(&files, manifest)?;
    for required in REQUIRED {
        if !files.iter().any(|f| f == required) {
            return Err(CoverageError::Pack(format!(
                "{required} is missing from {}",
                corpus.display()
            )));
        }
    }
    for dir in REQUIRED_DIRS {
        if !files.iter().any(|f| f.starts_with(dir)) {
            return Err(CoverageError::Pack(format!(
                "no file under {dir} in {}",
                corpus.display()
            )));
        }
    }
    if !files
        .iter()
        .any(|f| f.starts_with("goldens/") && f.ends_with("/index.json"))
    {
        return Err(CoverageError::Pack(format!(
            "no goldens/<snapshot>/index.json in {}",
            corpus.display()
        )));
    }
    std::fs::create_dir_all(out).map_err(io(out))?;
    let name = archive_name(version);
    let archive = out.join(&name);
    let tmp = out.join(format!("{name}.partial"));
    let top = format!("corpus-{version}");
    let file = File::create(&tmp).map_err(io(&tmp))?;
    let mut encoder = zstd::stream::Encoder::new(file, ZSTD_LEVEL).map_err(io(&tmp))?;
    encoder.include_checksum(true).map_err(io(&tmp))?;
    let mut builder = tar::Builder::new(encoder);

    let mut uncompressed = 0u64;
    let mut dirs_written = std::collections::BTreeSet::new();
    append_dir(&mut builder, &top, &tmp)?;
    for rel in &files {
        // parents first, once each, so extraction never depends on implicit directories
        let parts: Vec<&str> = rel.split('/').collect();
        for depth in 1..parts.len() {
            let parent = parts[..depth].join("/");
            if dirs_written.insert(parent.clone()) {
                append_dir(&mut builder, &format!("{top}/{parent}"), &tmp)?;
            }
        }
        let full = corpus.join(rel);
        let bytes = std::fs::read(&full).map_err(io(&full))?;
        uncompressed += bytes.len() as u64;
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(MTIME);
        header.set_uid(0);
        header.set_gid(0);
        builder
            .append_data(&mut header, format!("{top}/{rel}"), bytes.as_slice())
            .map_err(io(&tmp))?;
    }
    let encoder = builder.into_inner().map_err(io(&tmp))?;
    let mut file = encoder.finish().map_err(io(&tmp))?;
    file.flush().map_err(io(&tmp))?;
    file.sync_all().map_err(io(&tmp))?;
    drop(file);
    std::fs::rename(&tmp, &archive).map_err(io(&archive))?;

    let bytes = std::fs::read(&archive).map_err(io(&archive))?;
    let sha256 = sha256_hex(&bytes);
    let sums = out.join(SUMS_FILE);
    // the two-space form is what `sha256sum -c` reads
    std::fs::write(&sums, format!("{sha256}  {name}\n")).map_err(io(&sums))?;
    Ok(PackReport {
        archive,
        sums,
        sha256,
        files: files.len(),
        uncompressed_bytes: uncompressed,
        compressed_bytes: bytes.len() as u64,
    })
}

fn append_dir<W: Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    tmp: &Path,
) -> Result<(), CoverageError> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_size(0);
    header.set_mode(0o755);
    header.set_mtime(MTIME);
    header.set_uid(0);
    header.set_gid(0);
    builder
        .append_data(&mut header, format!("{path}/"), std::io::empty())
        .map_err(io(tmp))
}

/// Every regular file under `corpus`, as sorted forward-slash relative paths. Anything that is
/// not a regular file or a directory is refused, because a symlink would pack differently on
/// every machine.
pub fn collect(corpus: &Path) -> Result<Vec<String>, CoverageError> {
    let mut out = Vec::new();
    for item in walkdir::WalkDir::new(corpus).follow_links(false) {
        let item =
            item.map_err(|e| CoverageError::Pack(format!("walking {}: {e}", corpus.display())))?;
        let ft = item.file_type();
        if ft.is_dir() {
            continue;
        }
        if !ft.is_file() {
            return Err(CoverageError::Pack(format!(
                "{} is neither a file nor a directory",
                item.path().display()
            )));
        }
        let rel = item
            .path()
            .strip_prefix(corpus)
            .expect("walkdir yields paths under its root");
        let mut parts = Vec::new();
        for c in rel.components() {
            match c {
                std::path::Component::Normal(p) => parts.push(p.to_str().ok_or_else(|| {
                    CoverageError::Pack(format!("{} is not valid utf-8", rel.display()))
                })?),
                _ => {
                    return Err(CoverageError::Pack(format!(
                        "{} is not a plain relative path",
                        rel.display()
                    )));
                }
            }
        }
        out.push(parts.join("/"));
    }
    // byte order, not locale order, so every machine agrees
    out.sort();
    Ok(out)
}

// a half-finished regeneration leaves files no entry vouches for; packing them would ship
// bytes the manifest says nothing about
fn no_strays(files: &[String], manifest: &Manifest) -> Result<(), CoverageError> {
    let mut known = std::collections::BTreeSet::new();
    let mut entry_dirs = std::collections::BTreeSet::new();
    let mut chains = std::collections::BTreeSet::new();
    for e in &manifest.entries {
        for (_, v) in e.variants.iter() {
            known.insert(v.xdr_file.as_str());
            if let Some(f) = &v.expected_json_file {
                known.insert(f.as_str());
            }
            if let Some(f) = &v.oracle_file {
                known.insert(f.as_str());
            }
        }
        if let Some(v) = &e.vector {
            known.insert(v.path.as_str());
        }
        entry_dirs.insert(format!("entries/{}/{}/", e.family, e.id));
        if let Some(c) = &e.source.chain {
            chains.insert(format!("chains/{c}/"));
        }
    }
    for f in files {
        let stray = if f.starts_with("entries/") {
            !known.contains(f.as_str())
                && !(f.ends_with("/entry.json")
                    && entry_dirs.contains(&f[..f.len() - "entry.json".len()]))
        } else if f.starts_with("vectors/") {
            !known.contains(f.as_str())
        } else if f.starts_with("chains/") {
            !chains.iter().any(|c| f.starts_with(c.as_str()))
        } else {
            // outside the entry directories only a closed set of members ships, so a stray
            // anywhere else cannot ride along unverified
            !outside_member(f)
        };
        if stray {
            return Err(CoverageError::Pack(format!(
                "{f} belongs to no manifest entry; rebuild the manifest or remove it"
            )));
        }
    }
    Ok(())
}

/// The members allowed outside `entries/`, `vectors/` and `chains/`: the required files, the
/// golden agreement exceptions and one `index.json` per golden snapshot.
fn outside_member(f: &str) -> bool {
    if REQUIRED.contains(&f) || f == "goldens/agree-exceptions.toml" {
        return true;
    }
    let parts: Vec<&str> = f.split('/').collect();
    matches!(parts.as_slice(), ["goldens", snapshot, "index.json"] if !snapshot.is_empty())
}

fn verify_manifest_files(corpus: &Path, manifest: &Manifest) -> Result<(), CoverageError> {
    let mut refs: Vec<(&str, &str, &str)> = Vec::new();
    // a file named without its hash would ship unverified, so the pair is required
    let pair = |id: &str, what: &str, f: &Option<String>, h: &Option<String>| match (f, h) {
        (Some(_), None) => Err(CoverageError::Pack(format!(
            "{id}: names an {what} file without its sha256, so packing cannot vouch for it"
        ))),
        (None, Some(_)) => Err(CoverageError::Pack(format!(
            "{id}: records an {what} sha256 without its file"
        ))),
        _ => Ok(()),
    };
    for e in &manifest.entries {
        for (_, v) in e.variants.iter() {
            pair(
                &e.id,
                "expected_json",
                &v.expected_json_file,
                &v.expected_json_sha256,
            )?;
            pair(&e.id, "oracle", &v.oracle_file, &v.oracle_sha256)?;
            refs.push((&e.id, &v.xdr_file, &v.xdr_sha256));
            if let (Some(f), Some(h)) = (&v.expected_json_file, &v.expected_json_sha256) {
                refs.push((&e.id, f, h));
            }
            if let (Some(f), Some(h)) = (&v.oracle_file, &v.oracle_sha256) {
                refs.push((&e.id, f, h));
            }
        }
        if let Some(v) = &e.vector {
            refs.push((&e.id, &v.path, &v.sha256));
        }
    }
    for (id, file, sha) in refs {
        shapekit_manifest::read_verified(corpus, file, Some(sha))
            .map_err(|e| CoverageError::Pack(format!("{id}: {e}")))?;
    }
    Ok(())
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> CoverageError + '_ {
    move |source| CoverageError::Io {
        path: path.to_path_buf(),
        source,
    }
}
