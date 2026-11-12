//! pins.toml model, parsing and verification.

pub mod check;
pub mod remote;
pub mod verify;

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pins {
    pub schema: u32,
    #[serde(default, rename = "toolchain")]
    pub toolchains: Vec<ToolchainPin>,
    #[serde(default, rename = "image")]
    pub images: Vec<ImagePin>,
    #[serde(default)]
    pub git: Vec<GitPin>,
    #[serde(default, rename = "crate")]
    pub crates: Vec<CratePin>,
    #[serde(default)]
    pub npm: Vec<NpmPin>,
    #[serde(default)]
    pub gomod: Vec<GoModPin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Toolchain {
    Rust,
    Go,
    Node,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainPin {
    pub name: Toolchain,
    pub version: String,
    pub artifact: String,
    pub sha256: String,
    pub purpose: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImagePin {
    pub name: String,
    pub repository: String,
    pub tag: String,
    pub digest: String,
    pub purpose: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitPin {
    pub name: String,
    pub url: String,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub sha: Option<String>,
    pub purpose: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CratePin {
    pub name: String,
    pub version: String,
    pub checksum: String,
    pub purpose: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpmPin {
    pub name: String,
    pub version: String,
    pub integrity: String,
    pub purpose: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoModPin {
    pub module: String,
    pub version: String,
    pub origin_hash: String,
    pub purpose: String,
}

pub fn load(path: &Path) -> Result<Pins> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("in {}", path.display()))
}

pub fn parse(text: &str) -> Result<Pins> {
    let pins: Pins = toml::from_str(text)?;
    validate(&pins)?;
    Ok(pins)
}

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn require_purpose(what: &str, purpose: &str) -> Result<()> {
    if purpose.trim().is_empty() {
        bail!("{what}: purpose is empty");
    }
    Ok(())
}

fn require_unique(seen: &mut HashSet<String>, what: &str, key: String) -> Result<()> {
    if !seen.insert(key.clone()) {
        bail!("{what} {key} is listed twice");
    }
    Ok(())
}

// format checks run before any network call so a typo is reported as a typo,
// not as an upstream mismatch.
fn validate(pins: &Pins) -> Result<()> {
    if pins.schema != SCHEMA_VERSION {
        bail!(
            "unsupported schema {}, this build reads schema {SCHEMA_VERSION}",
            pins.schema
        );
    }

    let mut seen = HashSet::new();
    for t in &pins.toolchains {
        let what = format!("toolchain {:?}", t.name).to_lowercase();
        require_unique(&mut seen, "toolchain", format!("{:?}", t.name))?;
        if !is_lower_hex(&t.sha256, 64) {
            bail!("{what}: sha256 must be 64 lowercase hex characters");
        }
        if t.version.is_empty() || t.artifact.is_empty() {
            bail!("{what}: version and artifact are required");
        }
        require_purpose(&what, &t.purpose)?;
    }

    let mut seen = HashSet::new();
    for i in &pins.images {
        let what = format!("image {}", i.name);
        require_unique(&mut seen, "image", i.name.clone())?;
        // only docker hub is spoken to, a registry host in the path would be ignored
        let first = i.repository.split('/').next().unwrap_or_default();
        if i.repository.split('/').count() != 2 || first.contains('.') || first.contains(':') {
            bail!("{what}: repository must be a docker hub path like owner/name");
        }
        match i.digest.strip_prefix("sha256:") {
            Some(hex) if is_lower_hex(hex, 64) => {}
            _ => bail!("{what}: digest must be sha256: followed by 64 lowercase hex characters"),
        }
        if i.tag.is_empty() {
            bail!("{what}: tag is required");
        }
        require_purpose(&what, &i.purpose)?;
    }

    let mut seen = HashSet::new();
    for g in &pins.git {
        let what = format!("git {}", g.name);
        require_unique(&mut seen, "git", g.name.clone())?;
        if !g.url.starts_with("https://") {
            bail!("{what}: url must be https");
        }
        if let Some(r) = &g.git_ref
            && !r.starts_with("refs/")
        {
            bail!("{what}: ref must be a full ref name such as refs/tags/v1.0.0");
        }
        match &g.sha {
            Some(sha) if !is_lower_hex(sha, 40) => {
                bail!("{what}: sha must be 40 lowercase hex characters")
            }
            None if g.git_ref.is_none() => bail!("{what}: needs a ref, a sha, or both"),
            _ => {}
        }
        require_purpose(&what, &g.purpose)?;
    }

    let mut seen = HashSet::new();
    for c in &pins.crates {
        let what = format!("crate {}@{}", c.name, c.version);
        require_unique(&mut seen, "crate", format!("{}@{}", c.name, c.version))?;
        if !is_lower_hex(&c.checksum, 64) {
            bail!("{what}: checksum must be 64 lowercase hex characters");
        }
        require_purpose(&what, &c.purpose)?;
    }

    let mut seen = HashSet::new();
    for n in &pins.npm {
        let what = format!("npm {}@{}", n.name, n.version);
        require_unique(&mut seen, "npm", format!("{}@{}", n.name, n.version))?;
        if !n.integrity.starts_with("sha512-") {
            bail!("{what}: integrity must be an sha512 subresource integrity string");
        }
        require_purpose(&what, &n.purpose)?;
    }

    let mut seen = HashSet::new();
    for m in &pins.gomod {
        let what = format!("gomod {}@{}", m.module, m.version);
        require_unique(&mut seen, "gomod", format!("{}@{}", m.module, m.version))?;
        if !m.version.starts_with('v') {
            bail!("{what}: version must start with v");
        }
        if !is_lower_hex(&m.origin_hash, 40) {
            bail!("{what}: origin_hash must be 40 lowercase hex characters");
        }
        require_purpose(&what, &m.purpose)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
schema = 1

[[toolchain]]
name = "rust"
version = "1.99.1"
artifact = "channel-rust-1.99.1.toml"
sha256 = "fb152be80975c194aa5fc684b2aec3a6e14e2b855efe2faa4e5f8ae6e590f463"
purpose = "workspace toolchain"

[[image]]
name = "stellar-core"
repository = "stellar/stellar-core"
tag = "30.0.0-3642.0e49751aa.noble"
digest = "sha256:eb7a7c81a29b8665c8878d55e1b3979f0d7527426350c4ff258a9c7647ae5c51"
purpose = "standalone core"

[[git]]
name = "stellar-rpc"
url = "https://github.com/stellar/stellar-rpc"
ref = "refs/tags/v30.0.0"
sha = "13e53516633c3ce1281c7edbacb607107d3a4e55"
purpose = "rpc shapes"

[[git]]
name = "wallet-backend-issue716-prefix"
url = "https://github.com/stellar/wallet-backend"
sha = "79ea2acd1dc65a07166133cc04b12e76db314db0"
purpose = "parent of pr 693"

[[git]]
name = "actions-checkout"
url = "https://github.com/actions/checkout"
ref = "refs/tags/v7"
purpose = "ci checkout"

[[crate]]
name = "toml"
version = "1.2.1+spec-1.1.0"
checksum = "d5755c51863cdac711f1956baf8eb9588e060755f7d3c4c979e70ea1e1f32cd7"
purpose = "config files"

[[npm]]
name = "@stellar/stellar-sdk"
version = "17.4.0"
integrity = "sha512-IX6gi62yWQ+6Oz56LsVEEeSViSOWJL3BkttftwVmVsu+62nLTWNVsCmTWEzwgNLT/q5HGP25OnlU+hhy2E4wwA=="
purpose = "js testee"

[[gomod]]
module = "github.com/stellar/go-stellar-sdk"
version = "v0.8.0"
origin_hash = "921a5b16af9a72d973ab9ba666e96ad8bf2f6ce4"
purpose = "go testee"
"#;

    #[test]
    fn parses_every_table() {
        let pins = parse(SAMPLE).unwrap();
        assert_eq!(pins.toolchains.len(), 1);
        assert_eq!(pins.toolchains[0].name, Toolchain::Rust);
        assert_eq!(pins.images[0].repository, "stellar/stellar-core");
        assert_eq!(pins.git.len(), 3);
        assert_eq!(pins.git[0].git_ref.as_deref(), Some("refs/tags/v30.0.0"));
        assert!(pins.git[1].git_ref.is_none());
        assert!(pins.git[2].sha.is_none());
        assert_eq!(pins.crates[0].version, "1.2.1+spec-1.1.0");
        assert_eq!(pins.npm[0].name, "@stellar/stellar-sdk");
        assert_eq!(pins.gomod[0].module, "github.com/stellar/go-stellar-sdk");
    }

    #[test]
    fn repo_pins_file_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml");
        let pins = load(&path).unwrap();
        assert!(pins.git.iter().any(|g| g.name == "stellar-core"));
        assert!(pins.images.iter().any(|i| i.name == "stellar-core"));
    }

    fn err_of(text: &str) -> String {
        format!("{:#}", parse(text).unwrap_err())
    }

    #[test]
    fn rejects_wrong_schema() {
        assert!(err_of("schema = 2").contains("unsupported schema 2"));
    }

    #[test]
    fn rejects_unknown_field() {
        let text = SAMPLE.replace(
            "purpose = \"go testee\"",
            "purpose = \"go testee\"\nsum = \"x\"",
        );
        assert!(err_of(&text).contains("unknown field"));
    }

    #[test]
    fn rejects_short_sha() {
        let text = SAMPLE.replace("79ea2acd1dc65a07166133cc04b12e76db314db0", "79ea2acd");
        assert!(err_of(&text).contains("wallet-backend-issue716-prefix: sha must be 40"));
    }

    #[test]
    fn rejects_git_pin_without_ref_or_sha() {
        let text = SAMPLE.replace("sha = \"79ea2acd1dc65a07166133cc04b12e76db314db0\"\n", "");
        assert!(err_of(&text).contains("needs a ref, a sha, or both"));
    }

    #[test]
    fn rejects_short_ref_name() {
        let text = SAMPLE.replace("ref = \"refs/tags/v7\"", "ref = \"v7\"");
        assert!(err_of(&text).contains("full ref name"));
    }

    #[test]
    fn rejects_duplicate_crate_version() {
        let dup = "\n[[crate]]\nname = \"toml\"\nversion = \"1.2.1+spec-1.1.0\"\nchecksum = \"d5755c51863cdac711f1956baf8eb9588e060755f7d3c4c979e70ea1e1f32cd7\"\npurpose = \"again\"\n";
        let text = format!("{SAMPLE}{dup}");
        assert!(err_of(&text).contains("listed twice"));
    }

    #[test]
    fn rejects_non_docker_hub_repository() {
        let text = SAMPLE.replace(
            "repository = \"stellar/stellar-core\"",
            "repository = \"ghcr.io/stellar/stellar-core\"",
        );
        assert!(err_of(&text).contains("docker hub path"));
    }

    #[test]
    fn rejects_bad_digest() {
        let text = SAMPLE.replace("digest = \"sha256:", "digest = \"md5:");
        assert!(err_of(&text).contains("digest must be sha256"));
    }

    #[test]
    fn rejects_empty_purpose() {
        let text = SAMPLE.replace("purpose = \"js testee\"", "purpose = \" \"");
        assert!(err_of(&text).contains("purpose is empty"));
    }
}
