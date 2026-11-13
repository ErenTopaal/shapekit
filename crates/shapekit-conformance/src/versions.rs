//! `conformance/versions.toml`: the SDK version matrix and how each testee is built and run.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

pub const VERSIONS_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionsFile {
    pub schema: u32,
    /// Keyed by the short sdk name used everywhere else: `rust`, `go`, `js`.
    pub sdk: BTreeMap<String, Sdk>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sdk {
    /// What `hello` must report as `sdk`.
    pub package: String,
    /// The version treated as current: the one a plain `conformance run` uses.
    pub current: String,
    /// Where issues for this sdk are filed, `owner/repo`.
    pub issues_repo: String,
    /// Ops whose issues go to another repository (the rust strkey codec lives in its own crate).
    #[serde(default)]
    pub issues_repo_by_op: BTreeMap<String, String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    pub memory: MemoryLimit,
    pub versions: Vec<SdkVersion>,
}

fn default_timeout_ms() -> u64 {
    10_000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryLimit {
    pub resource: MemoryResource,
    pub mib: u64,
}

/// Which rlimit caps the testee. See docs/testee-protocol.md, "Limits the runner applies".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryResource {
    /// `RLIMIT_AS`, the whole address space.
    As,
    /// `RLIMIT_DATA`, private writable mappings; reserved but untouched regions do not count.
    Data,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SdkVersion {
    pub version: String,
    /// Run in order from the repository root before the testee starts.
    pub build: Vec<Step>,
    /// The testee command line, relative to the repository root.
    pub command: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The newest protocol whose XDR this version knows. A failure on an entry generated at
    /// a later protocol is reported as unsupported, not as a divergence: an sdk that predates
    /// a protocol cannot be expected to read it.
    #[serde(default)]
    pub max_protocol: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub argv: Vec<String>,
    /// Working directory relative to the repository root; the root when absent.
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// One sdk at one version, resolved from the file.
#[derive(Debug, Clone)]
pub struct Target {
    pub sdk: String,
    pub package: String,
    pub version: String,
    pub timeout_ms: u64,
    pub memory: MemoryLimit,
    pub build: Vec<Step>,
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub max_protocol: Option<u32>,
    /// Whether this is the sdk's `current` version, the one that votes for the sdk when
    /// implementation-defined checks are decided across sdks.
    pub current: bool,
    /// The longest response line the runner reads from this testee, in bytes without the
    /// newline; see `MAX_RESPONSE_LINE`.
    pub max_response_line: usize,
}

/// The response line limit docs/testee-protocol.md sets: a longer line is a protocol
/// violation, so a testee printing without end cannot grow the runner without bound.
pub const MAX_RESPONSE_LINE: usize = 256 << 20;

impl Target {
    /// `rust@30.0.0`, the column key in the matrix.
    pub fn key(&self) -> String {
        format!("{}@{}", self.sdk, self.version)
    }
}

impl VersionsFile {
    pub fn load(path: &Path) -> Result<VersionsFile> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let file: VersionsFile =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        file.check()
            .with_context(|| format!("checking {}", path.display()))?;
        Ok(file)
    }

    fn check(&self) -> Result<()> {
        ensure!(
            self.schema == VERSIONS_SCHEMA,
            "schema {} is not {VERSIONS_SCHEMA}",
            self.schema
        );
        for (name, sdk) in &self.sdk {
            ensure!(!sdk.versions.is_empty(), "sdk {name} lists no versions");
            ensure!(
                sdk.versions.iter().any(|v| v.version == sdk.current),
                "sdk {name}: current version {} is not in its version list",
                sdk.current
            );
            ensure!(sdk.memory.mib > 0, "sdk {name}: memory limit is zero");
            ensure!(sdk.timeout_ms > 0, "sdk {name}: timeout is zero");
            let mut seen = std::collections::BTreeSet::new();
            for v in &sdk.versions {
                ensure!(
                    seen.insert(&v.version),
                    "sdk {name} lists {} twice",
                    v.version
                );
                ensure!(
                    !v.command.is_empty(),
                    "sdk {name} {} has no command",
                    v.version
                );
                for s in &v.build {
                    ensure!(
                        !s.argv.is_empty(),
                        "sdk {name} {} has an empty build step",
                        v.version
                    );
                }
            }
        }
        Ok(())
    }

    pub fn sdk(&self, name: &str) -> Result<&Sdk> {
        self.sdk.get(name).with_context(|| {
            format!(
                "no sdk {name} in versions.toml (have: {})",
                self.sdk.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })
    }

    pub fn target(&self, sdk: &str, version: &str) -> Result<Target> {
        let s = self.sdk(sdk)?;
        let Some(v) = s.versions.iter().find(|v| v.version == version) else {
            bail!(
                "sdk {sdk} has no version {version} in versions.toml (have: {})",
                s.versions
                    .iter()
                    .map(|v| v.version.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        };
        Ok(Target {
            sdk: sdk.to_string(),
            package: s.package.clone(),
            version: v.version.clone(),
            timeout_ms: s.timeout_ms,
            memory: s.memory,
            build: v.build.clone(),
            command: v.command.clone(),
            env: v.env.clone(),
            max_protocol: v.max_protocol,
            current: v.version == s.current,
            max_response_line: MAX_RESPONSE_LINE,
        })
    }

    /// The targets a run covers: the named sdks (all when empty), at their current version or
    /// at every version.
    pub fn select(&self, sdks: &[String], all_versions: bool) -> Result<Vec<Target>> {
        let names: Vec<String> = if sdks.is_empty() {
            self.sdk.keys().cloned().collect()
        } else {
            sdks.to_vec()
        };
        let mut out = Vec::new();
        for name in &names {
            let sdk = self.sdk(name)?;
            if all_versions {
                for v in &sdk.versions {
                    out.push(self.target(name, &v.version)?);
                }
            } else {
                out.push(self.target(name, &sdk.current)?);
            }
        }
        Ok(out)
    }
}

/// Runs a target's build steps from `root`, failing on the first step that fails.
pub fn build(root: &Path, target: &Target) -> Result<()> {
    for step in &target.build {
        let dir = match &step.dir {
            Some(d) => root.join(d),
            None => root.to_path_buf(),
        };
        let status = std::process::Command::new(&step.argv[0])
            .args(&step.argv[1..])
            .current_dir(&dir)
            .envs(&step.env)
            // keep the runner's stdout for its own report
            .stdout(std::io::stderr())
            .status()
            .with_context(|| {
                format!(
                    "building {}: starting {}",
                    target.key(),
                    step.argv.join(" ")
                )
            })?;
        ensure!(
            status.success(),
            "building {}: `{}` in {} exited with {status}",
            target.key(),
            step.argv.join(" "),
            dir.display()
        );
    }
    Ok(())
}
