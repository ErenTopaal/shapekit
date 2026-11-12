//! HTTP access to a core container.
//!
//! Core's command and query servers listen on the container's loopback only
//! (`PUBLIC_HTTP_PORT=false`), and the container sits on an `--internal`
//! docker network, where docker does not publish ports to the host at all
//! (checked with docker 29.3.1: `-p 127.0.0.1:x:y` on an internal network
//! leaves the port unbound). So every request runs inside the container
//! through `docker exec`, using the python3 that ships in the core image.
//!
//! Not curl, although the image has it: curl 8.5.0 stops sending the request
//! head after 128 KiB, so a `tx?blob=` url for a large Soroban upload never
//! completes and core waits forever. Core's `tx` route only reads the url,
//! never a body, so the url has to carry the blob. The request travels as
//! json on stdin, which keeps blobs out of argv.

use std::io::Write as _;
use std::process::{Command, Stdio};

use crate::config::{HTTP_PORT, HTTP_QUERY_PORT};

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("running docker exec for {what}: {source}")]
    Spawn {
        what: String,
        source: std::io::Error,
    },
    #[error("http client inside {container} failed for {what} (exit {code:?}): {stderr}")]
    Client {
        container: String,
        what: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("unparseable curl output for {what}: {output:?}")]
    Output { what: String, output: String },
    #[error("core answered {what} with http {status}: {body}")]
    Status {
        what: String,
        status: u16,
        body: String,
    },
}

/// Percent-encodes everything outside the url unreserved set. Core url-decodes
/// both the request path and the post body, and turns `+` into a space, so
/// base64 must be fully escaped.
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3 / 2);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct CoreHttp {
    container: String,
}

impl CoreHttp {
    pub fn new(container: impl Into<String>) -> Self {
        Self {
            container: container.into(),
        }
    }

    /// GET on the command port. `path` is the route plus query string, with
    /// values already percent-encoded. Returns the body of a 200 answer.
    pub fn command(&self, path: &str) -> Result<String, HttpError> {
        self.run(path, HTTP_PORT, "GET", &format!("/{path}"), None)
    }

    /// POST on the query port with a form body (already percent-encoded).
    pub fn query(&self, route: &str, form_body: &str) -> Result<String, HttpError> {
        self.run(
            route,
            HTTP_QUERY_PORT,
            "POST",
            &format!("/{route}"),
            Some(form_body),
        )
    }

    fn run(
        &self,
        what: &str,
        port: u16,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<String, HttpError> {
        // the short name is what errors show; blobs can be huge
        let what: String = what.chars().take(120).collect();
        let request = serde_json::json!({
            "port": port,
            "timeout": REQUEST_TIMEOUT_SECS,
            "method": method,
            "path": path,
            "body": body,
        });
        let mut child = Command::new("docker")
            .args(["exec", "-i", &self.container, "python3", "-I", "-c", CLIENT])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| HttpError::Spawn {
                what: what.clone(),
                source,
            })?;
        {
            let mut stdin = child.stdin.take().expect("stdin is piped");
            stdin
                .write_all(request.to_string().as_bytes())
                .map_err(|source| HttpError::Spawn {
                    what: what.clone(),
                    source,
                })?;
        }
        let out = child
            .wait_with_output()
            .map_err(|source| HttpError::Spawn {
                what: what.clone(),
                source,
            })?;
        if !out.status.success() {
            return Err(HttpError::Client {
                container: self.container.clone(),
                what,
                code: out.status.code(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            });
        }
        let text = String::from_utf8(out.stdout).map_err(|e| HttpError::Output {
            what: what.clone(),
            output: String::from_utf8_lossy(e.as_bytes()).into_owned(),
        })?;
        let (code, body) = text.split_once('\n').ok_or_else(|| HttpError::Output {
            what: what.clone(),
            output: text.clone(),
        })?;
        let status: u16 = code.trim().parse().map_err(|_| HttpError::Output {
            what: what.clone(),
            output: text.clone(),
        })?;
        if status != 200 {
            return Err(HttpError::Status {
                what,
                status,
                body: body.to_owned(),
            });
        }
        Ok(body.to_owned())
    }
}

/// Seconds a single request may take. Manual closes of large ledgers are the
/// slowest calls.
pub const REQUEST_TIMEOUT_SECS: u64 = 300;

/// Runs inside the container: one request, then the status line and body.
const CLIENT: &str = "\
import sys, json, http.client
req = json.loads(sys.stdin.read())
headers = {}
if req['body'] is not None:
    headers['Content-Type'] = 'application/x-www-form-urlencoded'
c = http.client.HTTPConnection('127.0.0.1', req['port'], timeout=req['timeout'])
c.request(req['method'], req['path'], body=req['body'], headers=headers)
r = c.getresponse()
sys.stdout.buffer.write(str(r.status).encode() + b'\\n' + r.read())
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_base64_specials() {
        assert_eq!(percent_encode("ab+/=Z9-_.~"), "ab%2B%2F%3DZ9-_.~");
        assert_eq!(percent_encode("a b&c"), "a%20b%26c");
    }
}
