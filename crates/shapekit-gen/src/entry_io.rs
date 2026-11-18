//! Writing and clearing `corpus/entries/<family>/<entry-id>/` directories for xdr entries.

use std::path::Path;

use anyhow::{Context, Result, bail};
use shapekit_manifest::build::{ENTRIES_DIR, ENTRY_FILE};
use shapekit_manifest::xdr::{render_json, to_xdr_json};
use shapekit_manifest::{Entry, decode_limits, sha256_hex};
use shapekit_registry::SourceClass;
use walkdir::WalkDir;

/// Removes every entry directory whose `entry.json` has source class `class` and returns the
/// removed ids, sorted.
pub fn remove_entries_of_class(corpus: &Path, class: SourceClass) -> Result<Vec<String>> {
    let root = corpus.join(ENTRIES_DIR);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut removed = Vec::new();
    for item in WalkDir::new(&root)
        .min_depth(3)
        .max_depth(3)
        .sort_by_file_name()
    {
        let item = item.with_context(|| format!("walking {}", root.display()))?;
        if item.file_name() != ENTRY_FILE {
            continue;
        }
        let entry = read_entry(item.path())?;
        if entry.source.class == class {
            let dir = item.path().parent().expect("an entry file has a parent");
            std::fs::remove_dir_all(dir).with_context(|| format!("removing {}", dir.display()))?;
            removed.push(entry.id);
        }
    }
    removed.sort();
    Ok(removed)
}

/// The recipe capture slug rule: non-empty lowercase kebab-case, so a slug can never name a
/// path outside its family's directory.
pub fn check_slug(slug: &str) -> Result<()> {
    if slug.is_empty()
        || !slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        bail!("capture slug {slug:?} must be lowercase kebab-case");
    }
    Ok(())
}

/// Checks that `family` and `id` make a directory under `entries/` and nowhere else: the
/// family is dot-separated slugs and the id is `<family>.<slug>`.
pub fn check_entry_path(family: &str, id: &str) -> Result<()> {
    for part in family.split('.') {
        check_slug(part).with_context(|| format!("family {family:?}"))?;
    }
    let rest = id
        .strip_prefix(family)
        .and_then(|r| r.strip_prefix('.'))
        .with_context(|| format!("entry id {id:?} does not start with its family {family}."))?;
    check_slug(rest).with_context(|| format!("entry id {id:?}"))
}

fn read_entry(path: &Path) -> Result<Entry> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

/// One xdr entry rendered in memory, ready to be written: its directory relative to the
/// corpus, every file it holds, and the entry with file names and hashes filled in.
#[derive(Debug)]
pub struct RenderedEntry {
    pub rel_dir: String,
    pub files: Vec<(String, Vec<u8>)>,
    pub entry: Entry,
}

/// Renders `bytes` as the entry's only variant `variant` (`testnet`, `mainnet` or `any`), its
/// XDR-JSON next to it, and `entry.json`, without touching the disk. The variant record in
/// `entry` supplies the passphrase; file names and hashes are filled in here.
pub fn render_xdr_entry(mut entry: Entry, variant: &str, bytes: &[u8]) -> Result<RenderedEntry> {
    let type_name = entry
        .xdr_type
        .clone()
        .context("an xdr entry needs xdr_type")?;
    // the id comes from operator input; checked before it becomes a path to remove
    check_entry_path(&entry.family, &entry.id)?;
    let rel_dir = format!("{ENTRIES_DIR}/{}/{}", entry.family, entry.id);
    let json = to_xdr_json(&type_name, bytes, decode_limits(&type_name, bytes.len()))
        .with_context(|| format!("{}: rendering XDR-JSON", entry.id))?;
    let xdr_name = format!("{variant}.xdr");
    let json_name = format!("{variant}.expected.json");
    let slot = match variant {
        "testnet" => &mut entry.variants.testnet,
        "mainnet" => &mut entry.variants.mainnet,
        "any" => &mut entry.variants.any,
        other => bail!("unknown variant {other}"),
    };
    let v = slot
        .as_mut()
        .with_context(|| format!("{}: the {variant} variant record is missing", entry.id))?;
    v.xdr_file = format!("{rel_dir}/{xdr_name}");
    v.xdr_sha256 = sha256_hex(bytes);
    v.expected_json_file = Some(format!("{rel_dir}/{json_name}"));
    v.expected_json_sha256 = Some(sha256_hex(json.as_bytes()));
    let text = render_json(&serde_json::to_value(&entry)?);
    Ok(RenderedEntry {
        rel_dir,
        files: vec![
            (xdr_name, bytes.to_vec()),
            (json_name, json.into_bytes()),
            (ENTRY_FILE.to_owned(), text.into_bytes()),
        ],
        entry,
    })
}

/// Refuses to replace what is at `r`'s directory unless it is an entry of the same source
/// class: a directory without `entry.json` is never removed.
pub fn check_replaceable(corpus: &Path, r: &RenderedEntry) -> Result<()> {
    let dir = corpus.join(&r.rel_dir);
    let entry_file = dir.join(ENTRY_FILE);
    if dir.exists() && !entry_file.is_file() {
        bail!(
            "{} exists but holds no {ENTRY_FILE}, so it is not an entry; refusing to remove it",
            dir.display()
        );
    }
    if entry_file.exists() {
        let old = read_entry(&entry_file)?;
        if old.source.class != r.entry.source.class {
            bail!(
                "{} already holds a {} entry; refusing to replace it with a {} one",
                dir.display(),
                old.source.class.as_str(),
                r.entry.source.class.as_str()
            );
        }
    }
    Ok(())
}

/// Writes `r`'s files into `dir`, which must exist and be empty.
pub fn write_files(dir: &Path, r: &RenderedEntry) -> Result<()> {
    for (name, bytes) in &r.files {
        let path = dir.join(name);
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Moves the directory `staged` to `dir`, replacing what is there. The old directory is moved
/// aside first and put back if the move fails, so `dir` is either the old entry or the new
/// one, never a mix.
pub fn swap_in(staged: &Path, dir: &Path) -> Result<()> {
    let parent = dir.parent().context("an entry directory has a parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    if !dir.exists() {
        return std::fs::rename(staged, dir)
            .with_context(|| format!("moving {} to {}", staged.display(), dir.display()));
    }
    // renaming a directory onto an empty one replaces it, so the aside slot is a fresh empty
    // directory that is removed with whatever it holds once the swap is done
    let aside = tempfile::Builder::new()
        .prefix(".replaced-")
        .tempdir_in(parent)
        .with_context(|| format!("creating a directory in {}", parent.display()))?;
    std::fs::rename(dir, aside.path())
        .with_context(|| format!("moving {} aside", dir.display()))?;
    if let Err(e) = std::fs::rename(staged, dir) {
        std::fs::rename(aside.path(), dir)
            .with_context(|| format!("putting {} back after: {e}", dir.display()))?;
        return Err(e).with_context(|| format!("moving {} to {}", staged.display(), dir.display()));
    }
    aside
        .close()
        .with_context(|| format!("removing the replaced {}", dir.display()))
}

/// Writes `bytes` as the entry's only variant `variant` (`testnet`, `mainnet` or `any`), its
/// XDR-JSON next to it, and `entry.json`. The variant record in `entry` supplies the
/// passphrase; file names and hashes are filled in here. An existing directory for the same
/// id is replaced only when it holds an entry of the same source class; a directory without
/// `entry.json` is never removed. Everything is rendered and written beside the directory
/// first, so a failure leaves the existing entry as it was.
pub fn write_xdr_entry(corpus: &Path, entry: Entry, variant: &str, bytes: &[u8]) -> Result<Entry> {
    let r = render_xdr_entry(entry, variant, bytes)?;
    check_replaceable(corpus, &r)?;
    let dir = corpus.join(&r.rel_dir);
    let parent = dir.parent().context("an entry directory has a parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let staged = tempfile::Builder::new()
        .prefix(".staged-")
        .tempdir_in(parent)
        .with_context(|| format!("creating a directory in {}", parent.display()))?;
    write_files(staged.path(), &r)?;
    swap_in(staged.path(), &dir)?;
    Ok(r.entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_paths_stay_under_their_family() {
        check_entry_path(
            "cap23.claimable-balances",
            "cap23.claimable-balances.p30-x-1",
        )
        .unwrap();
        check_entry_path("history", "history.mainnet-big-ledger-meta").unwrap();
        for (family, id) in [
            ("history", "history./../../etc"),
            ("history", "history.a/b"),
            ("history", "other.slug"),
            ("history", "history."),
            ("history", "history.Upper"),
            ("../x", "../x.slug"),
            ("a..b", "a..b.slug"),
        ] {
            assert!(check_entry_path(family, id).is_err(), "{family} {id}");
        }
    }

    #[test]
    fn a_directory_without_entry_json_is_never_removed() {
        let corpus = tempfile::tempdir().unwrap();
        let dir = corpus.path().join("entries/history/history.x");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("keep.txt"), b"not an entry").unwrap();
        let entry: Entry = serde_json::from_value(serde_json::json!({
            "id": "history.x",
            "kind": "xdr",
            "family": "history",
            "caps": [],
            "protocol": {"introduced": 1, "valid_min": 1},
            "xdr_type": "ScVal",
            "source": {"class": "mainnet-history"},
            "synthetic": false,
            "passphrase_dependent": false,
            "variants": {"any": {"xdr_file": "", "xdr_sha256": ""}},
            "expected": {"validity": "valid", "decode": "must-accept", "roundtrip": true},
            "flags": [],
            "fields_exercised": [],
            "linked_issues": []
        }))
        .unwrap();
        // ScVal void
        let err = write_xdr_entry(corpus.path(), entry, "any", &[0, 0, 0, 1]).unwrap_err();
        assert!(err.to_string().contains("refusing to remove"), "{err}");
        assert!(dir.join("keep.txt").exists());
    }

    fn scval_entry() -> Entry {
        serde_json::from_value(serde_json::json!({
            "id": "history.x",
            "kind": "xdr",
            "family": "history",
            "caps": [],
            "protocol": {"introduced": 1, "valid_min": 1},
            "xdr_type": "ScVal",
            "source": {"class": "mainnet-history"},
            "synthetic": false,
            "passphrase_dependent": false,
            "variants": {"any": {"xdr_file": "", "xdr_sha256": ""}},
            "expected": {"validity": "valid", "decode": "must-accept", "roundtrip": true},
            "flags": [],
            "fields_exercised": [],
            "linked_issues": []
        }))
        .unwrap()
    }

    #[test]
    fn a_failed_rewrite_leaves_the_entry_as_it_was() {
        let corpus = tempfile::tempdir().unwrap();
        // ScVal void, then bytes that are no ScVal, so rendering the XDR-JSON fails
        write_xdr_entry(corpus.path(), scval_entry(), "any", &[0, 0, 0, 1]).unwrap();
        let dir = corpus.path().join("entries/history/history.x");
        let before: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(write_xdr_entry(corpus.path(), scval_entry(), "any", &[9, 9, 9]).is_err());
        let after: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(before, after);
        assert_eq!(std::fs::read(dir.join("any.xdr")).unwrap(), [0, 0, 0, 1]);
        // and a rewrite that succeeds replaces it, leaving nothing beside it
        let t = [0, 0, 0, 0, 0, 0, 0, 1];
        write_xdr_entry(corpus.path(), scval_entry(), "any", &t).unwrap();
        assert_eq!(std::fs::read(dir.join("any.xdr")).unwrap(), t);
        let siblings: Vec<_> = std::fs::read_dir(corpus.path().join("entries/history"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(siblings, ["history.x"]);
    }
}
