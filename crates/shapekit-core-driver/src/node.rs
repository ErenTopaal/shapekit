//! Lifecycle and control of one standalone stellar-core container.
//!
//! The container runs on its own `docker network create --internal` network,
//! so it has no route off the host. The driver checks that the network really
//! is internal before anything starts, and refuses otherwise: the mainnet
//! passphrase is only ever used on a network that cannot reach mainnet.
//! Containers run as the owner of the workdir, with every capability dropped,
//! so everything core writes stays removable by the caller.

use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde::Deserialize;
use stellar_xdr::{
    AccountId, ConfigUpgradeSet, DiagnosticEvent, LedgerEntry, LedgerEntryData, LedgerKey,
    LedgerKeyAccount, Limits, ReadXdr, TransactionEnvelope, TransactionResult, VecM, WriteXdr,
};

use crate::config::{
    CFG_FILE, CONTAINER_DATA_DIR, CONTAINER_PROFILE_DIR, CONTAINER_PROFILE_FILE, ConfigError,
    HISTORY_ARCHIVE_NAME, HISTORY_DIR, META_FILE, NodeConfig, UPGRADE_TIME,
};
use crate::http::{CoreHttp, HttpError, percent_encode};
use crate::meta::decode_limits;
use crate::sim::{FetchError, FetchedEntry, LedgerContext, SnapshotFetcher};

/// How long `start` waits for core to report `Synced!`.
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error("running `docker {args}`: {source}")]
    DockerSpawn {
        args: String,
        source: std::io::Error,
    },
    #[error("`docker {args}` failed (exit {code:?}): {stderr}{stdout}")]
    Docker {
        args: String,
        code: Option<i32>,
        stderr: String,
        stdout: String,
    },
    #[error(
        "docker image {0} is not present locally; pull it first with `docker pull {0}` (the driver never pulls, so runs stay offline)"
    )]
    ImageMissing(String),
    #[error(
        "local docker image {0} is not present; it is built on this host, not pulled (the coverage twin comes from twin/build.sh), and a rebuild gives a new image id to pin"
    )]
    LocalImageMissing(String),
    #[error(
        "docker network {0} is not internal; refusing to start core on a network with a route off the host"
    )]
    NetworkNotInternal(String),
    #[error("workdir {path}: {reason}")]
    Workdir { path: String, reason: String },
    #[error("workdir {path}: {source}")]
    WorkdirIo {
        path: String,
        source: std::io::Error,
    },
    #[error(
        "core did not report Synced! within {waited:?}, last state {state:?}; core log tail:\n{logs}"
    )]
    NotSynced {
        waited: Duration,
        state: String,
        logs: String,
    },
    #[error("core container {container} exited; log tail:\n{logs}")]
    Exited { container: String, logs: String },
    #[error("core rejected {what}: {message}")]
    Rejected { what: String, message: String },
    #[error("unexpected answer to {what}: {body}")]
    Unexpected { what: String, body: String },
    #[error("core is still booting and answers no command yet")]
    Booting,
    #[error("decoding {what}: {source}")]
    Xdr {
        what: String,
        source: stellar_xdr::Error,
    },
    #[error(
        "history archive did not publish checkpoint {checkpoint} within {waited:?} (published {published:?})"
    )]
    PublishTimeout {
        checkpoint: u32,
        waited: Duration,
        published: Option<u32>,
    },
    #[error("getledgerentry needs at least one key")]
    NoKeys,
    #[error("cleanup failed: {0}")]
    Cleanup(String),
    #[error("account {0} does not exist")]
    NoAccount(String),
    #[error("query server answered for ledger {got}, expected {want}")]
    StaleQuery { want: u32, got: u32 },
}

fn docker(args: &[&str]) -> Result<Output, NodeError> {
    Command::new("docker")
        .args(args)
        .output()
        .map_err(|source| NodeError::DockerSpawn {
            args: args.join(" "),
            source,
        })
}

fn docker_ok(args: &[&str]) -> Result<String, NodeError> {
    let out = docker(args)?;
    if !out.status.success() {
        return Err(NodeError::Docker {
            args: args.join(" "),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            stdout: tail(&String::from_utf8_lossy(&out.stdout), 40),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Whether `e` is docker reporting that the container does not exist, which is what a
/// release wants: `docker run` can fail before it creates the container it was named for.
fn no_such_container(e: &NodeError) -> bool {
    matches!(e, NodeError::Docker { stderr, .. } if stderr.contains("No such container"))
}

/// Whether docker refused because the network does not exist: one recorded before its
/// create failed was never made, and is no error to release.
fn no_such_network(e: &NodeError) -> bool {
    matches!(e, NodeError::Docker { stderr, .. } if stderr.contains("not found"))
}

fn tail(s: &str, lines: usize) -> String {
    let all: Vec<&str> = s.lines().collect();
    let start = all.len().saturating_sub(lines);
    let joined = all[start..].join("\n");
    if joined.is_empty() {
        joined
    } else {
        format!("\n{joined}")
    }
}

/// Names unique within the host for this process's lifetime.
fn unique_name(kind: &str) -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    format!(
        "shapekit-core-{kind}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// Removes what it holds when dropped, so an early return or panic between
/// creating a network and handing out a `CoreNode` leaves nothing behind.
#[derive(Debug, Default)]
struct DockerResources {
    container: Option<String>,
    network: Option<String>,
}

impl DockerResources {
    fn release(&mut self) -> Result<(), NodeError> {
        // a signal's cleanup removes the recorded node and network; removing them here too
        // would race it
        shapekit_process::cleanup::park_if_stopping();
        let mut failures = Vec::new();
        if let Some(c) = self.container.take() {
            // a node dropped while paused for idling (see `http::IDLE_PAUSE`) must run again
            // to shut down; on a running node this fails, which is fine
            let _ = docker(&["unpause", &c]);
            // stop first so core shuts down cleanly and flushes its stream
            if let Err(e) = docker_ok(&["stop", "-t", "20", &c])
                && !no_such_container(&e)
            {
                failures.push(e.to_string());
            }
            match docker_ok(&["rm", "-f", "-v", &c]) {
                Err(e) if !no_such_container(&e) => failures.push(e.to_string()),
                _ => shapekit_process::cleanup::container_gone(&c),
            }
        }
        if let Some(n) = self.network.take() {
            match docker_ok(&["network", "rm", &n]) {
                Err(e) if !no_such_network(&e) => failures.push(e.to_string()),
                _ => shapekit_process::cleanup::network_removed(&n),
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(NodeError::Cleanup(failures.join("; ")))
        }
    }
}

impl Drop for DockerResources {
    fn drop(&mut self) {
        if self.container.is_none() && self.network.is_none() {
            return;
        }
        // drop cannot return the error, and losing it would leave docker
        // objects behind without a trace
        if let Err(e) = self.release() {
            eprintln!("shapekit-core-driver: {e}");
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LedgerInfo {
    pub num: u32,
    pub version: u32,
    #[serde(rename = "closeTime")]
    pub close_time: u64,
    pub hash: String,
    #[serde(rename = "baseFee")]
    pub base_fee: u32,
    #[serde(rename = "baseReserve")]
    pub base_reserve: u32,
    #[serde(rename = "maxTxSetSize")]
    pub max_tx_set_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreInfo {
    pub build: String,
    pub ledger: LedgerInfo,
    pub network: String,
    pub protocol_version: u32,
    pub state: String,
}

#[derive(Deserialize)]
struct InfoEnvelope {
    info: CoreInfo,
}

/// What every http route answers until core has finished starting, with status 200:
/// `CommandHandler::safeRouter`, src/main/CommandHandler.cpp in stellar-core v30.0.0.
const BOOTING_ERROR: &str = "Core is booting, try again later";

/// Core's answer to `info`: its state, or the error every route gives while booting, which
/// is not a failure but a reason to ask again.
fn parse_info(body: &str) -> Result<CoreInfo, NodeError> {
    if let Ok(env) = serde_json::from_str::<InfoEnvelope>(body) {
        return Ok(env.info);
    }
    let booting = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .is_some_and(|v| v.get("error").and_then(|e| e.as_str()) == Some(BOOTING_ERROR));
    if booting {
        return Err(NodeError::Booting);
    }
    Err(NodeError::Unexpected {
        what: "info".into(),
        body: body.to_owned(),
    })
}

/// Whether `wait_synced` should keep polling after this error rather than give up.
fn not_up_yet(e: &NodeError) -> bool {
    // the http server is not listening for the first moments, then it answers but boots
    matches!(e, NodeError::Http(_) | NodeError::Booting)
}

/// `docker image inspect` failed: is the image just not pulled, or is docker itself
/// unreachable or refusing (no daemon, no permission)? Only the first is fixed by a pull.
fn image_missing(stderr: &str) -> bool {
    stderr.contains("No such image")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosedLedger {
    pub ledger_seq: u32,
    pub close_time: u64,
}

/// `TransactionQueue::AddResultCode`, as `tx` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitStatus {
    Pending,
    Duplicate,
    Error,
    TryAgainLater,
    Filtered,
}

#[derive(Debug, Clone)]
pub struct SubmitResult {
    pub status: SubmitStatus,
    /// Present exactly when status is `Error`.
    pub error: Option<TransactionResult>,
    /// Only sent when core runs with `ENABLE_DIAGNOSTICS_FOR_TX_SUBMISSION`.
    pub diagnostic_events: Option<Vec<DiagnosticEvent>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    Live,
    Archived,
    NotFound,
}

#[derive(Debug, Clone)]
pub struct QueriedEntry {
    pub state: EntryState,
    pub entry: Option<LedgerEntry>,
    /// Set for Soroban entries; core reports 0 for archived ones.
    pub live_until_ledger_seq: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct LedgerEntries {
    pub ledger_seq: u32,
    /// One per requested key, in request order.
    pub entries: Vec<QueriedEntry>,
}

/// Pulls the message out of core's `{"exception": "..."}` answers.
fn exception(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("exception")?.as_str().map(str::to_owned)
}

pub struct CoreNode {
    cfg: NodeConfig,
    workdir: PathBuf,
    http: CoreHttp,
    resources: DockerResources,
}

impl std::fmt::Debug for CoreNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoreNode")
            .field("container", &self.resources.container)
            .field("network", &self.resources.network)
            .field("workdir", &self.workdir)
            .finish()
    }
}

impl CoreNode {
    /// Boots a fresh node in `workdir`, which must exist and be empty.
    /// Returns once core reports `Synced!` on its genesis ledger.
    pub fn start(cfg: NodeConfig, workdir: &Path) -> Result<Self, NodeError> {
        let rendered = cfg.render()?;
        let shown = workdir.display().to_string();
        let wio = |source| NodeError::WorkdirIo {
            path: shown.clone(),
            source,
        };
        let workdir = workdir.canonicalize().map_err(wio)?;
        let meta = std::fs::metadata(&workdir).map_err(wio)?;
        if !meta.is_dir() {
            return Err(NodeError::Workdir {
                path: shown,
                reason: "not a directory".into(),
            });
        }
        // a leftover database or meta stream would silently change the chain
        if std::fs::read_dir(&workdir).map_err(wio)?.next().is_some() {
            return Err(NodeError::Workdir {
                path: shown,
                reason: "not empty".into(),
            });
        }
        std::fs::write(workdir.join(CFG_FILE), rendered).map_err(wio)?;
        std::fs::create_dir(workdir.join(HISTORY_DIR)).map_err(wio)?;
        let user = format!("{}:{}", meta.uid(), meta.gid());

        let image = cfg.image.reference();
        let args = ["image", "inspect", "--format", "{{.Id}}", &image];
        let inspected = docker(&args)?;
        if !inspected.status.success() {
            let stderr = String::from_utf8_lossy(&inspected.stderr).trim().to_owned();
            if image_missing(&stderr) {
                return Err(if cfg.image.local {
                    NodeError::LocalImageMissing(image)
                } else {
                    NodeError::ImageMissing(image)
                });
            }
            return Err(NodeError::Docker {
                args: args.join(" "),
                code: inspected.status.code(),
                stderr,
                stdout: tail(&String::from_utf8_lossy(&inspected.stdout), 40),
            });
        }

        let mut res = DockerResources::default();
        let network = unique_name("net");
        // recorded before the create, as the container below: a signal removes what the
        // record names, and removing one that was never created is no error
        shapekit_process::cleanup::network_created(&network);
        res.network = Some(network.clone());
        docker_ok(&[
            "network",
            "create",
            "--internal",
            "--label",
            "org.shapekit.core-driver=1",
            &network,
        ])?;
        let internal = docker_ok(&["network", "inspect", "--format", "{{.Internal}}", &network])?;
        if internal != "true" {
            return Err(NodeError::NetworkNotInternal(network));
        }

        let mount = format!("{}:{CONTAINER_DATA_DIR}", workdir.display());
        let conf = format!("{CONTAINER_DATA_DIR}/{CFG_FILE}");
        // a coverage build writes its profile when core exits; the directory
        // is outside the workdir so the profiles outlive it
        let profile_args: Vec<String> = match &cfg.image.profile_dir {
            Some(dir) => vec![
                "-v".into(),
                format!("{}:{CONTAINER_PROFILE_DIR}", dir.display()),
                "-e".into(),
                format!("LLVM_PROFILE_FILE={CONTAINER_PROFILE_FILE}"),
            ],
            None => Vec::new(),
        };
        let limits = cfg.limits.docker_args();
        let base = |extra: &[&str]| -> Vec<String> {
            let mut v: Vec<String> = vec!["run".into()];
            v.extend(extra.iter().map(|s| s.to_string()));
            v.extend(limits.iter().cloned());
            v.extend(profile_args.iter().cloned());
            for a in [
                "--network",
                &network,
                "--user",
                &user,
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--label",
                "org.shapekit.core-driver=1",
                "-v",
                &mount,
                "-w",
                CONTAINER_DATA_DIR,
                &image,
                "--conf",
                &conf,
            ] {
                v.push(a.to_owned());
            }
            v
        };
        for cmd in [vec!["new-db"], vec!["new-hist", HISTORY_ARCHIVE_NAME]] {
            let mut args = base(&["--rm"]);
            args.extend(cmd.iter().map(|s| s.to_string()));
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            docker_ok(&refs)?;
        }

        let container = unique_name("node");
        let mut args = base(&["-d", "--name", &container]);
        args.push("run".into());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        // recorded before the run: docker can create the container and then fail to start it,
        // and a container that exists must be removed; one that was never created is no
        // error to release (see `no_such_container`)
        res.container = Some(container.clone());
        shapekit_process::cleanup::container_started(&container);
        docker_ok(&refs)?;

        let node = Self {
            cfg,
            workdir,
            http: CoreHttp::new(container),
            resources: res,
        };
        node.wait_synced()?;
        Ok(node)
    }

    pub fn config(&self) -> &NodeConfig {
        &self.cfg
    }

    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    pub fn meta_path(&self) -> PathBuf {
        self.workdir.join(META_FILE)
    }

    pub fn history_dir(&self) -> PathBuf {
        self.workdir.join(HISTORY_DIR)
    }

    pub fn container(&self) -> &str {
        self.resources
            .container
            .as_deref()
            .expect("a live CoreNode always holds its container")
    }

    pub fn network(&self) -> &str {
        self.resources
            .network
            .as_deref()
            .expect("a live CoreNode always holds its network")
    }

    /// The last lines of core's log, for error reports.
    pub fn logs_tail(&self, lines: usize) -> String {
        match docker(&["logs", "--tail", &lines.to_string(), self.container()]) {
            Ok(out) => {
                let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
                s.push_str(&String::from_utf8_lossy(&out.stderr));
                s
            }
            Err(e) => format!("(could not read logs: {e})"),
        }
    }

    fn running(&self) -> Result<bool, NodeError> {
        Ok(docker_ok(&[
            "inspect",
            "--format",
            "{{.State.Running}}",
            self.container(),
        ])? == "true")
    }

    fn wait_synced(&self) -> Result<(), NodeError> {
        let start = Instant::now();
        let mut state = String::from("(no answer yet)");
        while start.elapsed() < SYNC_TIMEOUT {
            if !self.running()? {
                return Err(NodeError::Exited {
                    container: self.container().to_owned(),
                    logs: self.logs_tail(60),
                });
            }
            match self.info() {
                Ok(info) if info.state == "Synced!" => return Ok(()),
                Ok(info) => state = info.state,
                Err(e) if not_up_yet(&e) => state = e.to_string(),
                Err(e) => return Err(e),
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(NodeError::NotSynced {
            waited: SYNC_TIMEOUT,
            state,
            logs: self.logs_tail(60),
        })
    }

    pub fn info(&self) -> Result<CoreInfo, NodeError> {
        parse_info(&self.http.command("info")?)
    }

    /// Closes exactly `ledger_seq` at exactly `close_time`, which must be the
    /// next ledger. Fails if core closed anything else.
    pub fn close(&self, ledger_seq: u32, close_time: u64) -> Result<ClosedLedger, NodeError> {
        let what = format!("manualclose ledgerSeq={ledger_seq} closeTime={close_time}");
        let body = self.http.command(&format!(
            "manualclose?ledgerSeq={ledger_seq}&closeTime={close_time}"
        ))?;
        if let Some(message) = exception(&body) {
            return Err(NodeError::Rejected { what, message });
        }
        let want = format!(
            "Manually closed ledger with sequence number {ledger_seq} and closeTime {close_time}"
        );
        if body.trim() != want {
            return Err(NodeError::Unexpected { what, body });
        }
        Ok(ClosedLedger {
            ledger_seq,
            close_time,
        })
    }

    /// Arms a protocol upgrade and closes `ledger_seq` with it, then checks
    /// the ledger really runs `protocol`. A fresh network goes from genesis
    /// protocol 0 to any supported version in one step: core creates each
    /// version's new entries (Soroban settings at 20 and so on) for every
    /// version it crosses (`Upgrades::applyVersionUpgrade`).
    pub fn upgrade_to(
        &self,
        protocol: u32,
        ledger_seq: u32,
        close_time: u64,
    ) -> Result<ClosedLedger, NodeError> {
        let what = format!("upgrade to protocol {protocol}");
        let body = self.http.command(&format!(
            "upgrades?mode=set&upgradetime={}&protocolversion={protocol}",
            percent_encode(UPGRADE_TIME)
        ))?;
        if !body.trim().is_empty() {
            let message = exception(&body).unwrap_or(body);
            return Err(NodeError::Rejected { what, message });
        }
        let closed = self.close(ledger_seq, close_time)?;
        let info = self.info()?;
        if info.ledger.num != ledger_seq || info.ledger.version != protocol {
            return Err(NodeError::Unexpected {
                what,
                body: format!(
                    "after closing {ledger_seq} core is at ledger {} protocol {}",
                    info.ledger.num, info.ledger.version
                ),
            });
        }
        Ok(closed)
    }

    pub fn submit(&self, envelope: &TransactionEnvelope) -> Result<SubmitResult, NodeError> {
        let b64 = envelope
            .to_xdr_base64(Limits::none())
            .map_err(|source| NodeError::Xdr {
                what: "transaction envelope".into(),
                source,
            })?;
        self.submit_base64(&b64)
    }

    /// Submits a base64 `TransactionEnvelope` through `tx?blob=`.
    pub fn submit_base64(&self, envelope_b64: &str) -> Result<SubmitResult, NodeError> {
        let what = "tx".to_owned();
        let body = self
            .http
            .command(&format!("tx?blob={}", percent_encode(envelope_b64)))?;
        if let Some(message) = exception(&body) {
            return Err(NodeError::Rejected { what, message });
        }
        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|_| NodeError::Unexpected {
                what: what.clone(),
                body: body.clone(),
            })?;
        let status = match v.get("status").and_then(|s| s.as_str()) {
            Some("PENDING") => SubmitStatus::Pending,
            Some("DUPLICATE") => SubmitStatus::Duplicate,
            Some("ERROR") => SubmitStatus::Error,
            Some("TRY_AGAIN_LATER") => SubmitStatus::TryAgainLater,
            Some("FILTERED") => SubmitStatus::Filtered,
            _ => return Err(NodeError::Unexpected { what, body }),
        };
        let error = match v.get("error").and_then(|e| e.as_str()) {
            Some(b64) => Some(
                TransactionResult::from_xdr_base64(b64, decode_limits(b64.len())).map_err(
                    |source| NodeError::Xdr {
                        what: "tx error result".into(),
                        source,
                    },
                )?,
            ),
            None => None,
        };
        if (status == SubmitStatus::Error) != error.is_some() {
            return Err(NodeError::Unexpected { what, body });
        }
        let diagnostic_events = match v.get("diagnostic_events").and_then(|e| e.as_str()) {
            Some(b64) => Some(
                VecM::<DiagnosticEvent>::from_xdr_base64(b64, decode_limits(b64.len()))
                    .map_err(|source| NodeError::Xdr {
                        what: "tx diagnostic events".into(),
                        source,
                    })?
                    .to_vec(),
            ),
            None => None,
        };
        Ok(SubmitResult {
            status,
            error,
            diagnostic_events,
        })
    }

    /// Loads entries through the query server's `getledgerentry`, at the last
    /// closed ledger. TTL keys are not accepted by core; it reports TTLs
    /// alongside the Soroban entries instead.
    pub fn ledger_entries(&self, keys: &[LedgerKey]) -> Result<LedgerEntries, NodeError> {
        query_ledger_entries(&self.http, keys)
    }

    /// Sequence number of an account at the last closed ledger.
    pub fn account_seq(&self, account: &AccountId) -> Result<i64, NodeError> {
        let key = LedgerKey::Account(LedgerKeyAccount {
            account_id: account.clone(),
        });
        let got = self.ledger_entries(&[key])?;
        match got.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::Account(a)) => Ok(a.seq_num.0),
            _ => Err(NodeError::NoAccount(account.to_string())),
        }
    }

    /// The header values simulation needs, from the last closed ledger.
    pub fn ledger_context(&self) -> Result<LedgerContext, NodeError> {
        let info = self.info()?;
        Ok(LedgerContext {
            network_passphrase: self.cfg.passphrase.clone(),
            protocol_version: info.ledger.version,
            sequence_number: info.ledger.num,
            timestamp: info.ledger.close_time,
            base_reserve: info.ledger.base_reserve,
        })
    }

    /// A snapshot fetcher over the query server, pinned to the last closed
    /// ledger: it fails rather than mixing state from two ledgers if a
    /// ledger closes while a simulation runs.
    pub fn snapshot(&self) -> Result<NodeSnapshot, NodeError> {
        Ok(NodeSnapshot {
            http: self.http.clone(),
            ledger_seq: self.info()?.ledger.num,
        })
    }

    /// Arms a Soroban config upgrade and closes `ledger_seq` with it. The
    /// upgrade set must already be stored as a live temporary contract data
    /// entry; core checks that and the set's validity when it is armed
    /// (`CommandHandler::upgrades`, v30.0.0) and answers "Error setting
    /// configUpgradeSet" otherwise. Only the config upgrade is armed: the
    /// call replaces every pending upgrade parameter.
    pub fn config_upgrade(
        &self,
        key_base64: &str,
        ledger_seq: u32,
        close_time: u64,
    ) -> Result<ClosedLedger, NodeError> {
        self.upgrade_with(
            &[("configupgradesetkey", key_base64)],
            ledger_seq,
            close_time,
        )
    }

    /// Arms the upgrades named by `params`, pairs of core's `upgrades`
    /// query parameters (`maxsorobantxsetsize`, `basefee`,
    /// `configupgradesetkey` and so on, `CommandHandler::upgrades`, v30.0.0),
    /// and closes `ledger_seq` with them. The call replaces every pending
    /// upgrade parameter.
    pub fn upgrade_with(
        &self,
        params: &[(&str, &str)],
        ledger_seq: u32,
        close_time: u64,
    ) -> Result<ClosedLedger, NodeError> {
        let names: Vec<&str> = params.iter().map(|(k, _)| *k).collect();
        let what = format!("upgrade {}", names.join(", "));
        let mut query = format!(
            "upgrades?mode=set&upgradetime={}",
            percent_encode(UPGRADE_TIME)
        );
        for (k, v) in params {
            query.push_str(&format!("&{k}={}", percent_encode(v)));
        }
        let body = self.http.command(&query)?;
        if !body.trim().is_empty() {
            let message = exception(&body).unwrap_or(body);
            return Err(NodeError::Rejected { what, message });
        }
        self.close(ledger_seq, close_time)
    }

    /// `sorobaninfo?format=basic`: core's view of the live Soroban limits.
    pub fn soroban_info(&self) -> Result<serde_json::Value, NodeError> {
        let body = self.http.command("sorobaninfo?format=basic")?;
        serde_json::from_str(&body).map_err(|_| NodeError::Unexpected {
            what: "sorobaninfo".into(),
            body,
        })
    }

    /// `sorobaninfo?format=upgrade_xdr`: every upgradeable CONFIG_SETTING
    /// entry of the last closed ledger, as one upgrade set.
    pub fn soroban_upgrade_set(&self) -> Result<ConfigUpgradeSet, NodeError> {
        let body = self.http.command("sorobaninfo?format=upgrade_xdr")?;
        let b64 = body.trim();
        ConfigUpgradeSet::from_xdr_base64(b64, decode_limits(b64.len())).map_err(|source| {
            NodeError::Xdr {
                what: format!(
                    "sorobaninfo upgrade_xdr {:?}",
                    b64.chars().take(80).collect::<String>()
                ),
                source,
            }
        })
    }

    /// Latest checkpoint in the local archive's `.well-known/stellar-history.json`.
    pub fn published_checkpoint(&self) -> Result<Option<u32>, NodeError> {
        let path = self.history_dir().join(".well-known/stellar-history.json");
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(NodeError::WorkdirIo {
                    path: path.display().to_string(),
                    source,
                });
            }
        };
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            // core writes the file in place; a torn read retries next poll
            Err(_) => return Ok(None),
        };
        let has = v.get("currentLedger").and_then(|c| c.as_u64());
        // new-hist writes a placeholder with currentLedger 0
        Ok(has.and_then(|c| u32::try_from(c).ok()).filter(|c| *c > 0))
    }

    /// Waits until the archive has published `checkpoint`.
    pub fn wait_published(&self, checkpoint: u32, timeout: Duration) -> Result<(), NodeError> {
        // the wait polls files, not core, so without this the idle watcher would pause the
        // container mid publish and the wait would time out on a frozen node
        let _busy = self.http.busy()?;
        let start = Instant::now();
        loop {
            let got = self.published_checkpoint()?;
            if got.is_some_and(|c| c >= checkpoint) {
                return Ok(());
            }
            if start.elapsed() > timeout {
                return Err(NodeError::PublishTimeout {
                    checkpoint,
                    waited: timeout,
                    published: got,
                });
            }
            if !self.running()? {
                return Err(NodeError::Exited {
                    container: self.container().to_owned(),
                    logs: self.logs_tail(60),
                });
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Stops core, leaving its container and network for [`CoreNode::stop`]
    /// to remove. Once it returns nothing in the workdir changes any more, so
    /// files core writes on its own schedule, such as the history archive, can
    /// be copied without racing a write. The node answers no commands after.
    pub fn halt(&self) -> Result<(), NodeError> {
        self.http.quiesce();
        docker_ok(&["stop", "-t", "20", self.container()])?;
        Ok(())
    }

    /// Stops core and removes the container and its network. Dropping a
    /// node does the same but can only print failures.
    pub fn stop(mut self) -> Result<(), NodeError> {
        self.http.quiesce();
        self.resources.release()
    }
}

/// `getledgerentry` on the query server behind `http`.
fn query_ledger_entries(http: &CoreHttp, keys: &[LedgerKey]) -> Result<LedgerEntries, NodeError> {
    let what = "getledgerentry".to_owned();
    if keys.is_empty() {
        return Err(NodeError::NoKeys);
    }
    let mut form = Vec::with_capacity(keys.len());
    for k in keys {
        let b64 = k
            .to_xdr_base64(Limits::none())
            .map_err(|source| NodeError::Xdr {
                what: "ledger key".into(),
                source,
            })?;
        form.push(format!("key={}", percent_encode(&b64)));
    }
    let body = match http.query("getledgerentry", &form.join("&")) {
        Ok(b) => b,
        // core's query server answers a refused query with http 404 and
        // its reason as the body (QueryServer.cpp, v30.0.0)
        Err(HttpError::Status { status, body, .. }) => {
            return Err(NodeError::Rejected {
                what,
                message: format!("http {status}: {}", body.trim()),
            });
        }
        Err(e) => return Err(e.into()),
    };
    #[derive(Deserialize)]
    struct Raw {
        #[serde(rename = "ledgerSeq")]
        ledger_seq: u32,
        #[serde(default)]
        entries: Vec<RawEntry>,
    }
    #[derive(Deserialize)]
    struct RawEntry {
        state: String,
        entry: Option<String>,
        #[serde(rename = "liveUntilLedgerSeq")]
        live_until_ledger_seq: Option<u32>,
    }
    let raw: Raw = serde_json::from_str(&body).map_err(|_| NodeError::Unexpected {
        what: what.clone(),
        body: body.clone(),
    })?;
    if raw.entries.len() != keys.len() {
        return Err(NodeError::Unexpected { what, body });
    }
    let mut entries = Vec::with_capacity(raw.entries.len());
    for e in raw.entries {
        let state = match e.state.as_str() {
            "live" => EntryState::Live,
            "archived" => EntryState::Archived,
            "not-found" => EntryState::NotFound,
            _ => return Err(NodeError::Unexpected { what, body }),
        };
        let entry = match e.entry {
            Some(b64) => Some(
                LedgerEntry::from_xdr_base64(&b64, decode_limits(b64.len())).map_err(|source| {
                    NodeError::Xdr {
                        what: "ledger entry".into(),
                        source,
                    }
                })?,
            ),
            None => None,
        };
        if (state == EntryState::NotFound) == entry.is_some() {
            return Err(NodeError::Unexpected { what, body });
        }
        entries.push(QueriedEntry {
            state,
            entry,
            live_until_ledger_seq: e.live_until_ledger_seq,
        });
    }
    Ok(LedgerEntries {
        ledger_seq: raw.ledger_seq,
        entries,
    })
}

/// Snapshot fetcher over a node's query server, see [`CoreNode::snapshot`].
#[derive(Debug, Clone)]
pub struct NodeSnapshot {
    http: CoreHttp,
    ledger_seq: u32,
}

impl NodeSnapshot {
    pub fn ledger_seq(&self) -> u32 {
        self.ledger_seq
    }
}

impl SnapshotFetcher for NodeSnapshot {
    fn fetch(&self, keys: &[LedgerKey]) -> Result<Vec<Option<FetchedEntry>>, FetchError> {
        let got = query_ledger_entries(&self.http, keys)?;
        if got.ledger_seq != self.ledger_seq {
            return Err(Box::new(NodeError::StaleQuery {
                want: self.ledger_seq,
                got: got.ledger_seq,
            }));
        }
        Ok(got
            .entries
            .into_iter()
            .map(|q| {
                q.entry.map(|entry| FetchedEntry {
                    entry,
                    // core reports 0 for archived entries, which the
                    // simulation then treats as archived, as it should
                    live_until_ledger_seq: q.live_until_ledger_seq,
                })
            })
            .collect())
    }
}

/// Ledgers that end a history checkpoint (64 ledgers each, core's default
/// without `ARTIFICIALLY_ACCELERATE_TIME_FOR_TESTING`) up to `last`.
pub fn checkpoints_through(last: u32) -> Vec<u32> {
    (1..)
        .map(|k: u32| k * 64 - 1)
        .take_while(|c| *c <= last)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_ledgers() {
        assert!(checkpoints_through(62).is_empty());
        assert_eq!(checkpoints_through(63), vec![63]);
        assert_eq!(checkpoints_through(200), vec![63, 127, 191]);
    }

    #[test]
    fn info_answers_are_classified() {
        // the exact body core v30.0.0 sends while booting
        let booting = parse_info(r#"{"error": "Core is booting, try again later"}"#);
        assert!(matches!(booting, Err(NodeError::Booting)));
        assert!(not_up_yet(&booting.unwrap_err()));

        let info = parse_info(
            r#"{"info": {"build": "v30.0.0", "network": "n", "protocol_version": 30,
            "state": "Joining SCP", "ledger": {"num": 1, "version": 0, "closeTime": 0,
            "hash": "00", "baseFee": 100, "baseReserve": 100000000, "maxTxSetSize": 100}}}"#,
        )
        .unwrap();
        assert_eq!(info.state, "Joining SCP");

        for other in [
            r#"{"error": "something else"}"#,
            r#"{"exception": "Core is booting, try again later"}"#,
            "Core is booting, try again later",
            "",
        ] {
            let e = parse_info(other).unwrap_err();
            assert!(matches!(e, NodeError::Unexpected { .. }), "{other}: {e}");
            assert!(!not_up_yet(&e), "{other}");
        }
        let http = NodeError::Http(HttpError::Output {
            what: "info".into(),
            output: String::new(),
        });
        assert!(not_up_yet(&http));
    }

    #[test]
    fn image_inspect_failures_are_told_apart() {
        // docker 29 wording; older clients print "Error: No such image: ..."
        assert!(image_missing(
            "Error response from daemon: No such image: stellar/stellar-core:x"
        ));
        assert!(image_missing(
            "Error: No such image: stellar/stellar-core:x"
        ));
        assert!(!image_missing(
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?"
        ));
        assert!(!image_missing(
            "permission denied while trying to connect to the Docker daemon socket at unix:///var/run/docker.sock"
        ));
    }

    #[test]
    fn a_container_that_was_never_created_needs_no_release() {
        let docker_err = |stderr: &str| NodeError::Docker {
            args: "rm -f -v x".into(),
            code: Some(1),
            stderr: stderr.into(),
            stdout: String::new(),
        };
        // docker 29 wording
        assert!(no_such_container(&docker_err(
            "Error response from daemon: No such container: shapekit-node-1"
        )));
        assert!(!no_such_container(&docker_err(
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock."
        )));
        // a network recorded before its create failed
        assert!(no_such_network(&docker_err(
            "Error response from daemon: network shapekit-core-net-1-0 not found"
        )));
        assert!(!no_such_network(&docker_err(
            "Error response from daemon: error while removing network: network shapekit-core-net-1-0 has active endpoints"
        )));
    }

    #[test]
    fn exception_bodies() {
        assert_eq!(
            exception("{\"exception\": \"too early\"}").as_deref(),
            Some("too early")
        );
        assert_eq!(exception("Manually closed ledger"), None);
    }
}
