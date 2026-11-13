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

/// Writes `bytes` as the entry's only variant `variant` (`testnet`, `mainnet` or `any`), its
/// XDR-JSON next to it, and `entry.json`. The variant record in `entry` supplies the
/// passphrase; file names and hashes are filled in here. An existing directory for the same
/// id is replaced only when it holds an entry of the same source class; a directory without
/// `entry.json` is never removed.
pub fn write_xdr_entry(
    corpus: &Path,
    mut entry: Entry,
    variant: &str,
    bytes: &[u8],
) -> Result<Entry> {
    let type_name = entry
        .xdr_type
        .clone()
        .context("an xdr entry needs xdr_type")?;
    // the id comes from operator input; checked before it becomes a path to remove
    check_entry_path(&entry.family, &entry.id)?;
    let rel_dir = format!("{ENTRIES_DIR}/{}/{}", entry.family, entry.id);
    let dir = corpus.join(&rel_dir);
    let entry_file = dir.join(ENTRY_FILE);
    if dir.exists() && !entry_file.is_file() {
        bail!(
            "{} exists but holds no {ENTRY_FILE}, so it is not an entry; refusing to remove it",
            dir.display()
        );
    }
    if entry_file.exists() {
        let old = read_entry(&entry_file)?;
        if old.source.class != entry.source.class {
            bail!(
                "{} already holds a {} entry; refusing to replace it with a {} one",
                dir.display(),
                old.source.class.as_str(),
                entry.source.class.as_str()
            );
        }
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;

    let json = to_xdr_json(&type_name, bytes, decode_limits(&type_name, bytes.len()))
        .with_context(|| format!("{}: rendering XDR-JSON", entry.id))?;
    let xdr_name = format!("{variant}.xdr");
    let json_name = format!("{variant}.expected.json");
    std::fs::write(dir.join(&xdr_name), bytes)?;
    std::fs::write(dir.join(&json_name), json.as_bytes())?;

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
    std::fs::write(&entry_file, text)
        .with_context(|| format!("writing {}", entry_file.display()))?;
    Ok(entry)
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
        let err = write_xdr_entry(corpus.path(), entry, "any", &[0, 0, 0, 0]).unwrap_err();
        assert!(err.to_string().contains("refusing to remove"), "{err}");
        assert!(dir.join("keep.txt").exists());
    }
}
