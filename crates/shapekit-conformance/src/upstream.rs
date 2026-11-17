//! What the package registries and stellar/stellar-xdr publish, for `conformance sync-versions`.
//! Everything here only fetches and parses; the selection lives in `sync`.

use std::collections::BTreeMap;
use std::io::Read;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION};
use serde::Deserialize;

const USER_AGENT: &str = concat!("shapekit-sync-versions/", env!("CARGO_PKG_VERSION"));

/// The definitions repository whose release tags name protocols.
pub const XDR_REPO: &str = "https://github.com/stellar/stellar-xdr";

/// One published version of an sdk package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    /// UTC, `YYYY-MM-DDTHH:MM:SS` (see `utc_seconds`).
    pub published: String,
    /// Why the registry withdrew it (yanked crate, deprecated npm version, retracted module).
    pub withdrawn: Option<String>,
}

/// An sdk's releases and the version its registry calls the latest stable one.
#[derive(Debug, Clone)]
pub struct Published {
    pub releases: Vec<Release>,
    pub latest: String,
}

/// A stellar/stellar-xdr release tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XdrTag {
    /// `v28.0`.
    pub name: String,
    /// The protocol the tag is named for.
    pub major: u32,
    pub minor: u32,
    pub commit: String,
}

#[derive(Clone)]
pub struct Http {
    client: Client,
}

// lookups and connects fail transiently on busy resolvers; those are retried, a reply from the
// server, good or bad, is not
const ATTEMPTS: u32 = 3;

impl Http {
    pub fn new() -> Result<Http> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(120))
            .build()
            .context("building http client")?;
        Ok(Http { client })
    }

    fn get(&self, url: &str, github_api: bool) -> Result<Vec<u8>> {
        let token = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty());
        let mut attempt = 1;
        loop {
            let mut req = self.client.get(url);
            if github_api {
                req = req
                    .header(ACCEPT, "application/vnd.github+json")
                    .header("X-GitHub-Api-Version", "2022-11-28");
                if let Some(t) = &token {
                    req = req.header(AUTHORIZATION, format!("Bearer {t}"));
                }
            }
            match req.send() {
                Ok(resp) => {
                    let status = resp.status();
                    ensure!(status.is_success(), "GET {url} returned {status}");
                    return Ok(resp
                        .bytes()
                        .with_context(|| format!("reading body of {url}"))?
                        .to_vec());
                }
                Err(e) if attempt < ATTEMPTS && (e.is_connect() || e.is_timeout()) => {
                    eprintln!("retry {attempt}/{}: GET {url}: {e}", ATTEMPTS - 1);
                    std::thread::sleep(Duration::from_secs(u64::from(attempt) * 2));
                    attempt += 1;
                }
                Err(e) => return Err(e).with_context(|| format!("GET {url}")),
            }
        }
    }

    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        self.get(url, false)
    }

    pub fn get_text(&self, url: &str) -> Result<String> {
        String::from_utf8(self.get(url, false)?).with_context(|| format!("{url} is not utf-8"))
    }

    /// The GitHub REST api allows 60 unauthenticated requests an hour per address; a
    /// `GITHUB_TOKEN` in the environment is sent along.
    pub fn github_api(&self, path: &str) -> Result<String> {
        let url = format!("https://api.github.com/{path}");
        String::from_utf8(self.get(&url, true)?).with_context(|| format!("{url} is not utf-8"))
    }
}

/// Seconds since the epoch of an RFC 3339 time in UTC, so times from different registries
/// compare. Fractions of a second are dropped; another offset than UTC is refused rather than
/// guessed at.
pub fn utc_seconds(t: &str) -> Result<i64> {
    let bad = || anyhow::anyhow!("{t:?} is not an RFC 3339 time in UTC");
    ensure!(t.len() >= 20 && t.is_char_boundary(19), bad());
    let (head, tail) = t.split_at(19);
    let tail = match tail.strip_prefix('.') {
        Some(frac) => frac.trim_start_matches(|c: char| c.is_ascii_digit()),
        None => tail,
    };
    ensure!(matches!(tail, "Z" | "+00:00" | "z"), bad());
    let b = head.as_bytes();
    ensure!(
        b[4] == b'-'
            && b[7] == b'-'
            && (b[10] == b'T' || b[10] == b't')
            && b[13] == b':'
            && b[16] == b':',
        bad()
    );
    let num =
        |r: std::ops::Range<usize>| -> Result<i64> { head[r].parse::<i64>().map_err(|_| bad()) };
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
    ensure!(
        (1..=12).contains(&mo) && (1..=31).contains(&d) && h < 24 && mi < 60 && s < 61,
        bad()
    );
    // days from civil, proleptic gregorian
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Ok(days * 86_400 + h * 3_600 + mi * 60 + s)
}

/// The stellar/stellar-xdr release tags, `v<major>` or `v<major>.<minor>`, from `git ls-remote`.
pub fn xdr_tags() -> Result<Vec<XdrTag>> {
    let out = Command::new("git")
        .args(["ls-remote", "--tags", XDR_REPO])
        .output()
        .context("running git ls-remote")?;
    ensure!(
        out.status.success(),
        "git ls-remote --tags {XDR_REPO} exited with {}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
    );
    parse_xdr_tags(&String::from_utf8(out.stdout).context("git ls-remote output")?)
}

pub fn parse_xdr_tags(ls_remote: &str) -> Result<Vec<XdrTag>> {
    // an annotated tag is listed twice: the tag object, then `^{}` with the commit it peels to
    let mut commits: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for line in ls_remote.lines() {
        let Some((sha, name)) = line.split_once('\t') else {
            bail!("unexpected git ls-remote line {line:?}");
        };
        let Some(name) = name.strip_prefix("refs/tags/") else {
            continue;
        };
        let (name, peeled) = match name.strip_suffix("^{}") {
            Some(n) => (n, true),
            None => (name, false),
        };
        let e = commits
            .entry(name.to_string())
            .or_insert((sha.to_string(), peeled));
        if peeled {
            *e = (sha.to_string(), true);
        }
    }
    let mut tags = Vec::new();
    for (name, (commit, _)) in commits {
        let Some(rest) = name.strip_prefix('v') else {
            continue;
        };
        let mut parts = rest.split('.');
        let major = parts.next().and_then(|p| p.parse::<u32>().ok());
        let minor = match parts.next() {
            None => Some(0),
            Some(p) => p.parse::<u32>().ok(),
        };
        if let (Some(major), Some(minor), None) = (major, minor, parts.next()) {
            tags.push(XdrTag {
                name,
                major,
                minor,
                commit,
            });
        }
    }
    ensure!(!tags.is_empty(), "{XDR_REPO} has no v<protocol> tags");
    tags.sort_by_key(|t| (t.major, t.minor));
    Ok(tags)
}

/// When a stellar/stellar-xdr commit was made, in UTC seconds.
pub fn xdr_commit_time(http: &Http, commit: &str) -> Result<i64> {
    #[derive(Deserialize)]
    struct C {
        commit: Inner,
    }
    #[derive(Deserialize)]
    struct Inner {
        committer: Person,
    }
    #[derive(Deserialize)]
    struct Person {
        date: String,
    }
    let body = http.github_api(&format!("repos/stellar/stellar-xdr/commits/{commit}"))?;
    let c: C = serde_json::from_str(&body).context("parsing the commit")?;
    utc_seconds(&c.commit.committer.date)
}

// ---- crates.io

pub fn crates_published(http: &Http, name: &str) -> Result<Published> {
    #[derive(Deserialize)]
    struct Versions {
        versions: Vec<V>,
        meta: Meta,
    }
    #[derive(Deserialize)]
    struct V {
        num: String,
        yanked: bool,
        created_at: String,
    }
    #[derive(Deserialize)]
    struct Meta {
        next_page: Option<String>,
    }
    #[derive(Deserialize)]
    struct Krate {
        #[serde(rename = "crate")]
        krate: Info,
    }
    #[derive(Deserialize)]
    struct Info {
        max_stable_version: Option<String>,
    }
    let mut releases = Vec::new();
    let mut url = format!("https://crates.io/api/v1/crates/{name}/versions");
    loop {
        let page: Versions = serde_json::from_str(&http.get_text(&url)?)
            .with_context(|| format!("parsing {url}"))?;
        for v in page.versions {
            releases.push(Release {
                version: v.num,
                published: v.created_at,
                withdrawn: v.yanked.then(|| "yanked".to_string()),
            });
        }
        match page.meta.next_page {
            Some(q) => url = format!("https://crates.io/api/v1/crates/{name}/versions{q}"),
            None => break,
        }
    }
    let info: Krate =
        serde_json::from_str(&http.get_text(&format!("https://crates.io/api/v1/crates/{name}"))?)
            .context("parsing the crate")?;
    let latest = info
        .krate
        .max_stable_version
        .with_context(|| format!("crates.io lists no stable version of {name}"))?;
    Ok(Published { releases, latest })
}

/// The stellar-xdr crate ships the definitions commit it was generated from in `xdr-version`.
pub fn crate_xdr_commit(http: &Http, name: &str, version: &str) -> Result<String> {
    let url = format!("https://static.crates.io/crates/{name}/{name}-{version}.crate");
    let bytes = http.get_bytes(&url)?;
    let want = format!("{name}-{version}/xdr-version");
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&bytes[..]));
    for entry in archive
        .entries()
        .with_context(|| format!("reading {url}"))?
    {
        let mut entry = entry.with_context(|| format!("reading {url}"))?;
        if entry.path()?.to_str() == Some(want.as_str()) {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .with_context(|| format!("reading {want}"))?;
            return commit_hash(text.trim()).with_context(|| format!("{want} in {url}"));
        }
    }
    bail!("{url} has no {want}")
}

/// The version a crate release pins one of its normal (not dev, not build) dependencies to
/// with `=`. Any other requirement stops the command: a range names no single version.
pub fn crate_exact_dep(http: &Http, name: &str, version: &str, dep: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct Deps {
        dependencies: Vec<D>,
    }
    #[derive(Deserialize)]
    struct D {
        crate_id: String,
        req: String,
        kind: String,
    }
    let url = format!("https://crates.io/api/v1/crates/{name}/{version}/dependencies");
    let deps: Deps =
        serde_json::from_str(&http.get_text(&url)?).with_context(|| format!("parsing {url}"))?;
    let reqs: std::collections::BTreeSet<String> = deps
        .dependencies
        .into_iter()
        .filter(|d| d.crate_id == dep && d.kind == "normal")
        .map(|d| d.req)
        .collect();
    ensure!(
        reqs.len() == 1,
        "{name} {version} has {} normal requirements on {dep}: {reqs:?}",
        reqs.len()
    );
    let req = reqs.into_iter().next().expect("one requirement");
    let exact = req
        .strip_prefix('=')
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
        .with_context(|| format!("{name} {version} requires {dep} {req}, not one exact version"))?;
    Ok(exact.to_string())
}

/// The stellar-xdr release a soroban-sdk release links: soroban-sdk pins soroban-env-host,
/// which pins soroban-env-common, which pins stellar-xdr, each with `=`.
pub fn soroban_sdk_xdr_crate(http: &Http, name: &str, version: &str) -> Result<String> {
    let host = crate_exact_dep(http, name, version, "soroban-env-host")?;
    let common = crate_exact_dep(http, "soroban-env-host", &host, "soroban-env-common")?;
    crate_exact_dep(http, "soroban-env-common", &common, "stellar-xdr")
}

// ---- the go module proxy

const GO_PROXY: &str = "https://proxy.golang.org";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GoInfo {
    pub version: String,
    pub time: String,
    #[serde(default)]
    pub origin: Option<GoOrigin>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GoOrigin {
    #[serde(rename = "URL")]
    pub url: String,
    pub hash: String,
}

/// The proxy escapes upper case letters in module paths as `!` and the lower case letter.
fn go_escape(module: &str) -> String {
    let mut out = String::new();
    for c in module.chars() {
        if c.is_ascii_uppercase() {
            out.push('!');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub fn go_info(http: &Http, module: &str, version: &str) -> Result<GoInfo> {
    let url = format!("{GO_PROXY}/{}/@v/{version}.info", go_escape(module));
    serde_json::from_str(&http.get_text(&url)?).with_context(|| format!("parsing {url}"))
}

pub fn go_published(http: &Http, module: &str) -> Result<Published> {
    let base = format!("{GO_PROXY}/{}/@v", go_escape(module));
    let latest: GoInfo =
        serde_json::from_str(&http.get_text(&format!("{GO_PROXY}/{}/@latest", go_escape(module)))?)
            .context("parsing @latest")?;
    // retractions are declared in the go.mod of the newest version
    let retract = parse_retractions(&http.get_text(&format!("{base}/{}.mod", latest.version))?)?;
    let mut releases = Vec::new();
    for v in http.get_text(&format!("{base}/list"))?.lines() {
        let v = v.trim();
        if v.is_empty() {
            continue;
        }
        let info = go_info(http, module, v)?;
        let withdrawn = retract
            .iter()
            .any(|r| r.covers(v))
            .then(|| "retracted".to_string());
        releases.push(Release {
            version: v.to_string(),
            published: info.time,
            withdrawn,
        });
    }
    Ok(Published {
        releases,
        latest: latest.version,
    })
}

/// A `retract` directive of a go.mod: one version or a closed interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retraction {
    pub low: String,
    pub high: String,
}

impl Retraction {
    fn covers(&self, v: &str) -> bool {
        match (semver(v), semver(&self.low), semver(&self.high)) {
            (Some(v), Some(lo), Some(hi)) => lo <= v && v <= hi,
            _ => v == self.low || v == self.high,
        }
    }
}

fn semver(v: &str) -> Option<(u64, u64, u64, bool)> {
    let v = v.strip_prefix('v')?;
    let (core, pre) = match v.split_once('-') {
        Some((c, _)) => (c, true),
        None => (v, false),
    };
    let mut it = core.split('.').map(|p| p.parse::<u64>().ok());
    let (a, b, c) = (it.next()??, it.next()??, it.next()??);
    // a prerelease sorts before its release
    Some((a, b, c, !pre))
}

pub fn parse_retractions(gomod: &str) -> Result<Vec<Retraction>> {
    let mut out = Vec::new();
    let mut in_block = false;
    for raw in gomod.lines() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let spec = if in_block {
            if line == ")" {
                in_block = false;
                continue;
            }
            line
        } else if let Some(rest) = line.strip_prefix("retract") {
            let rest = rest.trim();
            if rest == "(" {
                in_block = true;
                continue;
            }
            rest
        } else {
            continue;
        };
        let r = match spec.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            Some(range) => {
                let Some((lo, hi)) = range.split_once(',') else {
                    bail!("unreadable retract range {spec:?}");
                };
                Retraction {
                    low: lo.trim().to_string(),
                    high: hi.trim().to_string(),
                }
            }
            None => Retraction {
                low: spec.to_string(),
                high: spec.to_string(),
            },
        };
        out.push(r);
    }
    ensure!(!in_block, "unterminated retract block");
    Ok(out)
}

/// go-stellar-sdk records the definitions commit its generated code comes from in
/// `xdr/xdr_commit_generated.txt`; it is read at the commit the proxy took the version from.
pub fn go_xdr_commit(http: &Http, module: &str, version: &str) -> Result<String> {
    let info = go_info(http, module, version)?;
    let origin = info
        .origin
        .with_context(|| format!("the proxy names no origin commit for {module} {version}"))?;
    let repo = github_repo(&origin.url)?;
    let url = format!(
        "https://raw.githubusercontent.com/{repo}/{}/xdr/xdr_commit_generated.txt",
        origin.hash
    );
    commit_hash(http.get_text(&url)?.trim()).with_context(|| format!("reading {url}"))
}

// ---- npm

#[derive(Debug, Deserialize)]
pub struct NpmPackage {
    #[serde(rename = "dist-tags")]
    pub dist_tags: BTreeMap<String, String>,
    pub time: BTreeMap<String, String>,
    pub versions: BTreeMap<String, NpmVersion>,
}

#[derive(Debug, Deserialize)]
pub struct NpmVersion {
    #[serde(default)]
    pub deprecated: Option<serde_json::Value>,
    #[serde(default, rename = "gitHead")]
    pub git_head: Option<String>,
    #[serde(default)]
    pub repository: Option<serde_json::Value>,
}

pub fn npm_package(http: &Http, name: &str) -> Result<NpmPackage> {
    let url = format!("https://registry.npmjs.org/{}", name.replace('/', "%2f"));
    serde_json::from_str(&http.get_text(&url)?).with_context(|| format!("parsing {url}"))
}

pub fn npm_published(pkg: &NpmPackage, name: &str) -> Result<Published> {
    let mut releases = Vec::new();
    for (v, meta) in &pkg.versions {
        let published = pkg
            .time
            .get(v)
            .with_context(|| format!("npm lists no publish time for {name} {v}"))?;
        releases.push(Release {
            version: v.clone(),
            published: published.clone(),
            withdrawn: meta.deprecated.as_ref().map(|_| "deprecated".to_string()),
        });
    }
    let latest = pkg
        .dist_tags
        .get("latest")
        .with_context(|| format!("{name} has no latest dist-tag"))?
        .clone();
    Ok(Published { releases, latest })
}

/// js-stellar-sdk pins the definitions commit in its Makefile (`XDR_COMMIT ?= <sha>`, older
/// releases `XDR_BASE_URL_CURR=.../stellar-xdr/raw/<sha>`); it is read at the commit npm
/// recorded as the version's `gitHead`.
pub fn npm_xdr_commit(http: &Http, pkg: &NpmPackage, name: &str, version: &str) -> Result<String> {
    let meta = pkg
        .versions
        .get(version)
        .with_context(|| format!("npm has no {name} {version}"))?;
    let head = meta
        .git_head
        .as_deref()
        .with_context(|| format!("npm recorded no gitHead for {name} {version}"))?;
    let repo_url = match &meta.repository {
        Some(serde_json::Value::Object(o)) => o.get("url").and_then(|u| u.as_str()),
        Some(serde_json::Value::String(s)) => Some(s.as_str()),
        _ => None,
    }
    .with_context(|| format!("npm recorded no repository for {name} {version}"))?;
    let repo = github_repo(repo_url)?;
    let url = format!("https://raw.githubusercontent.com/{repo}/{head}/Makefile");
    makefile_xdr_commit(&http.get_text(&url)?).with_context(|| format!("reading {url}"))
}

pub fn makefile_xdr_commit(makefile: &str) -> Result<String> {
    for line in makefile.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("XDR_COMMIT") {
            let rest = rest.trim_start();
            if let Some(v) = ["?=", ":=", "="]
                .iter()
                .find_map(|op| rest.strip_prefix(op))
            {
                return commit_hash(v.trim());
            }
        }
        if let Some(rest) = line.strip_prefix("XDR_BASE_URL_CURR") {
            let rest = rest.trim_start().trim_start_matches(['?', ':', '=']).trim();
            if let Some((_, sha)) = rest.rsplit_once("/raw/") {
                return commit_hash(sha.trim());
            }
        }
    }
    bail!("no XDR_COMMIT or XDR_BASE_URL_CURR line")
}

/// `owner/repo` of a GitHub url in any of the forms registries record.
pub fn github_repo(url: &str) -> Result<String> {
    let u = url.trim();
    let u = u.strip_prefix("git+").unwrap_or(u);
    let rest = [
        "https://github.com/",
        "http://github.com/",
        "git://github.com/",
        "ssh://git@github.com/",
        "git@github.com:",
    ]
    .iter()
    .find_map(|p| u.strip_prefix(p))
    .with_context(|| format!("{url} is not a GitHub repository"))?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let parts: Vec<&str> = rest.split('/').collect();
    ensure!(
        parts.len() == 2 && parts.iter().all(|p| !p.is_empty()),
        "{url} is not a GitHub repository"
    );
    Ok(rest.to_string())
}

fn commit_hash(s: &str) -> Result<String> {
    ensure!(
        s.len() == 40
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "{s:?} is not a commit hash"
    );
    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_times_from_every_registry_compare() {
        // crates.io, npm and the go proxy write their times differently
        let crates = utc_seconds("2026-07-29T17:49:27.123456+00:00").unwrap();
        let npm = utc_seconds("2026-07-29T17:49:27.000Z").unwrap();
        let go = utc_seconds("2026-07-29T17:49:27Z").unwrap();
        assert_eq!(crates, go);
        assert_eq!(npm, go);
        assert_eq!(utc_seconds("1970-01-01T00:00:00Z").unwrap(), 0);
        assert_eq!(utc_seconds("2000-03-01T00:00:00Z").unwrap(), 951_868_800);
        assert!(utc_seconds("2026-07-29T17:49:27+03:00").is_err());
        assert!(utc_seconds("2026-07-29").is_err());
    }

    #[test]
    fn tags_peel_and_keep_only_protocol_names() {
        let text = "aaaa\trefs/tags/v20\nbbbb\trefs/tags/v28.0\ncccc\trefs/tags/v28.0^{}\n\
                    dddd\trefs/tags/curr-next\neeee\trefs/tags/v30.0\n";
        let tags = parse_xdr_tags(text).unwrap();
        let got: Vec<_> = tags
            .iter()
            .map(|t| (t.name.as_str(), t.major, t.minor, t.commit.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("v20", 20, 0, "aaaa"),
                ("v28.0", 28, 0, "cccc"),
                ("v30.0", 30, 0, "eeee")
            ]
        );
    }

    #[test]
    fn retract_directives() {
        let gomod = "module x\n\nretract v1.0.0 // broken\nretract [v1.1.0, v1.1.3]\n\
                     retract (\n  v1.2.0\n  [v1.3.0, v1.3.1] // range\n)\n";
        let r = parse_retractions(gomod).unwrap();
        assert_eq!(r.len(), 4);
        let any = |v: &str| r.iter().any(|x| x.covers(v));
        assert!(any("v1.0.0"));
        assert!(any("v1.1.2"));
        assert!(!any("v1.1.4"));
        assert!(any("v1.2.0"));
        assert!(any("v1.3.1"));
        assert!(!any("v1.4.0"));
        assert!(parse_retractions("retract (\n v1.0.0\n").is_err());
    }

    #[test]
    fn makefile_commit_forms() {
        let new = "# comment XDR_COMMIT\nXDR_REPO ?= x\nXDR_COMMIT ?= 4f524bbac80c781c06e4fb5fc93a2d0d331d6705\n";
        assert_eq!(
            makefile_xdr_commit(new).unwrap(),
            "4f524bbac80c781c06e4fb5fc93a2d0d331d6705"
        );
        let old = "XDR_BASE_URL_CURR=https://github.com/stellar/stellar-xdr/raw/9c9c145953e80990d6ff1ae3a6a973a0ce6d0694\n\
                   XDR_BASE_URL_NEXT=https://github.com/stellar/stellar-xdr/raw/0000000000000000000000000000000000000000\n";
        assert_eq!(
            makefile_xdr_commit(old).unwrap(),
            "9c9c145953e80990d6ff1ae3a6a973a0ce6d0694"
        );
        assert!(makefile_xdr_commit("all:\n\ttsc\n").is_err());
    }

    #[test]
    fn github_repo_forms() {
        for u in [
            "git+https://github.com/stellar/js-stellar-sdk.git",
            "https://github.com/stellar/js-stellar-sdk",
            "git@github.com:stellar/js-stellar-sdk.git",
        ] {
            assert_eq!(github_repo(u).unwrap(), "stellar/js-stellar-sdk");
        }
        assert!(github_repo("https://gitlab.com/a/b").is_err());
        assert_eq!(go_escape("github.com/ErenX/y"), "github.com/!eren!x/y");
    }
}
