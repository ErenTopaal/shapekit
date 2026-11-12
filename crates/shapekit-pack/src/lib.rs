//! SEP-54 ledger lakes and history archive bundling for recorded chains.
//!
//! A committed chain directory holds `chain.json`, the canonical ledgers as a SEP-54 lake
//! in `ledgers/`, core's raw meta stream `ledgers.xdr` as provenance and the history
//! archive as `archive.tar`. See [`chain`] for the conversion from what the recorder
//! writes.

pub mod archive;
pub mod chain;
pub mod lake;
pub mod layout;

use std::path::Path;

pub use archive::{ARCHIVE_TAR, ArchiveFile, ArchiveIndex, archive_at, bundle_archive};
pub use chain::{ChainFile, Packed, pack_chain, verify_chain};
pub use lake::{Lake, LakeConfig, RawLedger, lake_digest, read_sep54, write_sep54};
pub use layout::Layout;

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("sep-54 layout: {0}")]
    Layout(String),
    #[error("{context}: {source}")]
    Xdr {
        context: String,
        source: stellar_xdr::Error,
    },
    #[error("{0}")]
    Format(String),
}

pub(crate) fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> PackError + '_ {
    move |source| PackError::Io {
        path: path.display().to_string(),
        source,
    }
}
