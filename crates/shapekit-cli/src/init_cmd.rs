//! `shapekit init adapter|testee`: writes a skeleton adapter or testee that speaks the
//! protocol from its first run. The templates are in crates/shapekit-cli/templates/.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};

#[derive(Subcommand)]
pub enum InitCommand {
    /// Write an adapter skeleton that puts an indexer behind the replay harness: the protocol
    /// side is complete, the part that starts the indexer and reads its store is yours to write.
    Adapter(AdapterArgs),
    /// Write a testee skeleton that puts an SDK behind the conformance runner: hello reports
    /// the installed SDK version, and each operation you add is listed and run.
    Testee(TesteeArgs),
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Lang {
    Go,
    Ts,
    Python,
}

impl Lang {
    fn name(self) -> &'static str {
        match self {
            Lang::Go => "go",
            Lang::Ts => "ts",
            Lang::Python => "python",
        }
    }
}

#[derive(Args)]
pub struct AdapterArgs {
    #[arg(long, value_enum)]
    lang: Lang,
    /// The indexer the adapter drives, as describe reports it, for example `my-indexer`.
    #[arg(long)]
    target: String,
    /// The exact build of the indexer under test, for example its full commit hash.
    #[arg(long)]
    target_version: String,
    /// Directory to write into; must not exist or be empty. `adapter-<target>` when absent.
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Args)]
pub struct TesteeArgs {
    #[arg(long, value_enum)]
    lang: Lang,
    /// The SDK package under test: an npm package (ts), a PyPI distribution (python) or a go
    /// module (go). The Stellar SDK of the language when absent.
    #[arg(long, requires_all = ["sdk_version", "issues_repo"])]
    package: Option<String>,
    /// The exact version of --package to install and test.
    #[arg(long, requires = "package")]
    sdk_version: Option<String>,
    /// The GitHub repository, `owner/name`, that `shapekit conformance draft-issues` points
    /// issues for --package at.
    #[arg(long, requires = "package")]
    issues_repo: Option<String>,
    /// Go only: a package of the module to link, so the module's version is in the build info.
    /// `<package>/xdr` for go-stellar-sdk; the module path itself otherwise.
    #[arg(long)]
    go_import: Option<String>,
    /// The short SDK name in versions.toml and in `shapekit check sdk --sdk`; the language name
    /// when absent.
    #[arg(long)]
    name: Option<String>,
    /// Directory to write into; must not exist or be empty. `testee-<name>` when absent.
    #[arg(long)]
    out: Option<PathBuf>,
}

/// The Stellar SDK of each language a testee is written in: package, version, issues
/// repository. The ts and go versions are the `current` of conformance/versions.toml (a test
/// keeps them equal); the python version is PyPI's latest stable release on 2026-11-17.
pub const DEFAULT_SDKS: [(Lang, &str, &str, &str); 3] = [
    (
        Lang::Ts,
        "@stellar/stellar-sdk",
        "17.4.0",
        "stellar/js-stellar-sdk",
    ),
    (
        Lang::Go,
        "github.com/stellar/go-stellar-sdk",
        "v0.9.0",
        "stellar/go-stellar-sdk",
    ),
    (
        Lang::Python,
        "stellar-sdk",
        "17.0.1",
        "StellarCN/py-stellar-base",
    ),
];

const GO_STELLAR_SDK: &str = "github.com/stellar/go-stellar-sdk";

/// One file of a skeleton: its name in the output directory, its template, whether it is run
/// directly.
type File = (&'static str, &'static str, bool);

fn adapter_files(lang: Lang) -> &'static [File] {
    match lang {
        Lang::Python => &[(
            "adapter.py",
            include_str!("../templates/adapter/python/adapter.py.tmpl"),
            true,
        )],
        Lang::Ts => &[
            (
                "adapter.ts",
                include_str!("../templates/adapter/ts/adapter.ts.tmpl"),
                false,
            ),
            (
                "package.json",
                include_str!("../templates/adapter/ts/package.json.tmpl"),
                false,
            ),
        ],
        Lang::Go => &[
            (
                "main.go",
                include_str!("../templates/adapter/go/main.go.tmpl"),
                false,
            ),
            (
                "go.mod",
                include_str!("../templates/adapter/go/go.mod.tmpl"),
                false,
            ),
        ],
    }
}

fn testee_files(lang: Lang) -> &'static [File] {
    match lang {
        Lang::Python => &[
            (
                "testee.py",
                include_str!("../templates/testee/python/testee.py.tmpl"),
                true,
            ),
            (
                "requirements.txt",
                include_str!("../templates/testee/python/requirements.txt.tmpl"),
                false,
            ),
            (
                "versions.toml",
                include_str!("../templates/testee/python/versions.toml.tmpl"),
                false,
            ),
        ],
        Lang::Ts => &[
            (
                "testee.ts",
                include_str!("../templates/testee/ts/testee.ts.tmpl"),
                false,
            ),
            (
                "package.json",
                include_str!("../templates/testee/ts/package.json.tmpl"),
                false,
            ),
            (
                "versions.toml",
                include_str!("../templates/testee/ts/versions.toml.tmpl"),
                false,
            ),
        ],
        Lang::Go => &[
            (
                "main.go",
                include_str!("../templates/testee/go/main.go.tmpl"),
                false,
            ),
            (
                "go.mod",
                include_str!("../templates/testee/go/go.mod.tmpl"),
                false,
            ),
            (
                "versions.toml",
                include_str!("../templates/testee/go/versions.toml.tmpl"),
                false,
            ),
        ],
    }
}

/// Replaces every `{{key}}` of `vars` and fails on one left over, so a template and its
/// variables cannot drift apart unnoticed.
fn render(template: &str, vars: &[(&str, String)]) -> Result<String> {
    let mut s = template.to_owned();
    for (k, v) in vars {
        s = s.replace(&format!("{{{{{k}}}}}"), v);
    }
    if let Some(i) = s.find("{{") {
        let rest = &s[i..];
        let end = rest.find("}}").map_or(rest.len(), |e| e + 2);
        bail!("template variable {} has no value", &rest[..end]);
    }
    Ok(s)
}

/// The protocol's operation names as the items of a list literal in `lang`.
fn op_list(lang: Lang) -> String {
    let ops = shapekit_conformance::testee::OPS;
    match lang {
        Lang::Python => ops
            .iter()
            .map(|o| format!("\"{o}\","))
            .collect::<Vec<_>>()
            .join("\n    "),
        Lang::Ts => ops
            .iter()
            .map(|o| format!("\"{o}\","))
            .collect::<Vec<_>>()
            .join("\n  "),
        Lang::Go => ops
            .iter()
            .map(|o| format!("\"{o}\": true,"))
            .collect::<Vec<_>>()
            .join("\n\t"),
    }
}

/// A name usable in a file name, a toml key and a package name.
fn check_name(what: &str, name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && name.starts_with(|c: char| c.is_ascii_lowercase());
    if !ok {
        bail!("{what} {name:?}: use lowercase letters, digits, - and _, starting with a letter");
    }
    Ok(())
}

fn write_all(out: &Path, files: &[File], vars: &[(&str, String)], readme: &str) -> Result<()> {
    if out.exists() {
        let mut entries =
            std::fs::read_dir(out).with_context(|| format!("reading {}", out.display()))?;
        if entries.next().is_some() {
            bail!("{} exists and is not empty", out.display());
        }
    }
    // render everything first, so a template error writes nothing
    let mut rendered = Vec::new();
    for (name, template, executable) in files {
        rendered.push((*name, render(template, vars)?, *executable));
    }
    rendered.push(("README.md", readme.to_owned(), false));
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    for (name, text, executable) in rendered {
        let path = out.join(name);
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        if executable {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .with_context(|| format!("making {} executable", path.display()))?;
        }
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn adapter(a: AdapterArgs) -> Result<bool> {
    check_name("--target", &a.target)?;
    if a.target_version.trim().is_empty() || a.target_version.contains('"') {
        bail!("--target-version must be a non-empty string without quotes");
    }
    let name = format!("{}-adapter", a.target);
    let out = a
        .out
        .unwrap_or_else(|| PathBuf::from(format!("adapter-{}", a.target)));
    let protocol = shapekit_core_driver::smoke::PROTOCOL;
    let vars = [
        ("adapter", name.clone()),
        ("target", a.target.clone()),
        ("target_version", a.target_version.clone()),
        (
            "adapter_protocol",
            shapekit_harness::protocol::ADAPTER_PROTOCOL_VERSION.to_string(),
        ),
        ("vocab", shapekit_oracle::ORACLE_VOCAB_VERSION.to_string()),
        ("protocol_min", protocol.to_string()),
        ("protocol_max", protocol.to_string()),
    ];
    let (build, command) = match a.lang {
        Lang::Python => (None, "python3 adapter.py".to_owned()),
        Lang::Ts => (None, "node adapter.ts".to_owned()),
        Lang::Go => (Some(format!("go build -o {name} .")), format!("./{name}")),
    };
    let dir = out.display();
    let build_line = build
        .as_ref()
        .map(|b| format!("(cd {dir} && {b})\n"))
        .unwrap_or_default();
    let command_in = match a.lang {
        Lang::Go => format!("{dir}/{name}"),
        Lang::Python => format!("python3 {dir}/adapter.py"),
        Lang::Ts => format!("node {dir}/adapter.ts"),
    };
    let readme = format!(
        "# {name}\n\n\
         A shapekit adapter for {target}, written by `shapekit init adapter --lang {lang}`. It \
         speaks adapter protocol version {proto}: describe, start, advance, status, export and \
         stop, in the rpc delivery mode, with no claims yet.\n\n\
         The part that knows {target} is the `Target` type at the end of the source. Until a \
         method is written, the adapter answers the request that needs it with a `target` \
         error naming the method, and the harness reports the chain as not evaluated with that \
         message. The adapter interface document of the shapekit repository \
         (docs/adapter-interface.md) describes every request, the claims and the vocabulary.\n\n\
         Run it on one chain from the root of a shapekit checkout:\n\n\
         ```sh\n{build_line}shapekit check indexer --adapter \"{command_in}\" --chain cap40.signed-payload-signer.p30\n```\n\n\
         The command in this directory is `{command}`.\n",
        target = a.target,
        lang = a.lang.name(),
        proto = shapekit_harness::protocol::ADAPTER_PROTOCOL_VERSION,
    );
    write_all(&out, adapter_files(a.lang), &vars, &readme)?;
    Ok(true)
}

fn testee(a: TesteeArgs) -> Result<bool> {
    let name = a.name.unwrap_or_else(|| a.lang.name().to_owned());
    check_name("--name", &name)?;
    let (package, version, issues) = match a.package {
        Some(p) => (
            p,
            a.sdk_version.expect("clap requires --sdk-version"),
            a.issues_repo.expect("clap requires --issues-repo"),
        ),
        None => {
            let (_, p, v, i) = DEFAULT_SDKS
                .iter()
                .find(|(l, ..)| l.name() == a.lang.name())
                .expect("every language has a default sdk");
            (p.to_string(), v.to_string(), i.to_string())
        }
    };
    for (flag, v) in [
        ("--package", &package),
        ("--sdk-version", &version),
        ("--issues-repo", &issues),
    ] {
        if v.trim().is_empty() || v.contains(['"', ' ', '\\']) {
            bail!("{flag} {v:?} must be non-empty, without spaces, quotes or backslashes");
        }
    }
    let go_import = match (a.lang, a.go_import) {
        (Lang::Go, Some(i)) => i,
        (Lang::Go, None) if package == GO_STELLAR_SDK => format!("{package}/xdr"),
        (Lang::Go, None) => package.clone(),
        (_, Some(_)) => bail!("--go-import applies to --lang go only"),
        (_, None) => String::new(),
    };
    let out = a
        .out
        .unwrap_or_else(|| PathBuf::from(format!("testee-{name}")));
    let dir = out
        .to_str()
        .context("the output path is not valid UTF-8")?
        .trim_end_matches('/')
        .to_owned();
    if dir.contains(['"', '\\']) {
        bail!("the output path {dir:?} must not hold quotes or backslashes");
    }
    let vars = [
        ("name", name.clone()),
        ("package", package.clone()),
        ("sdk_version", version.clone()),
        ("issues_repo", issues),
        ("go_import", go_import),
        ("dir", dir.clone()),
        (
            "testee_protocol",
            shapekit_conformance::testee::PROTOCOL_VERSION.to_string(),
        ),
        ("defined_ops", op_list(a.lang)),
        (
            "protocol_max",
            shapekit_core_driver::smoke::PROTOCOL.to_string(),
        ),
    ];
    let readme = format!(
        "# shapekit testee for {package}\n\n\
         Written by `shapekit init testee --lang {lang}`. It speaks testee protocol version \
         {proto}. `hello` reports the installed version of {package} ({version} as \
         installed by the build steps in `versions.toml`). It implements no operation yet: \
         hello lists none, so every check of a run counts as unsupported.\n\n\
         Add each operation to the ops table at the end of the source, calling {package}'s \
         public API only. The testee protocol document of the shapekit repository \
         (docs/testee-protocol.md) lists every operation, its request and result members, and \
         the error classes.\n\n\
         Run it over the corpus from the directory `shapekit init` ran in, the root of a \
         shapekit checkout:\n\n\
         ```sh\n\
         shapekit check sdk --sdk {name} --versions-file {dir}/versions.toml --known-failures {dir}/known-failures\n\
         ```\n\n\
         The build steps in `versions.toml` install {package} {version} into this directory \
         first; the matrix goes to `target/conformance/MATRIX.md`.\n",
        lang = a.lang.name(),
        proto = shapekit_conformance::testee::PROTOCOL_VERSION,
    );
    write_all(&out, testee_files(a.lang), &vars, &readme)?;
    Ok(true)
}

pub fn run(cmd: InitCommand) -> Result<bool> {
    match cmd {
        InitCommand::Adapter(a) => adapter(a),
        InitCommand::Testee(a) => testee(a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // the ts and go defaults follow the committed matrix's current versions
    #[test]
    fn default_sdks_follow_the_versions_file() {
        let file = shapekit_conformance::versions::VersionsFile::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/versions.toml"),
        )
        .unwrap();
        for (lang, package, version, _) in DEFAULT_SDKS {
            let key = match lang {
                Lang::Ts => "js",
                Lang::Go => "go",
                Lang::Python => continue,
            };
            let sdk = file.sdk(key).unwrap();
            assert_eq!(sdk.package, package);
            assert_eq!(sdk.current, version, "{key}");
        }
    }

    #[test]
    fn every_template_variable_is_given() {
        let err = render("a {{b}} c", &[("x", "y".into())]).unwrap_err();
        assert_eq!(err.to_string(), "template variable {{b}} has no value");
        assert_eq!(render("a {{b}} c", &[("b", "y".into())]).unwrap(), "a y c");
    }
}
