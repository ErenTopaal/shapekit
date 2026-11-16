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
    ADAPTER_PROTOCOL_VERSION, AdapterError, Command, DescribeResult, DumpResult, DumpRow,
    DumpTable, ErrorClass, ExportRecord, ExportResult, MIN_ADAPTER_PROTOCOL_VERSION, Request,
    Response, ResponseBody,
};

/// Longest response line accepted. An export record of one transaction is far smaller; a
/// longer line means the adapter is broken.
const MAX_LINE: usize = 64 * 1024 * 1024;

/// Late answers described one by one; beyond this only their number is kept, so an adapter
/// that keeps answering abandoned requests cannot grow the report without bound.
const MAX_LATE_NOTES: usize = 16;

/// How long `finish` waits for the stdout reader after the adapter is gone. A process the
/// adapter started can hold the pipe open past its death, and the reader would never end.
const READER_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum AdapterFailure {
    #[error("starting the adapter {command:?}: {source}")]
    Spawn {
        command: Vec<String>,
        source: std::io::Error,
    },
    #[error("no answer to {cmd} within {timeout:?}")]
    Timeout { cmd: String, timeout: Duration },
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
    TooLong,
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
    /// Lines of abandoned requests that arrived later and were skipped, for the report: the
    /// first [`MAX_LATE_NOTES`] described, then one line with the number of the rest.
    pub late_answers: Vec<String>,
    late_beyond: u64,
}

fn cmd_name(c: &Command) -> &'static str {
    match c {
        Command::Describe { .. } => "describe",
        Command::Start(_) => "start",
        Command::Advance { .. } => "advance",
        Command::Status => "status",
        Command::Export { .. } => "export",
        Command::Dump { .. } => "dump",
        Command::Restart => "restart",
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
                        Ok(n) if n > MAX_LINE => Line::TooLong,
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
            late_beyond: 0,
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
    fn next(
        &mut self,
        cmd: &str,
        id: &str,
        deadline: Instant,
        timeout: Duration,
    ) -> Result<Response, AdapterFailure> {
        loop {
            let resp = self.next_line(cmd, id, deadline, timeout)?;
            if resp.id == id {
                return Ok(resp);
            }
            if self.abandoned.contains(&resp.id) {
                let last = matches!(
                    resp.body,
                    ResponseBody::Ok { .. } | ResponseBody::Error { .. }
                );
                if last {
                    self.abandoned.remove(&resp.id);
                }
                if self.late_answers.len() >= MAX_LATE_NOTES {
                    self.late_beyond += 1;
                    continue;
                }
                self.late_answers.push(format!(
                    "a {} line for abandoned request {} arrived while waiting on {cmd}",
                    match resp.body {
                        ResponseBody::Ok { .. } => "final ok",
                        ResponseBody::Error { .. } => "final error",
                        ResponseBody::Record { .. } => "record",
                        ResponseBody::Table { .. } => "table",
                        ResponseBody::Row { .. } => "row",
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
        timeout: Duration,
    ) -> Result<Response, AdapterFailure> {
        let wait = deadline.saturating_duration_since(Instant::now());
        let line = match self.lines.recv_timeout(wait) {
            Ok(l) => l,
            Err(RecvTimeoutError::Timeout) => {
                // its answer may still come; the next request must not choke on it
                self.abandoned.insert(id.to_owned());
                // the configured timeout, not what was left of it at the last line
                return Err(AdapterFailure::Timeout {
                    cmd: cmd.into(),
                    timeout,
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
            Line::TooLong => {
                return Err(protocol(format!("a line longer than {MAX_LINE} bytes")));
            }
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
        match self.next(name, &id, deadline, timeout)?.body {
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
            ResponseBody::Record { .. } | ResponseBody::Table { .. } | ResponseBody::Row { .. } => {
                Err(AdapterFailure::Protocol {
                    cmd: name.into(),
                    message: "a streamed line outside an export or dump".into(),
                })
            }
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
            match self.next("export", &id, deadline, timeout)?.body {
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
                ResponseBody::Table { .. } | ResponseBody::Row { .. } => {
                    return Err(AdapterFailure::Protocol {
                        cmd: "export".into(),
                        message: "a dump line while answering export".into(),
                    });
                }
            }
        }
    }

    /// Sends `dump` and hands each table header and row to the callbacks as they arrive. A
    /// row of a table whose header has not come, a table sent twice, and a row whose cell
    /// count differs from its table's columns are protocol violations.
    pub fn dump(
        &mut self,
        tables: Vec<String>,
        timeout: Duration,
        on_table: &mut dyn FnMut(DumpTable),
        on_row: &mut dyn FnMut(DumpRow),
    ) -> Result<DumpResult, AdapterFailure> {
        let deadline = Instant::now() + timeout;
        let id = self.send(Command::Dump { tables })?;
        let protocol = |message: String| AdapterFailure::Protocol {
            cmd: "dump".into(),
            message,
        };
        let mut widths: std::collections::HashMap<String, usize> = Default::default();
        let (mut seen_tables, mut seen_rows) = (0u64, 0u64);
        loop {
            match self.next("dump", &id, deadline, timeout)?.body {
                ResponseBody::Table { table } => {
                    if widths
                        .insert(table.name.clone(), table.columns.len())
                        .is_some()
                    {
                        return Err(protocol(format!("table {} sent twice", table.name)));
                    }
                    seen_tables += 1;
                    on_table(table);
                }
                ResponseBody::Row { row } => {
                    let Some(width) = widths.get(&row.table) else {
                        return Err(protocol(format!(
                            "a row of {} before its table line",
                            row.table
                        )));
                    };
                    if row.cells.len() != *width {
                        return Err(protocol(format!(
                            "a row of {} has {} cells, the table has {width} columns",
                            row.table,
                            row.cells.len()
                        )));
                    }
                    seen_rows += 1;
                    on_row(row);
                }
                ResponseBody::Ok { result } => {
                    let r: DumpResult = serde_json::from_value(result).map_err(|e| {
                        protocol(format!("the result does not fit the dump result: {e}"))
                    })?;
                    if r.tables != seen_tables || r.rows != seen_rows {
                        return Err(protocol(format!(
                            "announced {} tables and {} rows, sent {seen_tables} and {seen_rows}",
                            r.tables, r.rows
                        )));
                    }
                    return Ok(r);
                }
                ResponseBody::Error { error } => {
                    return Err(AdapterFailure::Error {
                        cmd: "dump".into(),
                        error,
                    });
                }
                ResponseBody::Record { .. } => {
                    return Err(protocol("an export record while answering dump".into()));
                }
            }
        }
    }

    /// Sends `describe` for [`ADAPTER_PROTOCOL_VERSION`]. An adapter that refuses it as
    /// `unsupported` is asked again for each older version down to
    /// [`MIN_ADAPTER_PROTOCOL_VERSION`]; the answer must name the version asked for or an
    /// older one the harness still drives.
    pub fn describe(&mut self, timeout: Duration) -> Result<DescribeResult, AdapterFailure> {
        let mut version = ADAPTER_PROTOCOL_VERSION;
        loop {
            match self.call::<DescribeResult>(Command::Describe { protocol: version }, timeout) {
                Ok(d) => {
                    if !(MIN_ADAPTER_PROTOCOL_VERSION..=version).contains(&d.protocol) {
                        return Err(AdapterFailure::Protocol {
                            cmd: "describe".into(),
                            message: format!(
                                "asked for protocol {version}, the adapter answered {}; the harness drives {MIN_ADAPTER_PROTOCOL_VERSION} to {ADAPTER_PROTOCOL_VERSION}",
                                d.protocol
                            ),
                        });
                    }
                    return Ok(d);
                }
                Err(AdapterFailure::Error { error, .. })
                    if error.class == ErrorClass::Unsupported
                        && version > MIN_ADAPTER_PROTOCOL_VERSION =>
                {
                    version -= 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Late answers for the report, with the count of those not described.
    pub fn take_late_answers(&mut self) -> Vec<String> {
        let mut out = std::mem::take(&mut self.late_answers);
        if self.late_beyond > 0 {
            out.push(format!(
                "{} more late lines for abandoned requests were skipped",
                self.late_beyond
            ));
            self.late_beyond = 0;
        }
        out
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
            let until = Instant::now() + READER_GRACE;
            while !r.is_finished() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
            if !r.is_finished() {
                // the thread ends when the last holder of the pipe exits; joining would block
                // the run on a process the harness does not own
                return format!(
                    "{status}; stdout is still open after the adapter exited (a process it started holds it), its reader was left behind"
                );
            }
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
    fn a_timeout_names_the_configured_wait() {
        let (mut a, _dir) = spawn(SLOW);
        let err = a
            .call::<StatusResult>(Command::Status, Duration::from_millis(300))
            .unwrap_err();
        assert!(err.to_string().ends_with("within 300ms"), "{err}");
        a.finish(Duration::from_secs(5));
    }

    #[test]
    fn late_answers_are_bounded() {
        // nothing is answered until stop arrives, so every status request times out; then a
        // flood of answers to all of them comes before stop's own
        let script = r#"
i=0; while [ $i -lt 41 ]; do read x; i=$((i+1)); done
i=0; while [ $i -lt 40 ]; do echo "{\"id\":\"$i\",\"status\":\"ok\",\"result\":{}}"; i=$((i+1)); done
echo '{"id":"40","status":"ok","result":{"logs":""}}'; read y
"#;
        let (mut a, _dir) = spawn(script);
        for _ in 0..40 {
            let st = a.call::<StatusResult>(Command::Status, Duration::from_millis(1));
            assert!(matches!(st, Err(AdapterFailure::Timeout { .. })), "{st:?}");
        }
        let stop = a.call::<StopResult>(Command::Stop, Duration::from_secs(20));
        assert!(stop.is_ok(), "{stop:?}");
        let notes = a.take_late_answers();
        assert_eq!(notes.len(), MAX_LATE_NOTES + 1, "{notes:?}");
        assert!(notes.last().unwrap().starts_with("24 more"), "{notes:?}");
        a.finish(Duration::from_secs(5));
    }

    #[test]
    fn finish_does_not_wait_for_a_grandchild_holding_stdout() {
        let (a, _dir) = spawn("sleep 30 & read x");
        let started = Instant::now();
        let status = a.finish(Duration::from_millis(200));
        assert!(started.elapsed() < Duration::from_secs(10), "{status}");
        assert!(status.contains("left behind"), "{status}");
    }

    const DESCRIBE_V1: &str = r#"{"protocol":1,"adapter":"a","adapter_version":"1","target":"t","target_version":"v","delivery_modes":["rpc"],"default_delivery":"rpc","bootstrap":"ledger","vocab_version":2,"protocols":{"min":1,"max":30},"claims":{"changes":[]}}"#;

    #[test]
    fn a_version_1_adapter_is_asked_again_for_version_1() {
        // a version 1 adapter refuses any other version as unsupported
        let script = format!(
            r#"read a; echo '{{"id":"0","status":"error","error":{{"class":"unsupported","message":"protocol 2; this adapter speaks 1"}}}}'
read b; case "$b" in *'"protocol":1'*) echo '{{"id":"1","status":"ok","result":{DESCRIBE_V1}}}';; esac; read c"#
        );
        let (mut a, _dir) = spawn(&script);
        let d = a.describe(Duration::from_secs(20)).unwrap();
        assert_eq!(d.protocol, 1);
        assert!(!d.offers(crate::protocol::Capability::Dump));
        a.finish(Duration::from_secs(5));
    }

    #[test]
    fn a_target_error_on_describe_is_not_retried() {
        let (mut a, _dir) = spawn(
            r#"read a; echo '{"id":"0","status":"error","error":{"class":"target","message":"clone failed"}}'; read b"#,
        );
        let err = a.describe(Duration::from_secs(20)).unwrap_err();
        assert!(err.to_string().contains("clone failed"), "{err}");
        a.finish(Duration::from_secs(5));
    }

    #[test]
    fn a_dump_streams_tables_then_rows() {
        let script = r#"read a
echo '{"id":"0","status":"table","table":{"name":"accounts","columns":[{"name":"id","type":"bytea"},{"name":"n","type":"bigint"}]}}'
echo '{"id":"0","status":"row","row":{"table":"accounts","cells":["00ab","7"]}}'
echo '{"id":"0","status":"row","row":{"table":"accounts","cells":["00cd",null]}}'
echo '{"id":"0","status":"ok","result":{"tables":1,"rows":2}}'
read b"#;
        let (mut a, _dir) = spawn(script);
        let d = crate::dump::Dump::take(&mut a, Vec::new(), Duration::from_secs(20)).unwrap();
        assert_eq!(d.rows(), 2);
        assert_eq!(d.tables["accounts"].rows[1], [Some("00cd".into()), None]);
        a.finish(Duration::from_secs(5));
    }

    #[test]
    fn a_dump_row_must_fit_its_table() {
        let script = r#"read a
echo '{"id":"0","status":"table","table":{"name":"t","columns":[{"name":"id","type":"text"}]}}'
echo '{"id":"0","status":"row","row":{"table":"t","cells":["a","b"]}}'
read b"#;
        let (mut a, _dir) = spawn(script);
        let err = crate::dump::Dump::take(&mut a, Vec::new(), Duration::from_secs(20)).unwrap_err();
        assert!(format!("{err:#}").contains("has 2 cells"), "{err:#}");
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
