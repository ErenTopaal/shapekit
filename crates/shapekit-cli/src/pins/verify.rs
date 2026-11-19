//! Runs every pin check and prints the report.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Result, bail};

use super::check::{self, Verdict};
use super::remote::{self, Http};
use super::{Pins, Toolchain};

pub struct Outcome {
    pub kind: &'static str,
    pub label: String,
    pub verdict: Verdict,
}

type Job<'a> = Box<dyn Fn() -> Verdict + Send + Sync + 'a>;

fn net<T>(r: Result<T>) -> Result<T, String> {
    r.map_err(|e| crate::error_text::chain(e.as_ref()))
}

fn jobs<'a>(pins: &'a Pins, http: &'a Http) -> Vec<(&'static str, String, Job<'a>)> {
    let mut out: Vec<(&'static str, String, Job<'a>)> = Vec::new();

    for t in &pins.toolchains {
        let label = format!("{:?} {}", t.name, t.version).to_lowercase();
        out.push((
            "toolchain",
            label,
            Box::new(move || match t.name {
                Toolchain::Rust => {
                    let url = format!("https://static.rust-lang.org/dist/{}.sha256", t.artifact);
                    check::check_rust_channel(&net(http.get_text(&url))?, &t.artifact, &t.sha256)
                }
                Toolchain::Go => {
                    let body = net(http.get_text("https://go.dev/dl/?mode=json&include=all"))?;
                    check::check_go_release(&body, &t.version, &t.artifact, &t.sha256)
                }
                Toolchain::Node => {
                    let index = net(http.get_text("https://nodejs.org/dist/index.json"))?;
                    let sums = net(http.get_text(&format!(
                        "https://nodejs.org/dist/v{}/SHASUMS256.txt",
                        t.version
                    )))?;
                    check::check_node(&index, &sums, &t.version, &t.artifact, &t.sha256)
                }
                Toolchain::Flutter => {
                    let index = net(http.get_text(
                        "https://storage.googleapis.com/flutter_infra_release/releases/releases_linux.json",
                    ))?;
                    check::check_flutter(&index, &t.version, &t.artifact, &t.sha256)
                }
                Toolchain::Php => {
                    let doc = net(http.get_text(&format!(
                        "https://www.php.net/releases/index.php?json&version={}",
                        t.version
                    )))?;
                    check::check_php(&doc, &t.version, &t.artifact, &t.sha256)
                }
                Toolchain::Composer => {
                    let sums = net(http.get_text(&format!(
                        "https://getcomposer.org/download/{}/composer.phar.sha256sum",
                        t.version
                    )))?;
                    check::check_composer(&sums, &t.version, &t.artifact, &t.sha256)
                }
                Toolchain::Dotnet => {
                    let channel = t.version.split('.').take(2).collect::<Vec<_>>().join(".");
                    let doc = net(http.get_text(&format!(
                        "https://builds.dotnet.microsoft.com/dotnet/release-metadata/{channel}/releases.json"
                    )))?;
                    let sha512 = t.sha512.as_deref().ok_or("a dotnet pin needs sha512")?;
                    check::check_dotnet(&doc, &t.version, &t.artifact, sha512)
                }
                Toolchain::Libsodium => {
                    let url = format!(
                        "https://download.libsodium.org/libsodium/releases/{}",
                        t.artifact
                    );
                    let key = t
                        .minisign_key
                        .as_deref()
                        .ok_or("a libsodium pin needs minisign_key")?;
                    let bytes = net(http.get_bytes(&url))?;
                    let sig = net(http.get_text(&format!("{url}.minisig")))?;
                    check::check_minisigned(&bytes, &sig, key, &t.artifact, &t.sha256)
                }
            }),
        ));
    }

    for i in &pins.images {
        out.push((
            "image",
            format!("{}:{}", i.repository, i.tag),
            Box::new(move || {
                let digest = net(http.docker_hub_digest(&i.repository, &i.tag))?;
                check::check_image_digest(digest.as_deref(), &i.digest)
            }),
        ));
    }

    for g in &pins.git {
        out.push((
            "git",
            g.name.clone(),
            Box::new(move || match (&g.git_ref, &g.sha) {
                (Some(r), sha) => {
                    check::check_git_ref(&net(remote::ls_remote(&g.url, r))?, r, sha.as_deref())
                }
                (None, Some(sha)) => {
                    net(remote::commit_exists(&g.url, sha))?;
                    Ok(format!("commit {} exists", &sha[..12]))
                }
                // validate() rejects this combination before any job is built
                (None, None) => Err("pin has neither ref nor sha".to_string()),
            }),
        ));
    }

    for r in &pins.releases {
        out.push((
            "release",
            format!("{} {}", r.name, r.tag),
            Box::new(move || {
                // the tag must still name the pinned commit, and the asset the pinned bytes
                let url = format!("https://github.com/{}", r.repository);
                let git_ref = format!("refs/tags/{}", r.tag);
                let tag = check::check_git_ref(
                    &net(remote::ls_remote(&url, &git_ref))?,
                    &git_ref,
                    Some(&r.commit),
                )?;
                let doc = net(
                    http.get_github_api(&format!("repos/{}/releases/tags/{}", r.repository, r.tag))
                )?;
                let asset = check::check_github_release(&doc, &r.tag, &r.asset, &r.sha256)?;
                Ok(format!("{tag}, {asset}"))
            }),
        ));
    }

    for c in &pins.crates {
        out.push((
            "crate",
            format!("{}@{}", c.name, c.version),
            Box::new(move || {
                // the sparse index is what cargo itself reads, and unlike the
                // crates.io api it has no one-request-per-second crawler limit
                let url = format!(
                    "https://index.crates.io/{}",
                    net(remote::sparse_index_path(&c.name))?
                );
                check::check_crate(&net(http.get_text(&url))?, &c.version, &c.checksum)
            }),
        ));
    }

    for n in &pins.npm {
        out.push((
            "npm",
            format!("{}@{}", n.name, n.version),
            Box::new(move || {
                let url = format!(
                    "https://registry.npmjs.org/{}/{}",
                    n.name.replace('/', "%2f"),
                    n.version
                );
                check::check_npm(&net(http.get_text(&url))?, &n.version, &n.integrity)
            }),
        ));
    }

    for m in &pins.gomod {
        out.push((
            "gomod",
            format!("{}@{}", m.module, m.version),
            Box::new(move || {
                let url = format!(
                    "https://proxy.golang.org/{}/@v/{}.info",
                    remote::goproxy_escape(&m.module),
                    remote::goproxy_escape(&m.version)
                );
                check::check_gomod(&net(http.get_text(&url))?, &m.version, &m.origin_hash)
            }),
        ));
    }

    for p in &pins.pypi {
        out.push((
            "pypi",
            format!("{}@{} {}", p.name, p.version, p.file),
            Box::new(move || {
                let url = format!("https://pypi.org/pypi/{}/{}/json", p.name, p.version);
                check::check_pypi(&net(http.get_text(&url))?, &p.version, &p.file, &p.sha256)
            }),
        ));
    }

    for m in &pins.maven {
        out.push((
            "maven",
            format!("{}:{}@{} {}", m.group, m.artifact, m.version, m.file),
            Box::new(move || check_maven(http, &m.urls(), &m.file, &m.sha256)),
        ));
    }

    out
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split('/').next().unwrap_or(rest)
}

/// Downloads a maven file from the first of `urls` that serves it and checks its sha256. A
/// source that refuses or cannot be reached passes the file to the next one; a source that
/// serves other bytes fails the pin, whichever source it is. The verdict names the source that
/// served the bytes and what every source before it answered.
fn check_maven(http: &Http, urls: &[String], file: &str, sha256: &str) -> Verdict {
    let mut refused = Vec::new();
    for url in urls {
        match http.get_bytes(url) {
            Ok(bytes) => {
                let detail = check::check_sha256(&bytes, file, sha256)
                    .map_err(|e| format!("{e}, served by {}", host(url)))?;
                let mut out = format!("{detail}, served by {}", host(url));
                if !refused.is_empty() {
                    out.push_str(&format!("; before it: {}", refused.join("; ")));
                }
                return Ok(out);
            }
            Err(e) => refused.push(crate::error_text::chain(e.as_ref())),
        }
    }
    Err(format!("no source served {file}: {}", refused.join("; ")))
}

/// Runs all checks with up to `workers` at once and returns them in file order.
pub fn verify(pins: &Pins, workers: usize) -> Result<Vec<Outcome>> {
    let http = Http::new()?;
    let jobs = jobs(pins, &http);
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Verdict>>> = Mutex::new(vec![None; jobs.len()]);

    std::thread::scope(|s| {
        for _ in 0..workers.min(jobs.len()) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((_, _, job)) = jobs.get(i) else {
                        break;
                    };
                    let verdict = job();
                    results.lock().expect("a check panicked")[i] = Some(verdict);
                }
            });
        }
    });

    let results = results.into_inner().expect("a check panicked");
    Ok(jobs
        .into_iter()
        .zip(results)
        .map(|((kind, label, _), verdict)| Outcome {
            kind,
            label,
            verdict: verdict.expect("every job ran"),
        })
        .collect())
}

pub fn format_outcome(o: &Outcome) -> String {
    match &o.verdict {
        Ok(detail) => format!("ok    {:<9} {}  {detail}", o.kind, o.label),
        Err(reason) => format!("FAIL  {:<9} {}  {reason}", o.kind, o.label),
    }
}

/// Returns Ok(true) when every pin matched.
pub fn run(path: &Path, workers: usize) -> Result<bool> {
    let pins = super::load(path)?;
    let outcomes = verify(&pins, workers)?;
    // a file that pins nothing proves nothing, which a passing run must not suggest
    if outcomes.is_empty() {
        bail!(
            "{} lists no pins, so there is nothing to verify",
            path.display()
        );
    }
    for o in &outcomes {
        println!("{}", format_outcome(o));
    }
    let failed = outcomes.iter().filter(|o| o.verdict.is_err()).count();
    println!(
        "{} pins checked: {} ok, {failed} failed",
        outcomes.len(),
        outcomes.len() - failed
    );
    Ok(failed == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// A local http server that answers each request path from `routes` (status, body) and
    /// every other path with 404, until the test ends. Returns its base url.
    fn serve(routes: Vec<(&'static str, u16, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap() == 0 || header == "\r\n" {
                        break;
                    }
                }
                let (status, body) = routes
                    .iter()
                    .find(|(p, _, _)| *p == path)
                    .map_or((404, b"not found".to_vec()), |(_, s, b)| (*s, b.clone()));
                let head = format!(
                    "HTTP/1.1 {status} X\r\nserver: fixture\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        base
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(bytes))
    }

    const JAR: &str = "/maven2/g/a/1/a-1.jar";

    #[test]
    fn maven_refused_by_central_served_by_a_mirror() {
        let bytes = b"jar bytes".to_vec();
        let primary = serve(vec![]);
        let mirror = serve(vec![(JAR, 200, bytes.clone())]);
        let urls = [format!("{primary}{JAR}"), format!("{mirror}{JAR}")];
        let http = Http::new().unwrap();
        let detail = check_maven(&http, &urls, "a-1.jar", &sha256_hex(&bytes)).unwrap();
        assert!(
            detail.starts_with(&format!(
                "a-1.jar, sha256 matches, served by {}; before it: GET {primary}{JAR} returned 404",
                host(&mirror)
            )),
            "{detail}"
        );
        // the refusal keeps the header that says who answered it
        assert!(detail.contains("server: fixture"), "{detail}");
    }

    #[test]
    fn maven_served_by_central_names_it() {
        let bytes = b"jar bytes".to_vec();
        let primary = serve(vec![(JAR, 200, bytes.clone())]);
        let urls = [
            format!("{primary}{JAR}"),
            "http://127.0.0.1:1/unused".to_string(),
        ];
        let http = Http::new().unwrap();
        let detail = check_maven(&http, &urls, "a-1.jar", &sha256_hex(&bytes)).unwrap();
        assert_eq!(
            detail,
            format!("a-1.jar, sha256 matches, served by {}", host(&primary))
        );
    }

    #[test]
    fn maven_other_bytes_fail_from_any_source() {
        let pinned = sha256_hex(b"jar bytes");
        let http = Http::new().unwrap();

        // the mirror serves other bytes after central refused
        let primary = serve(vec![]);
        let mirror = serve(vec![(JAR, 200, b"other bytes".to_vec())]);
        let urls = [format!("{primary}{JAR}"), format!("{mirror}{JAR}")];
        let err = check_maven(&http, &urls, "a-1.jar", &pinned).unwrap_err();
        assert!(
            err.starts_with("a-1.jar has sha256 ")
                && err.ends_with(&format!(", served by {}", host(&mirror))),
            "{err}"
        );

        // central serves other bytes: the mirror that has the pinned ones is not asked
        let primary = serve(vec![(JAR, 200, b"other bytes".to_vec())]);
        let mirror = serve(vec![(JAR, 200, b"jar bytes".to_vec())]);
        let urls = [format!("{primary}{JAR}"), format!("{mirror}{JAR}")];
        let err = check_maven(&http, &urls, "a-1.jar", &pinned).unwrap_err();
        assert!(
            err.ends_with(&format!(", served by {}", host(&primary))),
            "{err}"
        );
    }

    #[test]
    fn maven_refused_everywhere_fails_with_every_answer() {
        let primary = serve(vec![]);
        let mirror = serve(vec![(JAR, 403, b"blocked".to_vec())]);
        let urls = [format!("{primary}{JAR}"), format!("{mirror}{JAR}")];
        let http = Http::new().unwrap();
        let err = check_maven(&http, &urls, "a-1.jar", &sha256_hex(b"x")).unwrap_err();
        assert!(err.starts_with("no source served a-1.jar: "), "{err}");
        assert!(
            err.contains(&format!("GET {primary}{JAR} returned 404")),
            "{err}"
        );
        assert!(
            err.contains(&format!("GET {mirror}{JAR} returned 403")),
            "{err}"
        );
    }

    #[test]
    fn maven_urls_try_central_then_its_mirrors() {
        let pins = crate::pins::parse(
            r#"
schema = 1
[[maven]]
group = "org.apache.maven"
artifact = "apache-maven"
version = "3.10.1"
file = "apache-maven-3.10.1-bin.tar.gz"
sha256 = "810b8348f0a40f9116c202093fae007ebeabf67699bf34d81ee7e272289e085a"
purpose = "p"
[[maven]]
group = "org.junit.jupiter"
artifact = "junit-jupiter"
version = "6.1.3"
file = "junit-jupiter-6.1.3.jar"
sha256 = "6b0a14075c74221de53047e363860b7d19fce802bebc152df2c35c4089c6c379"
purpose = "p"
"#,
        )
        .unwrap();
        assert_eq!(
            pins.maven[0].urls(),
            [
                "https://repo1.maven.org/maven2/org/apache/maven/apache-maven/3.10.1/apache-maven-3.10.1-bin.tar.gz",
                "https://repo.maven.apache.org/maven2/org/apache/maven/apache-maven/3.10.1/apache-maven-3.10.1-bin.tar.gz",
                "https://maven-central.storage-download.googleapis.com/maven2/org/apache/maven/apache-maven/3.10.1/apache-maven-3.10.1-bin.tar.gz",
                "https://archive.apache.org/dist/maven/maven-3/3.10.1/binaries/apache-maven-3.10.1-bin.tar.gz",
            ]
        );
        assert_eq!(
            pins.maven[1].urls(),
            [
                "https://repo1.maven.org/maven2/org/junit/jupiter/junit-jupiter/6.1.3/junit-jupiter-6.1.3.jar",
                "https://repo.maven.apache.org/maven2/org/junit/jupiter/junit-jupiter/6.1.3/junit-jupiter-6.1.3.jar",
                "https://maven-central.storage-download.googleapis.com/maven2/org/junit/jupiter/junit-jupiter/6.1.3/junit-jupiter-6.1.3.jar",
            ]
        );
    }

    #[test]
    fn report_lines() {
        let ok = Outcome {
            kind: "git",
            label: "stellar-core".into(),
            verdict: Ok("refs/tags/v30.0.0 -> 0e49751aa9dd".into()),
        };
        let bad = Outcome {
            kind: "crate",
            label: "hex@0.4.1".into(),
            verdict: Err("0.4.1 is yanked".into()),
        };
        assert_eq!(
            format_outcome(&ok),
            "ok    git       stellar-core  refs/tags/v30.0.0 -> 0e49751aa9dd"
        );
        assert_eq!(
            format_outcome(&bad),
            "FAIL  crate     hex@0.4.1  0.4.1 is yanked"
        );
    }
}
