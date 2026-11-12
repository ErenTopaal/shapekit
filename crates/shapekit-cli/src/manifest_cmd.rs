//! `shapekit manifest validate`, `build` and `schema`.

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
fn generated_with(pins: &pins::Pins) -> Result<GeneratedWith> {
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
