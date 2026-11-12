//! Decode depth bounds, defined once for every crate. The values are the `recommended` ones
//! in registry/caps-limits.toml; `tests/depth.rs` holds them to that file.

use stellar_xdr::Limits;

/// `xdr.envelope.depth`. From protocol 28 core refuses with txMALFORMED any envelope its rust
/// parser cannot read at this depth (`can_parse_transaction`, `validateXDRForProtocol`).
pub const ENVELOPE_DEPTH: u32 = 1000;

/// `xdr.meta.depth`. The parallel Soroban phase of a `LedgerCloseMeta` V2 holds its envelopes
/// 12 levels down, so a ledger carrying the deepest envelope core accepts needs
/// `ENVELOPE_DEPTH + 12`.
pub const META_DEPTH: u32 = 1012;

/// The depth bound for one value of `type_name`: [`META_DEPTH`] for a whole ledger, which
/// carries envelopes below its own levels, and [`ENVELOPE_DEPTH`] for everything else the
/// corpus holds (envelopes, their parts, transaction meta, events, entries). A
/// `LedgerCloseMetaBatch` adds two more levels above each ledger, so batches are split and
/// their ledgers decoded one by one (`shapekit_pack::lake::Batch::parse`).
pub fn decode_depth(type_name: &str) -> u32 {
    match type_name {
        "LedgerCloseMeta" => META_DEPTH,
        _ => ENVELOPE_DEPTH,
    }
}

/// [`decode_depth`] for `type_name`, with the length budget set to the input itself so a
/// decoder can never be talked into allocating more than it was given.
pub fn decode_limits(type_name: &str, len: usize) -> Limits {
    Limits {
        depth: decode_depth(type_name),
        len,
    }
}
