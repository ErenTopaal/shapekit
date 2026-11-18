//! Client side of the testee protocol: starts a testee under a memory limit, says hello, and
//! sends one request at a time with a timeout that covers writing the request and reading the
//! answer. A testee that times out, dies or breaks the protocol is restarted.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Map, Value};

#[cfg(not(target_os = "macos"))]
use crate::versions::MemoryResource;
use crate::versions::Target;

pub const PROTOCOL_VERSION: u64 = 2;

/// Protocol versions the runner talks to. Version 2 only adds ops, so a version 1 testee
/// still runs; it cannot list the new ops, whose checks then count as `unsupported`.
pub const PROTOCOLS: [u64; 2] = [1, 2];

/// Every op of protocol version 2 except `hello`.
pub const OPS: &[&str] = &[
    "decode_encode",
    "decode_json",
    "encode_json",
    "strkey_decode",
    "strkey_encode",
    "asset_order",
    "pool_id",
    "map_sort",
    "map_validate",
    "id_parse",
    "to_native",
    "address_from_scval",
    "tx_hash",
    "balance_id",
    "contract_id",
    "coverage_snapshot",
    "coverage_new",
];

// how much of a testee's stderr is kept for error reports
const STDERR_TAIL: usize = 16 * 1024;

/// What a testee answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Ok(Value),
    Error { class: ErrorClass, message: String },
    Unsupported(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Rejected,
    Panic,
    BadRequest,
    Internal,
}

impl ErrorClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorClass::Rejected => "rejected",
            ErrorClass::Panic => "panic",
            ErrorClass::BadRequest => "bad-request",
            ErrorClass::Internal => "internal",
        }
    }

    fn parse(s: &str) -> Option<ErrorClass> {
        Some(match s {
            "rejected" => ErrorClass::Rejected,
            "panic" => ErrorClass::Panic,
            "bad-request" => ErrorClass::BadRequest,
            "internal" => ErrorClass::Internal,
            _ => return None,
        })
    }
}

/// Why a request produced no reply at all.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    Timeout { ms: u64 },
    Exited { status: String, stderr: String },
    Protocol { message: String, stderr: String },
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Timeout { ms } => write!(f, "no answer within {ms} ms"),
            Failure::Exited { status, stderr } => {
                write!(f, "testee exited ({status})")?;
                if !stderr.is_empty() {
                    write!(f, "; stderr tail: {stderr}")?;
                }
                Ok(())
            }
            Failure::Protocol { message, stderr } => {
                write!(f, "protocol violation: {message}")?;
                if !stderr.is_empty() {
                    write!(f, "; stderr tail: {stderr}")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hello {
    pub protocol: u64,
    pub sdk: String,
    pub sdk_version: String,
    pub components: std::collections::BTreeMap<String, String>,
    pub ops: Vec<String>,
    pub decode_limits: String,
}

/// Why the stdout reader stopped short of a line.
#[derive(Debug)]
enum LineError {
    Io(std::io::Error),
    TooLong(usize),
    NotUtf8,
}

struct Running {
    child: Child,
    /// Request lines for the writer thread, which owns the child's stdin; dropping the sender
    /// closes stdin once the thread is idle.
    requests: Option<Sender<Vec<u8>>>,
    written: Receiver<std::io::Result<()>>,
    lines: Receiver<Result<String, LineError>>,
    stderr: Arc<Mutex<VecDeque<u8>>>,
    /// Stops the footprint sampler when the process is dropped.
    #[cfg(target_os = "macos")]
    _footprint: footprint::Watch,
}

pub struct Testee {
    target: Target,
    root: PathBuf,
    running: Option<Running>,
    next_id: u64,
    hello: Hello,
    /// How many times the process was started.
    pub starts: u32,
}

impl Testee {
    /// Starts the testee and checks its hello against the target.
    pub fn start(root: &Path, target: &Target) -> Result<Testee> {
        let mut t = Testee {
            target: target.clone(),
            root: root.to_path_buf(),
            running: None,
            next_id: 0,
            hello: Hello {
                protocol: 0,
                sdk: String::new(),
                sdk_version: String::new(),
                components: Default::default(),
                ops: Vec::new(),
                decode_limits: String::new(),
            },
            starts: 0,
        };
        t.restart()?;
        Ok(t)
    }

    pub fn hello(&self) -> &Hello {
        &self.hello
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn supports(&self, op: &str) -> bool {
        self.hello.ops.iter().any(|o| o == op)
    }

    fn restart(&mut self) -> Result<()> {
        self.stop()?;
        self.running = Some(spawn(&self.root, &self.target)?);
        self.starts += 1;
        let reply = self
            .exchange(Map::from_iter([("op".to_string(), Value::from("hello"))]))
            .map_err(|f| anyhow::anyhow!("{}: hello failed: {f}", self.target.key()))?;
        let Reply::Ok(result) = reply else {
            bail!("{}: hello was answered with {reply:?}", self.target.key());
        };
        let hello: Hello = serde_json::from_value(result)
            .with_context(|| format!("{}: hello result", self.target.key()))?;
        ensure!(
            PROTOCOLS.contains(&hello.protocol),
            "{}: testee speaks protocol {}, the runner 1 or {PROTOCOL_VERSION}",
            self.target.key(),
            hello.protocol
        );
        ensure!(
            hello.sdk == self.target.package,
            "{}: testee wraps {}, versions.toml says {}",
            self.target.key(),
            hello.sdk,
            self.target.package
        );
        ensure!(
            hello.sdk_version == self.target.version,
            "{}: testee was built with {} {}, not {}",
            self.target.key(),
            hello.sdk,
            hello.sdk_version,
            self.target.version
        );
        for op in &hello.ops {
            ensure!(
                OPS.contains(&op.as_str()),
                "{}: hello lists unknown op {op}",
                self.target.key()
            );
        }
        ensure!(
            hello.decode_limits == "honoured" || hello.decode_limits == "ignored",
            "{}: decode_limits is {:?}",
            self.target.key(),
            hello.decode_limits
        );
        self.hello = hello;
        Ok(())
    }

    /// Sends one request (`op` and its members, no id). A timeout or a dead process restarts
    /// the testee and returns the failure; the next request goes to the new process.
    pub fn request(&mut self, request: Map<String, Value>) -> Result<Result<Reply, Failure>> {
        match self.exchange(request) {
            Ok(reply) => Ok(Ok(reply)),
            Err(failure) => {
                self.restart().with_context(|| {
                    format!("{}: restarting after: {failure}", self.target.key())
                })?;
                Ok(Err(failure))
            }
        }
    }

    fn exchange(&mut self, mut request: Map<String, Value>) -> Result<Reply, Failure> {
        let timeout = self.target.timeout_ms;
        let running = self
            .running
            .as_mut()
            .expect("a started testee has a process");
        self.next_id += 1;
        let id = self.next_id.to_string();
        request.insert("id".into(), Value::from(id.clone()));
        let mut line = serde_json::to_vec(&Value::Object(request))
            .expect("serializing a json value cannot fail");
        line.push(b'\n');
        // a testee that stops reading blocks a write larger than the pipe buffer, so the write
        // runs on its own thread and shares the request's deadline with the read
        let deadline = Instant::now() + Duration::from_millis(timeout);
        let left = || deadline.saturating_duration_since(Instant::now());
        let sent = running
            .requests
            .as_ref()
            .is_some_and(|tx| tx.send(line).is_ok());
        if !sent {
            return Err(exited(running, "writing the request: stdin is closed"));
        }
        match running.written.recv_timeout(left()) {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(exited(running, &format!("writing the request: {err}"))),
            Err(RecvTimeoutError::Timeout) => return Err(Failure::Timeout { ms: timeout }),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(exited(running, "writing the request: stdin is closed"));
            }
        }
        let stderr = || tail(&running.stderr);
        let text = match running.lines.recv_timeout(left()) {
            Ok(Ok(text)) => text,
            Ok(Err(LineError::Io(err))) => {
                return Err(exited(running, &format!("reading stdout: {err}")));
            }
            Ok(Err(LineError::TooLong(max))) => {
                return Err(Failure::Protocol {
                    message: format!("response line longer than {max} bytes"),
                    stderr: stderr(),
                });
            }
            Ok(Err(LineError::NotUtf8)) => {
                return Err(Failure::Protocol {
                    message: "response line is not utf-8".into(),
                    stderr: stderr(),
                });
            }
            Err(RecvTimeoutError::Timeout) => return Err(Failure::Timeout { ms: timeout }),
            Err(RecvTimeoutError::Disconnected) => return Err(exited(running, "stdout closed")),
        };
        let value = parse_deep(&text).map_err(|e| Failure::Protocol {
            message: format!("response is not json: {e}"),
            stderr: stderr(),
        })?;
        parse_reply(&value, &id).map_err(|message| Failure::Protocol {
            message,
            stderr: stderr(),
        })
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(mut r) = self.running.take() {
            kill_group(&r.child)
                .with_context(|| format!("{}: stopping the testee", self.target.key()))?;
            r.child
                .wait()
                .with_context(|| format!("{}: reaping the testee", self.target.key()))?;
            shapekit_process::cleanup::group_gone(r.child.id());
        }
        Ok(())
    }
}

impl Drop for Testee {
    fn drop(&mut self) {
        if let Some(mut r) = self.running.take() {
            // closing stdin lets a healthy testee exit 0 on its own; the group is killed after
            // the grace period whatever the testee did, so a process it started cannot outlive
            // it, and before the testee is reaped, so the group id still names its processes
            drop(r.requests.take());
            std::thread::sleep(Duration::from_millis(50));
            let key = self.target.key();
            // drop cannot return an error, so a failure is reported where the run's output is
            if let Err(e) = kill_group(&r.child) {
                eprintln!(
                    "{key}: stopping the testee: {}",
                    shapekit_process::error_text::chain(e.as_ref())
                );
            }
            match r.child.wait() {
                Ok(_) => shapekit_process::cleanup::group_gone(r.child.id()),
                Err(e) => eprintln!("{key}: reaping the testee: {e}"),
            }
        }
    }
}

/// Kills every process of the testee's process group: the testee and whatever it started (a
/// shell's children, a package runner's interpreter), which a kill of the testee alone leaves
/// running with the testee's pipes open. A group with no process left is not an error.
fn kill_group(child: &Child) -> Result<()> {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;
    let pid = i32::try_from(child.id()).context("the testee's pid does not fit a pid_t")?;
    match killpg(Pid::from_raw(pid), Signal::SIGKILL) {
        Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
        Err(e) => Err(e).context("killing the testee's process group"),
    }
}

fn exited(running: &mut Running, what: &str) -> Failure {
    // give the process a moment to finish dying so the status is real
    let status = match running.child.wait_timeout_ms(500) {
        Some(s) => s,
        None => format!("still running after {what}"),
    };
    Failure::Exited {
        status: format!("{what}; {status}"),
        stderr: tail(&running.stderr),
    }
}

trait WaitTimeout {
    fn wait_timeout_ms(&mut self, ms: u64) -> Option<String>;
}

impl WaitTimeout for Child {
    fn wait_timeout_ms(&mut self, ms: u64) -> Option<String> {
        let step = 10;
        let mut waited = 0;
        loop {
            match self.try_wait() {
                Ok(Some(status)) => return Some(status.to_string()),
                Ok(None) if waited < ms => {
                    std::thread::sleep(Duration::from_millis(step));
                    waited += step;
                }
                Ok(None) => return None,
                Err(err) => return Some(format!("wait failed: {err}")),
            }
        }
    }
}

fn tail(buf: &Arc<Mutex<VecDeque<u8>>>) -> String {
    let guard = match buf.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let bytes: Vec<u8> = guard.iter().copied().collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

/// Parses a response line without serde_json's 128-level nesting limit: SEP-51 renderings of
/// deep values nest far beyond it.
pub fn parse_deep(text: &str) -> serde_json::Result<Value> {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    let value = Value::deserialize(&mut de)?;
    de.end()?;
    Ok(value)
}

/// Checks one response object against the protocol.
pub fn parse_reply(value: &Value, id: &str) -> Result<Reply, String> {
    let obj = value.as_object().ok_or("response is not an object")?;
    match obj.get("id").and_then(Value::as_str) {
        Some(got) if got == id => {}
        Some(got) => {
            return Err(format!(
                "response id {got:?} answers no request (sent {id:?})"
            ));
        }
        None => return Err("response has no string id".into()),
    }
    let status = obj
        .get("status")
        .and_then(Value::as_str)
        .ok_or("response has no string status")?;
    let allowed: &[&str] = match status {
        "ok" => &["id", "status", "result"],
        "error" => &["id", "status", "error"],
        "unsupported" => &["id", "status", "reason"],
        other => return Err(format!("unknown status {other:?}")),
    };
    if let Some(extra) = obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(format!("{status} response has member {extra:?}"));
    }
    match status {
        "ok" => match obj.get("result") {
            Some(r @ Value::Object(_)) => Ok(Reply::Ok(r.clone())),
            _ => Err("ok response has no result object".into()),
        },
        "error" => {
            let e = obj
                .get("error")
                .and_then(Value::as_object)
                .ok_or("error response has no error object")?;
            let class = e
                .get("class")
                .and_then(Value::as_str)
                .and_then(ErrorClass::parse)
                .ok_or("error.class is not one of rejected, panic, bad-request, internal")?;
            let message = e
                .get("message")
                .and_then(Value::as_str)
                .ok_or("error.message is not a string")?
                .to_string();
            Ok(Reply::Error { class, message })
        }
        _ => match obj.get("reason").and_then(Value::as_str) {
            Some(r) if !r.is_empty() => Ok(Reply::Unsupported(r.to_string())),
            _ => Err("unsupported response has no reason".into()),
        },
    }
}

fn spawn(root: &Path, target: &Target) -> Result<Running> {
    let program = resolve(root, &target.command[0]);
    let mut cmd = Command::new(&program);
    cmd.args(&target.command[1..])
        .current_dir(root)
        .envs(&target.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // its own process group, so a timeout or a stop reaches every process it starts
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    #[cfg(not(target_os = "macos"))]
    limit_memory(&mut cmd, target.memory.resource, target.memory.mib);
    let mut child = cmd.spawn().with_context(|| {
        format!(
            "{}: starting {} (build it first, see versions.toml)",
            target.key(),
            program.display()
        )
    })?;
    // a signal to shapekit does not reach a group of its own, so the signal thread kills it
    shapekit_process::cleanup::group_started(child.id());
    let stdin = child.stdin.take().context("child stdin")?;
    let stdout = child.stdout.take().context("child stdout")?;
    let mut stderr_pipe = child.stderr.take().context("child stderr")?;

    let (requests, request_rx) = channel::<Vec<u8>>();
    let (written_tx, written) = channel();
    std::thread::spawn(move || write_requests(stdin, request_rx, written_tx));

    let (tx, rx) = channel();
    let max = target.max_response_line;
    std::thread::spawn(move || read_lines(BufReader::new(stdout), max, tx));

    let stderr = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_TAIL)));
    #[cfg(target_os = "macos")]
    let footprint = footprint::watch(&child, target.memory.mib, Arc::clone(&stderr))
        .with_context(|| format!("{}: watching the memory of the testee", target.key()))?;
    let sink = Arc::clone(&stderr);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match stderr_pipe.read(&mut chunk) {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    let mut buf = match sink.lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    buf.extend(&chunk[..n]);
                    while buf.len() > STDERR_TAIL {
                        buf.pop_front();
                    }
                }
            }
        }
    });

    Ok(Running {
        child,
        requests: Some(requests),
        written,
        lines: rx,
        stderr,
        #[cfg(target_os = "macos")]
        _footprint: footprint,
    })
}

// owns the child's stdin; stops after the first failed write, since the pipe is then broken
fn write_requests(
    mut stdin: ChildStdin,
    requests: Receiver<Vec<u8>>,
    written: Sender<std::io::Result<()>>,
) {
    for line in requests {
        let result = stdin.write_all(&line).and_then(|()| stdin.flush());
        let failed = result.is_err();
        if written.send(result).is_err() || failed {
            return;
        }
    }
}

/// Sends each stdout line, without its line ending, until end of file. A line longer than
/// `max` bytes is not buffered past `max + 1` bytes: the reader reports it and stops, and the
/// runner restarts the testee.
fn read_lines(mut reader: impl BufRead, max: usize, tx: Sender<Result<String, LineError>>) {
    let cap = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
    loop {
        let mut buf = Vec::new();
        match (&mut reader).take(cap).read_until(b'\n', &mut buf) {
            Ok(0) => return,
            Ok(_) => {
                let complete = buf.last() == Some(&b'\n');
                if !complete && buf.len() > max {
                    let _ = tx.send(Err(LineError::TooLong(max)));
                    return;
                }
                while matches!(buf.last(), Some(b'\n' | b'\r')) {
                    buf.pop();
                }
                let line = String::from_utf8(buf).map_err(|_| LineError::NotUtf8);
                let stop = line.is_err();
                if tx.send(line).is_err() || stop {
                    return;
                }
            }
            Err(err) => {
                let _ = tx.send(Err(LineError::Io(err)));
                return;
            }
        }
    }
}

// a command with a slash is a path relative to the repository root; a bare name is looked up
// on PATH (node, go)
fn resolve(root: &Path, program: &str) -> PathBuf {
    if program.contains('/') {
        root.join(program)
    } else {
        PathBuf::from(program)
    }
}

/// Sets the limit in the child between fork and exec, so only the testee is capped.
#[cfg(not(target_os = "macos"))]
fn limit_memory(cmd: &mut Command, resource: MemoryResource, mib: u64) {
    use nix::sys::resource::{Resource, setrlimit};
    use std::os::unix::process::CommandExt;
    let resource = match resource {
        MemoryResource::As => Resource::RLIMIT_AS,
        MemoryResource::Data => Resource::RLIMIT_DATA,
    };
    let bytes = mib.saturating_mul(1024 * 1024);
    // safety: setrlimit is async-signal-safe and the closure allocates nothing
    unsafe {
        cmd.pre_exec(move || setrlimit(resource, bytes, bytes).map_err(std::io::Error::from));
    }
}

/// The memory limit on macOS. xnu refuses `RLIMIT_AS` and `RLIMIT_DATA` below the process's
/// current virtual size (EINVAL), and on arm64 every process maps hundreds of GiB before its
/// `main` runs, so no useful rlimit can be set. The runner samples the physical footprint of
/// the testee and of every process it started instead, the private memory each dirtied plus
/// what the kernel compressed (the figure Activity Monitor shows), and kills a process once
/// its own footprint passes the limit. That matches linux, where each process inherits the
/// rlimit and is capped on its own.
#[cfg(target_os = "macos")]
mod footprint {
    use std::collections::VecDeque;
    use std::process::Child;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use anyhow::{Result, bail};

    const INTERVAL: Duration = Duration::from_millis(10);

    pub struct Watch {
        stop: Arc<AtomicBool>,
    }

    impl Drop for Watch {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
        }
    }

    pub fn watch(child: &Child, mib: u64, stderr: Arc<Mutex<VecDeque<u8>>>) -> Result<Watch> {
        let stop = Arc::new(AtomicBool::new(false));
        let watch = Watch {
            stop: Arc::clone(&stop),
        };
        let pid = libc::pid_t::try_from(child.id())?;
        let limit = mib.saturating_mul(1024 * 1024);
        // the start time tells this process from a later one that reuses its pid
        let Some(started) = start_time(pid)? else {
            // already exited and reaped: nothing left to cap
            return Ok(watch);
        };
        std::thread::spawn(move || {
            let note = |text: String| {
                let mut buf = match stderr.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                buf.extend(format!("\nshapekit: {text}\n").as_bytes());
                while buf.len() > super::STDERR_TAIL {
                    buf.pop_front();
                }
            };
            while !stop.load(Ordering::Relaxed) {
                match start_time(pid) {
                    Ok(Some(t)) if t == started => {}
                    _ => return,
                }
                match over_limit(pid, limit) {
                    Ok(None) => {}
                    Ok(Some((p, bytes))) => {
                        // SAFETY: kill only sends a signal to a pid of the tree just measured
                        unsafe { libc::kill(p, libc::SIGKILL) };
                        let who = if p == pid {
                            "the testee".to_string()
                        } else {
                            format!("process {p}, which the testee started,")
                        };
                        note(format!(
                            "killed {who} at a physical footprint of {} MiB, over the {mib} MiB limit",
                            bytes / (1024 * 1024)
                        ));
                        if p == pid {
                            return;
                        }
                    }
                    Err(e) => {
                        // a testee whose memory cannot be read cannot be held to the limit
                        // SAFETY: as above, pid was checked to still be this child
                        unsafe { libc::kill(pid, libc::SIGKILL) };
                        note(format!(
                            "killed the testee, its memory could not be read: {}",
                            shapekit_process::error_text::chain(e.as_ref())
                        ));
                        return;
                    }
                }
                std::thread::sleep(INTERVAL);
            }
        });
        Ok(watch)
    }

    /// The first process of the tree under `root` (itself included) whose footprint is over
    /// `limit`, with that footprint.
    fn over_limit(root: libc::pid_t, limit: u64) -> Result<Option<(libc::pid_t, u64)>> {
        let mut todo = vec![root];
        while let Some(p) = todo.pop() {
            match phys_footprint(p)? {
                Some(bytes) if bytes > limit => return Ok(Some((p, bytes))),
                Some(_) => todo.extend(children(p)?),
                // exited between listing and reading
                None => {}
            }
        }
        Ok(None)
    }

    fn children(pid: libc::pid_t) -> Result<Vec<libc::pid_t>> {
        let mut cap = 64usize;
        loop {
            let mut buf: Vec<libc::pid_t> = vec![0; cap];
            let bytes = libc::c_int::try_from(cap * std::mem::size_of::<libc::pid_t>())?;
            // SAFETY: the buffer holds `bytes` bytes; the call returns how many pids it wrote
            let n = unsafe { libc::proc_listchildpids(pid, buf.as_mut_ptr().cast(), bytes) };
            if n < 0 {
                bail!(
                    "proc_listchildpids for pid {pid}: {}",
                    std::io::Error::last_os_error()
                );
            }
            let n = usize::try_from(n)?;
            if n < cap {
                buf.truncate(n);
                return Ok(buf);
            }
            // a full buffer may have cut the list short
            cap *= 4;
        }
    }

    /// The process's start time, or `None` when no process has the pid.
    fn start_time(pid: libc::pid_t) -> Result<Option<(u64, u64)>> {
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        // SAFETY: proc_pidinfo writes at most `size` bytes into the zeroed struct
        let (n, info) = unsafe {
            let mut info: libc::proc_bsdinfo = std::mem::zeroed();
            let n = libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            );
            (n, info)
        };
        if n == size {
            return Ok(Some((info.pbi_start_tvsec, info.pbi_start_tvusec)));
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            return Ok(None);
        }
        bail!("proc_pidinfo(PROC_PIDTBSDINFO) for pid {pid} returned {n}: {err}")
    }

    /// The process's physical footprint in bytes, or `None` when no process has the pid.
    fn phys_footprint(pid: libc::pid_t) -> Result<Option<u64>> {
        // SAFETY: proc_pid_rusage writes one rusage_info_v4 into the zeroed struct
        let (rc, info) = unsafe {
            let mut info: libc::rusage_info_v4 = std::mem::zeroed();
            let rc = libc::proc_pid_rusage(
                pid,
                libc::RUSAGE_INFO_V4,
                (&mut info as *mut libc::rusage_info_v4).cast(),
            );
            (rc, info)
        };
        if rc == 0 {
            return Ok(Some(info.ri_phys_footprint));
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            return Ok(None);
        }
        bail!("proc_pid_rusage for pid {pid}: {err}")
    }
}
