//! Corpus manifest types, XDR round-trip checks, XDR-JSON, shape signatures and coverage.

pub mod build;
mod depth;
pub mod framing;
mod model;
pub mod signature;
mod validate;
pub mod xdr;

pub use depth::{ENVELOPE_DEPTH, META_DEPTH, decode_depth, decode_limits};
pub use model::*;
pub use signature::{fragment_matches, reachable_arms, shape_signature};
pub use validate::validate;
pub use xdr::{roundtrip_check, to_xdr_json};

pub use shapekit_registry::Problem;
pub use stellar_xdr::Limits;
pub use stellar_xdr::VERSION as STELLAR_XDR_VERSION;

#[derive(Debug, thiserror::Error)]
pub enum ToolkitError {
    #[error("{0} is not an XDR type known to stellar-xdr {ver}", ver = stellar_xdr::VERSION.pkg)]
    UnknownType(String),
    #[error("decoding failed: {0}")]
    Decode(stellar_xdr::Error),
    #[error("encoding failed: {0}")]
    Encode(stellar_xdr::Error),
    #[error(
        "re-encoded bytes differ: {original_len} bytes in, {reencoded_len} out, first difference at byte {first_difference}"
    )]
    RoundtripMismatch {
        original_len: usize,
        reencoded_len: usize,
        first_difference: usize,
    },
    #[error("xdr-json: {0}")]
    Json(serde_json::Error),
    #[error("json schema: {0}")]
    Schema(String),
    #[error("value does not match its schema at {path}: {message}")]
    Shape { path: String, message: String },
    #[error("framing: {0}")]
    Frame(framing::FrameError),
}

/// Lowercase hex SHA-256 of `bytes`, the form every hash in the manifest uses.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
