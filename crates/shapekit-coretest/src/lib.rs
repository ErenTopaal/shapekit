//! stellar-core's own transaction tests as a data source.
//!
//! The pinned `-buildtests` image runs every `[tx]` test case at every protocol version
//! (`--all-versions`) and writes each transaction meta the tests apply, after core's own
//! normalisation, to a text dump (`--debug-test-tx-meta`). The same run records the short hash
//! of every meta (`--record-test-tx-meta`), the form core keeps in its repository as
//! `test-tx-meta-baseline-current`.
//!
//! [`run`] drives the containers and summarises what they wrote, [`dump`] reads the dump
//! format back.

pub mod cereal;
pub mod convert;
pub mod dump;
pub mod pins;
pub mod run;

/// The `name` of the test image entry in `pins.toml`.
pub const BUILDTESTS_IMAGE_PIN: &str = "stellar-core-buildtests";

/// Seed core's tests run with. core's committed baseline is recorded with it, and the short
/// hash key is derived from it (`src/crypto/ShortHash.cpp`, `seed`).
pub const RNG_SEED: u32 = 12345;

/// The catch tag that selects core's transaction tests.
pub const TX_TAG: &str = "[tx]";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("writing {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Pins {
        path: String,
        source: toml::de::Error,
    },
    #[error("{path} pins no image named {name:?}")]
    MissingImage { path: String, name: String },
    #[error("image pin {name:?} has a malformed digest {digest:?}")]
    BadDigest { name: String, digest: String },
    #[error("running docker: {0}")]
    Docker(String),
    #[error("{path}:{line}: {message}")]
    Dump {
        path: String,
        line: u64,
        message: String,
    },
    #[error("test name {0:?} holds characters a catch test spec cannot carry")]
    TestName(String),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
