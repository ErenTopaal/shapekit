//! The docker command line, the way the target's containers are run and watched.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;

fn last_line(s: &str) -> &str {
    s.trim().rsplit('\n').next().unwrap_or("")
}

/// Runs docker; the error carries docker's own stderr.
pub fn docker(args: &[String]) -> Result<String> {
    let out = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .context("running docker")?;
    if !out.status.success() {
        bail!(
            "docker {}: {}: {}",
            args.first().map(String::as_str).unwrap_or(""),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Runs docker with stdout and stderr in one buffer, for commands that pass a container's
/// two streams through (logs, run --rm).
pub fn docker_combined(args: &[String]) -> Result<String, (String, anyhow::Error)> {
    let run = || -> Result<(std::process::ExitStatus, String)> {
        // one pipe for both streams keeps their interleaving
        let (mut reader, writer) = std::io::pipe().context("creating a pipe")?;
        let mut child = Command::new("docker")
            .args(args)
            .stdin(Stdio::null())
            .stdout(writer.try_clone().context("duplicating a pipe")?)
            .stderr(writer)
            .spawn()
            .context("running docker")?;
        let mut buf = Vec::new();
        reader
            .read_to_end(&mut buf)
            .context("reading docker's output")?;
        let status = child.wait().context("waiting for docker")?;
        Ok((status, String::from_utf8_lossy(&buf).into_owned()))
    };
    match run() {
        Ok((status, text)) if status.success() => Ok(text),
        Ok((status, text)) => {
            let e = anyhow::anyhow!(
                "docker {}: {}: {}",
                args.first().map(String::as_str).unwrap_or(""),
                status,
                last_line(&text)
            );
            Err((text, e))
        }
        Err(e) => Err((String::new(), e)),
    }
}

/// Runs docker with `input` on stdin and its output copied to this process's stderr, which
/// the harness keeps as the adapter log.
pub fn docker_stream(args: &[String], input: &mut dyn Read) -> Result<()> {
    let mut child = Command::new("docker")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(std::io::stderr()))
        .stderr(Stdio::inherit())
        .spawn()
        .context("running docker")?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let copied = std::io::copy(input, &mut stdin);
    drop(stdin);
    let status = child.wait().context("waiting for docker")?;
    if !status.success() {
        bail!(
            "docker {}: {status}",
            args.first().map(String::as_str).unwrap_or("")
        );
    }
    copied.context("feeding docker")?;
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct State {
    pub running: bool,
    pub restarting: bool,
    #[serde(rename = "OOMKilled")]
    pub oom_killed: bool,
    pub pid: i64,
    pub error: String,
}

pub fn inspect_state(name: &str) -> Result<State> {
    let out = docker(&[
        "inspect".into(),
        "--format".into(),
        "{{json .State}}".into(),
        name.into(),
    ])?;
    serde_json::from_str(&out).with_context(|| format!("decoding the state of {name}"))
}

pub fn container_ip(name: &str, network: &str) -> Result<String> {
    let out = docker(&[
        "inspect".into(),
        "--format".into(),
        "{{json .NetworkSettings.Networks}}".into(),
        name.into(),
    ])?;
    #[derive(Deserialize)]
    struct Net {
        #[serde(rename = "IPAddress")]
        ip: String,
    }
    let nets: std::collections::BTreeMap<String, Net> =
        serde_json::from_str(&out).with_context(|| format!("decoding the networks of {name}"))?;
    match nets.get(network) {
        Some(n) if !n.ip.is_empty() => Ok(n.ip.clone()),
        _ => bail!("{name} has no address on {network}"),
    }
}

/// The cgroup v2 `memory.peak` file of a running container's process.
fn peak_path(st: &State) -> Result<PathBuf> {
    if !st.running || st.pid == 0 {
        bail!("ingest is not running, no cgroup to read");
    }
    let text = std::fs::read_to_string(format!("/proc/{}/cgroup", st.pid))
        .context("reading the ingest cgroup")?;
    for line in text.trim().lines() {
        if let Some(rest) = line.strip_prefix("0::") {
            return Ok(PathBuf::from("/sys/fs/cgroup")
                .join(rest.trim_start_matches('/'))
                .join("memory.peak"));
        }
    }
    bail!("ingest is not in a cgroup v2 hierarchy, memory.peak is unavailable")
}

#[derive(Default)]
struct PeakState {
    peak: Option<u64>,
    err: String,
}

/// Samples a container's peak memory once a second while it runs; the cgroup goes away with
/// the container, so the largest value seen is kept.
pub struct PeakSampler {
    name: String,
    state: Arc<Mutex<PeakState>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn sample(path: &PathBuf, state: &Mutex<PeakState>) {
    match std::fs::read_to_string(path) {
        Ok(t) => match t.trim().parse::<u64>() {
            Ok(v) => {
                let mut s = state.lock().expect("peak lock");
                s.peak = Some(s.peak.map_or(v, |p| p.max(v)));
            }
            Err(e) => note(state, format!("memory.peak is {:?}: {e}", t.trim())),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => note(state, e.to_string()),
    }
}

fn note(state: &Mutex<PeakState>, msg: String) {
    let mut s = state.lock().expect("peak lock");
    if s.err != msg {
        eprintln!("peak memory: {msg}");
        s.err = msg;
    }
}

impl PeakSampler {
    pub fn start(name: &str) -> PeakSampler {
        let state = Arc::new(Mutex::new(PeakState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (n, st, sp) = (name.to_owned(), state.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let mut path: Option<PathBuf> = None;
            while !sp.load(Ordering::Relaxed) {
                if path.is_none() {
                    match inspect_state(&n).and_then(|s| peak_path(&s)) {
                        Ok(p) => path = Some(p),
                        Err(e) => note(&st, e.to_string()),
                    }
                }
                if let Some(p) = &path {
                    sample(p, &st);
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        PeakSampler {
            name: name.to_owned(),
            state,
            stop,
            thread: Some(thread),
        }
    }

    /// Samples now, when the container runs, and returns the peak so far.
    pub fn peak(&self, st: &State) -> Option<u64> {
        if st.running
            && let Ok(p) = peak_path(st)
        {
            sample(&p, &self.state);
        }
        self.state.lock().expect("peak lock").peak
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let _ = &self.name;
    }
}

impl Drop for PeakSampler {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The tail of a container's log, at most `max` bytes.
pub fn logs_tail(name: &str, lines: u32, max: usize) -> Result<String> {
    let out = docker_combined(&[
        "logs".into(),
        "--tail".into(),
        lines.to_string(),
        name.into(),
    ])
    .map_err(|(_, e)| e)?;
    if out.len() > max {
        let mut cut = out.len() - max;
        while !out.is_char_boundary(cut) {
            cut += 1;
        }
        return Ok(out[cut..].to_owned());
    }
    Ok(out)
}

/// Writes a line to the adapter log (stderr).
pub fn log(msg: &str) {
    let mut e = std::io::stderr().lock();
    let _ = writeln!(e, "{msg}");
}
