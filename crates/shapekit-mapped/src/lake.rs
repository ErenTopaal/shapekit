//! Releases a chain's SEP-54 lake to the target batch by batch, so the target sees only the
//! ledgers the harness made visible: into an S3 bucket, a GCS bucket or a directory.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};

struct Batch {
    key: String,
    path: PathBuf,
    end: u32,
}

pub enum Sink {
    /// `PUT <bucket_url>/<key>`.
    S3 { bucket_url: String },
    /// fake-gcs-server's JSON API: objects `<prefix><key>` in `bucket`.
    Gcs {
        url: String,
        bucket: String,
        prefix: String,
    },
    /// Files under `dir`.
    Dir { dir: PathBuf },
}

pub struct Lake {
    root: PathBuf,
    batches: Vec<Batch>,
    next: usize,
    config: bool,
    sink: Sink,
    client: reqwest::blocking::Client,
}

/// The last ledger of a batch file name: `FFFFFFFD--2.xdr.zst` holds ledger 2,
/// `FFFFFFFD--2-5.xdr.zst` ledgers 2 to 5.
fn batch_end(name: &str) -> Option<u32> {
    let stem = name.strip_suffix(".xdr.zst")?;
    let (prefix, range) = stem.split_once("--")?;
    if prefix.len() != 8 || !prefix.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let last = range.rsplit('-').next()?;
    last.parse().ok()
}

impl Lake {
    pub fn new(root: &Path, sink: Sink) -> Result<Lake> {
        let mut batches = Vec::new();
        for e in walkdir::WalkDir::new(root) {
            let e = e.with_context(|| format!("reading the lake at {}", root.display()))?;
            if !e.file_type().is_file() {
                continue;
            }
            let Some(end) = e.file_name().to_str().and_then(batch_end) else {
                continue;
            };
            let rel = e
                .path()
                .strip_prefix(root)
                .expect("walkdir stays under its root");
            let key = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            batches.push(Batch {
                key,
                path: e.path().to_owned(),
                end,
            });
        }
        if batches.is_empty() {
            bail!("the lake at {} has no batches", root.display());
        }
        batches.sort_by_key(|b| b.end);
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Lake {
            root: root.to_owned(),
            batches,
            next: 0,
            config: false,
            sink,
            client,
        })
    }

    /// Creates the GCS bucket, retrying until the emulator answers or `wait` passes.
    pub fn prepare(&self, wait: Duration) -> Result<()> {
        let Sink::Gcs { url, bucket, .. } = &self.sink else {
            return Ok(());
        };
        let deadline = Instant::now() + wait;
        loop {
            let res = self
                .client
                .post(format!("{url}/storage/v1/b?project=shapekit"))
                .header("Content-Type", "application/json")
                .body(serde_json::json!({ "name": bucket }).to_string())
                .send();
            let err = match res {
                Ok(r) if r.status().is_success() || r.status().as_u16() == 409 => return Ok(()),
                Ok(r) => format!("{} {}", r.status(), r.text().unwrap_or_default().trim()),
                Err(e) => e.to_string(),
            };
            if Instant::now() > deadline {
                bail!("the GCS store did not accept bucket {bucket} within {wait:?}: {err}");
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    /// Releases `.config.json` and every batch whose last ledger is at or below `head`.
    pub fn copy_up_to(&mut self, head: u32) -> Result<()> {
        if !self.config {
            let p = self.root.join(".config.json");
            self.put(".config.json", &p)?;
            self.config = true;
        }
        while self.next < self.batches.len() && self.batches[self.next].end <= head {
            let (key, path) = {
                let b = &self.batches[self.next];
                (b.key.clone(), b.path.clone())
            };
            self.put(&key, &path)?;
            self.next += 1;
        }
        Ok(())
    }

    fn put(&self, key: &str, path: &Path) -> Result<()> {
        let body = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        match &self.sink {
            Sink::S3 { bucket_url } => {
                let r = self
                    .client
                    .put(format!("{bucket_url}/{key}"))
                    .body(body)
                    .send()
                    .with_context(|| format!("PUT {key}"))?;
                if r.status().as_u16() != 200 {
                    bail!("PUT {key}: {}", r.status());
                }
            }
            Sink::Gcs {
                url,
                bucket,
                prefix,
            } => {
                let mut u = reqwest::Url::parse(&format!("{url}/upload/storage/v1/b/{bucket}/o"))?;
                u.query_pairs_mut()
                    .append_pair("uploadType", "media")
                    .append_pair("name", &format!("{prefix}{key}"));
                let r = self
                    .client
                    .post(u)
                    .header("Content-Type", "application/octet-stream")
                    .body(body)
                    .send()
                    .with_context(|| format!("uploading {key}"))?;
                if r.status().as_u16() != 200 {
                    let s = r.status();
                    bail!(
                        "uploading {key}: {s} {}",
                        r.text().unwrap_or_default().trim()
                    );
                }
            }
            Sink::Dir { dir } => {
                let dest = dir.join(key);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
                // written aside and renamed, so the target never reads half a batch
                let tmp = dest.with_extension("partial");
                std::fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
                std::fs::rename(&tmp, &dest)
                    .with_context(|| format!("renaming {}", tmp.display()))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_batches_up_to_the_head_into_a_directory() {
        let root = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".config.json"), "{}").unwrap();
        let part = root.path().join("FFFFFFFF--0-63999");
        std::fs::create_dir_all(&part).unwrap();
        for (name, body) in [("FFFFFFFD--2.xdr.zst", "a"), ("FFFFFFFC--3.xdr.zst", "b")] {
            std::fs::write(part.join(name), body).unwrap();
        }
        assert_eq!(batch_end("FFFFFFFD--2-5.xdr.zst"), Some(5));
        let mut lake = Lake::new(
            root.path(),
            Sink::Dir {
                dir: out.path().to_owned(),
            },
        )
        .unwrap();
        lake.copy_up_to(2).unwrap();
        assert!(out.path().join(".config.json").exists());
        assert!(
            out.path()
                .join("FFFFFFFF--0-63999/FFFFFFFD--2.xdr.zst")
                .exists()
        );
        assert!(
            !out.path()
                .join("FFFFFFFF--0-63999/FFFFFFFC--3.xdr.zst")
                .exists()
        );
        lake.copy_up_to(3).unwrap();
        assert!(
            out.path()
                .join("FFFFFFFF--0-63999/FFFFFFFC--3.xdr.zst")
                .exists()
        );
    }
}
