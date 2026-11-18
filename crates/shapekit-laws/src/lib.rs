//! The law book: statements every `LedgerCloseMeta` must satisfy, checked without interpreting
//! what the ledger means. The truth for each comes from the input itself (encoding laws) or
//! from hashes the network signed (network anchors), never from a reader of ours.
//!
//! | law | statement |
//! |---|---|
//! | `roundtrip` | decoding and re-encoding gives the input bytes |
//! | `noncanonical-rejected` | the input with a trailing word, or with its last byte cut, does not decode |
//! | `header-hash` | sha256 of the header is the hash stored next to it |
//! | `header-chain` | a ledger's `previousLedgerHash` is the hash of the ledger before it |
//! | `tx-set-hash` | the tx set's contents hash is `scpValue.txSetHash` |
//! | `tx-set-previous` | the tx set names the header's previous ledger hash |
//! | `result-set-hash` | sha256 of the result set in apply order is `txSetResultHash` |
//! | `tx-hash` | the envelopes' hashes are the hashes core wrote into the results, inner ones too |
//! | `pool-id` | every pool entry's id, and the pool share trustline a `ChangeTrust` makes, derive from the pool parameters |
//! | `contract-id` | a contract created by a top-level host function lives at the id derived from its preimage |
//! | `balance-id` | every claimable balance created derives from its operation id or revoke id |

pub mod derive;
mod ledger;

use std::collections::BTreeMap;

use serde::Serialize;
use stellar_xdr::{LedgerCloseMeta, LedgerCloseMetaBatch, Limits, ReadXdr, WriteXdr};

pub use ledger::check_ledger;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Law {
    Roundtrip,
    NoncanonicalRejected,
    HeaderHash,
    HeaderChain,
    TxSetHash,
    TxSetPrevious,
    ResultSetHash,
    TxHash,
    PoolId,
    ContractId,
    BalanceId,
}

impl Law {
    pub const ALL: [Law; 11] = [
        Law::Roundtrip,
        Law::NoncanonicalRejected,
        Law::HeaderHash,
        Law::HeaderChain,
        Law::TxSetHash,
        Law::TxSetPrevious,
        Law::ResultSetHash,
        Law::TxHash,
        Law::PoolId,
        Law::ContractId,
        Law::BalanceId,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Law::Roundtrip => "roundtrip",
            Law::NoncanonicalRejected => "noncanonical-rejected",
            Law::HeaderHash => "header-hash",
            Law::HeaderChain => "header-chain",
            Law::TxSetHash => "tx-set-hash",
            Law::TxSetPrevious => "tx-set-previous",
            Law::ResultSetHash => "result-set-hash",
            Law::TxHash => "tx-hash",
            Law::PoolId => "pool-id",
            Law::ContractId => "contract-id",
            Law::BalanceId => "balance-id",
        }
    }
}

/// One failed law instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Violation {
    pub law: Law,
    /// The ledger the violation is in, when the input decoded far enough to tell.
    pub ledger: Option<u32>,
    pub detail: String,
}

/// How often each law was applied and how often it failed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub checked: u64,
    pub violated: u64,
}

/// What a check saw, summed over every ledger it was given.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    pub ledgers: u64,
    /// Entries of `txProcessing`, one per applied transaction.
    pub transactions: u64,
    /// Envelopes in the tx sets, by kind.
    pub envelopes: BTreeMap<&'static str, u64>,
    /// Ledgers by protocol version.
    pub protocols: BTreeMap<u32, u64>,
    pub laws: BTreeMap<Law, Tally>,
    /// Instances a law could not decide, by law: created contracts whose preimage is not in a
    /// top-level host function, tx sets with the empty-set hash core skips the check for,
    /// ledgers checked without a network id, and ledgers that do not follow the ledger checked
    /// before them (header chain).
    pub undecided: BTreeMap<Law, u64>,
    pub violations: Vec<Violation>,
}

impl Report {
    pub fn new() -> Report {
        Report {
            laws: Law::ALL.iter().map(|l| (*l, Tally::default())).collect(),
            ..Report::default()
        }
    }

    pub fn holds(&self) -> bool {
        self.violations.is_empty()
    }

    /// Records one application of `law`; `failure` is `Some(detail)` when it did not hold.
    pub fn record(&mut self, law: Law, ledger: Option<u32>, failure: Option<String>) {
        let t = self.laws.entry(law).or_default();
        t.checked += 1;
        if let Some(detail) = failure {
            t.violated += 1;
            self.violations.push(Violation {
                law,
                ledger,
                detail,
            });
        }
    }

    pub fn undecided(&mut self, law: Law) {
        *self.undecided.entry(law).or_default() += 1;
    }

    pub fn merge(&mut self, other: Report) {
        self.ledgers += other.ledgers;
        self.transactions += other.transactions;
        for (k, v) in other.envelopes {
            *self.envelopes.entry(k).or_default() += v;
        }
        for (k, v) in other.protocols {
            *self.protocols.entry(k).or_default() += v;
        }
        for (k, v) in other.laws {
            let t = self.laws.entry(k).or_default();
            t.checked += v.checked;
            t.violated += v.violated;
        }
        for (k, v) in other.undecided {
            *self.undecided.entry(k).or_default() += v;
        }
        self.violations.extend(other.violations);
    }
}

/// The type an input file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// One `LedgerCloseMeta`.
    Ledger,
    /// A `LedgerCloseMetaBatch`, the unit of a SEP-54 data lake object.
    Batch,
}

impl Input {
    /// The decode depth bound for one input of this type. A ledger gets
    /// [`shapekit_manifest::META_DEPTH`], the bound every crate decodes ledgers with; a batch
    /// holds its ledgers two levels further down (the batch struct and its ledger array).
    /// Without a bound a deeply nested input overflows the stack instead of failing to decode.
    pub fn depth(self) -> u32 {
        match self {
            Input::Ledger => shapekit_manifest::META_DEPTH,
            Input::Batch => shapekit_manifest::META_DEPTH + 2,
        }
    }
}

/// `depth` with the length budget set to the bytes being decoded, so a decoder can never be
/// talked into allocating more than it was given.
fn limits(depth: u32, bytes: &[u8]) -> Limits {
    Limits {
        depth,
        len: bytes.len(),
    }
}

/// The encoding laws over raw bytes of type `T`: the bytes decode and re-encode to themselves,
/// and the two nearest non-canonical neighbours (a trailing zero word, the last byte cut) do not
/// decode. Every decode is bounded by `depth` (see [`Input::depth`]). Returns the decoded
/// value when the input decodes at all.
pub fn check_encoding<T: ReadXdr + WriteXdr>(
    bytes: &[u8],
    depth: u32,
    report: &mut Report,
) -> Result<T, stellar_xdr::Error> {
    let value = T::from_xdr(bytes, limits(depth, bytes))?;
    // the value decoded within `depth`, so encoding it again recurses no deeper
    let again = value.to_xdr(Limits::none());
    let failure = match again {
        Ok(b) if b == bytes => None,
        Ok(b) => Some(format!(
            "re-encoding gives {} bytes differing from the {} input bytes",
            b.len(),
            bytes.len()
        )),
        Err(e) => Some(format!("re-encoding failed: {e}")),
    };
    report.record(Law::Roundtrip, None, failure);

    let mut longer = bytes.to_vec();
    longer.extend_from_slice(&[0; 4]);
    let shorter = &bytes[..bytes.len().saturating_sub(1)];
    let mut failures = Vec::new();
    if T::from_xdr(&longer, limits(depth, &longer)).is_ok() {
        failures.push("the input plus a trailing zero word decodes");
    }
    if T::from_xdr(shorter, limits(depth, shorter)).is_ok() {
        failures.push("the input without its last byte decodes");
    }
    report.record(
        Law::NoncanonicalRejected,
        None,
        (!failures.is_empty()).then(|| failures.join("; ")),
    );
    Ok(value)
}

/// Header chain state across ledgers handed to [`Checker::ledger`] in order.
#[derive(Default)]
struct Previous {
    seq: u32,
    hash: [u8; 32],
}

/// Checks ledgers one at a time and keeps only the last header hash between them, so a stream
/// of any length is checked in the memory of one ledger.
pub struct Checker {
    network: Option<derive::Digest32>,
    previous: Option<Previous>,
    pub report: Report,
}

impl Checker {
    pub fn new(passphrase: &str) -> Checker {
        Checker {
            network: Some(derive::network_id(passphrase)),
            previous: None,
            report: Report::new(),
        }
    }

    /// A checker for ledgers whose network is not known, such as hand-made ones. The laws that
    /// hash with the network id (tx-hash, balance-id, contract-id) are counted as undecided.
    pub fn without_network() -> Checker {
        Checker {
            network: None,
            previous: None,
            report: Report::new(),
        }
    }

    /// Checks the encoding laws on `bytes` and every law on the ledgers they hold.
    pub fn bytes(&mut self, bytes: &[u8], input: Input) -> Result<(), stellar_xdr::Error> {
        match input {
            Input::Ledger => {
                let lcm =
                    check_encoding::<LedgerCloseMeta>(bytes, input.depth(), &mut self.report)?;
                self.ledger(&lcm);
            }
            Input::Batch => {
                let batch =
                    check_encoding::<LedgerCloseMetaBatch>(bytes, input.depth(), &mut self.report)?;
                for lcm in batch.ledger_close_metas.iter() {
                    self.ledger(lcm);
                }
            }
        }
        Ok(())
    }

    /// Checks every ledger law on one decoded ledger, the header chain against the ledger
    /// checked just before it when that one is its direct predecessor. A ledger that follows
    /// another one but not its predecessor (a gap, a repeat or a step back between inputs)
    /// leaves the header chain undecided: the ledger its `previousLedgerHash` names was never
    /// seen, so the law can neither hold nor fail.
    pub fn ledger(&mut self, lcm: &LedgerCloseMeta) {
        let (seq, hash, prev_hash) =
            ledger::check_ledger(lcm, self.network.as_ref(), &mut self.report);
        match &self.previous {
            Some(p) if p.seq.checked_add(1) == Some(seq) => {
                let failure = (prev_hash != p.hash).then(|| {
                    format!(
                        "previousLedgerHash {} is not the hash {} of ledger {}",
                        hex::encode(prev_hash),
                        hex::encode(p.hash),
                        p.seq
                    )
                });
                self.report.record(Law::HeaderChain, Some(seq), failure);
            }
            Some(_) => self.report.undecided(Law::HeaderChain),
            None => {}
        }
        self.previous = Some(Previous { seq, hash });
    }
}

/// Reads an input file: zstd frames are decompressed first (SEP-54 lake objects are
/// `.xdr.zst`). A frame that decompresses to more than
/// [`shapekit_pack::lake::MAX_BATCH_LEN`] bytes is refused as corrupt, the bound every lake
/// reader uses, rather than read into memory whatever its size.
pub fn read_input(bytes: Vec<u8>) -> std::io::Result<Vec<u8>> {
    const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
    if bytes.starts_with(&ZSTD_MAGIC) {
        decompress_at_most(&bytes, shapekit_pack::lake::MAX_BATCH_LEN)
    } else {
        Ok(bytes)
    }
}

fn decompress_at_most(frame: &[u8], max: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    zstd::stream::Decoder::new(frame)?
        .take(max + 1)
        .read_to_end(&mut out)?;
    if out.len() as u64 > max {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("decompresses to more than {max} bytes"),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::decompress_at_most;

    #[test]
    fn a_frame_past_the_bound_is_refused() {
        let frame = zstd::encode_all(&[1u8, 2, 3][..], 1).unwrap();
        assert_eq!(decompress_at_most(&frame, 3).unwrap(), [1, 2, 3]);
        assert!(decompress_at_most(&frame, 2).is_err());
    }
}
