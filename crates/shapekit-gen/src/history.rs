//! Captures full `LedgerCloseMeta` values from testnet or mainnet history as
//! `testnet-history` and `mainnet-history` corpus entries.
//!
//! Two capture paths exist and the caller picks one; there is no automatic fallback, and the
//! path used is recorded in the entry:
//!
//! - `lake`: the public SEP-54 data lake in the `aws-public-blockchain` bucket (us-east-2, read
//!   anonymously over HTTPS). Pubnet lives at `v1.1/stellar/ledgers/pubnet`. Testnet lives under
//!   `v1.1/stellar/ledgers/testnet/<date>/`, one prefix per network reset; the newest prefix
//!   whose `.config.json` names the testnet passphrase is the live network.
//! - `rpc`: `getLedgers` on a stellar-rpc endpoint, which only serves its retention window.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use shapekit_manifest::{
    DecodeExpectation, Entry, EntryKind, EntryProtocol, Expected, MAINNET_PASSPHRASE, Source,
    TESTNET_PASSPHRASE, Validity, Variant, Variants, decode_limits, fragment_matches,
    roundtrip_check, sha256_hex, shape_signature,
};
use shapekit_pack::lake::{Batch, MAX_BATCH_LEN};
use shapekit_registry::{Registry, SourceClass};
use stellar_xdr::{
    GeneralizedTransactionSet, LedgerCloseMeta, Limits, ReadXdr, StellarValueExt,
    TransactionEnvelope, TransactionPhase, TxSetComponent, WriteXdr,
};

use crate::entry_io;
use crate::golden::{decode_lcm, header, lcm_version, ledger_seq, ledger_version, tx_meta_version};

pub const LAKE_ENDPOINT: &str = "https://aws-public-blockchain.s3.us-east-2.amazonaws.com";
pub const PUBNET_LAKE_PREFIX: &str = "v1.1/stellar/ledgers/pubnet";
pub const TESTNET_LAKE_ROOT: &str = "v1.1/stellar/ledgers/testnet";
pub const TESTNET_RPC: &str = "https://soroban-testnet.stellar.org";
pub const LAKE_PATH: &str = "sep-54 public data lake";
// a lake object holds ledgersPerBatch ledgers; pubnet ledgers stay well below this. it
// also bounds a getLedgers reply, which carries one ledger as base64
const MAX_OBJECT: u64 = 512 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Testnet,
    Mainnet,
}

impl Network {
    pub fn passphrase(self) -> &'static str {
        match self {
            Network::Testnet => TESTNET_PASSPHRASE,
            Network::Mainnet => MAINNET_PASSPHRASE,
        }
    }

    pub fn class(self) -> SourceClass {
        match self {
            Network::Testnet => SourceClass::TestnetHistory,
            Network::Mainnet => SourceClass::MainnetHistory,
        }
    }

    pub fn variant(self) -> &'static str {
        match self {
            Network::Testnet => "testnet",
            Network::Mainnet => "mainnet",
        }
    }
}

/// `.config.json` at the root of a SEP-54 lake.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LakeConfig {
    pub network_passphrase: String,
    pub version: String,
    pub compression: String,
    pub ledgers_per_batch: u32,
    pub batches_per_partition: u32,
}

/// The object key of the batch holding `seq`, relative to the lake prefix, in the layout
/// go-stellar-sdk's datastore writes (SEP-54): partition and batch directories are named by
/// `u32::MAX - start` in upper hex, then the decimal range.
pub fn lake_object_key(cfg: &LakeConfig, seq: u32) -> Result<String> {
    ensure!(cfg.ledgers_per_batch > 0, "ledgersPerBatch is 0");
    ensure!(cfg.batches_per_partition > 0, "batchesPerPartition is 0");
    let ext = match cfg.compression.as_str() {
        "zstd" => "zst",
        other => bail!("unsupported lake compression {other}"),
    };
    let per_batch = u64::from(cfg.ledgers_per_batch);
    let per_partition = per_batch * u64::from(cfg.batches_per_partition);
    let seq = u64::from(seq);
    let range = |start: u64, size: u64| -> (u64, u64) { (start, start + size - 1) };
    let mut key = String::new();
    if per_partition > 1 {
        let (start, end) = range(seq / per_partition * per_partition, per_partition);
        key.push_str(&format!(
            "{:08X}--{start}-{end}/",
            u64::from(u32::MAX) - start
        ));
    }
    let (start, end) = range(seq / per_batch * per_batch, per_batch);
    key.push_str(&format!("{:08X}--{start}", u64::from(u32::MAX) - start));
    if end != start {
        key.push_str(&format!("-{end}"));
    }
    key.push_str(&format!(".xdr.{ext}"));
    Ok(key)
}

/// A captured ledger and how it was obtained.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub bytes: Vec<u8>,
    /// `sep-54 public data lake` or `stellar-rpc getLedgers`.
    pub path: &'static str,
    pub url: String,
}

pub fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("shapekit/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(120))
        .build()
        .context("building the http client")
}

fn get(client: &reqwest::blocking::Client, url: &str) -> Result<Option<Vec<u8>>> {
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let resp = resp
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;
    let mut body = Vec::new();
    resp.take(MAX_OBJECT + 1)
        .read_to_end(&mut body)
        .with_context(|| format!("reading {url}"))?;
    ensure!(
        body.len() as u64 <= MAX_OBJECT,
        "{url} is larger than {MAX_OBJECT} bytes"
    );
    Ok(Some(body))
}

fn lake_config(client: &reqwest::blocking::Client, prefix: &str) -> Result<Option<LakeConfig>> {
    let url = format!("{LAKE_ENDPOINT}/{prefix}/.config.json");
    match get(client, &url)? {
        None => Ok(None),
        Some(body) => Ok(Some(
            serde_json::from_slice(&body).with_context(|| format!("parsing {url}"))?,
        )),
    }
}

/// The newest testnet lake prefix (`v1.1/stellar/ledgers/testnet/<date>`) with a config for the
/// testnet passphrase. Older prefixes hold networks that were reset away.
pub fn testnet_lake_prefix(client: &reqwest::blocking::Client) -> Result<String> {
    let url = format!("{LAKE_ENDPOINT}/?list-type=2&delimiter=/&prefix={TESTNET_LAKE_ROOT}/");
    let body = get(client, &url)?.with_context(|| format!("{url} not found"))?;
    let text = String::from_utf8(body).context("bucket listing is not utf-8")?;
    ensure!(
        !text.contains("<IsTruncated>true</IsTruncated>"),
        "the testnet prefix listing is truncated"
    );
    let mut dated: Vec<String> = text
        .split("<Prefix>")
        .skip(1)
        .filter_map(|s| {
            s.split_once("</Prefix>")
                .map(|(p, _)| p.trim_end_matches('/'))
        })
        .filter(|p| {
            p.strip_prefix(TESTNET_LAKE_ROOT)
                .and_then(|r| r.strip_prefix('/'))
                .is_some_and(|d| {
                    d.len() == 10 && d.bytes().all(|b| b.is_ascii_digit() || b == b'-')
                })
        })
        .map(str::to_string)
        .collect();
    dated.sort();
    for prefix in dated.iter().rev() {
        if let Some(cfg) = lake_config(client, prefix)?
            && cfg.network_passphrase == TESTNET_PASSPHRASE
        {
            return Ok(prefix.clone());
        }
    }
    bail!("no testnet lake prefix under {TESTNET_LAKE_ROOT} has a testnet config")
}

/// Fetches ledger `seq` from the data lake. `prefix` overrides the lake prefix (for testnet,
/// to read a network that has since been reset).
pub fn fetch_lake(
    client: &reqwest::blocking::Client,
    network: Network,
    prefix: Option<&str>,
    seq: u32,
) -> Result<Fetched> {
    let prefix = match (prefix, network) {
        (Some(p), _) => p.trim_end_matches('/').to_string(),
        (None, Network::Mainnet) => PUBNET_LAKE_PREFIX.to_string(),
        (None, Network::Testnet) => testnet_lake_prefix(client)?,
    };
    let cfg = lake_config(client, &prefix)?
        .with_context(|| format!("{LAKE_ENDPOINT}/{prefix} has no .config.json"))?;
    ensure!(
        cfg.network_passphrase == network.passphrase(),
        "{prefix} is a lake for {:?}, not {:?}",
        cfg.network_passphrase,
        network.passphrase()
    );
    let url = format!("{LAKE_ENDPOINT}/{prefix}/{}", lake_object_key(&cfg, seq)?);
    let compressed = get(client, &url)?
        .with_context(|| format!("ledger {seq} is not in the lake ({url} does not exist)"))?;
    let raw = decompress_capped(&compressed, MAX_BATCH_LEN, &url)?;
    // each ledger is decoded on its own at xdr.meta.depth: decoded whole, the batch's struct
    // and vector spend two more levels above every ledger
    let batch =
        Batch::parse(raw).with_context(|| format!("decoding {url} as LedgerCloseMetaBatch"))?;
    ensure!(
        batch.start <= seq && seq <= batch.end,
        "{url} holds ledgers {} to {}, not {seq}",
        batch.start,
        batch.end
    );
    let bytes = batch
        .ledger(seq)
        .with_context(|| format!("{url} has no ledger {seq}"))?
        .to_vec();
    Ok(Fetched {
        bytes,
        path: LAKE_PATH,
        url,
    })
}

/// Decompresses a downloaded zstd object, failing once it passes `limit` bytes: the frame
/// header's content size is the server's claim, so it is neither trusted nor needed.
fn decompress_capped(compressed: &[u8], limit: u64, what: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    zstd::stream::Decoder::new(compressed)
        .with_context(|| format!("decompressing {what}"))?
        .take(limit + 1)
        .read_to_end(&mut out)
        .with_context(|| format!("decompressing {what}"))?;
    ensure!(
        out.len() as u64 <= limit,
        "{what} decompresses to more than {limit} bytes"
    );
    Ok(out)
}

/// Fetches ledger `seq` through stellar-rpc `getLedgers`.
pub fn fetch_rpc(client: &reqwest::blocking::Client, rpc: &str, seq: u32) -> Result<Fetched> {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLedgers",
        "params": {"startLedger": seq, "pagination": {"limit": 1}, "xdrFormat": "base64"},
    });
    let resp = client
        .post(rpc)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(&request)?)
        .send()
        .with_context(|| format!("getLedgers on {rpc}"))?
        .error_for_status()
        .with_context(|| format!("getLedgers on {rpc}"))?;
    // the url is the caller's, so the reply is untrusted: read no more than one ledger needs
    let mut body = Vec::new();
    resp.take(MAX_OBJECT + 1)
        .read_to_end(&mut body)
        .with_context(|| format!("reading the getLedgers reply from {rpc}"))?;
    ensure!(
        body.len() as u64 <= MAX_OBJECT,
        "the getLedgers reply from {rpc} is larger than {MAX_OBJECT} bytes"
    );
    let resp: serde_json::Value = serde_json::from_slice(&body)
        .with_context(|| format!("parsing the getLedgers reply from {rpc}"))?;
    if let Some(err) = resp.get("error") {
        bail!("getLedgers on {rpc} for ledger {seq} failed: {err}");
    }
    let ledger = resp
        .pointer("/result/ledgers/0")
        .with_context(|| format!("getLedgers on {rpc} returned no ledger for {seq}"))?;
    let got = ledger.get("sequence").and_then(serde_json::Value::as_u64);
    ensure!(
        got == Some(u64::from(seq)),
        "getLedgers on {rpc} returned ledger {got:?}, not {seq} (outside its retention window?)"
    );
    let b64 = ledger
        .get("metadataXdr")
        .and_then(serde_json::Value::as_str)
        .context("getLedgers reply has no metadataXdr")?;
    // xdr decoding is strict (no trailing bytes, zero padding), so re-encoding gives back the
    // served bytes exactly
    let lcm = LedgerCloseMeta::from_xdr_base64(b64, decode_limits("LedgerCloseMeta", b64.len()))
        .context("decoding metadataXdr")?;
    Ok(Fetched {
        bytes: lcm.to_xdr(Limits::none())?,
        path: "stellar-rpc getLedgers",
        url: rpc.to_string(),
    })
}

/// What a capture stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slice {
    /// The whole `LedgerCloseMeta`, one entry.
    Ledger,
    /// One transaction, cut byte for byte out of the ledger as two entries: `<slug>-envelope`
    /// (its `TransactionEnvelope` from the transaction set) and `<slug>-meta` (its
    /// `TransactionResultMeta`, or `TransactionResultMetaV1` in a v2 ledger, from
    /// `tx_processing`). Used where the whole ledger would be megabytes of XDR-JSON.
    Tx,
}

pub struct CaptureRequest<'a> {
    pub network: Network,
    pub ledger: u32,
    pub tx: Option<&'a str>,
    pub slice: Slice,
    pub family: &'a str,
    pub slug: &'a str,
    pub linked_issues: Vec<String>,
    pub note: Option<&'a str>,
}

/// What a capture wrote and what the ledger holds.
#[derive(Debug)]
pub struct CaptureReport {
    pub entries: Vec<Entry>,
    pub matched_fragments: Vec<String>,
    pub tx_index: Option<u32>,
    pub close_time: u64,
    pub close_time_ms: Option<u64>,
    pub stellar_value_ext: &'static str,
    pub tx_count: usize,
    pub lcm_version: u32,
    pub protocol: u32,
}

/// Checks the fetched ledger (sequence, round trip, transaction presence, family membership)
/// and writes it as one entry, or as a transaction slice of two.
pub fn write_capture(
    corpus: &Path,
    registry: &Registry,
    req: &CaptureRequest<'_>,
    fetched: &Fetched,
    date: &str,
) -> Result<CaptureReport> {
    let family = registry
        .family(req.family)
        .with_context(|| format!("unknown family {}", req.family))?;
    let lcm = decode_lcm(&fetched.bytes)?;
    ensure!(
        ledger_seq(&lcm) == req.ledger,
        "fetched ledger {} instead of {}",
        ledger_seq(&lcm),
        req.ledger
    );
    roundtrip_check(
        "LedgerCloseMeta",
        &fetched.bytes,
        decode_limits("LedgerCloseMeta", fetched.bytes.len()),
    )?;

    let hashes = tx_hashes(&lcm);
    let tx = req.tx.map(str::to_ascii_lowercase);
    let tx_index = match &tx {
        None => None,
        Some(want) => {
            let pos = hashes
                .iter()
                .position(|h| h == want)
                .with_context(|| format!("transaction {want} is not in ledger {}", req.ledger))?;
            Some(u32::try_from(pos)?)
        }
    };

    // (type, bytes, slug suffix) of every entry to write
    let parts: Vec<(&'static str, Vec<u8>, &'static str)> = match (req.slice, &tx, tx_index) {
        (Slice::Ledger, _, _) => vec![("LedgerCloseMeta", fetched.bytes.clone(), "")],
        (Slice::Tx, Some(hash), Some(index)) => {
            let envelope = find_envelope(&lcm, req.network.passphrase(), hash)?;
            let (meta_type, meta) = result_meta(&lcm, index as usize)?;
            vec![
                ("TransactionEnvelope", envelope, "-envelope"),
                (meta_type, meta, "-meta"),
            ]
        }
        (Slice::Tx, _, _) => bail!("a transaction slice needs --tx"),
    };

    let mut signature = BTreeSet::new();
    for (type_name, bytes, _) in &parts {
        signature.extend(shape_signature(
            type_name,
            bytes,
            decode_limits(type_name, bytes.len()),
        )?);
    }
    let matched: Vec<String> = family
        .signature
        .iter()
        .filter(|f| fragment_matches(&signature, f))
        .cloned()
        .collect();
    ensure!(
        !matched.is_empty(),
        "the captured {} matches none of the signature fragments of {}, so it is not an entry of that family",
        if req.slice == Slice::Tx {
            "transaction"
        } else {
            "ledger"
        },
        family.id
    );

    let hdr = header(&lcm);
    let (ext_name, close_time_ms) = match &hdr.scp_value.ext {
        StellarValueExt::Basic => ("basic", None),
        StellarValueExt::Signed(_) => ("signed", None),
        StellarValueExt::EmptyTxSet(_) => ("empty_tx_set", None),
        StellarValueExt::SignedMs(v) => ("signed_ms", Some(v.close_time_ms.0)),
        StellarValueExt::EmptyTxSetMs(v) => ("empty_tx_set_ms", Some(v.close_time_ms.0)),
    };
    let ledger_hash = match &lcm {
        LedgerCloseMeta::V0(v) => &v.ledger_header.hash,
        LedgerCloseMeta::V1(v) => &v.ledger_header.hash,
        LedgerCloseMeta::V2(v) => &v.ledger_header.hash,
    };
    let mut notes = format!(
        "captured {date} through the {} from {}. ledger {} hash {}, protocol {}, LedgerCloseMeta v{}",
        fetched.path,
        fetched.url,
        req.ledger,
        hex_lower(&ledger_hash.0),
        ledger_version(&lcm),
        lcm_version(&lcm)
    );
    if fetched.path == LAKE_PATH {
        // the lake was produced by a current core replaying history with backfill enabled, so
        // old ledgers carry the meta versions that replay emits, not the ones of their day
        notes.push_str(". the public lake holds history as re-emitted by a current core (TransactionMeta v4 for every protocol)");
    }
    if req.slice == Slice::Tx {
        notes.push_str(&format!(
            ". transaction slice: the envelope and the result meta of transaction {} cut byte for byte out of the ledger",
            tx.as_deref().unwrap_or_default()
        ));
    }
    if let Some(n) = req.note {
        notes.push_str(". ");
        notes.push_str(n);
    }

    let mut entries = Vec::new();
    for (type_name, bytes, suffix) in parts {
        let variant = Variant {
            network_passphrase: Some(req.network.passphrase().to_string()),
            xdr_file: String::new(),
            xdr_sha256: String::new(),
            expected_json_file: None,
            expected_json_sha256: None,
            oracle_file: None,
            oracle_sha256: None,
        };
        let mut variants = Variants::default();
        match req.network {
            Network::Testnet => variants.testnet = Some(variant),
            Network::Mainnet => variants.mainnet = Some(variant),
        }
        let is_ledger = type_name == "LedgerCloseMeta";
        let entry = Entry {
            id: format!("{}.{}{suffix}", family.id, req.slug),
            kind: EntryKind::Xdr,
            family: family.id.clone(),
            caps: family.caps.clone(),
            protocol: EntryProtocol {
                introduced: family.protocol_introduced,
                generated_at: Some(ledger_version(&lcm)),
                valid_min: family.protocol_valid.min,
                valid_max: family.protocol_valid.max,
            },
            xdr_type: Some(type_name.to_string()),
            lcm_version: is_ledger.then(|| lcm_version(&lcm)),
            tx_meta_version: if is_ledger || suffix == "-meta" {
                tx_meta_version(&lcm)
            } else {
                None
            },
            source: Source {
                class: req.network.class(),
                recipe: None,
                chain: None,
                ledger_seq: Some(req.ledger),
                tx_hash: tx.clone(),
                tx_index,
                upstream: None,
                provenance_url: Some(fetched.url.clone()),
                normalization: None,
                core_rejection: None,
            },
            synthetic: false,
            passphrase_dependent: true,
            variants,
            vector: None,
            expected: Expected {
                validity: Validity::Valid,
                decode: DecodeExpectation::MustAccept,
                roundtrip: true,
            },
            flags: Vec::new(),
            fields_exercised: Vec::new(),
            linked_issues: req.linked_issues.clone(),
            notes: Some(notes.clone()),
        };
        entries.push(entry_io::write_xdr_entry(
            corpus,
            entry,
            req.network.variant(),
            &bytes,
        )?);
    }
    Ok(CaptureReport {
        entries,
        matched_fragments: matched,
        tx_index,
        close_time: hdr.scp_value.close_time.0,
        close_time_ms,
        stellar_value_ext: ext_name,
        tx_count: hashes.len(),
        lcm_version: lcm_version(&lcm),
        protocol: ledger_version(&lcm),
    })
}

/// The bytes of the envelope in the ledger's transaction set whose hash under `passphrase`
/// is `hash`.
pub fn find_envelope(lcm: &LedgerCloseMeta, passphrase: &str, hash: &str) -> Result<Vec<u8>> {
    let network_id = network_id(passphrase);
    for env in envelopes(lcm) {
        if hex_lower(&env.hash(network_id)?) == hash {
            return Ok(env.to_xdr(Limits::none())?);
        }
    }
    bail!("no envelope in the transaction set hashes to {hash}")
}

fn envelopes(lcm: &LedgerCloseMeta) -> Vec<&TransactionEnvelope> {
    match lcm {
        LedgerCloseMeta::V0(v) => v.tx_set.txs.iter().collect(),
        LedgerCloseMeta::V1(v) => generalized_envelopes(&v.tx_set),
        LedgerCloseMeta::V2(v) => generalized_envelopes(&v.tx_set),
    }
}

fn generalized_envelopes(set: &GeneralizedTransactionSet) -> Vec<&TransactionEnvelope> {
    let GeneralizedTransactionSet::V1(v1) = set;
    let mut out = Vec::new();
    for phase in v1.phases.iter() {
        match phase {
            TransactionPhase::V0(components) => {
                for c in components.iter() {
                    let TxSetComponent::TxsetCompTxsMaybeDiscountedFee(c) = c;
                    out.extend(c.txs.iter());
                }
            }
            TransactionPhase::V1(parallel) => {
                for stage in parallel.execution_stages.iter() {
                    for cluster in stage.0.iter() {
                        out.extend(cluster.0.iter());
                    }
                }
            }
        }
    }
    out
}

/// The result meta of the transaction at apply index `index`, with its XDR type name.
pub fn result_meta(lcm: &LedgerCloseMeta, index: usize) -> Result<(&'static str, Vec<u8>)> {
    let limits = Limits::none();
    let out = match lcm {
        LedgerCloseMeta::V0(v) => (
            "TransactionResultMeta",
            v.tx_processing.get(index).map(|t| t.to_xdr(limits)),
        ),
        LedgerCloseMeta::V1(v) => (
            "TransactionResultMeta",
            v.tx_processing.get(index).map(|t| t.to_xdr(limits)),
        ),
        LedgerCloseMeta::V2(v) => (
            "TransactionResultMetaV1",
            v.tx_processing.get(index).map(|t| t.to_xdr(limits)),
        ),
    };
    match out {
        (name, Some(bytes)) => Ok((name, bytes?)),
        (_, None) => bail!("no transaction at apply index {index}"),
    }
}

fn network_id(passphrase: &str) -> [u8; 32] {
    let hex = sha256_hex(passphrase.as_bytes());
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).expect("sha256_hex is hex");
    }
    out
}

/// Transaction hashes in apply order, from the result pairs (so no passphrase is needed).
pub fn tx_hashes(lcm: &LedgerCloseMeta) -> Vec<String> {
    match lcm {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|t| hex_lower(&t.result.transaction_hash.0))
            .collect(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|t| hex_lower(&t.result.transaction_hash.0))
            .collect(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|t| hex_lower(&t.result.transaction_hash.0))
            .collect(),
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Today's UTC date as `YYYY-MM-DD`.
pub fn today_utc() -> Result<String> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before 1970")?
        .as_secs();
    Ok(civil_date(i64::try_from(secs / 86_400)?))
}

// days since 1970-01-01 to a proleptic gregorian date (howard hinnant's civil_from_days)
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> LakeConfig {
        LakeConfig {
            network_passphrase: TESTNET_PASSPHRASE.into(),
            version: "1.0".into(),
            compression: "zstd".into(),
            ledgers_per_batch: 1,
            batches_per_partition: 64000,
        }
    }

    #[test]
    fn object_keys_match_the_live_lake() {
        // keys as listed from the bucket on 2026-11-12
        assert_eq!(
            lake_object_key(&cfg(), 5_567_999).unwrap(),
            "FFAC03FF--5504000-5567999/FFAB0A00--5567999.xdr.zst"
        );
        assert_eq!(
            lake_object_key(&cfg(), 5_695_601).unwrap(),
            "FFAA0FFF--5632000-5695999/FFA9178E--5695601.xdr.zst"
        );
        let mut batched = cfg();
        batched.ledgers_per_batch = 64;
        batched.batches_per_partition = 10;
        assert_eq!(
            lake_object_key(&batched, 130).unwrap(),
            "FFFFFFFF--0-639/FFFFFF7F--128-191.xdr.zst"
        );
        let mut flat = cfg();
        flat.batches_per_partition = 1;
        assert_eq!(lake_object_key(&flat, 7).unwrap(), "FFFFFFF8--7.xdr.zst");
    }

    #[test]
    fn decompression_stops_at_the_cap() {
        let raw = vec![7u8; 1 << 20];
        let small = zstd::bulk::compress(&raw, 3).unwrap();
        // a megabyte of one byte compresses to a few hundred: the ratio a bomb exploits
        assert!(small.len() < 4096);
        assert_eq!(decompress_capped(&small, 1 << 20, "x").unwrap(), raw);
        let e = decompress_capped(&small, (1 << 20) - 1, "x").unwrap_err();
        assert!(e.to_string().contains("more than"), "{e}");
        assert!(decompress_capped(b"not zstd", 1 << 20, "x").is_err());
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(20_769), "2026-11-12");
        assert_eq!(civil_date(11_016), "2000-02-29");
    }
}
