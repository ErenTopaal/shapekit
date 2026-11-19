//! `shapekit conformance sync-versions`: derives the sdk version matrix in
//! `conformance/versions.toml` and the per-version testee files under `testees/*/versions/`
//! from what crates.io, the Go module proxy, npm, PyPI and Maven Central publish.
//!
//! Which versions: for every release line (a major version; for 0.x a minor version) the
//! newest stable release that is not yanked, deprecated or retracted and whose XDR knows
//! `[sync] min_protocol` or later, plus every version `conformance/reproductions.toml` or
//! `conformance/known-failures/<sdk>.txt` names. The registry's latest stable release is
//! `current`.
//!
//! A version's XDR is the stellar/stellar-xdr commit its code was generated from: the
//! crate's `xdr-version` file, go-stellar-sdk's `xdr/xdr_commit_generated.txt`,
//! js-stellar-sdk's Makefile, each at the commit the registry took the release from, and
//! py-stellar-base's and java-stellar-sdk's Makefiles at the release tag (PyPI and Maven
//! Central record no commit). The
//! stellar-xdr release tag at that commit names a protocol, and `max_protocol` extends it to
//! the protocol before the next tag, because a protocol without a tag of its own changed no
//! XDR. A commit that is no release tag stops the command: there is then no protocol to
//! read off.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

use crate::known;
use crate::reproduce::ReproductionsFile;
use crate::upstream::{self, Http, Published, XdrTag, utc_seconds};
use crate::versions::{MemoryResource, Sdk, VersionsFile};

/// The sdks the command knows how to discover and build, in file order.
pub const SDKS: [&str; 11] = [
    "rust",
    "go",
    "js",
    "soroban",
    "python",
    "java",
    "dart",
    "php",
    "dotnet",
    "kmp",
    "stellar-cli",
];

/// The CPython every python testee venv is made with; pins.toml pins its build.
pub const PYTHON: &str = "3.15.2";

/// The file of py-stellar-base that declares a release's version, checked at the release tag.
const PYTHON_VERSION_FILE: &str = "stellar_sdk/__version__.py";

/// The file of java-stellar-sdk that declares a release's version, checked at the release tag.
const JAVA_VERSION_FILE: &str = "build.gradle.kts";

/// How many times a java or kmp testee pom names its sdk version: the dependency and the build
/// directory.
const JAVA_POM_VERSIONS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Semver {
    major: u64,
    minor: u64,
    patch: u64,
}

/// A stable `MAJOR.MINOR.PATCH`, with the `v` Go writes; anything else (prereleases, build
/// metadata) is not a stable release.
fn stable(v: &str) -> Option<Semver> {
    let core = v.strip_prefix('v').unwrap_or(v);
    let mut it = core.split('.');
    let mut part = || -> Option<u64> {
        let p = it.next()?;
        (!p.is_empty()
            && p.bytes().all(|b| b.is_ascii_digit())
            && (p == "0" || !p.starts_with('0')))
        .then(|| p.parse().ok())
        .flatten()
    };
    let s = Semver {
        major: part()?,
        minor: part()?,
        patch: part()?,
    };
    it.next().is_none().then_some(s)
}

/// A stable `MAJOR.MINOR.PATCH` as a sortable triple, for registries that name no latest
/// version of their own.
pub fn stable_triple(v: &str) -> Option<(u64, u64, u64)> {
    stable(v).map(|s| (s.major, s.minor, s.patch))
}

/// The release line a version belongs to and its label: `17.x`, `v0.7.x`.
fn line(v: &str, s: Semver) -> ((u64, u64), String) {
    let prefix = if v.starts_with('v') { "v" } else { "" };
    if s.major == 0 {
        ((0, s.minor), format!("{prefix}0.{}.x", s.minor))
    } else {
        ((s.major, 0), format!("{prefix}{}.x", s.major))
    }
}

/// The XDR a version was generated from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xdr {
    pub tag: String,
    pub commit: String,
    pub max_protocol: u32,
}

/// The newest protocol a release tag's definitions describe: its own, extended up to the one
/// before the next tag.
fn tag_max_protocol(tags: &[XdrTag], major: u32) -> u32 {
    tags.iter()
        .map(|t| t.major)
        .filter(|&m| m > major)
        .min()
        .map_or(major, |next| next - 1)
}

pub fn xdr_of_commit(tags: &[XdrTag], commit: &str) -> Result<Xdr> {
    let Some(tag) = tags
        .iter()
        .filter(|t| t.commit == commit)
        .max_by_key(|t| (t.major, t.minor))
    else {
        bail!("stellar-xdr commit {commit} is not at any release tag");
    };
    Ok(Xdr {
        tag: tag.name.clone(),
        commit: commit.to_string(),
        max_protocol: tag_max_protocol(tags, tag.major),
    })
}

/// The XDR of a commit an sdk pins: a release tag, or a `curr` branch commit generated from one
/// (see [`upstream::xdr_curr_source`]), which then carries the tag's protocols.
pub fn xdr_of_pinned(http: &Http, tags: &[XdrTag], commit: &str) -> Result<Xdr> {
    if tags.iter().any(|t| t.commit == commit) {
        return xdr_of_commit(tags, commit);
    }
    let source = upstream::xdr_curr_source(http, tags, commit)?;
    let mut x = xdr_of_commit(tags, &source)?;
    x.tag = format!("curr from {}", x.tag);
    x.commit = commit.to_string();
    Ok(x)
}

/// The oldest tag whose definitions know `min_protocol`: no release published before its
/// commit can carry it.
pub fn floor_tag(tags: &[XdrTag], min_protocol: u32) -> Result<&XdrTag> {
    tags.iter()
        .find(|t| tag_max_protocol(tags, t.major) >= min_protocol)
        .with_context(|| format!("no stellar-xdr release tag knows protocol {min_protocol}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    pub version: String,
    pub reasons: Vec<String>,
    pub xdr: Xdr,
}

/// Picks an sdk's versions. `pinned` maps each version the repository's own files name to
/// why; `floor` is when the floor tag's commit was made; `xdr_of` looks a version's XDR up.
pub fn choose(
    published: &Published,
    pinned: &BTreeMap<String, Vec<String>>,
    floor: i64,
    min_protocol: u32,
    xdr_of: &mut dyn FnMut(&str) -> Result<Xdr>,
) -> Result<Vec<Chosen>> {
    let mut newest: BTreeMap<(u64, u64), (Semver, &upstream::Release, String)> = BTreeMap::new();
    for r in &published.releases {
        let Some(s) = stable(&r.version) else {
            continue;
        };
        if r.withdrawn.is_some() {
            continue;
        }
        let (key, label) = line(&r.version, s);
        match newest.get(&key) {
            Some((have, _, _)) if *have >= s => {}
            _ => {
                newest.insert(key, (s, r, label));
            }
        }
    }

    let mut out: BTreeMap<Semver, Chosen> = BTreeMap::new();
    for (s, r, label) in newest.values() {
        let published_at =
            utc_seconds(&r.published).with_context(|| format!("publish time of {}", r.version))?;
        if published_at < floor {
            continue;
        }
        let xdr = xdr_of(&r.version)?;
        if xdr.max_protocol >= min_protocol {
            out.insert(
                *s,
                Chosen {
                    version: r.version.clone(),
                    reasons: vec![format!("newest {label} release")],
                    xdr,
                },
            );
        }
    }

    for (v, why) in pinned {
        ensure!(
            published.releases.iter().any(|r| &r.version == v),
            "{v} is named ({}) but not published",
            why.join("; ")
        );
        let s = stable(v).with_context(|| format!("{v} is named but is not a stable release"))?;
        match out.get_mut(&s) {
            Some(c) => c.reasons.extend(why.iter().cloned()),
            None => {
                out.insert(
                    s,
                    Chosen {
                        version: v.clone(),
                        reasons: why.clone(),
                        xdr: xdr_of(v)?,
                    },
                );
            }
        }
    }

    ensure!(
        out.values().any(|c| c.version == published.latest),
        "the latest release {} does not know protocol {min_protocol}",
        published.latest
    );
    Ok(out.into_values().collect())
}

/// The versions the repository's own files name for each sdk, with why.
pub fn pinned_versions(
    root: &Path,
    reproductions: &Path,
    known_dir: &Path,
) -> Result<BTreeMap<String, BTreeMap<String, Vec<String>>>> {
    let mut out: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let repro = ReproductionsFile::load(&root.join(reproductions))?;
    for r in &repro.reproductions {
        for (list, verb) in [(&r.fail, "fails"), (&r.pass, "passes")] {
            for v in list {
                out.entry(r.sdk.clone())
                    .or_default()
                    .entry(v.clone())
                    .or_default()
                    .push(format!("{verb} reproduction {}", r.id));
            }
        }
    }
    for sdk in SDKS {
        let lines = known::load(&root.join(known_dir), sdk)?;
        let named: BTreeSet<&str> = lines.iter().filter_map(|l| l.version.as_deref()).collect();
        for v in named {
            out.entry(sdk.to_string())
                .or_default()
                .entry(v.to_string())
                .or_default()
                .push(format!("named in known-failures/{sdk}.txt"));
        }
    }
    Ok(out)
}

/// Everything one sync derives for one sdk.
#[derive(Debug, Clone)]
pub struct SdkPlan {
    pub sdk: String,
    pub current: String,
    pub chosen: Vec<Chosen>,
}

pub struct SyncOptions {
    pub root: PathBuf,
    pub versions: PathBuf,
    pub reproductions: PathBuf,
    pub known_dir: PathBuf,
}

/// Asks the registries and derives every sdk's versions.
pub fn discover(opts: &SyncOptions) -> Result<(VersionsFile, Vec<SdkPlan>)> {
    let file = VersionsFile::load(&opts.root.join(&opts.versions))?;
    let policy = file
        .sync
        .as_ref()
        .with_context(|| format!("{} has no [sync] table", opts.versions.display()))?;
    let names: BTreeSet<&str> = file.sdk.keys().map(String::as_str).collect();
    ensure!(
        names == SDKS.iter().copied().collect(),
        "sync-versions knows the sdks {}; {} lists {}",
        SDKS.join(", "),
        opts.versions.display(),
        names.into_iter().collect::<Vec<_>>().join(", ")
    );
    let pinned = pinned_versions(&opts.root, &opts.reproductions, &opts.known_dir)?;
    let http = Http::new()?;
    let tags = upstream::xdr_tags()?;
    let floor = floor_tag(&tags, policy.min_protocol)?;
    let floor_time = upstream::xdr_commit_time(&http, &floor.commit)
        .with_context(|| format!("dating stellar-xdr {}", floor.name))?;

    let mut plans = Vec::new();
    for sdk in SDKS {
        let s = file.sdk(sdk)?;
        let empty = BTreeMap::new();
        let pins = pinned.get(sdk).unwrap_or(&empty);
        let mut cache: BTreeMap<String, Xdr> = BTreeMap::new();
        let (published, chosen) = match sdk {
            "rust" => {
                let p = upstream::crates_published(&http, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        xdr_of_commit(&tags, &upstream::crate_xdr_commit(&http, &s.package, v)?)
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            "go" => {
                let p = upstream::go_published(&http, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        xdr_of_commit(&tags, &upstream::go_xdr_commit(&http, &s.package, v)?)
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            "js" => {
                let pkg = upstream::npm_package(&http, &s.package)?;
                let p = upstream::npm_published(&pkg, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        xdr_of_commit(
                            &tags,
                            &upstream::npm_xdr_commit(&http, &pkg, &s.package, v)?,
                        )
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            "soroban" => {
                let p = upstream::crates_published(&http, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        let xdr = upstream::soroban_sdk_xdr_crate(&http, &s.package, v)?;
                        xdr_of_commit(
                            &tags,
                            &upstream::crate_xdr_commit(&http, "stellar-xdr", &xdr)?,
                        )
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            "python" => {
                let p = upstream::pypi_published(&http, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        let repo = upstream::pypi_repository(&http, &s.package, v)?;
                        xdr_of_commit(
                            &tags,
                            &upstream::tagged_xdr_commit(&http, &repo, v, PYTHON_VERSION_FILE)?,
                        )
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            "java" => {
                let p = upstream::maven_published(&http, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        let repo = upstream::maven_repository(&http, &s.package, v)?;
                        xdr_of_commit(
                            &tags,
                            &upstream::tagged_xdr_commit(&http, &repo, v, JAVA_VERSION_FILE)?,
                        )
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            "dart" => {
                let p = upstream::pub_published(&http, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        xdr_of_pinned(
                            &http,
                            &tags,
                            &upstream::pub_xdr_commit(&http, &s.package, v)?,
                        )
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            "php" => {
                let p = upstream::packagist_published(&http, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        xdr_of_pinned(
                            &http,
                            &tags,
                            &upstream::packagist_xdr_commit(&http, &s.package, v)?,
                        )
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            "dotnet" => {
                let p = upstream::nuget_published(&http, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        xdr_of_pinned(
                            &http,
                            &tags,
                            &upstream::nuget_xdr_commit(&http, &tags, &s.package, v)?,
                        )
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            "kmp" => {
                let p = upstream::maven_published(&http, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        let repo = upstream::maven_repository(&http, &s.package, v)?;
                        xdr_of_pinned(&http, &tags, &upstream::kmp_xdr_commit(&http, &repo, v)?)
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            "stellar-cli" => {
                let p = upstream::crates_published(&http, &s.package)?;
                let mut f = |v: &str| -> Result<Xdr> {
                    lookup(&mut cache, v, || {
                        let xdr = upstream::crate_locked_dep(&http, &s.package, v, "stellar-xdr")?;
                        xdr_of_commit(
                            &tags,
                            &upstream::crate_xdr_commit(&http, "stellar-xdr", &xdr)?,
                        )
                    })
                };
                let c = choose(&p, pins, floor_time, policy.min_protocol, &mut f)?;
                (p, c)
            }
            _ => unreachable!("checked against SDKS above"),
        };
        plans.push(SdkPlan {
            sdk: sdk.to_string(),
            current: published.latest.clone(),
            chosen,
        });
    }
    Ok((file, plans))
}

fn lookup(
    cache: &mut BTreeMap<String, Xdr>,
    v: &str,
    f: impl FnOnce() -> Result<Xdr>,
) -> Result<Xdr> {
    if let Some(x) = cache.get(v) {
        return Ok(x.clone());
    }
    let x = f().with_context(|| format!("reading the xdr of {v}"))?;
    cache.insert(v.to_string(), x.clone());
    Ok(x)
}

const HEADER: &str = "\
# the sdk version matrix of the conformance runner and how each testee is built and started.
# `shapekit conformance sync-versions` writes this file and testees/*/versions/ from what
# crates.io, the go module proxy, npm, pypi, maven central, pub.dev, packagist and nuget
# publish; `sync-versions --check` reports whether they are up to date. change the per-sdk settings or [sync] here and run it again; the
# version lists are not edited by hand. paths are relative to the repository root;
# `shapekit conformance run` runs the build steps in order, then starts `command` and checks
# that its hello reports exactly `package` and `version`.
#
# which versions: for every release line (a major version; for 0.x a minor version) the
# newest stable release that is not yanked, deprecated or retracted and whose xdr knows
# [sync] min_protocol or later, plus every version that conformance/reproductions.toml or
# conformance/known-failures/<sdk>.txt names. the comment above each version says which of
# these brought it in.
#
# per sdk:
#   package          what hello must report as sdk
#   current          the registry's latest stable release: the version a plain
#                    `conformance run` uses, and the version that casts the sdk's one vote when
#                    implementation-defined checks are decided across sdks (a run without it
#                    lets the sdk's last version in the run vote instead). the wallet-backend
#                    adapter and the linter use their own go-stellar-sdk pin in pins.toml
#   issues_repo      where draft-issues points the issues
#   kind             \"cli\" for a command line tool, so drafted issues call it the CLI; an sdk
#                    otherwise
#   timeout_ms       per-request timeout
#   memory           rlimit set in the testee between fork and exec. \"as\" caps the address
#                    space (rust, go reserve little up front); \"data\" caps private writable
#                    memory, used for node, whose v8 reserves a large PROT_NONE cage that an
#                    address-space cap would refuse before any data is touched
#
# per version, max_protocol is the newest protocol whose xdr that version knows; a failure on
# an entry generated at a later protocol is reported as unsupported rather than as a
# divergence. it comes from the stellar/stellar-xdr commit the version's code was generated
# from (the crate's xdr-version file, go-stellar-sdk's xdr/xdr_commit_generated.txt,
# js-stellar-sdk's Makefile at the commit npm records as gitHead, py-stellar-base's and
# java-stellar-sdk's Makefiles at the release tag, since pypi and maven central record no commit,
# for soroban-sdk the xdr-version file of the stellar-xdr release that its soroban-env-host's
# soroban-env-common pins with `=`, for stellar_flutter_sdk the Makefile in the archive pub.dev
# serves, for soneso/stellar-php-sdk the Makefile at the commit packagist records as the source,
# and for stellar-dotnet-sdk, which names no commit, the stellar-xdr commit whose .x files have
# the same git blob ids as the ones its nuspec's commit ships). a commit on stellar-xdr's curr
# branch whose message says it was generated from a release tag's commit carries that tag. the
# release tag at that commit names a protocol, extended up to the protocol before the next tag,
# because a protocol without a tag of its own changed no xdr (v28.0 covers protocol 29).
#
# rust: one source (testees/rust/src) with one thin cargo manifest and lock file per version
# under testees/rust/versions/<version>/, each built into its own target directory.
# soroban: the same layout as rust (testees/soroban/src, testees/soroban/versions/<version>/),
# linking soroban-sdk with its testutils feature, which runs the host in the testee process.
# go: one source (testees/go) built per version with an alternate module file
# testees/go/versions/<version>.mod (and its .sum) through go's -modfile flag, outside the
# go.work workspace, which -modfile cannot be combined with.
# js: one testee, compiled in a copy of testees/js under target/testees/js/build so a run
# never touches the source tree, is copied next to a per-version install of the
# sdk under target/testees/js/<version>/, so node resolves @stellar/stellar-sdk to that
# version. each install is `npm ci` from the package.json and lock file committed under
# testees/js/versions/<version>/, so the sdk's own dependencies (js-xdr above all) are pinned
# too.
# python: one source (testees/python) run by the python of a per-version venv under
# target/testees/python/<version>/venv, made with uv from the hash-locked
# testees/python/versions/<version>/requirements.txt, so the sdk and every dependency it
# imports (xdrlib3, pynacl) are pinned.
# java: one source (testees/java/src) built by maven per version from
# testees/java/versions/<version>/pom.xml into target/testees/java/<version>/, whose lib/ jars
# are checked against testees/java/versions/<version>/lib.sha256 by scripts/check-jars.sh (every
# hash, and no jar the file does not list), so the resolved dependency tree (gson above all) is
# pinned, not only the sdk version.
# dart: the sources in testees/dart are copied next to the pubspec and lock file committed under
# testees/dart/versions/<version>/ into target/testees/dart/<version>/, resolved with `flutter
# pub get --enforce-lockfile` (stellar_flutter_sdk declares a flutter dependency, so the flutter
# sdk resolves it; the testee itself runs on the dart vm), given its type table by
# tool/gentypes.dart from the schema and the installed sdk, and compiled with `dart compile exe`.
# php: the sources in testees/php are copied next to the composer files committed under
# testees/php/versions/<version>/ into target/testees/php/<version>/, installed with `composer
# install` from the lock file, and given their type table by tools/gentypes.php.
# dotnet: one source (testees/dotnet/Program.cs) with one project file and nuget lock file per
# version under testees/dotnet/versions/<version>/, built with `dotnet build` in locked mode.
";

fn quote(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

fn array(items: &[String]) -> String {
    format!(
        "[{}]",
        items
            .iter()
            .map(|s| quote(s))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// The build steps and command of one version, as the file writes them.
fn build_lines(sdk: &str, v: &str) -> (Vec<String>, Vec<String>) {
    let s = |x: &str| x.to_string();
    match sdk {
        "rust" | "soroban" => {
            let manifest = format!("testees/{sdk}/versions/{v}/Cargo.toml");
            let target = format!("target/testees/{sdk}/{v}");
            (
                vec![format!(
                    "  {{ argv = {} }},",
                    array(&[
                        s("cargo"),
                        s("build"),
                        s("--release"),
                        s("--locked"),
                        s("--manifest-path"),
                        manifest,
                        s("--target-dir"),
                        target.clone(),
                    ])
                )],
                vec![format!("{target}/release/shapekit-testee-{sdk}")],
            )
        }
        "go" => (
            vec![format!(
                "  {{ argv = {}, dir = \"testees/go\", env = {{ GOWORK = \"off\" }} }},",
                array(&[
                    s("go"),
                    s("build"),
                    s("-tags"),
                    crate::versions::go_build_tag(v),
                    s("-modfile"),
                    format!("versions/{v}.mod"),
                    s("-o"),
                    format!("../../target/testees/go/{v}/testee"),
                    s("."),
                ])
            )],
            vec![format!("target/testees/go/{v}/testee")],
        ),
        "js" => {
            let t = format!("target/testees/js/{v}");
            let src = format!("testees/js/versions/{v}");
            (
                vec![
                    // the testee compiles in a copy under target/, so a run never rewrites
                    // testees/js/node_modules or dist under anything else using the checkout
                    format!(
                        "  {{ argv = {} }},",
                        array(&[s("rm"), s("-rf"), s(JS_BUILD)])
                    ),
                    format!(
                        "  {{ argv = {} }},",
                        array(&[s("mkdir"), s("-p"), s(JS_BUILD)])
                    ),
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("cp"),
                            s("testees/js/package.json"),
                            s("testees/js/package-lock.json"),
                            s("testees/js/tsconfig.json"),
                            s("testees/js/.npmrc"),
                            format!("{JS_BUILD}/"),
                        ])
                    ),
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("cp"),
                            s("-R"),
                            s("testees/js/src"),
                            format!("{JS_BUILD}/src")
                        ])
                    ),
                    format!(
                        "  {{ argv = {}, dir = {} }},",
                        array(&[s("npm"), s("ci"), s("--no-audit"), s("--no-fund")]),
                        quote(JS_BUILD)
                    ),
                    format!(
                        "  {{ argv = {}, dir = {} }},",
                        array(&[s("npm"), s("run"), s("build")]),
                        quote(JS_BUILD)
                    ),
                    format!("  {{ argv = {} }},", array(&[s("rm"), s("-rf"), t.clone()])),
                    format!(
                        "  {{ argv = {} }},",
                        array(&[s("mkdir"), s("-p"), t.clone()])
                    ),
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("cp"),
                            format!("{src}/package.json"),
                            format!("{src}/package-lock.json"),
                            format!("{t}/"),
                        ])
                    ),
                    format!(
                        "  {{ argv = {}, dir = {} }},",
                        array(&[
                            s("npm"),
                            s("ci"),
                            s("--no-audit"),
                            s("--no-fund"),
                            s("--ignore-scripts")
                        ]),
                        quote(&t)
                    ),
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("cp"),
                            s("-R"),
                            format!("{JS_BUILD}/dist"),
                            format!("{t}/dist")
                        ])
                    ),
                ],
                vec![s("node"), format!("{t}/dist/main.js")],
            )
        }
        "python" => {
            let venv = format!("target/testees/python/{v}/venv");
            let python = format!("{venv}/bin/python");
            (
                vec![
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("uv"),
                            s("venv"),
                            s("--quiet"),
                            s("--clear"),
                            s("--no-python-downloads"),
                            s("--python"),
                            s(PYTHON),
                            venv.clone(),
                        ])
                    ),
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("uv"),
                            s("pip"),
                            s("sync"),
                            s("--quiet"),
                            s("--require-hashes"),
                            s("--python"),
                            python.clone(),
                            format!("testees/python/versions/{v}/requirements.txt"),
                        ])
                    ),
                ],
                // -B keeps bytecode out of the source tree, -E and -s keep PYTHON* variables and
                // the user site directory from changing what is imported
                vec![
                    python,
                    s("-B"),
                    s("-E"),
                    s("-s"),
                    s("testees/python/main.py"),
                ],
            )
        }
        "java" => {
            let t = format!("target/testees/java/{v}");
            (
                vec![
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("mvn"),
                            s("-B"),
                            s("-q"),
                            s("-f"),
                            format!("testees/java/versions/{v}/pom.xml"),
                            s("-DskipTests"),
                            s("package"),
                        ])
                    ),
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("scripts/check-jars.sh"),
                            format!("{t}/lib"),
                            format!("testees/java/versions/{v}/lib.sha256"),
                        ])
                    ),
                ],
                vec![s("java"), s("-jar"), format!("{t}/testee.jar")],
            )
        }
        "dart" => {
            let t = format!("target/testees/dart/{v}");
            let src = format!("testees/dart/versions/{v}");
            let step = |argv: &[String], dir: Option<&str>| match dir {
                Some(d) => format!("  {{ argv = {}, dir = {} }},", array(argv), quote(d)),
                None => format!("  {{ argv = {} }},", array(argv)),
            };
            (
                vec![
                    step(&[s("rm"), s("-rf"), t.clone()], None),
                    step(&[s("mkdir"), s("-p"), t.clone()], None),
                    step(
                        &[
                            s("cp"),
                            s("-R"),
                            s("testees/dart/bin"),
                            s("testees/dart/lib"),
                            s("testees/dart/test"),
                            s("testees/dart/tool"),
                            format!("{src}/pubspec.yaml"),
                            format!("{src}/pubspec.lock"),
                            format!("{t}/"),
                        ],
                        None,
                    ),
                    step(
                        &[s("flutter"), s("pub"), s("get"), s("--enforce-lockfile")],
                        Some(&t),
                    ),
                    step(
                        &[
                            s("dart"),
                            s("run"),
                            s("tool/gentypes.dart"),
                            format!("../../../../schema/xdr/{DART_SCHEMA}/xdr.json"),
                            s("lib/tables_gen.dart"),
                        ],
                        Some(&t),
                    ),
                    step(
                        &[
                            s("dart"),
                            s("compile"),
                            s("exe"),
                            s("bin/main.dart"),
                            s("-o"),
                            s("testee"),
                        ],
                        Some(&t),
                    ),
                ],
                vec![format!("{t}/testee")],
            )
        }
        "php" => {
            let t = format!("target/testees/php/{v}");
            let src = format!("testees/php/versions/{v}");
            let step = |argv: &[String], dir: Option<&str>| match dir {
                Some(d) => format!("  {{ argv = {}, dir = {} }},", array(argv), quote(d)),
                None => format!("  {{ argv = {} }},", array(argv)),
            };
            (
                vec![
                    step(&[s("rm"), s("-rf"), t.clone()], None),
                    step(&[s("mkdir"), s("-p"), t.clone()], None),
                    step(
                        &[
                            s("cp"),
                            s("-R"),
                            s("testees/php/bin"),
                            s("testees/php/src"),
                            s("testees/php/tests"),
                            s("testees/php/tools"),
                            format!("{src}/composer.json"),
                            format!("{src}/composer.lock"),
                            format!("{t}/"),
                        ],
                        None,
                    ),
                    step(
                        &[
                            s("composer"),
                            s("install"),
                            s("--no-interaction"),
                            s("--no-progress"),
                            s("--no-dev"),
                        ],
                        Some(&t),
                    ),
                    step(
                        &[
                            s("php"),
                            s("tools/gentypes.php"),
                            format!("../../../../schema/xdr/{DART_SCHEMA}/xdr.json"),
                            s("src/TablesGen.php"),
                        ],
                        Some(&t),
                    ),
                ],
                vec![s("php"), format!("{t}/bin/testee.php")],
            )
        }
        "dotnet" => {
            let t = format!("target/testees/dotnet/{v}");
            (
                vec![format!(
                    "  {{ argv = {} }},",
                    array(&[
                        s("dotnet"),
                        s("build"),
                        format!("testees/dotnet/versions/{v}/ShapekitTestee.csproj"),
                        s("-c"),
                        s("Release"),
                        s("-p:RestoreLockedMode=true"),
                        s("-o"),
                        t.clone(),
                    ])
                )],
                vec![s("dotnet"), format!("{t}/ShapekitTestee.dll")],
            )
        }
        "kmp" => {
            let t = format!("target/testees/kmp/{v}");
            (
                vec![
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("mvn"),
                            s("-B"),
                            s("-q"),
                            s("-f"),
                            format!("testees/kmp/versions/{v}/pom.xml"),
                            s("-DskipTests"),
                            s("package"),
                        ])
                    ),
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("scripts/check-jars.sh"),
                            format!("{t}/lib"),
                            format!("testees/kmp/versions/{v}/lib.sha256"),
                        ])
                    ),
                ],
                vec![s("java"), s("-jar"), format!("{t}/testee.jar")],
            )
        }
        // one testee build runs every version's release binary, which fetch.sh installs after
        // checking it against the version's asset.sha256
        "stellar-cli" => {
            let testee = s("target/testees/stellar-cli/testee");
            (
                vec![
                    format!(
                        "  {{ argv = {} }},",
                        array(&[
                            s("cargo"),
                            s("build"),
                            s("--release"),
                            s("--locked"),
                            s("--manifest-path"),
                            s("testees/stellar-cli/Cargo.toml"),
                            s("--target-dir"),
                            testee.clone(),
                        ])
                    ),
                    format!(
                        "  {{ argv = {} }},",
                        array(&[s("testees/stellar-cli/fetch.sh"), s(v)])
                    ),
                ],
                vec![
                    format!("{testee}/release/shapekit-testee-stellar-cli"),
                    format!("target/testees/stellar-cli/{v}/stellar"),
                ],
            )
        }
        _ => unreachable!("only the sdks in SDKS are rendered"),
    }
}

/// Where the js testee compiles, a copy of `testees/js` outside the source tree.
const JS_BUILD: &str = "target/testees/js/build";

fn render_sdk(out: &mut String, name: &str, sdk: &Sdk, plan: &SdkPlan) {
    let _ = writeln!(out, "[sdk.{name}]");
    let _ = writeln!(out, "package = {}", quote(&sdk.package));
    let _ = writeln!(out, "current = {}", quote(&plan.current));
    let _ = writeln!(out, "issues_repo = {}", quote(&sdk.issues_repo));
    if !sdk.issues_repo_by_op.is_empty() {
        let pairs: Vec<String> = sdk
            .issues_repo_by_op
            .iter()
            .map(|(k, v)| format!("{k} = {}", quote(v)))
            .collect();
        let _ = writeln!(out, "issues_repo_by_op = {{ {} }}", pairs.join(", "));
    }
    let _ = writeln!(out, "timeout_ms = {}", sdk.timeout_ms);
    let resource = match sdk.memory.resource {
        MemoryResource::As => "as",
        MemoryResource::Data => "data",
    };
    let _ = writeln!(
        out,
        "memory = {{ resource = \"{resource}\", mib = {} }}",
        sdk.memory.mib
    );
    if sdk.kind == crate::versions::PackageKind::Cli {
        let _ = writeln!(out, "kind = \"cli\"");
    }
    for c in &plan.chosen {
        let (steps, command) = build_lines(name, &c.version);
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "# {}: {}; xdr {} ({}), protocols up to {}",
            c.version,
            c.reasons.join("; "),
            c.xdr.tag,
            &c.xdr.commit[..8],
            c.xdr.max_protocol
        );
        let _ = writeln!(out, "[[sdk.{name}.versions]]");
        let _ = writeln!(out, "version = {}", quote(&c.version));
        let _ = writeln!(out, "build = [");
        for s in steps {
            let _ = writeln!(out, "{s}");
        }
        let _ = writeln!(out, "]");
        let _ = writeln!(out, "command = {}", array(&command));
        let _ = writeln!(out, "max_protocol = {}", c.xdr.max_protocol);
    }
}

/// The whole versions.toml for these plans; per-sdk settings come from `file`.
pub fn render(file: &VersionsFile, plans: &[SdkPlan]) -> Result<String> {
    let policy = file.sync.as_ref().context("no [sync] table")?;
    let mut out = String::from(HEADER);
    let _ = writeln!(out);
    let _ = writeln!(out, "schema = {}", file.schema);
    let _ = writeln!(out);
    let _ = writeln!(out, "[sync]");
    let _ = writeln!(out, "min_protocol = {}", policy.min_protocol);
    for plan in plans {
        let _ = writeln!(out);
        render_sdk(&mut out, &plan.sdk, file.sdk(&plan.sdk)?, plan);
    }
    Ok(out)
}

/// The files a version's testee needs, relative to the root.
fn testee_files(sdk: &str, v: &str) -> Vec<String> {
    match sdk {
        "rust" | "soroban" => vec![
            format!("testees/{sdk}/versions/{v}/Cargo.toml"),
            format!("testees/{sdk}/versions/{v}/Cargo.lock"),
        ],
        "go" => vec![
            format!("testees/go/versions/{v}.mod"),
            format!("testees/go/versions/{v}.sum"),
        ],
        "js" => vec![
            format!("testees/js/versions/{v}/package.json"),
            format!("testees/js/versions/{v}/package-lock.json"),
        ],
        "python" => vec![
            format!("testees/python/versions/{v}/requirements.in"),
            format!("testees/python/versions/{v}/requirements.txt"),
        ],
        "java" => vec![
            format!("testees/java/versions/{v}/pom.xml"),
            format!("testees/java/versions/{v}/lib.sha256"),
        ],
        "dart" => vec![
            format!("testees/dart/versions/{v}/pubspec.yaml"),
            format!("testees/dart/versions/{v}/pubspec.lock"),
        ],
        "php" => vec![
            format!("testees/php/versions/{v}/composer.json"),
            format!("testees/php/versions/{v}/composer.lock"),
        ],
        "dotnet" => vec![
            format!("testees/dotnet/versions/{v}/ShapekitTestee.csproj"),
            format!("testees/dotnet/versions/{v}/packages.lock.json"),
        ],
        "kmp" => vec![
            format!("testees/kmp/versions/{v}/pom.xml"),
            format!("testees/kmp/versions/{v}/lib.sha256"),
        ],
        "stellar-cli" => vec![format!("testees/stellar-cli/versions/{v}/asset.sha256")],
        _ => unreachable!("only the sdks in SDKS have testees"),
    }
}

/// The schema whose types the dart and php testees' builds write their type tables from.
const DART_SCHEMA: &str = "v30.0";

/// The versions that have testee files on disk.
fn testee_versions(root: &Path, sdk: &str) -> Result<BTreeSet<String>> {
    let dir = root.join(format!("testees/{sdk}/versions"));
    let mut out = BTreeSet::new();
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    for e in rd {
        let e = e.with_context(|| format!("reading {}", dir.display()))?;
        let name = e.file_name().to_string_lossy().into_owned();
        let v = match sdk {
            "go" => match name
                .strip_suffix(".mod")
                .or_else(|| name.strip_suffix(".sum"))
            {
                Some(v) => v.to_string(),
                None => bail!("unexpected file {}", e.path().display()),
            },
            _ => name,
        };
        out.insert(v);
    }
    Ok(out)
}

/// What differs between the derived matrix and the repository.
#[derive(Debug, Default)]
pub struct Drift {
    pub lines: Vec<String>,
    pub file_changed: bool,
}

impl Drift {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && !self.file_changed
    }
}

pub fn drift(
    root: &Path,
    versions: &Path,
    old: &VersionsFile,
    plans: &[SdkPlan],
    rendered: &str,
) -> Result<Drift> {
    let mut d = Drift::default();
    let path = root.join(versions);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    d.file_changed = text != rendered;
    for plan in plans {
        let sdk = old.sdk(&plan.sdk)?;
        let had: BTreeMap<&str, Option<u32>> = sdk
            .versions
            .iter()
            .map(|v| (v.version.as_str(), v.max_protocol))
            .collect();
        let want: BTreeSet<&str> = plan.chosen.iter().map(|c| c.version.as_str()).collect();
        for c in &plan.chosen {
            match had.get(c.version.as_str()) {
                None => d.lines.push(format!(
                    "{} {}: new ({}; protocols up to {})",
                    plan.sdk,
                    c.version,
                    c.reasons.join("; "),
                    c.xdr.max_protocol
                )),
                Some(mp) if *mp != Some(c.xdr.max_protocol) => d.lines.push(format!(
                    "{} {}: max_protocol {} -> {}",
                    plan.sdk,
                    c.version,
                    mp.map_or("none".to_string(), |m| m.to_string()),
                    c.xdr.max_protocol
                )),
                Some(_) => {}
            }
        }
        for v in had.keys() {
            if !want.contains(v) {
                d.lines
                    .push(format!("{} {v}: no longer selected", plan.sdk));
            }
        }
        if sdk.current != plan.current {
            d.lines.push(format!(
                "{}: current {} -> {}",
                plan.sdk, sdk.current, plan.current
            ));
        }
        let on_disk = testee_versions(root, &plan.sdk)?;
        for c in &plan.chosen {
            for f in testee_files(&plan.sdk, &c.version) {
                if !root.join(&f).is_file() {
                    d.lines
                        .push(format!("{} {}: {f} is missing", plan.sdk, c.version));
                }
            }
        }
        for v in &on_disk {
            if !want.contains(v.as_str()) {
                d.lines.push(format!(
                    "{} {v}: testee files for an unselected version",
                    plan.sdk
                ));
            }
        }
    }
    Ok(d)
}

fn run_step(argv: &[&str], dir: &Path, env: &[(&str, &str)]) -> Result<()> {
    let status = Command::new(argv[0])
        .args(&argv[1..])
        .current_dir(dir)
        .envs(env.iter().copied())
        .stdout(std::io::stderr())
        .status()
        .with_context(|| format!("starting {}", argv.join(" ")))?;
    ensure!(
        status.success(),
        "`{}` in {} exited with {status}",
        argv.join(" "),
        dir.display()
    );
    Ok(())
}

/// A new version's manifest is the current version's with the version swapped; the count of
/// occurrences is fixed per sdk, so a template that changed shape stops the command instead of
/// producing a half-edited file.
fn from_template(template: &str, old: &str, new: &str, expect: usize) -> Result<String> {
    let n = template.matches(old).count();
    ensure!(
        n == expect,
        "the template names {old} {n} times, expected {expect}; update sync-versions"
    );
    Ok(template.replace(old, new))
}

/// Writes the testee files of a version that has none, from the current version's.
fn write_testee(root: &Path, sdk: &str, template_version: &str, v: &str) -> Result<()> {
    match sdk {
        "rust" | "soroban" => {
            let from = root.join(format!(
                "testees/{sdk}/versions/{template_version}/Cargo.toml"
            ));
            let text = std::fs::read_to_string(&from)
                .with_context(|| format!("reading {}", from.display()))?;
            let dir = root.join(format!("testees/{sdk}/versions/{v}"));
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("Cargo.toml"),
                from_template(&text, template_version, v, 2)?,
            )?;
            run_step(
                &[
                    "cargo",
                    "generate-lockfile",
                    "--manifest-path",
                    "Cargo.toml",
                ],
                &dir,
                &[],
            )
        }
        "go" => {
            let dir = root.join("testees/go");
            let from = dir.join(format!("versions/{template_version}.mod"));
            let text = std::fs::read_to_string(&from)
                .with_context(|| format!("reading {}", from.display()))?;
            std::fs::write(
                dir.join(format!("versions/{v}.mod")),
                from_template(&text, template_version, v, 1)?,
            )?;
            run_step(
                &["go", "mod", "tidy", &format!("-modfile=versions/{v}.mod")],
                &dir,
                &[("GOWORK", "off")],
            )
        }
        "js" => {
            let from = root.join(format!(
                "testees/js/versions/{template_version}/package.json"
            ));
            let text = std::fs::read_to_string(&from)
                .with_context(|| format!("reading {}", from.display()))?;
            let dir = root.join(format!("testees/js/versions/{v}"));
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("package.json"),
                from_template(&text, template_version, v, 3)?,
            )?;
            run_step(
                &[
                    "npm",
                    "install",
                    "--package-lock-only",
                    "--ignore-scripts",
                    "--no-audit",
                    "--no-fund",
                ],
                &dir,
                &[],
            )
        }
        "python" => {
            let from = root.join(format!(
                "testees/python/versions/{template_version}/requirements.in"
            ));
            let text = std::fs::read_to_string(&from)
                .with_context(|| format!("reading {}", from.display()))?;
            let dir = root.join(format!("testees/python/versions/{v}"));
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("requirements.in"),
                from_template(&text, template_version, v, 1)?,
            )?;
            let minor = PYTHON.rsplit_once('.').map_or(PYTHON, |(m, _)| m);
            run_step(
                &[
                    "uv",
                    "pip",
                    "compile",
                    "--quiet",
                    "--generate-hashes",
                    "--universal",
                    "--python-version",
                    minor,
                    "--no-header",
                    "requirements.in",
                    "-o",
                    "requirements.txt",
                ],
                &dir,
                &[],
            )
        }
        "java" => {
            let from = root.join(format!("testees/java/versions/{template_version}/pom.xml"));
            let text = std::fs::read_to_string(&from)
                .with_context(|| format!("reading {}", from.display()))?;
            let dir = root.join(format!("testees/java/versions/{v}"));
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("pom.xml"),
                from_template(&text, template_version, v, JAVA_POM_VERSIONS)?,
            )?;
            // builds the version and writes lib.sha256 from the jars maven resolved
            run_step(&["testees/java/lock.sh", v], root, &[])
        }
        "dart" => {
            let from = root.join(format!(
                "testees/dart/versions/{template_version}/pubspec.yaml"
            ));
            let text = std::fs::read_to_string(&from)
                .with_context(|| format!("reading {}", from.display()))?;
            let dir = root.join(format!("testees/dart/versions/{v}"));
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("pubspec.yaml"),
                from_template(&text, template_version, v, 2)?,
            )?;
            run_step(&["flutter", "pub", "get"], &dir, &[])
        }
        "php" => {
            let from = root.join(format!(
                "testees/php/versions/{template_version}/composer.json"
            ));
            let text = std::fs::read_to_string(&from)
                .with_context(|| format!("reading {}", from.display()))?;
            let dir = root.join(format!("testees/php/versions/{v}"));
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("composer.json"),
                from_template(&text, template_version, v, 1)?,
            )?;
            run_step(
                &["composer", "update", "--no-install", "--no-interaction"],
                &dir,
                &[],
            )
        }
        "dotnet" => {
            let from = root.join(format!(
                "testees/dotnet/versions/{template_version}/ShapekitTestee.csproj"
            ));
            let text = std::fs::read_to_string(&from)
                .with_context(|| format!("reading {}", from.display()))?;
            let dir = root.join(format!("testees/dotnet/versions/{v}"));
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("ShapekitTestee.csproj"),
                from_template(&text, template_version, v, 2)?,
            )?;
            run_step(&["dotnet", "restore", "--use-lock-file"], &dir, &[])
        }
        "kmp" => {
            let from = root.join(format!("testees/kmp/versions/{template_version}/pom.xml"));
            let text = std::fs::read_to_string(&from)
                .with_context(|| format!("reading {}", from.display()))?;
            let dir = root.join(format!("testees/kmp/versions/{v}"));
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("pom.xml"),
                from_template(&text, template_version, v, JAVA_POM_VERSIONS)?,
            )?;
            // builds the version and writes lib.sha256 from the jars maven resolved
            run_step(&["testees/kmp/lock.sh", v], root, &[])
        }
        // the sha256 github records for the release asset, then the install it pins
        "stellar-cli" => run_step(&["testees/stellar-cli/fetch.sh", "--lock", v], root, &[]),
        _ => unreachable!("only the sdks in SDKS have testees"),
    }
}

fn remove_testee(root: &Path, sdk: &str, v: &str) -> Result<()> {
    match sdk {
        "go" => {
            for ext in ["mod", "sum"] {
                let p = root.join(format!("testees/go/versions/{v}.{ext}"));
                if p.exists() {
                    std::fs::remove_file(&p)
                        .with_context(|| format!("removing {}", p.display()))?;
                }
            }
        }
        _ => {
            let p = root.join(format!("testees/{sdk}/versions/{v}"));
            std::fs::remove_dir_all(&p).with_context(|| format!("removing {}", p.display()))?;
        }
    }
    Ok(())
}

/// Brings versions.toml and the testee files in line with the plans. Files of versions that
/// already have them are left alone: their lock files are the pins.
pub fn apply(
    root: &Path,
    versions: &Path,
    old: &VersionsFile,
    plans: &[SdkPlan],
    rendered: &str,
) -> Result<Vec<String>> {
    let mut done = Vec::new();
    for plan in plans {
        let template = &old.sdk(&plan.sdk)?.current;
        let on_disk = testee_versions(root, &plan.sdk)?;
        let want: BTreeSet<&str> = plan.chosen.iter().map(|c| c.version.as_str()).collect();
        for c in &plan.chosen {
            let missing = testee_files(&plan.sdk, &c.version)
                .iter()
                .any(|f| !root.join(f).is_file());
            if missing {
                write_testee(root, &plan.sdk, template, &c.version)
                    .with_context(|| format!("writing the {} {} testee", plan.sdk, c.version))?;
                done.push(format!("wrote the {} {} testee files", plan.sdk, c.version));
            }
        }
        for v in &on_disk {
            if !want.contains(v.as_str()) {
                remove_testee(root, &plan.sdk, v)?;
                done.push(format!("removed the {} {v} testee files", plan.sdk));
            }
        }
    }
    let path = root.join(versions);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    if text != rendered {
        std::fs::write(&path, rendered).with_context(|| format!("writing {}", path.display()))?;
        done.push(format!("wrote {}", versions.display()));
    }
    Ok(done)
}

/// One line per version for the report.
pub fn summary(plans: &[SdkPlan]) -> String {
    let mut out = String::new();
    for p in plans {
        let _ = writeln!(
            out,
            "{}: {} versions, current {}",
            p.sdk,
            p.chosen.len(),
            p.current
        );
        for c in &p.chosen {
            let _ = writeln!(
                out,
                "  {} xdr {} protocols up to {}: {}",
                c.version,
                c.xdr.tag,
                c.xdr.max_protocol,
                c.reasons.join("; ")
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upstream::Release;

    fn tags() -> Vec<XdrTag> {
        [
            ("v26.0", 26, "c26"),
            ("v27.0", 27, "c27"),
            ("v28.0", 28, "c28"),
            ("v30.0", 30, "c30"),
        ]
        .iter()
        .map(|(n, m, c)| XdrTag {
            name: n.to_string(),
            major: *m,
            minor: 0,
            commit: c.to_string(),
        })
        .collect()
    }

    #[test]
    fn tags_extend_to_the_protocol_before_the_next() {
        let t = tags();
        assert_eq!(xdr_of_commit(&t, "c28").unwrap().max_protocol, 29);
        assert_eq!(xdr_of_commit(&t, "c30").unwrap().max_protocol, 30);
        assert_eq!(xdr_of_commit(&t, "c27").unwrap().max_protocol, 27);
        assert!(xdr_of_commit(&t, "c29").is_err());
        assert_eq!(floor_tag(&t, 29).unwrap().name, "v28.0");
        assert_eq!(floor_tag(&t, 30).unwrap().name, "v30.0");
        assert!(floor_tag(&t, 31).is_err());
    }

    #[test]
    fn stable_versions_only() {
        assert!(stable("17.4.0").is_some());
        assert!(stable("v0.7.1").is_some());
        assert!(stable("17.0.0-rc.1").is_none());
        assert!(stable("22.0.0-rc.1.1").is_none());
        assert!(stable("1.2").is_none());
        assert!(stable("01.2.3").is_none());
    }

    fn rel(v: &str, t: &str) -> Release {
        Release {
            version: v.to_string(),
            published: format!("{t}T12:00:00Z"),
            withdrawn: None,
        }
    }

    // the js releases as npm lists them on 2026-11-16 from 15.x on, with the xdr commit each
    // one's Makefile names at its gitHead; this is today's 7-version matrix
    #[test]
    fn js_matrix_of_today() {
        let releases = vec![
            rel("15.0.0", "2026-03-30"),
            rel("15.1.0", "2026-05-04"),
            rel("16.0.0-rc.1", "2026-06-05"),
            rel("16.0.0", "2026-06-15"),
            rel("16.0.1", "2026-06-18"),
            rel("16.1.0", "2026-07-22"),
            rel("16.2.0", "2026-07-29"),
            rel("17.0.0-rc.1", "2026-08-11"),
            rel("17.0.0", "2026-08-20"),
            rel("17.0.1", "2026-08-25"),
            rel("16.3.0", "2026-08-28"),
            rel("17.1.0", "2026-09-14"),
            rel("17.2.0", "2026-09-28"),
            rel("17.2.1", "2026-10-01"),
            rel("16.3.1", "2026-10-01"),
            rel("17.3.0", "2026-10-21"),
            rel("16.4.0", "2026-10-21"),
            rel("16.4.1", "2026-10-22"),
            rel("17.4.0", "2026-10-25"),
        ];
        let commit = |v: &str| match v {
            "16.0.0" | "16.0.1" | "16.1.0" | "16.2.0" => "c27",
            "16.3.0" | "16.3.1" | "17.0.0" | "17.0.1" | "17.1.0" | "17.2.0" | "17.2.1" => "c28",
            _ => "c30",
        };
        let published = Published {
            releases,
            latest: "17.4.0".to_string(),
        };
        let mut pinned: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for v in ["16.3.0", "17.2.0", "16.3.1", "17.2.1", "17.3.0"] {
            pinned.insert(v.to_string(), vec!["reproduction".to_string()]);
        }
        pinned.insert("16.4.1".to_string(), vec!["known".to_string()]);
        let t = tags();
        let floor = utc_seconds("2026-07-29T17:49:27Z").unwrap();
        let mut asked = Vec::new();
        let chosen = choose(&published, &pinned, floor, 29, &mut |v| {
            asked.push(v.to_string());
            xdr_of_commit(&t, commit(v))
        })
        .unwrap();
        let got: Vec<(&str, u32)> = chosen
            .iter()
            .map(|c| (c.version.as_str(), c.xdr.max_protocol))
            .collect();
        assert_eq!(
            got,
            [
                ("16.3.0", 29),
                ("16.3.1", 29),
                ("16.4.1", 30),
                ("17.2.0", 29),
                ("17.2.1", 29),
                ("17.3.0", 30),
                ("17.4.0", 30)
            ]
        );
        // 15.x ended before the floor tag existed, so its xdr is never fetched
        assert!(!asked.iter().any(|v| v.starts_with("15.")));
        let newest17 = chosen.iter().find(|c| c.version == "17.4.0").unwrap();
        assert_eq!(newest17.reasons, ["newest 17.x release"]);
        let both = chosen.iter().find(|c| c.version == "16.4.1").unwrap();
        assert_eq!(both.reasons, ["newest 16.x release", "known"]);
    }

    #[test]
    fn a_newer_release_joins_and_old_lines_below_the_floor_stay_out() {
        let published = Published {
            releases: vec![
                rel("v0.6.1", "2026-08-10"),
                rel("v0.7.0", "2026-08-03"),
                rel("v0.7.3", "2026-08-24"),
                rel("v0.9.0", "2026-11-10"),
                rel("v0.10.0", "2026-12-01"),
                Release {
                    withdrawn: Some("retracted".to_string()),
                    ..rel("v0.10.1", "2026-12-02")
                },
            ],
            latest: "v0.10.0".to_string(),
        };
        let t = tags();
        let commit = |v: &str| match v {
            "v0.6.1" => "c27",
            "v0.7.0" | "v0.7.3" => "c28",
            _ => "c30",
        };
        let floor = utc_seconds("2026-07-29T17:49:27Z").unwrap();
        let chosen = choose(&published, &BTreeMap::new(), floor, 29, &mut |v| {
            xdr_of_commit(&t, commit(v))
        })
        .unwrap();
        let got: Vec<&str> = chosen.iter().map(|c| c.version.as_str()).collect();
        assert_eq!(got, ["v0.7.3", "v0.9.0", "v0.10.0"]);
    }

    #[test]
    fn a_named_version_that_is_not_published_stops() {
        let published = Published {
            releases: vec![rel("1.0.0", "2026-09-01")],
            latest: "1.0.0".to_string(),
        };
        let mut pinned = BTreeMap::new();
        pinned.insert("0.9.0".to_string(), vec!["reproduction x".to_string()]);
        let t = tags();
        let err = choose(&published, &pinned, 0, 29, &mut |_| {
            xdr_of_commit(&t, "c30")
        })
        .unwrap_err();
        assert!(format!("{err:#}").contains("not published"));
    }

    #[test]
    fn the_venv_python_is_the_pinned_build() {
        let pins =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pins.toml"))
                .unwrap();
        let asset = format!("asset = \"cpython-{PYTHON}+");
        assert!(
            pins.contains(&asset),
            "pins.toml pins no python-build-standalone build of {PYTHON}"
        );
    }

    #[test]
    fn template_counts_guard_the_edit() {
        assert_eq!(
            from_template("a 1.0.0 b \"=1.0.0\"", "1.0.0", "2.0.0", 2).unwrap(),
            "a 2.0.0 b \"=2.0.0\""
        );
        assert!(from_template("a 1.0.0", "1.0.0", "2.0.0", 2).is_err());
    }

    #[test]
    fn rendered_file_parses_back() {
        let file = VersionsFile::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/versions.toml"),
        )
        .unwrap();
        let plans: Vec<SdkPlan> = SDKS
            .iter()
            .map(|sdk| {
                let s = file.sdk(sdk).unwrap();
                SdkPlan {
                    sdk: sdk.to_string(),
                    current: s.current.clone(),
                    chosen: s
                        .versions
                        .iter()
                        .map(|v| Chosen {
                            version: v.version.clone(),
                            reasons: vec!["test".to_string()],
                            xdr: Xdr {
                                tag: "v30.0".to_string(),
                                commit: "4f524bbac80c781c06e4fb5fc93a2d0d331d6705".to_string(),
                                max_protocol: v.max_protocol.unwrap(),
                            },
                        })
                        .collect(),
                }
            })
            .collect();
        let text = render(&file, &plans).unwrap();
        let back: VersionsFile = toml::from_str(&text).unwrap();
        for sdk in SDKS {
            let (a, b) = (file.sdk(sdk).unwrap(), back.sdk(sdk).unwrap());
            assert_eq!(a.package, b.package);
            assert_eq!(a.current, b.current);
            assert_eq!(a.issues_repo, b.issues_repo);
            assert_eq!(a.issues_repo_by_op, b.issues_repo_by_op);
            assert_eq!(a.timeout_ms, b.timeout_ms);
            assert_eq!(a.memory, b.memory);
            assert_eq!(a.versions.len(), b.versions.len());
            for (x, y) in a.versions.iter().zip(&b.versions) {
                assert_eq!(x.version, y.version);
                assert_eq!(x.command, y.command);
                assert_eq!(x.max_protocol, y.max_protocol);
                assert_eq!(x.env, y.env);
                assert_eq!(x.build.len(), y.build.len());
                for (s, t) in x.build.iter().zip(&y.build) {
                    assert_eq!(s.argv, t.argv);
                    assert_eq!(s.dir, t.dir);
                    assert_eq!(s.env, t.env);
                }
            }
        }
    }
}
