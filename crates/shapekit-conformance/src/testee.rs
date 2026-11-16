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

use crate::versions::{MemoryResource, Target};

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
    "tx_hash",
    "balance_id",
    "contract_id",
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
        self.stop();
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

    fn stop(&mut self) {
        if let Some(mut r) = self.running.take() {
            // the process may already be gone; kill and wait only reap it
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
    }
}

impl Drop for Testee {
    fn drop(&mut self) {
        if let Some(mut r) = self.running.take() {
            // closing stdin lets a healthy testee exit 0 on its own
            drop(r.requests.take());
            match r.child.try_wait() {
                Ok(Some(_)) => {}
                _ => {
                    std::thread::sleep(Duration::from_millis(50));
                    if !matches!(r.child.try_wait(), Ok(Some(_))) {
                        let _ = r.child.kill();
                    }
                    let _ = r.child.wait();
                }
            }
        }
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
    limit_memory(&mut cmd, target.memory.resource, target.memory.mib);
    let mut child = cmd.spawn().with_context(|| {
        format!(
            "{}: starting {} (build it first, see versions.toml)",
            target.key(),
            program.display()
        )
    })?;
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
