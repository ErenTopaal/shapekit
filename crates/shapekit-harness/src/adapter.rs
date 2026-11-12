//! The harness side of the adapter protocol: one adapter child process, requests on its
//! stdin, response lines read by a thread from its stdout.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command as Process, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, sync_channel};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;

use crate::protocol::{
    AdapterError, Command, ExportRecord, ExportResult, Request, Response, ResponseBody,
};

/// Longest response line accepted. An export record of one transaction is far smaller; a
/// longer line means the adapter is broken.
const MAX_LINE: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum AdapterFailure {
    #[error("starting the adapter {command:?}: {source}")]
    Spawn {
        command: Vec<String>,
        source: std::io::Error,
    },
    #[error("no answer to {cmd} within {waited:?}")]
    Timeout { cmd: String, waited: Duration },
    #[error("the adapter exited while answering {cmd} ({status})")]
    Exited { cmd: String, status: String },
    #[error("protocol violation answering {cmd}: {message}")]
    Protocol { cmd: String, message: String },
    #[error("{cmd} failed ({}): {}", class_name(.error), .error.message)]
    Error { cmd: String, error: AdapterError },
    #[error("writing to the adapter: {0}")]
    Write(std::io::Error),
}

fn class_name(e: &AdapterError) -> String {
    serde_json::to_value(e.class)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

enum Line {
    Text(String),
    TooLong(usize),
    Eof,
    ReadError(String),
}

pub struct Adapter {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Line>,
    next_id: u64,
    reader: Option<std::thread::JoinHandle<()>>,
    /// Requests the harness stopped waiting for, whose answers may still arrive.
    abandoned: HashSet<String>,
    /// Lines of abandoned requests that arrived later and were skipped, for the report.
    pub late_answers: Vec<String>,
}

fn cmd_name(c: &Command) -> &'static str {
    match c {
        Command::Describe { .. } => "describe",
        Command::Start(_) => "start",
        Command::Advance { .. } => "advance",
        Command::Status => "status",
        Command::Export { .. } => "export",
        Command::Stop => "stop",
    }
}

impl Adapter {
    /// Starts `command` (program then arguments) with its stderr going to `stderr_log`.
    pub fn spawn(command: &[String], stderr_log: &Path) -> Result<Adapter, AdapterFailure> {
        let spawn_err = |source| AdapterFailure::Spawn {
            command: command.to_vec(),
            source,
        };
        let (program, args) = command
            .split_first()
            .ok_or_else(|| spawn_err(std::io::Error::other("the adapter command is empty")))?;
        let log = File::create(stderr_log).map_err(spawn_err)?;
        let mut child = Process::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .spawn()
            .map_err(spawn_err)?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let stdin = child.stdin.take();
        // bounded, so a chatty adapter waits for the harness instead of filling memory
        let (tx, rx) = sync_channel(64);
        let reader = std::thread::Builder::new()
            .name("adapter-stdout".into())
            .spawn(move || {
                let mut r = BufReader::new(stdout);
                let mut buf = Vec::new();
                loop {
                    buf.clear();
                    let read = std::io::Read::take(&mut r, MAX_LINE as u64 + 1)
                        .read_until(b'\n', &mut buf);
                    let msg = match read {
                        Ok(0) => Line::Eof,
                        Ok(n) if n > MAX_LINE => Line::TooLong(n),
                        Ok(_) => match String::from_utf8(std::mem::take(&mut buf)) {
                            Ok(s) => Line::Text(s),
                            Err(e) => Line::ReadError(format!("a line is not utf-8: {e}")),
                        },
                        Err(e) => Line::ReadError(e.to_string()),
                    };
                    let last = !matches!(msg, Line::Text(_));
                    if tx.send(msg).is_err() || last {
                        return;
                    }
                }
            })
            .map_err(spawn_err)?;
        Ok(Adapter {
            child,
            stdin,
            lines: rx,
            next_id: 0,
            reader: Some(reader),
            abandoned: HashSet::new(),
            late_answers: Vec::new(),
        })
    }

    fn send(&mut self, cmd: Command) -> Result<String, AdapterFailure> {
        let id = self.next_id.to_string();
        self.next_id += 1;
        let mut line = serde_json::to_string(&Request {
            id: id.clone(),
            command: cmd,
        })
        .expect("a request is plain json");
        line.push('\n');
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| AdapterFailure::Write(std::io::Error::other("stdin is closed")))?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(AdapterFailure::Write)?;
        Ok(id)
    }

    fn exit_status(&mut self) -> String {
        // the pipe closed, so the process is gone or about to be
        for _ in 0..50 {
            if let Ok(Some(s)) = self.child.try_wait() {
                return s.to_string();
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        "still running with stdout closed".into()
    }

    /// Reads the next response line for `id`. A line that answers a request this adapter
    /// gave up on (see `abandoned`) is skipped, so an answer that arrives after its timeout,
    /// while the harness is already waiting on the next request (in practice `stop`), is not
    /// a protocol violation. Any other foreign id still is.
    fn next(&mut self, cmd: &str, id: &str, deadline: Instant) -> Result<Response, AdapterFailure> {
        loop {
            let resp = self.next_line(cmd, id, deadline)?;
            if resp.id == id {
                return Ok(resp);
            }
            if self.abandoned.contains(&resp.id) {
                let last = !matches!(resp.body, ResponseBody::Record { .. });
                if last {
                    self.abandoned.remove(&resp.id);
                }
                self.late_answers.push(format!(
                    "a {} line for abandoned request {} arrived while waiting on {cmd}",
                    match resp.body {
                        ResponseBody::Ok { .. } => "final ok",
                        ResponseBody::Error { .. } => "final error",
                        ResponseBody::Record { .. } => "record",
                    },
                    resp.id
                ));
                continue;
            }
            return Err(AdapterFailure::Protocol {
                cmd: cmd.into(),
                message: format!("response id {:?}, the open request is {id:?}", resp.id),
            });
        }
    }

    fn next_line(
        &mut self,
        cmd: &str,
        id: &str,
        deadline: Instant,
    ) -> Result<Response, AdapterFailure> {
        let wait = deadline.saturating_duration_since(Instant::now());
        let line = match self.lines.recv_timeout(wait) {
            Ok(l) => l,
            Err(RecvTimeoutError::Timeout) => {
                // its answer may still come; the next request must not choke on it
                self.abandoned.insert(id.to_owned());
                return Err(AdapterFailure::Timeout {
                    cmd: cmd.into(),
                    waited: wait,
                });
            }
            Err(RecvTimeoutError::Disconnected) => Line::Eof,
        };
        let protocol = |message: String| AdapterFailure::Protocol {
            cmd: cmd.into(),
            message,
        };
        let text = match line {
            Line::Text(t) => t,
            Line::Eof => {
                return Err(AdapterFailure::Exited {
                    cmd: cmd.into(),
                    status: self.exit_status(),
                });
            }
            Line::TooLong(n) => return Err(protocol(format!("a line of more than {n} bytes"))),
            Line::ReadError(e) => return Err(protocol(e)),
        };
        let resp: Response = serde_json::from_str(text.trim_end()).map_err(|e| {
            let shown: String = text.chars().take(300).collect();
            protocol(format!("not a response line ({e}): {shown}"))
        })?;
        Ok(resp)
    }

    /// Sends `cmd` and returns its result.
    pub fn call<T: DeserializeOwned>(
        &mut self,
        cmd: Command,
        timeout: Duration,
    ) -> Result<T, AdapterFailure> {
        let name = cmd_name(&cmd);
        let deadline = Instant::now() + timeout;
        let id = self.send(cmd)?;
        match self.next(name, &id, deadline)?.body {
            ResponseBody::Ok { result } => {
                serde_json::from_value(result).map_err(|e| AdapterFailure::Protocol {
                    cmd: name.into(),
                    message: format!("the result does not fit the {name} result: {e}"),
                })
            }
            ResponseBody::Error { error } => Err(AdapterFailure::Error {
                cmd: name.into(),
                error,
            }),
            ResponseBody::Record { .. } => Err(AdapterFailure::Protocol {
                cmd: name.into(),
                message: "a record line outside an export".into(),
            }),
        }
    }

    /// Sends `export` and hands each record to `on_record` as it arrives.
    pub fn export(
        &mut self,
        from: u32,
        to: u32,
        timeout: Duration,
        on_record: &mut dyn FnMut(ExportRecord),
    ) -> Result<u64, AdapterFailure> {
        let deadline = Instant::now() + timeout;
        let id = self.send(Command::Export { from, to })?;
        let mut seen = 0u64;
        loop {
            match self.next("export", &id, deadline)?.body {
                ResponseBody::Record { record } => {
                    seen += 1;
                    on_record(*record);
                }
                ResponseBody::Ok { result } => {
                    let r: ExportResult =
                        serde_json::from_value(result).map_err(|e| AdapterFailure::Protocol {
                            cmd: "export".into(),
                            message: format!("the result does not fit the export result: {e}"),
                        })?;
                    if r.records != seen {
                        return Err(AdapterFailure::Protocol {
                            cmd: "export".into(),
                            message: format!("announced {} records, sent {seen}", r.records),
                        });
                    }
                    return Ok(seen);
                }
                ResponseBody::Error { error } => {
                    return Err(AdapterFailure::Error {
                        cmd: "export".into(),
                        error,
                    });
                }
            }
        }
    }

    /// Closes stdin and waits up to `grace` for the adapter to exit, then kills it.
    pub fn finish(mut self, grace: Duration) -> String {
        self.stdin = None;
        let deadline = Instant::now() + grace;
        let status = loop {
            match self.child.try_wait() {
                Ok(Some(s)) => break s.to_string(),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Ok(None) => {
                    let killed = self.child.kill();
                    let waited = self.child.wait();
                    break match (killed, waited) {
                        (Ok(()), Ok(s)) => format!("killed after {grace:?} ({s})"),
                        (k, w) => format!("kill: {k:?}, wait: {w:?}"),
                    };
                }
                Err(e) => break format!("waiting for the adapter: {e}"),
            }
        };
        if let Some(r) = self.reader.take() {
            // dropping the receiver unblocks a reader stuck on a full channel
            drop(std::mem::replace(&mut self.lines, sync_channel(1).1));
            if r.join().is_err() {
                return format!("{status}; the stdout reader panicked");
            }
        }
        status
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        if self.reader.is_some() {
            // finish was not called: an error path; never leave the process behind
            self.stdin = None;
            if let Ok(None) = self.child.try_wait()
                && let Err(e) = self.child.kill()
            {
                eprintln!("shapekit harness: killing the adapter: {e}");
            }
            if let Err(e) = self.child.wait() {
                eprintln!("shapekit harness: waiting for the adapter: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{StatusResult, StopResult};

    // an adapter that answers each request two seconds late
    const SLOW: &str = r#"
read a; sleep 2; echo '{"id":"0","status":"ok","result":{"last_ingested_ledger":null,"health":"running","last_error":null,"peak_memory_bytes":null}}'
read b; echo '{"id":"1","status":"ok","result":{"logs":""}}'
"#;

    fn spawn(script: &str) -> (Adapter, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let a = Adapter::spawn(
            &["sh".into(), "-c".into(), script.into()],
            &dir.path().join("stderr.log"),
        )
        .unwrap();
        (a, dir)
    }

    #[test]
    fn a_late_answer_before_the_next_reply_is_skipped() {
        let (mut a, _dir) = spawn(SLOW);
        let st = a.call::<StatusResult>(Command::Status, Duration::from_millis(300));
        assert!(matches!(st, Err(AdapterFailure::Timeout { .. })), "{st:?}");
        let stop = a.call::<StopResult>(Command::Stop, Duration::from_secs(20));
        assert!(stop.is_ok(), "{stop:?}");
        assert_eq!(a.late_answers.len(), 1, "{:?}", a.late_answers);
        a.finish(Duration::from_secs(5));
    }

    #[test]
    fn a_foreign_id_is_still_a_violation() {
        let (mut a, _dir) =
            spawn(r#"read a; echo '{"id":"7","status":"ok","result":{"logs":""}}'; read b"#);
        let stop = a.call::<StopResult>(Command::Stop, Duration::from_secs(20));
        assert!(
            matches!(stop, Err(AdapterFailure::Protocol { .. })),
            "{stop:?}"
        );
        a.finish(Duration::from_secs(5));
    }
}
