//! Network and git access for `pins verify`. Everything here only fetches;
//! the comparisons live in `check`.

use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION};

// maven central asks a tool to name itself, its version and a contact path
// (central.sonatype.org/faq/429-tooling-provider)
const USER_AGENT: &str = concat!(
    "shapekit-pins-verify/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/ErenTopaal/shapekit)"
);

// multi-arch images answer with an index, single-arch ones with a manifest.
// listing all four types makes the registry return the digest of whatever the
// tag points at, which is what `docker pull <image>@<digest>` uses.
const MANIFEST_ACCEPT: &str = "application/vnd.oci.image.index.v1+json, \
application/vnd.docker.distribution.manifest.list.v2+json, \
application/vnd.docker.distribution.manifest.v2+json, \
application/vnd.oci.image.manifest.v1+json";

#[derive(Clone)]
pub struct Http {
    client: Client,
}

// lookups and connects fail transiently on busy resolvers, and a burst of
// parallel checks is exactly what trips them. those failures are retried a
// couple of times and each retry is reported on stderr. a reply from the
// server, good or bad, is never retried.
const ATTEMPTS: u32 = 3;

fn with_retries<T>(
    what: &str,
    is_transient: impl Fn(&anyhow::Error) -> bool,
    f: impl Fn() -> Result<T>,
) -> Result<T> {
    let mut attempt = 1;
    loop {
        match f() {
            Err(err) if attempt < ATTEMPTS && is_transient(&err) => {
                eprintln!(
                    "retry {attempt}/{}: {what}: {}",
                    ATTEMPTS - 1,
                    crate::error_text::chain(err.as_ref())
                );
                std::thread::sleep(Duration::from_secs(u64::from(attempt) * 2));
                attempt += 1;
            }
            other => return other,
        }
    }
}

fn http_transient(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|e| e.downcast_ref::<reqwest::Error>())
        .any(|e| e.is_connect() || e.is_timeout())
}

fn git_transient(err: &anyhow::Error) -> bool {
    let msg = crate::error_text::chain(err.as_ref());
    [
        "Could not resolve host",
        "Failed to connect",
        "Connection timed out",
        "Connection reset",
    ]
    .iter()
    .any(|m| msg.contains(m))
}

// a refusal names who answered: a cdn or a filter in front of a registry can answer a file that
// exists with 403 or 404, and the status alone does not tell that from a missing file
const ANSWERED_BY: [&str; 5] = ["server", "via", "x-served-by", "cf-ray", "cf-mitigated"];

fn ensure_success(resp: Response, url: &str) -> Result<Response> {
    let status = resp.status();
    if !status.is_success() {
        let mut by = Vec::new();
        for name in ANSWERED_BY {
            if let Some(v) = resp.headers().get(name).and_then(|v| v.to_str().ok()) {
                by.push(format!("{name}: {v}"));
            }
        }
        let body = resp.text().unwrap_or_default();
        let body: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
        let body: String = body.chars().take(160).collect();
        bail!("GET {url} returned {status} ({}); body: {body:?}", by.join(", "));
    }
    Ok(resp)
}

impl Http {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(60))
            .build()
            .context("building http client")?;
        Ok(Self { client })
    }

    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        with_retries(&format!("GET {url}"), http_transient, || {
            let resp = self
                .client
                .get(url)
                .send()
                .with_context(|| format!("GET {url}"))?;
            Ok(ensure_success(resp, url)?
                .bytes()
                .with_context(|| format!("reading body of {url}"))?
                .to_vec())
        })
    }

    pub fn get_text(&self, url: &str) -> Result<String> {
        with_retries(&format!("GET {url}"), http_transient, || {
            let resp = self
                .client
                .get(url)
                .send()
                .with_context(|| format!("GET {url}"))?;
            ensure_success(resp, url)?
                .text()
                .with_context(|| format!("reading body of {url}"))
        })
    }

    /// GETs a GitHub REST API document. The api allows 60 unauthenticated
    /// requests an hour per address, which a shared CI runner can use up, so a
    /// `GITHUB_TOKEN` in the environment (what Actions provides) is sent along.
    pub fn get_github_api(&self, path: &str) -> Result<String> {
        let url = format!("https://api.github.com/{path}");
        let token = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty());
        with_retries(&format!("GET {url}"), http_transient, || {
            let mut req = self
                .client
                .get(&url)
                .header(ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28");
            if let Some(token) = &token {
                req = req.header(AUTHORIZATION, format!("Bearer {token}"));
            }
            let resp = req.send().with_context(|| format!("GET {url}"))?;
            ensure_success(resp, &url)?
                .text()
                .with_context(|| format!("reading body of {url}"))
        })
    }

    /// Returns the manifest digest a Docker Hub tag currently points at.
    pub fn docker_hub_digest(&self, repository: &str, tag: &str) -> Result<Option<String>> {
        let token_url = format!(
            "https://auth.docker.io/token?service=registry.docker.io&scope=repository:{repository}:pull"
        );
        let token: serde_json::Value = serde_json::from_str(&self.get_text(&token_url)?)
            .with_context(|| format!("parsing token response from {token_url}"))?;
        let Some(token) = token["token"].as_str() else {
            bail!("docker hub token response has no token field");
        };
        let url = format!("https://registry-1.docker.io/v2/{repository}/manifests/{tag}");
        let resp = with_retries(&format!("HEAD {url}"), http_transient, || {
            self.client
                .head(&url)
                .header(ACCEPT, MANIFEST_ACCEPT)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .send()
                .with_context(|| format!("HEAD {url}"))
        })?;
        let status = resp.status();
        if !status.is_success() {
            bail!("HEAD {url} returned {status}");
        }
        let digest = match resp.headers().get("docker-content-digest") {
            Some(v) => Some(
                v.to_str()
                    .context("Docker-Content-Digest header is not ascii")?
                    .to_string(),
            ),
            None => None,
        };
        Ok(digest)
    }
}

/// Why `name` is not a crate name crates.io accepts, or `None` when it is one: 1 to 64
/// ASCII letters, digits, `-` and `_`, starting with a letter.
pub fn crate_name_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() || name.len() > 64 {
        return Some("a crate name is 1 to 64 characters");
    }
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return Some("a crate name starts with an ASCII letter");
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Some("a crate name holds only ASCII letters, digits, - and _");
    }
    None
}

/// Path of a crate's file in the crates.io sparse index. The index layout cuts the name
/// into byte prefixes, so a name that is not a crate name is refused rather than cut.
pub fn sparse_index_path(name: &str) -> Result<String> {
    if let Some(why) = crate_name_problem(name) {
        bail!("{name:?} is not a crate name: {why}");
    }
    let name = name.to_ascii_lowercase();
    Ok(match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    })
}

/// The go proxy escapes upper case letters as `!` plus the lower case letter.
pub fn goproxy_escape(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if c.is_ascii_uppercase() {
            out.push('!');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn git(args: &[&str], dir: Option<&std::path::Path>) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        // never wait on a credential prompt for a repo that turned private or vanished
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let out = cmd
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "git {} failed ({}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    String::from_utf8(out.stdout).context("git output is not utf-8")
}

pub fn ls_remote(url: &str, git_ref: &str) -> Result<String> {
    let peeled = format!("{git_ref}^{{}}");
    let args = ["ls-remote", url, git_ref, &peeled];
    with_retries(&format!("git ls-remote {url}"), git_transient, || {
        git(&args, None)
    })
}

/// Confirms a bare commit exists on the remote by fetching just that commit.
/// ls-remote only lists ref tips, so a commit in the middle of history needs this.
pub fn commit_exists(url: &str, sha: &str) -> Result<()> {
    let dir = shapekit_process::scratch::scratch_dir("shapekit-pins-git-")
        .context("creating a scratch git directory")?;
    git(&["init", "--quiet"], Some(dir.path()))?;
    let args = ["fetch", "--quiet", "--depth=1", "--filter=tree:0", url, sha];
    with_retries(&format!("git fetch {url} {sha}"), git_transient, || {
        git(&args, Some(dir.path()))
    })?;
    let kind = git(&["cat-file", "-t", sha], Some(dir.path()))?;
    if kind.trim() != "commit" {
        bail!("{sha} is a {}, not a commit", kind.trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_only_transient_failures() {
        let calls = std::cell::Cell::new(0);
        let out: Result<u32> = with_retries(
            "test",
            |_| true,
            || {
                calls.set(calls.get() + 1);
                if calls.get() < 2 {
                    bail!("Could not resolve host: github.com")
                } else {
                    Ok(7)
                }
            },
        );
        assert_eq!(out.unwrap(), 7);
        assert_eq!(calls.get(), 2);

        calls.set(0);
        let out: Result<u32> = with_retries(
            "test",
            |_| false,
            || {
                calls.set(calls.get() + 1);
                bail!("HTTP 404")
            },
        );
        assert!(out.is_err());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn git_transient_messages() {
        let dns = anyhow::anyhow!(
            "git ls-remote failed (exit status: 128): fatal: unable to access 'https://github.com/stellar/stellar-xdr/': Could not resolve host: github.com"
        );
        let missing = anyhow::anyhow!(
            "git fetch failed (exit status: 128): fatal: remote error: upload-pack: not our ref 0e49751aa9ddf892302cca02cea7f3f5d97df9f0"
        );
        assert!(git_transient(&dns));
        assert!(!git_transient(&missing));
    }

    #[test]
    fn sparse_index_paths() {
        let path = |n| sparse_index_path(n).unwrap();
        assert_eq!(path("a"), "1/a");
        assert_eq!(path("hex"), "3/h/hex");
        assert_eq!(path("tar"), "3/t/tar");
        assert_eq!(path("clap"), "cl/ap/clap");
        assert_eq!(path("stellar-xdr"), "st/el/stellar-xdr");
        assert_eq!(path("Inflector"), "in/fl/inflector");
        // names the index cannot hold are refused, not cut mid character
        for bad in ["", "é", "aé", "1abc", "a b", &"a".repeat(65)] {
            assert!(sparse_index_path(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn goproxy_escaping() {
        assert_eq!(
            goproxy_escape("github.com/stellar/go-stellar-sdk"),
            "github.com/stellar/go-stellar-sdk"
        );
        assert_eq!(
            goproxy_escape("github.com/BurntSushi/toml"),
            "github.com/!burnt!sushi/toml"
        );
    }
}
