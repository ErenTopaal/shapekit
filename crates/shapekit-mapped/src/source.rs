//! The target's source: a git clone, facts read from files at the commit, and the image built
//! from the commit on pinned base images.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result, bail};
use sha2::{Digest, Sha256};

use crate::docker;
use crate::mapping::{Fact, Image};
use crate::template::Context;

pub struct Source {
    pub url: String,
    pub dir: PathBuf,
    pub commit: String,
}

impl Source {
    fn git(&self, args: &[&str]) -> Result<Vec<u8>> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .context("running git")?;
        if !out.status.success() {
            bail!(
                "git {}: {}: {}",
                args[0],
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(out.stdout)
    }

    /// Makes sure the clone exists and holds the commit, cloning or fetching it.
    pub fn ensure(&self) -> Result<()> {
        if !self.dir.join(".git").exists() {
            if let Some(parent) = self.dir.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            let status = Command::new("git")
                .args(["clone", "--quiet", &self.url])
                .arg(&self.dir)
                .stdin(Stdio::null())
                .stdout(Stdio::from(std::io::stderr()))
                .stderr(Stdio::inherit())
                .status()
                .context("running git clone")?;
            if !status.success() {
                bail!("cloning {}: {status}", self.url);
            }
        }
        let obj = format!("{}^{{commit}}", self.commit);
        if self.git(&["cat-file", "-e", &obj]).is_ok() {
            return Ok(());
        }
        self.git(&["fetch", "--quiet", "origin", &self.commit])
            .with_context(|| {
                format!(
                    "commit {} is not in {} and could not be fetched",
                    self.commit,
                    self.dir.display()
                )
            })?;
        Ok(())
    }

    pub fn show(&self, path: &str) -> Result<Vec<u8>> {
        self.git(&["show", &format!("{}:{path}", self.commit)])
    }

    /// Reads every fact at the commit.
    pub fn facts(&self, facts: &[Fact]) -> Result<BTreeMap<String, String>> {
        let mut out = BTreeMap::new();
        for f in facts {
            let text = String::from_utf8_lossy(&self.show(&f.file)?).into_owned();
            let v = if let Some(c) = &f.contains {
                text.contains(c.as_str()).to_string()
            } else {
                let re = regex::Regex::new(f.pattern.as_deref().expect("validated"))?;
                let Some(m) = re.captures(&text).and_then(|c| c.get(1)) else {
                    bail!(
                        "fact {}: {} at {} does not match {:?}",
                        f.name,
                        f.file,
                        self.commit,
                        re.as_str()
                    );
                };
                let cap = m.as_str().to_owned();
                match &f.values {
                    None => cap,
                    Some(values) => match values.get(&cap) {
                        Some(v) => v.clone(),
                        None => bail!(
                            "fact {}: {} at {} reads {cap:?}, which the mapping's values do not list",
                            f.name,
                            f.file,
                            self.commit
                        ),
                    },
                }
            };
            out.insert(f.name.clone(), v);
        }
        Ok(out)
    }
}

/// The external images a Dockerfile builds FROM, in order and without repeats. Stage names
/// defined earlier and `scratch` are not images.
pub fn dockerfile_bases(dockerfile: &str) -> Result<Vec<String>> {
    let mut bases = Vec::new();
    let mut stages = std::collections::BTreeSet::new();
    for line in dockerfile.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() || !f[0].eq_ignore_ascii_case("FROM") {
            continue;
        }
        let args: Vec<&str> = f[1..]
            .iter()
            .copied()
            .skip_while(|a| a.starts_with("--"))
            .collect();
        let Some(r) = args.first() else {
            bail!("FROM with no image: {line:?}");
        };
        if r.contains('$') {
            bail!("FROM {r} uses a build argument; the base cannot be pinned");
        }
        let lower = r.to_ascii_lowercase();
        if !stages.contains(&lower) && *r != "scratch" && !bases.iter().any(|b| b == r) {
            bases.push(r.to_string());
        }
        if args.len() >= 3 && args[1].eq_ignore_ascii_case("AS") {
            stages.insert(args[2].to_ascii_lowercase());
        }
    }
    if bases.is_empty() {
        bail!("no FROM line");
    }
    Ok(bases)
}

/// The `--build-context` flags that replace each base with its pinned digest, and the digest
/// of that set for the image label.
pub fn pinned_contexts(
    bases: &[String],
    pins: &BTreeMap<String, String>,
) -> Result<(Vec<String>, String)> {
    let mut flags = Vec::new();
    let mut set = Vec::new();
    for b in bases {
        let Some(p) = pins.get(b) else {
            bail!("base image {b} has no pinned digest; add it to image.bases");
        };
        flags.push("--build-context".to_owned());
        flags.push(format!("{b}=docker-image://{p}"));
        set.push(format!("{b}={p}"));
    }
    set.sort();
    let sum = Sha256::digest(set.join("\n").as_bytes());
    Ok((flags, hex::encode(sum)))
}

/// Builds the commit with its Dockerfile into `tag`, unless an image of that tag built on the
/// same pinned bases exists. `git archive` of the commit is the build context.
pub fn ensure_image(
    src: &Source,
    img: &Image,
    ctx: &Context,
    builder: Option<&str>,
) -> Result<String> {
    let tag = ctx.expand(img.tag.as_deref().expect("validated: a build has a tag"))?;
    let dockerfile_path = img.dockerfile.as_deref().unwrap_or("Dockerfile");
    let dockerfile = String::from_utf8_lossy(&src.show(dockerfile_path)?).into_owned();
    let bases = dockerfile_bases(&dockerfile)
        .with_context(|| format!("the {dockerfile_path} at {}", src.commit))?;
    let (contexts, base_set) = pinned_contexts(&bases, &img.bases)
        .with_context(|| format!("the {dockerfile_path} at {}", src.commit))?;
    if let Some(label) = &img.base_label {
        let fmt = format!("{{{{index .Config.Labels \"{label}\"}}}}");
        if let Ok(got) = docker::docker(&[
            "image".into(),
            "inspect".into(),
            "--format".into(),
            fmt,
            tag.clone(),
        ]) && got == base_set
        {
            return Ok(tag);
        }
    }
    let mut args: Vec<String> = vec!["buildx".into(), "build".into(), "--load".into()];
    if let Some(b) = builder {
        args.push("--builder".into());
        args.push(b.into());
    }
    args.extend(contexts);
    if dockerfile_path != "Dockerfile" {
        args.push("-f".into());
        args.push(dockerfile_path.into());
    }
    for (k, v) in ctx.expand_map(&img.build_args)? {
        args.push("--build-arg".into());
        args.push(format!("{k}={v}"));
    }
    for (k, v) in ctx.expand_map(&img.labels)? {
        args.push("--label".into());
        args.push(format!("{k}={v}"));
    }
    if let Some(label) = &img.base_label {
        args.push("--label".into());
        args.push(format!("{label}={base_set}"));
    }
    args.push("-t".into());
    args.push(tag.clone());
    args.push("-".into());
    let mut archive = Command::new("git")
        .arg("-C")
        .arg(&src.dir)
        .args(["archive", "--format=tar", &src.commit])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running git archive")?;
    let mut out = archive.stdout.take().expect("stdout is piped");
    let built = docker::docker_stream(&args, &mut out);
    // drain whatever docker did not read so git archive can exit
    let drained = std::io::copy(&mut out, &mut std::io::sink());
    let finished = archive
        .wait_with_output()
        .context("waiting for git archive")?;
    built.with_context(|| format!("building {}", src.commit))?;
    if !finished.status.success() {
        bail!(
            "git archive {}: {}: {}",
            src.commit,
            finished.status,
            String::from_utf8_lossy(&finished.stderr).trim()
        );
    }
    drained.with_context(|| format!("git archive {}", src.commit))?;
    Ok(tag)
}

/// The user cache directory, as Go's `os.UserCacheDir` finds it.
pub fn user_cache_dir() -> Result<PathBuf> {
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        return Ok(Path::new(&home).join("Library/Caches"));
    }
    if let Some(x) = std::env::var_os("XDG_CACHE_HOME").filter(|x| !x.is_empty()) {
        return Ok(PathBuf::from(x));
    }
    let home = std::env::var_os("HOME").context("neither XDG_CACHE_HOME nor HOME is set")?;
    Ok(Path::new(&home).join(".cache"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases_skip_stages_and_scratch() {
        let df = "FROM golang:1.25.9-bookworm AS api-build\nRUN x\nFROM --platform=linux/amd64 debian:bookworm-slim\nFROM api-build\nFROM scratch\n";
        assert_eq!(
            dockerfile_bases(df).unwrap(),
            vec!["golang:1.25.9-bookworm", "debian:bookworm-slim"]
        );
        assert!(dockerfile_bases("FROM $BASE\n").is_err());
        let pins: BTreeMap<String, String> = [("a".to_owned(), "x@sha256:1".to_owned())].into();
        assert!(pinned_contexts(&["b".to_owned()], &pins).is_err());
        let (flags, digest) = pinned_contexts(&["a".to_owned()], &pins).unwrap();
        assert_eq!(
            flags,
            vec!["--build-context", "a=docker-image://x@sha256:1"]
        );
        assert_eq!(digest.len(), 64);
    }
}
