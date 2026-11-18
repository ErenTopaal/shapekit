//! Writing and clearing `corpus/entries/<family>/<entry-id>/` directories for xdr entries.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use shapekit_manifest::build::{ENTRIES_DIR, ENTRY_FILE};
use shapekit_manifest::xdr::{render_json, to_xdr_json};
use shapekit_manifest::{Entry, decode_limits, sha256_hex};
use shapekit_registry::SourceClass;
use walkdir::WalkDir;

/// Removes every entry directory whose `entry.json` has source class `class`, apart from the
/// ids in `keep`, and returns the removed ids, sorted. Staging directories (dot-prefixed,
/// see [`shapekit_manifest::build::build`]) are not entries and are left alone.
pub fn remove_entries_of_class(
    corpus: &Path,
    class: SourceClass,
    keep: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let root = corpus.join(ENTRIES_DIR);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut removed = Vec::new();
    // no min_depth: walkdir does not offer the directories above it to filter_entry, and
    // a staging directory sits at entries/<family>/.staged-*
    let walk = WalkDir::new(&root)
        .max_depth(3)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'));
    for item in walk {
        let item = item.with_context(|| format!("walking {}", root.display()))?;
        if item.depth() != 3 || item.file_name() != ENTRY_FILE {
            continue;
        }
        let entry = read_entry(item.path())?;
        if entry.source.class == class && !keep.contains(&entry.id) {
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

/// Moves the directory `staged` to `dir`, replacing what is there in one step, so no reader
/// ever finds `dir` missing or half written. An existing `dir` is exchanged with `staged`
/// atomically (Linux `renameat2` with `RENAME_EXCHANGE`, macOS `renamex_np` with
/// `RENAME_SWAP`), and the old entry, which is then at `staged`, is removed; a new one is
/// renamed into place without replacing anything that appears there meanwhile. On an error
/// before the exchange, `dir` is untouched; on one after it, `dir` holds the new entry and
/// the error says the old copy at `staged` was not removed. `staged` must be on the same
/// filesystem as `dir`.
pub fn swap_in(staged: &Path, dir: &Path) -> Result<()> {
    let parent = dir.parent().context("an entry directory has a parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    if !dir.exists() {
        return shapekit_process::rename::no_replace(staged, dir)
            .with_context(|| format!("moving {} to {}", staged.display(), dir.display()));
    }
    shapekit_process::rename::exchange(staged, dir)
        .with_context(|| format!("exchanging {} with {}", staged.display(), dir.display()))?;
    std::fs::remove_dir_all(staged).with_context(|| {
        format!(
            "{} now holds the new entry; removing the replaced one from {}",
            dir.display(),
            staged.display()
        )
    })
}

/// Writes `bytes` as the entry's only variant `variant` (`testnet`, `mainnet` or `any`), its
/// XDR-JSON next to it, and `entry.json`. The variant record in `entry` supplies the
/// passphrase; file names and hashes are filled in here. An existing directory for the same
/// id is replaced only when it holds an entry of the same source class; a directory without
/// `entry.json` is never removed. Everything is rendered and written to a staging directory
/// beside it first and then exchanged with it in one step ([`swap_in`]), so a failure leaves
/// the existing entry as it was. A signal removes the staging directory.
pub fn write_xdr_entry(corpus: &Path, entry: Entry, variant: &str, bytes: &[u8]) -> Result<Entry> {
    let r = render_xdr_entry(entry, variant, bytes)?;
    check_replaceable(corpus, &r)?;
    let dir = corpus.join(&r.rel_dir);
    let parent = dir.parent().context("an entry directory has a parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let staged = shapekit_process::scratch::scratch_in(parent, ".staged-")
        .with_context(|| format!("creating a directory in {}", parent.display()))?;
    write_files(staged.path(), &r)?;
    swap_in(staged.path(), &dir)?;
    // the staged directory is the entry now, or held the old one and was removed: dropping
    // the scratch record must not touch that path again
    staged.keep();
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

    // the swap exchanges the two directories in one step, so a failure never leaves the
    // entry missing, and nothing is removed before the new copy is in place
    #[test]
    fn swap_in_exchanges_and_a_failure_keeps_the_old_entry() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("entries/f/f.x");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("entry.json"), "old").unwrap();
        // a staged directory that does not exist: the exchange fails, the entry stays
        let missing = t.path().join("entries/f/.staged-missing");
        assert!(swap_in(&missing, &dir).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.join("entry.json")).unwrap(),
            "old"
        );
        let staged = t.path().join("entries/f/.staged-1");
        std::fs::create_dir(&staged).unwrap();
        std::fs::write(staged.join("entry.json"), "new").unwrap();
        swap_in(&staged, &dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("entry.json")).unwrap(),
            "new"
        );
        assert!(!staged.exists(), "the replaced copy is removed");
        // a new entry is moved, and the staged path is gone
        let staged = t.path().join("entries/f/.staged-2");
        std::fs::create_dir(&staged).unwrap();
        let fresh = t.path().join("entries/g/g.y");
        swap_in(&staged, &fresh).unwrap();
        assert!(fresh.is_dir() && !staged.exists());
    }

    #[test]
    fn removing_a_class_skips_staging_directories_and_kept_ids() {
        let corpus = tempfile::tempdir().unwrap();
        write_xdr_entry(corpus.path(), scval_entry(), "any", &[0, 0, 0, 1]).unwrap();
        let family = corpus.path().join("entries/history");
        let staged = family.join(".staged-x");
        std::fs::create_dir(&staged).unwrap();
        std::fs::copy(
            family.join("history.x/entry.json"),
            staged.join("entry.json"),
        )
        .unwrap();
        let class = scval_entry().source.class;
        let keep = BTreeSet::from(["history.x".to_owned()]);
        assert!(
            remove_entries_of_class(corpus.path(), class, &keep)
                .unwrap()
                .is_empty()
        );
        assert!(family.join("history.x").is_dir());
        assert_eq!(
            remove_entries_of_class(corpus.path(), class, &BTreeSet::new()).unwrap(),
            ["history.x"]
        );
        assert!(
            staged.join("entry.json").is_file(),
            "a staging copy is not an entry"
        );
    }
}
