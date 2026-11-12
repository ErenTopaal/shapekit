//! `shapekit serve`: replays one recorded chain over HTTP.
//!
//! Routes:
//! - `POST /` and `POST /rpc`: JSON-RPC 2.0, the read methods of stellar-rpc (see [`rpc`]).
//! - `GET /ledgers/<key>`: the chain's SEP-54 lake, `.config.json` and every batch whose
//!   ledgers are all at or below the visible head.
//! - `GET /archive/<path>`: the history archive as it stood at the newest checkpoint at or
//!   below the head, including `.well-known/stellar-history.json`.
//! - `GET /control/head` reports the head; `POST /control/head` with `{"ledger": N}` moves
//!   it, so a harness can release ledgers one at a time.
//!
//! Every answer comes from the recorded chain. Ledgers are read from the lake on demand
//! through a cache bounded in bytes.

pub mod replay;
pub mod rpc;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

pub use crate::replay::DEFAULT_CACHE_BYTES;
use crate::replay::Replay;

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("chain: {0}")]
    Chain(String),
    #[error("{0}")]
    Head(String),
    #[error("{0}")]
    Io(String),
}

#[derive(Debug, Clone)]
pub struct ServeConfig {
    /// A committed chain directory, `corpus/chains/<chain-id>/<network>`.
    pub chain_dir: PathBuf,
    pub listen: SocketAddr,
    /// Visible head at start; the chain's last ledger when `None`.
    pub head: Option<u32>,
    pub cache_bytes: usize,
    /// Print every JSON-RPC method called, to stderr.
    pub log_requests: bool,
}

#[derive(Clone)]
struct App {
    replay: Arc<Replay>,
    log_requests: bool,
}

fn router(app: App) -> Router {
    Router::new()
        .route("/", post(rpc_route))
        .route("/rpc", post(rpc_route))
        .route("/ledgers/{*key}", get(lake_route))
        .route("/archive/{*path}", get(archive_route))
        .route("/control/head", get(head_get).post(head_post))
        .with_state(app)
}

async fn rpc_route(State(app): State<App>, body: Bytes) -> Response {
    let replay = app.replay.clone();
    let log = app.log_requests;
    let out = tokio::task::spawn_blocking(move || {
        let req: Value = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return Some(rpc::parse_error(&e)),
        };
        let one = |r: &Value| {
            if log {
                eprintln!(
                    "rpc {}",
                    r.get("method").and_then(Value::as_str).unwrap_or("?")
                );
            }
            rpc::handle(&replay, r)
        };
        match &req {
            Value::Array(items) if !items.is_empty() => {
                let out: Vec<Value> = items.iter().filter_map(one).collect();
                (!out.is_empty()).then_some(Value::Array(out))
            }
            other => one(other),
        }
    })
    .await;
    match out {
        Ok(Some(v)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            v.to_string(),
        )
            .into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn file_response(path: Option<PathBuf>, content_type: &'static str) -> Response {
    let Some(path) = path else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            bytes,
        )
            .into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("{}: {e}", path.display()),
        )
            .into_response(),
    }
}

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

async fn lake_route(State(app): State<App>, UrlPath(key): UrlPath<String>) -> Response {
    file_response(app.replay.lake_file(&key), content_type(&key)).await
}

async fn archive_route(State(app): State<App>, UrlPath(path): UrlPath<String>) -> Response {
    file_response(app.replay.archive_file(&path), content_type(&path)).await
}

fn head_json(r: &Replay) -> Value {
    json!({"head": r.head(), "first": r.first, "last": r.last})
}

async fn head_get(State(app): State<App>) -> Response {
    axum::Json(head_json(&app.replay)).into_response()
}

async fn head_post(State(app): State<App>, body: Bytes) -> Response {
    let ledger = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| v.get("ledger").and_then(Value::as_u64))
        .and_then(|l| u32::try_from(l).ok());
    let Some(ledger) = ledger else {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": "expected {\"ledger\": <u32>}"})),
        )
            .into_response();
    };
    match app.replay.set_head(ledger) {
        Ok(()) => axum::Json(head_json(&app.replay)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// A server running on its own thread and runtime.
pub struct Server {
    addr: SocketAddr,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<Result<(), ServeError>>>,
}

impl Server {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Stops accepting connections and waits for the server thread.
    pub fn stop(mut self) -> Result<(), ServeError> {
        self.shutdown()
    }

    fn shutdown(&mut self) -> Result<(), ServeError> {
        if let Some(s) = self.stop.take() {
            // the receiver is gone only if the server already ended, which join reports
            let _ = s.send(());
        }
        match self.thread.take() {
            Some(t) => t
                .join()
                .map_err(|_| ServeError::Io("the server thread panicked".into()))?,
            None => Ok(()),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown() {
            eprintln!("shapekit serve: {e}");
        }
    }
}

/// Opens the chain, binds the listener and serves on a background thread.
pub fn start(cfg: ServeConfig) -> Result<Server, ServeError> {
    let replay = Arc::new(Replay::open(&cfg.chain_dir, cfg.head, cfg.cache_bytes)?);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| ServeError::Io(format!("starting the runtime: {e}")))?;
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind(cfg.listen))
        .map_err(|e| ServeError::Io(format!("binding {}: {e}", cfg.listen)))?;
    let addr = listener
        .local_addr()
        .map_err(|e| ServeError::Io(format!("reading the bound address: {e}")))?;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let app = App {
        replay,
        log_requests: cfg.log_requests,
    };
    let thread = std::thread::Builder::new()
        .name("shapekit-serve".into())
        .spawn(move || {
            runtime.block_on(async move {
                axum::serve(listener, router(app))
                    .with_graceful_shutdown(async {
                        // a dropped sender also means stop
                        let _ = rx.await;
                    })
                    .await
                    .map_err(|e| ServeError::Io(format!("serving: {e}")))
            })
        })
        .map_err(|e| ServeError::Io(format!("spawning the server thread: {e}")))?;
    Ok(Server {
        addr,
        stop: Some(tx),
        thread: Some(thread),
    })
}
