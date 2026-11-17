//! The adapter protocol over stdin and stdout: one JSON request per line in, response lines
//! out. Logs go to stderr.

use std::io::{BufRead, Write};

use anyhow::{Context as _, Result};
use serde_json::Value;
use shapekit_harness::protocol::{
    AdapterError, Command, ErrorClass, ExportRecord, Request, Response, ResponseBody,
};

use crate::adapter::{Adapter, Res};

fn write(out: &mut dyn Write, id: &str, body: ResponseBody) -> Result<()> {
    let line = serde_json::to_string(&Response {
        id: id.to_owned(),
        body,
    })?;
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush().context("writing a response")
}

fn finish<T: serde::Serialize>(out: &mut dyn Write, id: &str, r: Res<T>) -> Result<()> {
    let body = match r {
        Ok(v) => ResponseBody::Ok {
            result: serde_json::to_value(v)?,
        },
        Err(error) => ResponseBody::Error { error },
    };
    write(out, id, body)
}

/// Answers requests until `stop` or the end of input, then tears the target down.
pub fn serve(adapter: &mut Adapter, input: &mut dyn BufRead, out: &mut dyn Write) -> Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).context("reading requests")? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let raw: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let error = AdapterError {
                    class: ErrorClass::BadRequest,
                    message: format!("request is not a json object: {e}"),
                };
                write(out, "", ResponseBody::Error { error })?;
                continue;
            }
        };
        let id = raw
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let req: Request = match serde_json::from_value(raw) {
            Ok(r) if !id.is_empty() => r,
            res => {
                let message = match res {
                    Err(e) => format!("bad request: {e}"),
                    Ok(_) => "request has no id".to_owned(),
                };
                let error = AdapterError {
                    class: ErrorClass::BadRequest,
                    message,
                };
                write(out, &id, ResponseBody::Error { error })?;
                continue;
            }
        };
        match req.command {
            Command::Describe { protocol } => finish(out, &id, adapter.describe(protocol))?,
            Command::Start(p) => finish(out, &id, adapter.start(*p))?,
            Command::Advance { head } => finish(out, &id, adapter.advance(head))?,
            Command::Status => finish(out, &id, adapter.status())?,
            Command::Export { from, to } => {
                if from > to {
                    let error = AdapterError {
                        class: ErrorClass::BadRequest,
                        message: format!("export from {from} is after to {to}"),
                    };
                    write(out, &id, ResponseBody::Error { error })?;
                    continue;
                }
                let mut emit = |rec: ExportRecord| {
                    write(
                        out,
                        &id,
                        ResponseBody::Record {
                            record: Box::new(rec),
                        },
                    )
                };
                let r = adapter.export(from, to, &mut emit);
                finish(out, &id, r)?;
            }
            Command::Dump { tables } => {
                // both callbacks write to the same stream, one after the other
                let cell = std::cell::RefCell::new(&mut *out);
                let mut table =
                    |t| write(*cell.borrow_mut(), &id, ResponseBody::Table { table: t });
                let mut row = |r| write(*cell.borrow_mut(), &id, ResponseBody::Row { row: r });
                let r = adapter.dump(&tables, &mut table, &mut row);
                finish(*cell.borrow_mut(), &id, r)?;
            }
            Command::Restart => finish(out, &id, adapter.restart())?,
            Command::Stop => {
                let r = adapter.stop();
                finish(out, &id, r)?;
                return Ok(());
            }
        }
    }
    adapter
        .teardown()
        .context("tearing down after end of input")?;
    Ok(())
}
