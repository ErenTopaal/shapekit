//! Standalone node configuration and the stellar-core cfg file it renders to.
//!
//! The rendered text is a pure function of [`NodeConfig`], so two runs with
//! the same config boot byte-identical nodes. Nothing in it depends on the
//! host, the time or the working directory: every path is a fixed path inside
//! the container, and the workdir is bind-mounted there.
//!
//! The node seed is SHA-256 of [`NODE_SEED_DOMAIN`] followed by the passphrase
//! bytes, with no separator or length prefix (the domain is fixed, so the
//! split is unambiguous). The seed signs every SCP value and so ends up in each
//! ledger header, which is why it must be derived and not random. It depends on
//! the passphrase only so that the testnet and mainnet chains have distinct
//! validators, the way the real networks do.

use std::path::Path;

use sha2::{Digest, Sha256};
use stellar_strkey::ed25519::{PrivateKey, PublicKey};

/// Domain string hashed in front of the passphrase to get the node seed.
/// Changing it changes every recorded ledger header.
pub const NODE_SEED_DOMAIN: &[u8] = b"shapekit/node-seed/v1";

/// The `name` of the core image entry in `pins.toml`.
pub const CORE_IMAGE_PIN: &str = "stellar-core";

/// Highest ledger protocol stellar-core 30.0.0 can run.
pub const MAX_PROTOCOL: u32 = 30;

/// Command port inside the container. Bound to the container's loopback only.
pub const HTTP_PORT: u16 = 11626;

/// Query server port inside the container, also loopback only.
pub const HTTP_QUERY_PORT: u16 = 11627;

/// Where the workdir is mounted inside the container.
pub const CONTAINER_DATA_DIR: &str = "/data";

/// File names inside the workdir.
pub const CFG_FILE: &str = "stellar-core.cfg";
pub const META_FILE: &str = "meta.xdr";
pub const HISTORY_DIR: &str = "history";
pub const HISTORY_ARCHIVE_NAME: &str = "local";

/// Close time of the first ledger the driver closes is derived from this
/// epoch (2026-01-01T00:00:00Z). stellar-core runs a virtual clock under
/// `RUN_STANDALONE` plus `MANUAL_CLOSE`, so a close time before the wall
/// clock is accepted (manualclose compares against that clock, not the
/// wall clock: `ApplicationImpl::setManualCloseVirtualTime`, v30.0.0).
pub const CHAIN_EPOCH: u64 = 1_767_225_600;

/// Seconds between consecutive ledger close times.
///
/// The standalone virtual clock is not frozen between closes: whenever the
/// main loop is idle it jumps straight to the next pending timer, so virtual
/// time runs ahead of the last close time at roughly a thousand virtual
/// seconds per wall second (with `FLOOD_DEMAND_PERIOD_MS=1`, about a hundred
/// times more without it). manualclose rejects a close time behind that
/// clock. One day per ledger leaves about a minute of wall time between
/// closes before a close is refused, and still fits 63,000 ledgers below
/// core's year-2200 cap.
pub const CLOSE_STEP: u64 = 86_400;

/// Largest close time manualclose accepts (first second of 2200, UTC).
pub const MAX_CLOSE_TIME: u64 = 7_258_118_400;

/// Deterministic close time for a ledger sequence.
pub fn close_time(ledger_seq: u32) -> u64 {
    CHAIN_EPOCH + u64::from(ledger_seq) * CLOSE_STEP
}

/// Smallest `TESTING_MINIMUM_PERSISTENT_ENTRY_LIFETIME` core accepts
/// (`MinimumSorobanNetworkConfig::MINIMUM_PERSISTENT_ENTRY_LIFETIME`).
pub const MIN_PERSISTENT_TTL_FLOOR: u32 = 10;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("passphrase must be non-empty printable ascii without quotes or backslashes: {0:?}")]
    BadPassphrase(String),
    #[error("protocol {0} is outside 1..={MAX_PROTOCOL}")]
    BadProtocol(u32),
    #[error("BACKFILL_STELLAR_ASSET_EVENTS requires EMIT_CLASSIC_EVENTS (Config.cpp, v30.0.0)")]
    BackfillWithoutClassicEvents,
    #[error(
        "TESTING_MINIMUM_PERSISTENT_ENTRY_LIFETIME {0} is below core's floor of {MIN_PERSISTENT_TTL_FLOOR} (Config.cpp, v30.0.0)"
    )]
    MinPersistentTtl(u32),
    #[error("reading {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Parse {
        path: String,
        source: toml::de::Error,
    },
    #[error("{path} has no [[image]] named {name:?}")]
    MissingImage { path: String, name: String },
    #[error("image pin {name:?} has a malformed digest {digest:?}")]
    BadDigest { name: String, digest: String },
    #[error(
        "image pin {name:?} must have exactly one of digest (registry image) and image_id (local image), it has {}",
        if *both { "both" } else { "neither" }
    )]
    DigestOrImageId { name: String, both: bool },
}

/// A container image pinned by content.
///
/// A registry image is pinned by its manifest digest (`digest = ...` in the
/// pins file) and handed to docker as `repository@digest`. A locally built
/// image, such as the coverage twin (`twin/pins.toml`), has no manifest
/// digest; it is pinned by its image ID (`image_id = ...`), the digest of its
/// configuration, and handed to docker as that ID alone. Both forms name
/// exactly one image content. The tag is kept only for humans.
///
/// For a local image `digest` holds the image ID, so a chain's `chain.json`
/// records the ID in the same field a registry image's digest goes in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CoreImage {
    pub repository: String,
    pub tag: String,
    pub digest: String,
    /// Pinned by local image ID rather than registry manifest digest.
    #[serde(skip)]
    pub local: bool,
    /// Host directory every container of this image writes its LLVM coverage
    /// profiles into, for a coverage build such as the twin. Set by the
    /// caller, never read from pins; `None` runs containers as they are.
    #[serde(skip)]
    pub profile_dir: Option<std::path::PathBuf>,
}

#[derive(serde::Deserialize)]
struct PinsImages {
    #[serde(default)]
    image: Vec<ImageEntry>,
}

#[derive(serde::Deserialize)]
struct ImageEntry {
    name: String,
    repository: String,
    tag: String,
    digest: Option<String>,
    image_id: Option<String>,
}

fn is_sha256_ref(s: &str) -> bool {
    let hex_part = s.strip_prefix("sha256:").unwrap_or("");
    hex_part.len() == 64 && hex_part.bytes().all(|b| b.is_ascii_hexdigit())
}

impl CoreImage {
    /// Reads the core image pin from a `pins.toml`.
    pub fn from_pins(path: &Path) -> Result<Self, ConfigError> {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: shown.clone(),
            source,
        })?;
        let pins: PinsImages = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: shown.clone(),
            source,
        })?;
        let entry = pins
            .image
            .into_iter()
            .find(|i| i.name == CORE_IMAGE_PIN)
            .ok_or_else(|| ConfigError::MissingImage {
                path: shown,
                name: CORE_IMAGE_PIN.to_owned(),
            })?;
        let (digest, local) = match (entry.digest, entry.image_id) {
            (Some(d), None) => (d, false),
            (None, Some(id)) => (id, true),
            (d, id) => {
                return Err(ConfigError::DigestOrImageId {
                    name: entry.name,
                    both: d.is_some() && id.is_some(),
                });
            }
        };
        if !is_sha256_ref(&digest) {
            return Err(ConfigError::BadDigest {
                name: entry.name,
                digest,
            });
        }
        Ok(Self {
            repository: entry.repository,
            tag: entry.tag,
            digest,
            local,
            profile_dir: None,
        })
    }

    /// What docker is given: `repository@sha256:...` for a registry image,
    /// the bare `sha256:...` image ID for a local one.
    pub fn reference(&self) -> String {
        if self.local {
            self.digest.clone()
        } else {
            format!("{}@{}", self.repository, self.digest)
        }
    }
}

/// Where a container with a profile directory finds it.
pub const CONTAINER_PROFILE_DIR: &str = "/profiles";

/// `LLVM_PROFILE_FILE` inside such a container. Every container of a node
/// runs core as process 1, so the host name (the container id) tells their
/// profiles apart.
pub const CONTAINER_PROFILE_FILE: &str = "/profiles/core-%h-%p.profraw";

/// Everything that decides what a standalone node produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeConfig {
    pub passphrase: String,
    pub image: CoreImage,
    /// Ledger protocol the chain is upgraded to after genesis.
    pub protocol: u32,
    /// `TESTING_SOROBAN_HIGH_LIMIT_OVERRIDE`: the Soroban settings created at
    /// the protocol 20 upgrade use core's test override values instead of the
    /// tiny initial ones.
    pub soroban_high_limits: bool,
    pub emit_classic_events: bool,
    pub backfill_stellar_asset_events: bool,
    /// `ENABLE_SOROBAN_DIAGNOSTIC_EVENTS`. Off for recorded chains: with it
    /// on, every invocation also gets `core_metrics` diagnostic events, one
    /// of which (`invoke_time_nsecs`) carries the host call's wall-clock
    /// duration (`maybePopulateMetricsInDiagnosticEvents`,
    /// InvokeHostFunctionOpFrame.cpp, v30.0.0), so the meta would differ on
    /// every run. Core's golden check zeroes that value; the recorder keeps
    /// values as core wrote them, so the event is left out instead.
    pub diagnostic_events: bool,
    /// `TESTING_MINIMUM_PERSISTENT_ENTRY_LIFETIME`, parsed by release builds:
    /// the minimum persistent TTL written into the state archival settings
    /// created at the protocol 20 upgrade. Lets a chain archive a persistent
    /// entry within a few ledgers instead of core's initial 4096. A later
    /// settings upgrade replaces it, entries created before keep their TTL.
    pub min_persistent_ttl: Option<u32>,
    /// `INVARIANT_CHECKS = [".*"]`: core runs every invariant (`src/invariant/`) on every
    /// ledger it closes and stops with a fatal error when one fails, so whatever the node
    /// applied passed all of them. Off for recorded chains, which never needed it; the probe
    /// turns it on.
    pub invariant_checks: bool,
    /// `EMIT_LEDGER_CLOSE_META_EXT_V1`: from protocol 20 every ledger close meta carries the
    /// extension with the Soroban write fee (`LedgerCloseMetaFrame::setNetworkConfiguration`,
    /// v30.0.0). Off by default in core and for recorded chains.
    pub emit_ledger_close_meta_ext_v1: bool,
    /// Docker resource limits of the node's containers. They bound how much of the host a
    /// node may take and never change what it writes.
    pub limits: ContainerLimits,
}

/// `docker run --memory` and `--cpus` for every container a node starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerLimits {
    /// Bytes; also the memory plus swap limit, so the node cannot swap past it.
    pub memory_bytes: u64,
    /// Thousandths of a cpu, so `2000` is `--cpus 2`.
    pub millicpus: u32,
}

impl Default for ContainerLimits {
    /// 2 GiB and 2 cpus. A standalone node closing test ledgers stays far below that, and
    /// several explorer nodes side by side still leave the host room.
    fn default() -> Self {
        Self {
            memory_bytes: 2 << 30,
            millicpus: 2000,
        }
    }
}

impl ContainerLimits {
    /// The `docker run` arguments.
    pub fn docker_args(&self) -> Vec<String> {
        vec![
            "--memory".into(),
            self.memory_bytes.to_string(),
            "--memory-swap".into(),
            self.memory_bytes.to_string(),
            "--cpus".into(),
            format!("{}.{:03}", self.millicpus / 1000, self.millicpus % 1000),
        ]
    }
}

/// Node seed for a passphrase, see the module docs for the layout.
pub fn node_seed(passphrase: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(NODE_SEED_DOMAIN);
    h.update(passphrase.as_bytes());
    h.finalize().into()
}

/// The validator's public key as a `G...` strkey.
pub fn node_public_key(passphrase: &str) -> String {
    let signing = ed25519_dalek::SigningKey::from_bytes(&node_seed(passphrase));
    PublicKey(signing.verifying_key().to_bytes())
        .to_string()
        .as_str()
        .to_owned()
}

fn passphrase_ok(p: &str) -> bool {
    !p.is_empty()
        && p.bytes()
            .all(|b| (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\')
}

impl NodeConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        // core's cfg parser takes a toml basic string, so quotes and
        // backslashes would need escaping; no real passphrase has them
        if !passphrase_ok(&self.passphrase) {
            return Err(ConfigError::BadPassphrase(self.passphrase.clone()));
        }
        if self.protocol == 0 || self.protocol > MAX_PROTOCOL {
            return Err(ConfigError::BadProtocol(self.protocol));
        }
        if self.backfill_stellar_asset_events && !self.emit_classic_events {
            return Err(ConfigError::BackfillWithoutClassicEvents);
        }
        if let Some(ttl) = self.min_persistent_ttl
            && ttl < MIN_PERSISTENT_TTL_FLOOR
        {
            return Err(ConfigError::MinPersistentTtl(ttl));
        }
        Ok(())
    }

    /// Renders the stellar-core cfg. Deterministic: same config, same bytes.
    pub fn render(&self) -> Result<String, ConfigError> {
        self.validate()?;
        let seed = PrivateKey(node_seed(&self.passphrase));
        let data = CONTAINER_DATA_DIR;
        let mut s = String::new();
        let mut line = |l: String| {
            s.push_str(&l);
            s.push('\n');
        };
        line("# rendered by shapekit-core-driver from a NodeConfig, do not edit".into());
        line(format!("NETWORK_PASSPHRASE=\"{}\"", self.passphrase));
        line(format!("NODE_SEED=\"{} self\"", seed.as_unredacted()));
        line("NODE_IS_VALIDATOR=true".into());
        line("RUN_STANDALONE=true".into());
        line("MANUAL_CLOSE=true".into());
        line("UNSAFE_QUORUM=true".into());
        line("FAILURE_SAFETY=0".into());
        line(format!("HTTP_PORT={HTTP_PORT}"));
        // loopback only: the driver talks to core through docker exec
        line("PUBLIC_HTTP_PORT=false".into());
        line(format!("HTTP_QUERY_PORT={HTTP_QUERY_PORT}"));
        line("QUERY_THREAD_POOL_SIZE=2".into());
        line(format!("DATABASE=\"sqlite3://{data}/stellar.db\""));
        line(format!("BUCKET_DIR_PATH=\"{data}/buckets\""));
        line(format!("METADATA_OUTPUT_STREAM=\"{data}/{META_FILE}\""));
        line("METADATA_DEBUG_LEDGERS=0".into());
        line("LOG_FILE_PATH=\"\"".into());
        line("LOG_COLOR=false".into());
        // apply on the main thread, as core's own standalone example does
        line("PARALLEL_LEDGER_APPLY=false".into());
        // the shortest recurring timer bounds each idle jump of the virtual
        // clock, so 1 ms keeps it close behind the last close (see CLOSE_STEP)
        line("FLOOD_DEMAND_PERIOD_MS=1".into());
        line(format!(
            "ENABLE_SOROBAN_DIAGNOSTIC_EVENTS={}",
            self.diagnostic_events
        ));
        line(format!("EMIT_CLASSIC_EVENTS={}", self.emit_classic_events));
        // written only when on, so every config without it renders as before
        if self.emit_ledger_close_meta_ext_v1 {
            line("EMIT_LEDGER_CLOSE_META_EXT_V1=true".into());
        }
        line(format!(
            "BACKFILL_STELLAR_ASSET_EVENTS={}",
            self.backfill_stellar_asset_events
        ));
        line(format!(
            "TESTING_SOROBAN_HIGH_LIMIT_OVERRIDE={}",
            self.soroban_high_limits
        ));
        if let Some(ttl) = self.min_persistent_ttl {
            line(format!("TESTING_MINIMUM_PERSISTENT_ENTRY_LIFETIME={ttl}"));
        }
        if self.invariant_checks {
            line("INVARIANT_CHECKS=[\".*\"]".into());
        }
        line(String::new());
        line("[QUORUM_SET]".into());
        line("THRESHOLD_PERCENT=100".into());
        line("VALIDATORS=[\"$self\"]".into());
        line(String::new());
        line(format!("[HISTORY.{HISTORY_ARCHIVE_NAME}]"));
        let hist = format!("{data}/{HISTORY_DIR}");
        line(format!("get=\"cp {hist}/{{0}} {{1}}\""));
        line(format!("put=\"cp {{0}} {hist}/{{1}}\""));
        line(format!("mkdir=\"mkdir -p {hist}/{{0}}\""));
        Ok(s)
    }
}

/// Upgrade time handed to core's `upgrades` command. Core nominates an armed
/// upgrade once the last closed ledger's close time has reached it, and
/// genesis closes at 0, so the epoch of unix time makes the upgrade land in
/// the very next ledger whatever close time that ledger gets.
pub const UPGRADE_TIME: &str = "1970-01-01T00:00:00Z";

/// Renders a summary line for logs and error context.
pub fn describe(cfg: &NodeConfig) -> String {
    format!(
        "{} protocol {} image {}",
        cfg.passphrase,
        cfg.protocol,
        cfg.image.reference()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limits_are_two_gib_and_two_cpus() {
        assert_eq!(
            ContainerLimits::default().docker_args(),
            [
                "--memory",
                "2147483648",
                "--memory-swap",
                "2147483648",
                "--cpus",
                "2.000"
            ]
        );
        let half = ContainerLimits {
            memory_bytes: 1,
            millicpus: 500,
        };
        assert_eq!(half.docker_args()[5], "0.500");
    }

    fn sample(passphrase: &str) -> NodeConfig {
        NodeConfig {
            passphrase: passphrase.to_owned(),
            image: CoreImage {
                repository: "stellar/stellar-core".into(),
                tag: "t".into(),
                digest: format!("sha256:{}", "0".repeat(64)),
                local: false,
                profile_dir: None,
            },
            protocol: 30,
            soroban_high_limits: true,
            emit_classic_events: true,
            backfill_stellar_asset_events: true,
            diagnostic_events: true,
            min_persistent_ttl: None,
            invariant_checks: false,
            emit_ledger_close_meta_ext_v1: false,
            limits: ContainerLimits::default(),
        }
    }

    #[test]
    fn invariant_checks_render_only_when_asked() {
        let mut c = sample("Test SDF Network ; September 2015");
        assert!(!c.render().unwrap().contains("INVARIANT_CHECKS"));
        c.invariant_checks = true;
        assert!(
            c.render()
                .unwrap()
                .contains("\nINVARIANT_CHECKS=[\".*\"]\n")
        );
    }

    #[test]
    fn render_is_deterministic_and_passphrase_bound() {
        let a = sample("Test SDF Network ; September 2015");
        let b = sample("Public Global Stellar Network ; September 2015");
        assert_eq!(a.render().unwrap(), a.render().unwrap());
        let ra = a.render().unwrap();
        let rb = b.render().unwrap();
        assert_ne!(ra, rb);
        assert!(ra.contains("NETWORK_PASSPHRASE=\"Test SDF Network ; September 2015\"\n"));
        assert!(ra.contains("put=\"cp {0} /data/history/{1}\"\n"));
        assert_ne!(node_seed(&a.passphrase), node_seed(&b.passphrase));
    }

    #[test]
    fn node_seed_layout() {
        let mut h = Sha256::new();
        h.update(b"shapekit/node-seed/v1Test SDF Network ; September 2015");
        let want: [u8; 32] = h.finalize().into();
        assert_eq!(node_seed("Test SDF Network ; September 2015"), want);
    }

    #[test]
    fn rejects_bad_configs() {
        let mut c = sample("bad \" quote");
        assert!(matches!(c.render(), Err(ConfigError::BadPassphrase(_))));
        c = sample("ok");
        c.protocol = 31;
        assert!(matches!(c.render(), Err(ConfigError::BadProtocol(31))));
        c.protocol = 30;
        c.emit_classic_events = false;
        assert!(matches!(
            c.render(),
            Err(ConfigError::BackfillWithoutClassicEvents)
        ));
        c.emit_classic_events = true;
        c.min_persistent_ttl = Some(9);
        assert!(matches!(c.render(), Err(ConfigError::MinPersistentTtl(9))));
        c.min_persistent_ttl = Some(10);
        assert!(
            c.render()
                .unwrap()
                .contains("TESTING_MINIMUM_PERSISTENT_ENTRY_LIFETIME=10\n")
        );
    }

    #[test]
    fn close_schedule() {
        assert_eq!(close_time(2), CHAIN_EPOCH + 2 * CLOSE_STEP);
        let max_seq = (MAX_CLOSE_TIME - CHAIN_EPOCH) / CLOSE_STEP;
        assert!(max_seq > 60_000);
    }

    fn pin_file(body: &str) -> tempfile::NamedTempFile {
        use std::io::Write as _;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(body.as_bytes()).unwrap();
        f
    }

    #[test]
    fn reads_a_local_image_pin() {
        let id = format!("sha256:{}", "ab".repeat(32));
        let f = pin_file(&format!(
            "[[image]]\nname = \"stellar-core\"\nrepository = \"shapekit/core-twin\"\ntag = \"30.0.0-cov\"\nimage_id = \"{id}\"\n"
        ));
        let img = CoreImage::from_pins(f.path()).unwrap();
        assert!(img.local);
        assert_eq!(img.reference(), id);
        // the flag is not part of what a chain records
        let json = serde_json::to_value(&img).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"repository": "shapekit/core-twin", "tag": "30.0.0-cov", "digest": id})
        );
    }

    #[test]
    fn an_image_pin_needs_exactly_one_reference() {
        let d = format!("sha256:{}", "cd".repeat(32));
        let both = pin_file(&format!(
            "[[image]]\nname = \"stellar-core\"\nrepository = \"r\"\ntag = \"t\"\ndigest = \"{d}\"\nimage_id = \"{d}\"\n"
        ));
        assert!(matches!(
            CoreImage::from_pins(both.path()),
            Err(ConfigError::DigestOrImageId { both: true, .. })
        ));
        let neither =
            pin_file("[[image]]\nname = \"stellar-core\"\nrepository = \"r\"\ntag = \"t\"\n");
        assert!(matches!(
            CoreImage::from_pins(neither.path()),
            Err(ConfigError::DigestOrImageId { both: false, .. })
        ));
        let short = pin_file(
            "[[image]]\nname = \"stellar-core\"\nrepository = \"r\"\ntag = \"t\"\nimage_id = \"sha256:abc\"\n",
        );
        assert!(matches!(
            CoreImage::from_pins(short.path()),
            Err(ConfigError::BadDigest { .. })
        ));
    }

    #[test]
    fn reads_the_repo_pin() {
        let pins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml");
        let img = CoreImage::from_pins(&pins).unwrap();
        assert_eq!(img.repository, "stellar/stellar-core");
        assert!(img.reference().starts_with("stellar/stellar-core@sha256:"));
        assert!(!img.local);
    }
}
