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
            Box::new(move || {
                check::check_sha256(&net(http.get_bytes(&m.url()))?, &m.file, &m.sha256)
            }),
        ));
    }

    out
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
