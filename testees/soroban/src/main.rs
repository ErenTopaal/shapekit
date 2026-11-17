//! The soroban-sdk testee for the shapekit conformance runner: answers testee protocol
//! requests (docs/testee-protocol.md) through the contract SDK's own conversions, run on the
//! host that soroban-sdk's `testutils` feature links. One source, built once per pinned
//! soroban-sdk version from the manifests under `versions/`.

mod ops;

use std::io::{BufRead, Write};
use std::process::ExitCode;

use serde_json::{Value, json};

pub const PROTOCOL_VERSION: u32 = 2;

// the host converts nested values recursively, so requests run on a thread whose stack is far
// larger than the default main thread's
const STACK_BYTES: usize = 512 << 20;

fn main() -> ExitCode {
    // the default hook prints every caught panic with the host's whole debug event log; one
    // line on stderr is enough, the message goes into the response
    std::panic::set_hook(Box::new(|info| {
        let first = ops::panic_text(info.payload())
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        eprintln!("testee: panic: {first}");
    }));
    let worker = std::thread::Builder::new()
        .name("testee".into())
        .stack_size(STACK_BYTES)
        .spawn(serve);
    let result = match worker {
        Ok(handle) => match handle.join() {
            Ok(result) => result,
            Err(_) => Err("the request loop panicked outside a request".to_string()),
        },
        Err(err) => Err(format!("starting the request thread: {err}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("testee: {message}");
            ExitCode::FAILURE
        }
    }
}

fn serve() -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut line = String::new();
    loop {
        line.clear();
        let read = input
            .read_line(&mut line)
            .map_err(|e| format!("reading stdin: {e}"))?;
        if read == 0 {
            return Ok(());
        }
        let text = line.trim_end_matches(['\n', '\r']);
        if text.is_empty() {
            continue;
        }
        let response = handle_line(text);
        let rendered =
            serde_json::to_string(&response).map_err(|e| format!("rendering a response: {e}"))?;
        out.write_all(rendered.as_bytes())
            .and_then(|()| out.write_all(b"\n"))
            .and_then(|()| out.flush())
            .map_err(|e| format!("writing stdout: {e}"))?;
    }
}

/// Answers one request line. Never panics: sdk panics are caught and classified.
pub fn handle_line(line: &str) -> Value {
    let request = match ops::Request::parse(line) {
        Ok(r) => r,
        Err((id, message)) => return error_response(&id, ops::ErrorClass::BadRequest, &message),
    };
    let id = request.id.clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ops::run(&request)));
    match outcome {
        Ok(ops::Outcome::Ok(result)) => json!({"id": id, "status": "ok", "result": result}),
        Ok(ops::Outcome::Error(class, message)) => error_response(&id, class, &message),
        Ok(ops::Outcome::Unsupported(reason)) => {
            json!({"id": id, "status": "unsupported", "reason": reason})
        }
        Err(payload) => {
            let message = ops::panic_text(&*payload);
            error_response(&id, ops::classify_panic(&message), &message)
        }
    }
}

fn error_response(id: &str, class: ops::ErrorClass, message: &str) -> Value {
    json!({"id": id, "status": "error", "error": {"class": class.as_str(), "message": message}})
}

#[cfg(test)]
mod tests;
