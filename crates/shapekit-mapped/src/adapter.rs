//! The adapter protocol's commands, carried out as the mapping file says.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;
use shapekit_harness::protocol::{
    ADAPTER_PROTOCOL_VERSION, AdapterError, Capability, DeliveryMode, DescribeResult, DumpResult,
    DumpRow, DumpTable, ErrorClass, ExportRecord, ExportResult, Health,
    MIN_ADAPTER_PROTOCOL_VERSION, ProtocolRange, StartParams, StartResult, StatusResult,
    StopResult,
};
use shapekit_oracle::vocab::ORACLE_VOCAB_VERSION;
use shapekit_process::error_text::chain;

use crate::docker::{self, PeakSampler};
use crate::lake::{Lake, Sink};
use crate::mapping::{Mapping, Service};
use crate::source::{self, Source};
use crate::store::{DumpError, Store};
use crate::template::Context;

/// The adapter's name in `describe`.
pub const ADAPTER: &str = "shapekit-mapped-adapter";

/// Every container gets this label with the harness's network name, so the harness can
/// remove what a killed adapter left.
pub const NETWORK_LABEL: &str = "shapekit.docker_network";

/// stop returns at most this much of the ingest log.
const MAX_STOP_LOGS: usize = 1 << 20;

pub type Res<T> = Result<T, AdapterError>;

pub fn err(class: ErrorClass, message: impl std::fmt::Display) -> AdapterError {
    AdapterError {
        class,
        message: message.to_string(),
    }
}

fn target_err(e: anyhow::Error) -> AdapterError {
    err(ErrorClass::Target, chain(e.as_ref()))
}

/// How the adapter was invoked.
pub struct Options {
    pub mapping: Mapping,
    /// `--var name=value`.
    pub vars: BTreeMap<String, String>,
    /// docker's `--cpus` for every container.
    pub cpus: Option<String>,
    /// The buildx builder images are built with.
    pub builder: Option<String>,
}

struct Running {
    passphrase: String,
    store: Option<Store>,
    ingest: Option<String>,
    sampler: Option<PeakSampler>,
    lake: Option<Lake>,
    containers: Vec<String>,
}

pub struct Adapter {
    opts: Options,
    ctx: Context,
    source: Option<Source>,
    facts_read: bool,
    run: Option<Running>,
}

fn random_suffix() -> String {
    let mut b = [0u8; 4];
    // the kernel's randomness, without another dependency; the pid and time make up for a
    // missing /dev/urandom
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut b))
        .is_err()
    {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        b = (t ^ std::process::id()).to_le_bytes();
    }
    hex::encode(b)
}

impl Adapter {
    /// Resolves the variables (defaults, `--var`, patterns) without touching docker or git.
    pub fn new(opts: Options) -> anyhow::Result<Adapter> {
        let m = &opts.mapping;
        let mut ctx = Context::default();
        ctx.set(
            "cache",
            source::user_cache_dir()?.to_string_lossy().into_owned(),
        );
        for k in opts.vars.keys() {
            if !m.vars.contains_key(k) {
                anyhow::bail!("--var {k}: the mapping defines no variable {k}");
            }
        }
        for (name, spec) in &m.vars {
            let v = match (opts.vars.get(name), &spec.default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => ctx.expand(d)?,
                (None, None) if spec.required => {
                    anyhow::bail!(
                        "the mapping needs --var {name}=<value>{}",
                        spec.about
                            .as_deref()
                            .map(|a| format!(": {a}"))
                            .unwrap_or_default()
                    )
                }
                (None, None) => String::new(),
            };
            if let Some(p) = &spec.pattern {
                let re = regex::Regex::new(&format!("^(?:{p})$"))?;
                if !re.is_match(&v) {
                    anyhow::bail!("--var {name}={v:?} does not match {p}");
                }
            }
            ctx.set(name.clone(), v);
        }
        let source = match &m.source {
            Some(s) => Some(Source {
                url: s.git.clone(),
                dir: PathBuf::from(ctx.expand(&s.dir)?),
                commit: ctx.expand(&s.commit)?,
            }),
            None => None,
        };
        Ok(Adapter {
            opts,
            ctx,
            source,
            facts_read: false,
            run: None,
        })
    }

    fn mapping(&self) -> &Mapping {
        &self.opts.mapping
    }

    fn read_facts(&mut self) -> Res<()> {
        if self.facts_read {
            return Ok(());
        }
        let Some(src) = &self.source else {
            self.facts_read = true;
            return Ok(());
        };
        src.ensure().map_err(target_err)?;
        let facts = self.opts.mapping.source.as_ref().map(|s| &s.facts);
        let values = src
            .facts(facts.map(Vec::as_slice).unwrap_or_default())
            .map_err(target_err)?;
        for (k, v) in values {
            self.ctx.set(format!("fact.{k}"), v);
        }
        self.facts_read = true;
        Ok(())
    }

    pub fn describe(&mut self, protocol: u32) -> Res<DescribeResult> {
        if !(MIN_ADAPTER_PROTOCOL_VERSION..=ADAPTER_PROTOCOL_VERSION).contains(&protocol) {
            return Err(err(
                ErrorClass::Unsupported,
                format!(
                    "protocol {protocol}; this adapter speaks {MIN_ADAPTER_PROTOCOL_VERSION} to {ADAPTER_PROTOCOL_VERSION}"
                ),
            ));
        }
        self.read_facts()?;
        let t = &self.mapping().target;
        let parse = |s: &str, what: &str| -> Res<u32> {
            let v = self.ctx.expand(s).map_err(target_err)?;
            v.parse().map_err(|_| {
                err(
                    ErrorClass::Target,
                    format!("target.protocols.{what} is {v:?}, not a protocol number"),
                )
            })
        };
        let protocols = ProtocolRange {
            min: parse(&t.protocols.min, "min")?,
            max: parse(&t.protocols.max, "max")?,
        };
        Ok(DescribeResult {
            protocol,
            adapter: ADAPTER.to_owned(),
            adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
            target: t.name.clone(),
            target_version: self.ctx.expand(&t.version).map_err(target_err)?,
            delivery_modes: t.delivery_modes.clone(),
            default_delivery: t.default_delivery,
            bootstrap: t.bootstrap,
            vocab_version: ORACLE_VOCAB_VERSION,
            protocols,
            claims: self.mapping().claims.clone(),
            capabilities: if protocol >= 2 {
                vec![Capability::Dump, Capability::Restart]
            } else {
                vec![]
            },
        })
    }

    fn run_flags(&self, network: &str) -> Vec<String> {
        let mut f = vec![
            "--label".to_owned(),
            format!("{NETWORK_LABEL}={network}"),
            "--label".to_owned(),
            format!("shapekit.adapter={}", self.mapping().target.name),
        ];
        if let Some(c) = &self.opts.cpus {
            f.push("--cpus".into());
            f.push(c.clone());
        }
        f
    }

    pub fn start(&mut self, p: StartParams) -> Res<StartResult> {
        if self.run.is_some() {
            return Err(err(ErrorClass::BadRequest, "start sent twice"));
        }
        let Some(network) = p.docker_network.clone() else {
            return Err(err(
                ErrorClass::Unsupported,
                "the target runs in containers; the harness must pass docker_network",
            ));
        };
        if !self
            .mapping()
            .target
            .delivery_modes
            .contains(&p.delivery.mode)
        {
            return Err(err(
                ErrorClass::Unsupported,
                format!(
                    "delivery mode {:?}: the mapping offers {}",
                    p.delivery.mode.name(),
                    self.mapping()
                        .target
                        .delivery_modes
                        .iter()
                        .map(|m| m.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
        if p.limits.memory_bytes == 0 {
            return Err(err(ErrorClass::BadRequest, "limits.memory_bytes is 0"));
        }
        self.read_facts()?;
        self.run = Some(Running {
            passphrase: p.network_passphrase.clone(),
            store: None,
            ingest: None,
            sampler: None,
            lake: None,
            containers: Vec::new(),
        });
        self.bring_up(&p, &network)
    }

    fn running(&mut self) -> &mut Running {
        self.run.as_mut().expect("start set the run")
    }

    fn bring_up(&mut self, p: &StartParams, network: &str) -> Res<StartResult> {
        let image = self.image()?;
        let image_id = docker::docker(&[
            "image".into(),
            "inspect".into(),
            "--format".into(),
            "{{.Id}}".into(),
            image.clone(),
        ])
        .map_err(target_err)?;
        let c = &mut self.ctx;
        c.set("image", image.clone());
        c.set("image.id", image_id);
        c.set("chain", p.chain.clone());
        c.set("network", p.network.clone());
        c.set("network_passphrase", p.network_passphrase.clone());
        c.set("protocol", p.protocol.to_string());
        c.set("delivery", p.delivery.mode.name());
        c.set("rpc_url", p.delivery.rpc_url.clone());
        c.set("archive_url", p.delivery.archive_url.clone());
        c.set("lake.url", p.delivery.lake.url.clone());
        c.set("lake.path", p.delivery.lake.path.clone());
        c.set(
            "lake.ledgers_per_batch",
            p.delivery.lake.ledgers_per_batch.to_string(),
        );
        c.set(
            "lake.batches_per_partition",
            p.delivery.lake.batches_per_partition.to_string(),
        );
        c.set("bootstrap", p.ledgers.bootstrap.to_string());
        c.set("last", p.ledgers.last.to_string());
        c.set(
            "checkpoint_frequency",
            p.ledgers.checkpoint_frequency.to_string(),
        );
        c.set("memory_bytes", p.limits.memory_bytes.to_string());
        c.set("docker_network", network.to_owned());
        c.set("workdir", p.workdir.clone());
        let base = self
            .mapping()
            .target
            .container_prefix
            .clone()
            .unwrap_or_else(|| self.mapping().target.name.clone());
        let prefix = format!("{base}-{}", random_suffix());
        let mode = p.delivery.mode;

        let services: Vec<Service> = self
            .mapping()
            .services
            .iter()
            .filter(|s| s.delivery.as_ref().is_none_or(|d| d.contains(&mode)))
            .cloned()
            .collect();
        let mut components = BTreeMap::new();
        for s in &services {
            self.start_service(s, &prefix, network)?;
            if let Some(k) = &s.component {
                components.insert(k.clone(), s.image.clone());
            }
        }

        let db = &self.mapping().database;
        let url = self.ctx.expand(&db.url).map_err(target_err)?;
        let init = db.init.clone();
        let mut store = Store::connect(&url).map_err(target_err)?;
        for sql in &init {
            store.execute(sql).map_err(target_err)?;
        }
        self.running().store = Some(store);

        let mut mounts: Vec<String> = Vec::new();
        let sink = match mode {
            DeliveryMode::Rpc => None,
            DeliveryMode::Sep54S3 => {
                let d = self.mapping().delivery.s3.clone().expect("validated");
                Some(Sink::S3 {
                    bucket_url: self.ctx.expand(&d.bucket_url).map_err(target_err)?,
                })
            }
            DeliveryMode::Sep54Gcs => {
                let d = self.mapping().delivery.gcs.clone().expect("validated");
                Some(Sink::Gcs {
                    url: self.ctx.expand(&d.url).map_err(target_err)?,
                    bucket: self.ctx.expand(&d.bucket).map_err(target_err)?,
                    prefix: self.ctx.expand(&d.prefix).map_err(target_err)?,
                })
            }
            DeliveryMode::Sep54Filesystem => {
                let d = self
                    .mapping()
                    .delivery
                    .filesystem
                    .clone()
                    .expect("validated");
                let dir = PathBuf::from(&p.workdir).join("lake");
                std::fs::create_dir_all(&dir).map_err(|e| {
                    err(
                        ErrorClass::Internal,
                        format!("creating {}: {e}", dir.display()),
                    )
                })?;
                mounts.push("-v".into());
                mounts.push(format!("{}:{}:ro", dir.display(), d.mount));
                self.ctx.set("lake.dir", d.mount.clone());
                Some(Sink::Dir { dir })
            }
        };
        if let Some(sink) = sink {
            let mut lake =
                Lake::new(std::path::Path::new(&p.delivery.lake.path), sink).map_err(target_err)?;
            lake.prepare(Duration::from_secs(180)).map_err(target_err)?;
            lake.copy_up_to(p.ledgers.bootstrap).map_err(|e| {
                err(
                    ErrorClass::Target,
                    format!(
                        "copying the lake up to ledger {}: {}",
                        p.ledgers.bootstrap,
                        chain(e.as_ref())
                    ),
                )
            })?;
            self.running().lake = Some(lake);
        }

        let env = self.env(mode)?;
        let steps = self.mapping().steps.clone();
        for (i, step) in steps.iter().enumerate() {
            let name = format!("{prefix}-step{}", i + 1);
            // a step runs under the harness's memory limit, as the ingest container does
            let limit = p.limits.memory_bytes.to_string();
            let mut args: Vec<String> = vec![
                "run".into(),
                "--rm".into(),
                "--name".into(),
                name.clone(),
                "--network".into(),
                network.into(),
                "--memory".into(),
                limit.clone(),
                "--memory-swap".into(),
                limit,
            ];
            args.extend(self.run_flags(network));
            args.extend(mounts.iter().cloned());
            for e in &env {
                args.push("-e".into());
                args.push(e.clone());
            }
            let img = match &step.image {
                Some(i) => self.ctx.expand(i).map_err(target_err)?,
                None => image.clone(),
            };
            args.push(img);
            args.extend(self.ctx.expand_all(&step.command).map_err(target_err)?);
            self.running().containers.push(name);
            let out = docker::docker_combined(&args);
            let (text, res) = match out {
                Ok(t) => (t, Ok(())),
                Err((t, e)) => (t, Err(e)),
            };
            docker::log(&format!("{}:\n{text}", step.name));
            res.map_err(|e| {
                err(
                    ErrorClass::Target,
                    format!("{}: {}", step.name, chain(e.as_ref())),
                )
            })?;
        }

        let ingest = format!("{prefix}-ingest");
        let limit = p.limits.memory_bytes.to_string();
        let mut args: Vec<String> = vec![
            "run".into(),
            "-d".into(),
            "--name".into(),
            ingest.clone(),
            "--network".into(),
            network.into(),
            "--memory".into(),
            limit.clone(),
            // equal to --memory: no swap, so the limit is a real ceiling
            "--memory-swap".into(),
            limit,
        ];
        if let Some(r) = &self.mapping().ingest.restart {
            args.push("--restart".into());
            args.push(r.clone());
        }
        args.extend(self.run_flags(network));
        args.extend(mounts);
        for e in &env {
            args.push("-e".into());
            args.push(e.clone());
        }
        let img = match &self.mapping().ingest.image {
            Some(i) => self.ctx.expand(i).map_err(target_err)?,
            None => image,
        };
        args.push(img);
        let cmd = self.mapping().ingest.command.clone();
        args.extend(self.ctx.expand_all(&cmd).map_err(target_err)?);
        self.running().containers.push(ingest.clone());
        docker::docker(&args).map_err(|e| {
            err(
                ErrorClass::Target,
                format!("starting ingest: {}", chain(e.as_ref())),
            )
        })?;
        let r = self.running();
        r.ingest = Some(ingest.clone());
        r.sampler = Some(PeakSampler::start(&ingest));

        for (k, v) in self.mapping().components.clone() {
            components.insert(k, self.ctx.expand(&v).map_err(target_err)?);
        }
        Ok(StartResult { components })
    }

    fn image(&mut self) -> Res<String> {
        let img = self.mapping().image.clone();
        if let Some(name) = &img.name {
            let name = self.ctx.expand(name).map_err(target_err)?;
            let present = docker::docker(&["image".into(), "inspect".into(), name.clone()]);
            if present.is_err() {
                docker::docker(&["pull".into(), name.clone()]).map_err(target_err)?;
            }
            return Ok(name);
        }
        let src = self
            .source
            .as_ref()
            .expect("validated: a build has a source");
        source::ensure_image(src, &img, &self.ctx, self.opts.builder.as_deref()).map_err(target_err)
    }

    fn start_service(&mut self, s: &Service, prefix: &str, network: &str) -> Res<()> {
        let name = format!("{prefix}-{}", s.name);
        let mut args: Vec<String> = vec![
            "run".into(),
            "-d".into(),
            "--name".into(),
            name.clone(),
            "--network".into(),
            network.into(),
            "--network-alias".into(),
            name.clone(),
        ];
        if let Some(m) = &s.memory {
            let m = self.ctx.expand(m).map_err(target_err)?;
            args.extend(["--memory".into(), m.clone(), "--memory-swap".into(), m]);
        }
        for (k, v) in self.ctx.expand_map(&s.env).map_err(target_err)? {
            args.push("-e".into());
            args.push(format!("{k}={v}"));
        }
        args.extend(self.run_flags(network));
        if let Some(e) = &s.entrypoint {
            args.push("--entrypoint".into());
            args.push(e.clone());
        }
        args.push(s.image.clone());
        args.extend(self.ctx.expand_all(&s.command).map_err(target_err)?);
        self.running().containers.push(name.clone());
        docker::docker(&args).map_err(|e| {
            err(
                ErrorClass::Target,
                format!("starting {}: {}", s.name, chain(e.as_ref())),
            )
        })?;
        let ip = docker::container_ip(&name, network).map_err(target_err)?;
        self.ctx
            .set(format!("service.{}.host", s.name), name.clone());
        self.ctx.set(format!("service.{}.ip", s.name), ip);
        self.ctx
            .set(format!("service.{}.image", s.name), s.image.clone());
        let Some(ready) = &s.ready else {
            return Ok(());
        };
        let deadline = Instant::now() + Duration::from_secs(ready.seconds);
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| err(ErrorClass::Internal, e))?;
        loop {
            let (ok, why, pause) = if let Some(cmd) = &ready.exec {
                let mut a: Vec<String> = vec!["exec".into(), name.clone()];
                a.extend(self.ctx.expand_all(cmd).map_err(target_err)?);
                match docker::docker(&a) {
                    Ok(_) => (true, String::new(), 500),
                    Err(e) => (false, chain(e.as_ref()), 500),
                }
            } else {
                let url = self
                    .ctx
                    .expand(ready.http.as_deref().expect("validated"))
                    .map_err(target_err)?;
                match client.get(&url).send() {
                    Ok(r) if r.status().as_u16() == 200 => (true, String::new(), 1000),
                    Ok(r) => (false, format!("GET {url}: {}", r.status()), 1000),
                    Err(e) => (false, format!("GET {url}: {e}"), 1000),
                }
            };
            if ok {
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err(err(
                    ErrorClass::Target,
                    format!("{} was not ready within {}s: {why}", s.name, ready.seconds),
                ));
            }
            std::thread::sleep(Duration::from_millis(pause));
        }
    }

    /// `NAME=value` for every variable of `[env]` and of each `[[env_when]]` that applies.
    fn env(&self, mode: DeliveryMode) -> Res<Vec<String>> {
        let m = self.mapping();
        let mut all = m.env.clone();
        for w in &m.env_when {
            let delivery_ok = w.delivery.is_none_or(|d| d == mode);
            let facts_ok = w
                .fact
                .iter()
                .all(|(k, v)| self.ctx.get(&format!("fact.{k}")) == Some(v.as_str()));
            if delivery_ok && facts_ok {
                all.extend(w.env.clone());
            }
        }
        all.iter()
            .map(|(k, v)| Ok(format!("{k}={}", self.ctx.expand(v).map_err(target_err)?)))
            .collect()
    }

    pub fn advance(&mut self, head: u32) -> Res<Value> {
        let Some(run) = self.run.as_mut() else {
            return Err(err(ErrorClass::BadRequest, "advance before start"));
        };
        if let Some(lake) = run.lake.as_mut() {
            lake.copy_up_to(head).map_err(|e| {
                err(
                    ErrorClass::Target,
                    format!(
                        "copying the lake up to ledger {head}: {}",
                        chain(e.as_ref())
                    ),
                )
            })?;
        }
        Ok(serde_json::json!({ "head": head }))
    }

    pub fn status(&mut self) -> Res<StatusResult> {
        let status = self.mapping().status.clone();
        let Some(run) = self.run.as_mut() else {
            return Err(err(
                ErrorClass::BadRequest,
                "status before a successful start",
            ));
        };
        let (Some(ingest), Some(store)) = (run.ingest.clone(), run.store.as_mut()) else {
            return Err(err(
                ErrorClass::BadRequest,
                "status before a successful start",
            ));
        };
        let st = docker::inspect_state(&ingest).map_err(target_err)?;
        let peak = run.sampler.as_ref().and_then(|s| s.peak(&st));
        let cur = store.cursor(&status.cursor).map_err(target_err)?;
        let logs = docker::docker_combined(&[
            "logs".into(),
            "--tail".into(),
            status.log_tail.to_string(),
            ingest.clone(),
        ])
        .map_err(|(_, e)| {
            err(
                ErrorClass::Target,
                format!("reading ingest logs: {}", chain(e.as_ref())),
            )
        })?;
        let mut last_error = last_error(&status.error, &logs);
        let crashed = match &status.crash {
            Some(c) => regex::Regex::new(c)
                .map_err(|e| err(ErrorClass::Internal, e))?
                .is_match(&logs),
            None => false,
        };
        let alive = st.running || st.restarting;
        let health = if st.oom_killed {
            Health::Oom
        } else if crashed || !alive {
            Health::Crashed
        } else if cur.is_none() {
            Health::Starting
        } else {
            Health::Running
        };
        if !st.running && !st.error.is_empty() && last_error.is_none() {
            last_error = Some(st.error.clone());
        }
        Ok(StatusResult {
            last_ingested_ledger: cur,
            health,
            last_error,
            peak_memory_bytes: peak,
        })
    }

    pub fn export(
        &mut self,
        from: u32,
        to: u32,
        emit: &mut dyn FnMut(ExportRecord) -> anyhow::Result<()>,
    ) -> Res<ExportResult> {
        let Some(run) = self.run.as_mut() else {
            return Err(err(
                ErrorClass::BadRequest,
                "export before a successful start",
            ));
        };
        let Some(store) = run.store.as_mut() else {
            return Err(err(
                ErrorClass::BadRequest,
                "export before a successful start",
            ));
        };
        let records = store
            .export(&self.opts.mapping, &run.passphrase, from, to, emit)
            .map_err(target_err)?;
        Ok(ExportResult { records })
    }

    pub fn dump(
        &mut self,
        tables: &[String],
        table: &mut dyn FnMut(DumpTable) -> anyhow::Result<()>,
        row: &mut dyn FnMut(DumpRow) -> anyhow::Result<()>,
    ) -> Res<DumpResult> {
        let Some(store) = self.run.as_mut().and_then(|r| r.store.as_mut()) else {
            return Err(err(
                ErrorClass::BadRequest,
                "dump before a successful start",
            ));
        };
        store.dump(tables, table, row).map_err(|e| match e {
            DumpError::UnknownTable(n) => err(
                ErrorClass::BadRequest,
                format!("no table {n:?} in the store"),
            ),
            DumpError::Store(e) => target_err(e),
        })
    }

    /// docker restarts the ingest container, which keeps the database, so the target resumes
    /// from its stored cursor as it does after a deployment restart.
    pub fn restart(&mut self) -> Res<Value> {
        let Some(ingest) = self.run.as_ref().and_then(|r| r.ingest.clone()) else {
            return Err(err(
                ErrorClass::BadRequest,
                "restart before a successful start",
            ));
        };
        docker::docker(&["restart".into(), ingest]).map_err(target_err)?;
        Ok(serde_json::json!({}))
    }

    pub fn stop(&mut self) -> Res<StopResult> {
        let logs = self.teardown().map_err(|e| {
            err(
                ErrorClass::Internal,
                format!("removing containers: {}", chain(e.as_ref())),
            )
        })?;
        Ok(StopResult { logs })
    }

    /// Removes every container the target created and returns the tail of the ingest log.
    pub fn teardown(&mut self) -> anyhow::Result<String> {
        let Some(mut run) = self.run.take() else {
            return Ok(String::new());
        };
        if let Some(mut s) = run.sampler.take() {
            s.stop();
        }
        let mut errs = Vec::new();
        let mut logs = String::new();
        if let Some(i) = &run.ingest {
            match docker::logs_tail(i, 20000, MAX_STOP_LOGS) {
                Ok(l) => logs = l,
                Err(e) if chain(e.as_ref()).contains("No such container") => {}
                Err(e) => errs.push(chain(e.as_ref())),
            }
        }
        drop(run.store.take());
        // the ingest container first, so nothing writes while its services go away
        for name in run.containers.iter().rev() {
            if let Err(e) = docker::docker(&["rm".into(), "-f".into(), "-v".into(), name.clone()])
                && !chain(e.as_ref()).contains("No such container")
            {
                errs.push(chain(e.as_ref()));
            }
        }
        if errs.is_empty() {
            Ok(logs)
        } else {
            anyhow::bail!("{}", errs.join("; "))
        }
    }
}

type CompiledRule<'a> = (regex::Regex, Option<&'a Vec<String>>, Option<regex::Regex>);

/// The newest log line that a rule matches.
pub fn last_error(rules: &[crate::mapping::ErrorRule], logs: &str) -> Option<String> {
    let compiled: Vec<CompiledRule<'_>> = rules
        .iter()
        .filter_map(|r| {
            Some((
                regex::Regex::new(&r.pattern).ok()?,
                r.values.as_ref(),
                match &r.also {
                    Some(a) => Some(regex::Regex::new(a).ok()?),
                    None => None,
                },
            ))
        })
        .collect();
    let mut last = None;
    for line in logs.lines() {
        let hit = compiled.iter().any(|(re, values, also)| {
            let Some(c) = re.captures(line) else {
                return false;
            };
            if let Some(vs) = values {
                let Some(g) = c.get(1) else { return false };
                if !vs.iter().any(|v| v == g.as_str()) {
                    return false;
                }
            }
            also.as_ref().is_none_or(|a| a.is_match(line))
        });
        if hit {
            last = Some(line.to_owned());
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::target_err;

    // shaped like the library errors: the message ends with the source, which source()
    // also returns
    #[derive(Debug)]
    struct Reading(std::io::Error);

    impl std::fmt::Display for Reading {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "reading /x.json: {}", self.0)
        }
    }

    impl std::error::Error for Reading {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn a_target_error_prints_each_cause_once() {
        let io = || std::io::Error::from(std::io::ErrorKind::NotFound);
        let e = anyhow::Error::new(Reading(io())).context("starting the target");
        let message = target_err(e).message;
        assert_eq!(
            message,
            format!("starting the target: reading /x.json: {}", io())
        );
    }
}
