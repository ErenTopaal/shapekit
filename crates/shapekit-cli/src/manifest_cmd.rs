//! `shapekit manifest validate`, `build`, `schema`, `rerender` and `arm-caps`.

use std::path::Path;

use anyhow::{Context, Result, bail};
use shapekit_manifest::{
    CoreImage, GeneratedWith, XdrDefinitions, build, manifest_schema, validate,
};
use shapekit_registry::Registry;

use crate::pins;

pub fn validate_file(file: &Path, registry_dir: &Path) -> Result<bool> {
    let manifest = build::load(file).with_context(|| format!("loading {}", file.display()))?;
    let registry = Registry::load(registry_dir)
        .with_context(|| format!("loading the registry in {}", registry_dir.display()))?;
    let root = file.parent().unwrap_or(Path::new("."));
    let problems = validate(&manifest, &registry, root);
    for p in &problems {
        println!("{p}");
    }
    if problems.is_empty() {
        println!("manifest ok: {} entries", manifest.entries.len());
        Ok(true)
    } else {
        println!(
            "{} problems in {} entries",
            problems.len(),
            manifest.entries.len()
        );
        Ok(false)
    }
}

pub fn build_file(corpus: &Path, pins_file: &Path, corpus_version: &str) -> Result<bool> {
    let pins = pins::load(pins_file)?;
    let generated_with = generated_with(&pins)?;
    let manifest = build::build(corpus, corpus_version, generated_with)
        .with_context(|| format!("building the manifest from {}", corpus.display()))?;
    let out = corpus.join("manifest.json");
    std::fs::write(&out, build::render(&manifest))
        .with_context(|| format!("writing {}", out.display()))?;
    println!(
        "wrote {} with {} entries",
        out.display(),
        manifest.entries.len()
    );
    Ok(true)
}

// the header names the stack the corpus was produced with; the xdr crate is the one linked
// into this binary, and must agree with what pins.toml says was used
pub(crate) fn generated_with(pins: &pins::Pins) -> Result<GeneratedWith> {
    let linked = shapekit_manifest::STELLAR_XDR_VERSION;
    if !pins
        .crates
        .iter()
        .any(|c| c.name == "stellar-xdr" && c.version == linked.pkg)
    {
        bail!(
            "pins.toml does not pin stellar-xdr {}, the version this build links",
            linked.pkg
        );
    }
    let xdr = pins
        .git
        .iter()
        .find(|g| g.name == "stellar-xdr")
        .context("pins.toml has no stellar-xdr git pin")?;
    let commit = xdr
        .sha
        .clone()
        .context("the stellar-xdr git pin has no sha")?;
    if commit != linked.xdr.trim() {
        bail!(
            "pins.toml pins xdr definitions {commit}, but stellar-xdr {} was built from {}",
            linked.pkg,
            linked.xdr.trim()
        );
    }
    let tag = xdr
        .git_ref
        .as_deref()
        .and_then(|r| r.strip_prefix("refs/tags/"))
        .context("the stellar-xdr git pin has no tag ref")?
        .to_string();
    let core = pins
        .images
        .iter()
        .find(|i| i.name == "stellar-core")
        .context("pins.toml has no stellar-core image")?;
    // the host's major version tracks the protocol, like the xdr crate's
    let xdr_major = linked.pkg.split('.').next().unwrap_or_default();
    let hosts: Vec<&String> = pins
        .crates
        .iter()
        .filter(|c| c.name == "soroban-env-host" && c.version.split('.').next() == Some(xdr_major))
        .map(|c| &c.version)
        .collect();
    let [host] = hosts.as_slice() else {
        bail!(
            "expected one soroban-env-host {xdr_major}.x pin, found {}",
            hosts.len()
        );
    };
    Ok(GeneratedWith {
        stellar_xdr: linked.pkg.to_string(),
        xdr_definitions: XdrDefinitions { tag, commit },
        core_image: CoreImage {
            repository: core.repository.clone(),
            tag: core.tag.clone(),
            digest: core.digest.clone(),
        },
        soroban_env_host: (*host).clone(),
    })
}

pub fn write_schema(out: &Path) -> Result<bool> {
    std::fs::write(out, manifest_schema()).with_context(|| format!("writing {}", out.display()))?;
    println!("wrote {}", out.display());
    Ok(true)
}

/// Every `entry.json` under `<corpus>/entries`, sorted. A walk error is an error, and a
/// corpus without an entries directory is refused, so a wrong path cannot pass as empty.
fn entry_files(corpus: &Path) -> Result<Vec<std::path::PathBuf>> {
    let entries_dir = corpus.join("entries");
    if !entries_dir.is_dir() {
        bail!("{} is not a directory", entries_dir.display());
    }
    let mut files = Vec::new();
    for e in walkdir::WalkDir::new(&entries_dir) {
        let e = e.with_context(|| format!("walking {}", entries_dir.display()))?;
        if e.file_type().is_file() && e.file_name() == "entry.json" {
            files.push(e.into_path());
        }
    }
    files.sort();
    Ok(files)
}

fn read_entry(file: &Path) -> Result<shapekit_manifest::Entry> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", file.display()))
}

fn write_entry(file: &Path, entry: &shapekit_manifest::Entry) -> Result<()> {
    let out = shapekit_manifest::xdr::render_json(&serde_json::to_value(entry)?);
    std::fs::write(file, out).with_context(|| format!("writing {}", file.display()))
}

/// Rewrites each entry's expected XDR-JSON files from its XDR files with the current
/// renderer, and the sha256 the entry file records. Prints every file that changed. Every
/// variant of an entry is rendered before any of its files is written.
pub fn rerender(corpus: &Path, dry_run: bool) -> Result<bool> {
    let files = entry_files(corpus)?;
    let mut changed = 0;
    for entry_file in &files {
        let mut entry = read_entry(entry_file)?;
        let Some(type_name) = entry.xdr_type.clone() else {
            continue;
        };
        let mut writes = Vec::new();
        for slot in [
            &mut entry.variants.testnet,
            &mut entry.variants.mainnet,
            &mut entry.variants.any,
        ] {
            let Some(v) = slot.as_mut() else { continue };
            let Some(json_file) = v.expected_json_file.clone() else {
                continue;
            };
            let bytes = std::fs::read(corpus.join(&v.xdr_file))
                .with_context(|| format!("reading {}", v.xdr_file))?;
            let limits = shapekit_manifest::decode_limits(&type_name, bytes.len());
            let json = shapekit_manifest::xdr::to_xdr_json(&type_name, &bytes, limits)
                .with_context(|| format!("{}: rendering {}", entry.id, v.xdr_file))?;
            let path = corpus.join(&json_file);
            let old = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            if old == json {
                continue;
            }
            v.expected_json_sha256 = Some(shapekit_manifest::sha256_hex(json.as_bytes()));
            writes.push((path, json));
        }
        for (path, _) in &writes {
            println!("{}", path.display());
        }
        changed += writes.len();
        if writes.is_empty() || dry_run {
            continue;
        }
        for (path, json) in &writes {
            std::fs::write(path, json.as_bytes())
                .with_context(|| format!("writing {}", path.display()))?;
        }
        write_entry(entry_file, &entry)?;
    }
    println!(
        "{changed} expected files {} in {} ({} entries read)",
        if dry_run { "would change" } else { "rewritten" },
        corpus.display(),
        files.len()
    );
    Ok(true)
}

/// Sets the caps of every entry that records the arms it witnesses to the CAPs those arms came
/// with (`registry/caps.toml` xdr_additions), or PRE-CAP when none did. With `check`, lists
/// the entries whose caps differ and fails instead of writing.
pub fn arm_caps(corpus: &Path, registry_dir: &Path, check: bool) -> Result<bool> {
    let registry = Registry::load(registry_dir)
        .with_context(|| format!("loading the registry in {}", registry_dir.display()))?;
    let table = shapekit_manifest::arm_caps::ArmCaps::new(&registry);
    let files = entry_files(corpus)?;
    let mut with_arms = 0;
    let mut differ = 0;
    for file in &files {
        let mut entry = read_entry(file)?;
        if entry.arms.is_empty() {
            continue;
        }
        with_arms += 1;
        let caps = table.for_arms(&entry.arms).map_err(|unknown| {
            anyhow::anyhow!(
                "{}: arms {} are not union arms of the xdr definitions",
                entry.id,
                unknown.join(", ")
            )
        })?;
        if caps == entry.caps {
            continue;
        }
        differ += 1;
        println!(
            "{}: [{}] -> [{}]",
            entry.id,
            entry.caps.join(", "),
            caps.join(", ")
        );
        if !check {
            entry.caps = caps;
            write_entry(file, &entry)?;
        }
    }
    println!(
        "{with_arms} of {} entries record their arms; {differ} {}",
        files.len(),
        if check { "differ" } else { "retagged" }
    );
    Ok(!check || differ == 0)
}
