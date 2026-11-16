//! A SEP-54 ledger lake read one object at a time, over HTTP from a public bucket or from a
//! directory. Nothing is cached: each object is fetched, handed over and dropped.

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use shapekit_pack::lake::{CONFIG_FILE, MAX_BATCH_LEN};
use shapekit_pack::layout::parse_batch_name;
use shapekit_pack::{LakeConfig, Layout};

/// The public bucket SDF writes both networks' lakes into (aws-public-blockchain, us-east-2).
pub const PUBLIC_ENDPOINT: &str = "https://aws-public-blockchain.s3.us-east-2.amazonaws.com";
pub const PUBNET_PREFIX: &str = "v1.1/stellar/ledgers/pubnet";
/// Testnet resets get a dated prefix each, `<root>/<yyyy-mm-dd>`.
pub const TESTNET_ROOT: &str = "v1.1/stellar/ledgers/testnet";

pub const PUBNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";
pub const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";

/// A compressed object above this is refused before it is decompressed.
const MAX_OBJECT: u64 = 512 << 20;

/// Attempts per object. A lake object either exists or not, so only transport errors and
/// server errors are retried; a missing object fails at once.
const ATTEMPTS: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Network {
    Pubnet,
    Testnet,
}

impl Network {
    pub fn passphrase(self) -> &'static str {
        match self {
            Network::Pubnet => PUBNET_PASSPHRASE,
            Network::Testnet => TESTNET_PASSPHRASE,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Network::Pubnet => "pubnet",
            Network::Testnet => "testnet",
        }
    }

    pub fn contract(self) -> shapekit_contracts::audit::AuditNetwork {
        match self {
            Network::Pubnet => shapekit_contracts::audit::AuditNetwork::Pubnet,
            Network::Testnet => shapekit_contracts::audit::AuditNetwork::Testnet,
        }
    }
}

enum Store {
    Http {
        client: reqwest::blocking::Client,
        /// The bucket endpoint, for listings.
        endpoint: String,
        /// The lake root inside the bucket, without slashes at either end.
        prefix: String,
    },
    Dir(PathBuf),
}

/// One lake: where it is, its config and the network it holds.
pub struct Lake {
    store: Store,
    pub config: LakeConfig,
    pub layout: Layout,
    pub network: Network,
    /// Objects fetched again after a transport or server error.
    retries: AtomicU64,
}

/// One fetched object, still compressed.
pub struct Object {
    pub key: String,
    pub bytes: Vec<u8>,
}

pub fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("shapekit/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(120))
        .build()
        .context("building the http client")
}

impl Lake {
    /// The public lake of `network`: the pubnet prefix, or the newest testnet prefix whose
    /// config names the testnet passphrase.
    pub fn public(network: Network) -> Result<Lake> {
        let client = http_client()?;
        let prefix = match network {
            Network::Pubnet => PUBNET_PREFIX.to_string(),
            Network::Testnet => newest_testnet_prefix(&client, PUBLIC_ENDPOINT)?,
        };
        Lake::http(client, PUBLIC_ENDPOINT, &prefix, network)
    }

    /// A lake in an S3-compatible bucket reachable without credentials.
    pub fn http(
        client: reqwest::blocking::Client,
        endpoint: &str,
        prefix: &str,
        network: Network,
    ) -> Result<Lake> {
        let store = Store::Http {
            client,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            prefix: prefix.trim_matches('/').to_string(),
        };
        Lake::open(store, network)
    }

    /// A lake on disk, as `shapekit pack` writes one.
    pub fn dir(path: PathBuf, network: Network) -> Result<Lake> {
        Lake::open(Store::Dir(path), network)
    }

    fn open(store: Store, network: Network) -> Result<Lake> {
        let retries = AtomicU64::new(0);
        let location = store.location();
        let bytes = store
            .get(CONFIG_FILE, &retries)?
            .with_context(|| format!("{location} has no {CONFIG_FILE}"))?;
        let config: LakeConfig = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {location}/{CONFIG_FILE}"))?;
        ensure!(
            config.network_passphrase == network.passphrase(),
            "{location} holds {:?}, not {}",
            config.network_passphrase,
            network.name()
        );
        ensure!(
            config.compression == shapekit_pack::lake::COMPRESSION,
            "{location} uses {} compression; SEP-54 defines only zstd",
            config.compression
        );
        let layout = config.layout()?;
        Ok(Lake {
            store,
            config,
            layout,
            network,
            retries,
        })
    }

    /// Where the lake is, for messages.
    pub fn location(&self) -> String {
        self.store.location()
    }

    pub fn retries(&self) -> u64 {
        self.retries.load(Ordering::Relaxed)
    }

    /// The compressed object holding ledger `seq`. A missing object is an error: a ledger
    /// the run asked for that the lake does not have is never skipped.
    pub fn fetch(&self, seq: u32) -> Result<Object> {
        let key = self.layout.object_key(seq);
        let bytes = self
            .get(&key)?
            .with_context(|| format!("ledger {seq} is not in {} ({key})", self.location()))?;
        Ok(Object { key, bytes })
    }

    /// The newest ledger the lake holds: the last ledger of the first batch of the first
    /// partition, since both are named by `u32::MAX - start` and list newest first.
    pub fn newest(&self) -> Result<u32> {
        let names = |listing: Vec<String>| -> Vec<String> {
            listing
                .into_iter()
                .map(|n| {
                    n.trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .to_string()
                })
                .collect()
        };
        let batch_dir = if self.layout.partitioned() {
            let partitions = names(self.list("", true)?);
            let newest = partitions
                .iter()
                .filter_map(|p| parse_range(p).map(|r| (r, p)))
                .max_by_key(|(r, _)| r.0)
                .with_context(|| format!("{} lists no partition", self.location()))?;
            format!("{}/", newest.1)
        } else {
            String::new()
        };
        let batches = names(self.list(&batch_dir, false)?);
        let newest = batches
            .iter()
            .filter_map(|b| parse_batch_name(b))
            .max_by_key(|r| r.0)
            .with_context(|| format!("{}/{batch_dir} lists no batch", self.location()))?;
        Ok(newest.1)
    }

    /// The first names under `dir` in key order, directories when `dirs` is set, objects
    /// otherwise. Keys sort newest first, so the first page holds the newest entries.
    fn list(&self, dir: &str, dirs: bool) -> Result<Vec<String>> {
        match &self.store {
            Store::Http {
                client,
                endpoint,
                prefix,
            } => {
                let url = format!(
                    "{endpoint}/?list-type=2&delimiter=/&max-keys=50&prefix={prefix}/{dir}"
                );
                let body = http_get(client, &url, &self.retries)?
                    .with_context(|| format!("{url} not found"))?;
                let text = String::from_utf8(body).context("bucket listing is not utf-8")?;
                Ok(xml_values(&text, if dirs { "Prefix" } else { "Key" })
                    .into_iter()
                    .filter(|v| v.len() > prefix.len() + 1 + dir.len())
                    .collect())
            }
            Store::Dir(root) => {
                let path = root.join(dir);
                let mut out = Vec::new();
                for e in std::fs::read_dir(&path)
                    .with_context(|| format!("listing {}", path.display()))?
                {
                    let e = e?;
                    if e.file_type()?.is_dir() == dirs {
                        out.push(e.file_name().to_string_lossy().into_owned());
                    }
                }
                out.sort();
                Ok(out)
            }
        }
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.store.get(key, &self.retries)
    }
}

impl Store {
    fn location(&self) -> String {
        match self {
            Store::Http {
                endpoint, prefix, ..
            } => format!("{endpoint}/{prefix}"),
            Store::Dir(p) => p.display().to_string(),
        }
    }

    fn get(&self, key: &str, retries: &AtomicU64) -> Result<Option<Vec<u8>>> {
        match self {
            Store::Http {
                client,
                endpoint,
                prefix,
            } => http_get(client, &format!("{endpoint}/{prefix}/{key}"), retries),
            Store::Dir(root) => {
                let path = root.join(key);
                match std::fs::metadata(&path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
                    Ok(m) => ensure!(
                        m.len() <= MAX_OBJECT,
                        "{} is larger than {MAX_OBJECT} bytes",
                        path.display()
                    ),
                }
                Ok(Some(
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
                ))
            }
        }
    }
}

fn http_get(
    client: &reqwest::blocking::Client,
    url: &str,
    retries: &AtomicU64,
) -> Result<Option<Vec<u8>>> {
    let mut last = None;
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            retries.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(500 << attempt));
        }
        match http_once(client, url) {
            Ok(Fetch::Found(b)) => return Ok(Some(b)),
            Ok(Fetch::Missing) => return Ok(None),
            Err(Fatal(e)) => return Err(e),
            Err(Transient(e)) => last = Some(e),
        }
    }
    Err(last.expect("at least one attempt ran"))
        .with_context(|| format!("GET {url} failed {ATTEMPTS} times"))
}

enum Fetch {
    Found(Vec<u8>),
    Missing,
}

enum FetchError {
    Fatal(anyhow::Error),
    Transient(anyhow::Error),
}
use FetchError::{Fatal, Transient};

fn http_once(client: &reqwest::blocking::Client, url: &str) -> Result<Fetch, FetchError> {
    let resp = client
        .get(url)
        .send()
        .map_err(|e| Transient(anyhow::Error::new(e).context(format!("GET {url}"))))?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(Fetch::Missing);
    }
    if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(Transient(anyhow::anyhow!("GET {url}: {status}")));
    }
    if !status.is_success() {
        return Err(Fatal(anyhow::anyhow!("GET {url}: {status}")));
    }
    let mut body = Vec::new();
    resp.take(MAX_OBJECT + 1)
        .read_to_end(&mut body)
        .map_err(|e| Transient(anyhow::Error::new(e).context(format!("reading {url}"))))?;
    if body.len() as u64 > MAX_OBJECT {
        return Err(Fatal(anyhow::anyhow!(
            "{url} is larger than {MAX_OBJECT} bytes"
        )));
    }
    Ok(Fetch::Found(body))
}

/// Decompresses one object, failing once it passes [`MAX_BATCH_LEN`]: the frame's content
/// size is the server's claim, so it is neither trusted nor needed.
pub fn decompress(object: &Object) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    zstd::stream::Decoder::new(object.bytes.as_slice())
        .with_context(|| format!("decompressing {}", object.key))?
        .take(MAX_BATCH_LEN + 1)
        .read_to_end(&mut out)
        .with_context(|| format!("decompressing {}", object.key))?;
    if out.len() as u64 > MAX_BATCH_LEN {
        bail!(
            "{} decompresses to more than {MAX_BATCH_LEN} bytes",
            object.key
        );
    }
    Ok(out)
}

/// `<hex>--<start>-<end>`, a partition directory name, as its range.
fn parse_range(name: &str) -> Option<(u32, u32)> {
    let (inv, range) = name.split_once("--")?;
    let inv = u32::from_str_radix(inv, 16)
        .ok()
        .filter(|_| inv.len() == 8)?;
    let (s, e) = range.split_once('-')?;
    let (s, e) = (s.parse::<u32>().ok()?, e.parse::<u32>().ok()?);
    (u32::MAX - s == inv && s <= e).then_some((s, e))
}

/// The text of every `<tag>` element in an S3 listing, unescaped for the five XML entities.
fn xml_values(text: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    text.split(open.as_str())
        .skip(1)
        .filter_map(|s| s.split_once(close.as_str()).map(|(v, _)| v))
        .map(|v| {
            v.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&amp;", "&")
        })
        .collect()
}

/// The newest dated testnet prefix with a config for the testnet passphrase; older ones hold
/// networks that were reset away.
fn newest_testnet_prefix(client: &reqwest::blocking::Client, endpoint: &str) -> Result<String> {
    let url = format!("{endpoint}/?list-type=2&delimiter=/&prefix={TESTNET_ROOT}/");
    let retries = AtomicU64::new(0);
    let body = http_get(client, &url, &retries)?.with_context(|| format!("{url} not found"))?;
    let text = String::from_utf8(body).context("bucket listing is not utf-8")?;
    ensure!(
        !text.contains("<IsTruncated>true</IsTruncated>"),
        "the testnet prefix listing is truncated"
    );
    let mut dated: Vec<String> = xml_values(&text, "Prefix")
        .into_iter()
        .map(|p| p.trim_end_matches('/').to_string())
        .filter(|p| {
            p.strip_prefix(TESTNET_ROOT)
                .and_then(|r| r.strip_prefix('/'))
                .is_some_and(|d| {
                    d.len() == 10 && d.bytes().all(|b| b.is_ascii_digit() || b == b'-')
                })
        })
        .collect();
    dated.sort();
    for prefix in dated.iter().rev() {
        let url = format!("{endpoint}/{prefix}/{CONFIG_FILE}");
        if let Some(body) = http_get(client, &url, &retries)? {
            let cfg: LakeConfig =
                serde_json::from_slice(&body).with_context(|| format!("parsing {url}"))?;
            if cfg.network_passphrase == TESTNET_PASSPHRASE {
                return Ok(prefix.clone());
            }
        }
    }
    bail!("no testnet lake prefix under {TESTNET_ROOT} has a testnet config")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_values_and_partition_names() {
        let xml = "<ListBucketResult><Contents><Key>v1.1/stellar/ledgers/pubnet/.config.json</Key></Contents>\
            <CommonPrefixes><Prefix>v1.1/stellar/ledgers/pubnet/FC18F9FF--65472000-65535999/</Prefix></CommonPrefixes>\
            <CommonPrefixes><Prefix>a&amp;b</Prefix></CommonPrefixes></ListBucketResult>";
        assert_eq!(
            xml_values(xml, "Prefix"),
            vec![
                "v1.1/stellar/ledgers/pubnet/FC18F9FF--65472000-65535999/".to_string(),
                "a&b".to_string()
            ]
        );
        assert_eq!(
            parse_range("FC18F9FF--65472000-65535999"),
            Some((65472000, 65535999))
        );
        // the inverted start must agree with the decimal one
        assert_eq!(parse_range("FC18F9FE--65472000-65535999"), None);
        assert_eq!(parse_range(".config.json"), None);
    }
}
