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

fn read_entry(path: &Path) -> Result<Entry> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

/// Writes `bytes` as the entry's only variant `variant` (`testnet`, `mainnet` or `any`), its
/// XDR-JSON next to it, and `entry.json`. The variant record in `entry` supplies the
/// passphrase; file names and hashes are filled in here. An existing directory for the same
/// id is replaced only when it holds an entry of the same source class.
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
    let rel_dir = format!("{ENTRIES_DIR}/{}/{}", entry.family, entry.id);
    let dir = corpus.join(&rel_dir);
    let entry_file = dir.join(ENTRY_FILE);
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
