//! Pure comparisons between a pin and what an upstream source returned.
//!
//! Every function takes the raw upstream response text, so the logic is tested
//! without the network. `Ok` carries a short detail for the report, `Err` the
//! reason the pin failed.

use serde_json::Value;

pub type Verdict = Result<String, String>;

fn short(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

/// Parses `git ls-remote` output into (sha, ref name) pairs.
pub fn parse_ls_remote(output: &str) -> Result<Vec<(String, String)>, String> {
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let (sha, name) = line
                .split_once('\t')
                .ok_or_else(|| format!("unexpected ls-remote line: {line:?}"))?;
            Ok((sha.to_string(), name.to_string()))
        })
        .collect()
}

/// Checks a ref against ls-remote output. An annotated tag is compared through
/// its peeled `^{}` entry, which names the commit rather than the tag object.
pub fn check_git_ref(ls_remote: &str, git_ref: &str, pinned: Option<&str>) -> Verdict {
    let refs = parse_ls_remote(ls_remote)?;
    let peeled = format!("{git_ref}^{{}}");
    let lookup = |name: &str| {
        refs.iter()
            .find(|(_, n)| n == name)
            .map(|(s, _)| s.as_str())
    };
    let Some(target) = lookup(&peeled).or_else(|| lookup(git_ref)) else {
        return Err(format!("{git_ref} not found on the remote"));
    };
    match pinned {
        None => Ok(format!(
            "{git_ref} exists at {} (floating, not sha-pinned)",
            short(target)
        )),
        Some(sha) if sha == target => Ok(format!("{git_ref} -> {}", short(sha))),
        Some(sha) => Err(format!("{git_ref} resolves to {target}, pinned {sha}")),
    }
}

/// Checks one version record from the crates.io sparse index. The index file
/// holds one JSON object per line, one line per published version.
pub fn check_crate(index_file: &str, version: &str, checksum: &str) -> Verdict {
    for line in index_file.lines().filter(|l| !l.trim().is_empty()) {
        let record: Value =
            serde_json::from_str(line).map_err(|e| format!("bad index line: {e}"))?;
        if record["vers"].as_str() != Some(version) {
            continue;
        }
        if record["yanked"].as_bool() != Some(false) {
            return Err(format!("{version} is yanked"));
        }
        let Some(cksum) = record["cksum"].as_str() else {
            return Err(format!("index record for {version} has no cksum"));
        };
        if cksum != checksum {
            return Err(format!("checksum is {cksum}, pinned {checksum}"));
        }
        return Ok(format!("{version}, not yanked, checksum matches"));
    }
    Err(format!("version {version} is not published"))
}

/// Checks the npm registry document for one package version.
pub fn check_npm(version_doc: &str, version: &str, integrity: &str) -> Verdict {
    let doc: Value =
        serde_json::from_str(version_doc).map_err(|e| format!("bad registry response: {e}"))?;
    if doc["version"].as_str() != Some(version) {
        return Err(format!(
            "registry returned version {}, expected {version}",
            doc["version"]
        ));
    }
    match doc["dist"]["integrity"].as_str() {
        Some(i) if i == integrity => Ok(format!("{version}, integrity matches")),
        Some(i) => Err(format!("integrity is {i}, pinned {integrity}")),
        None => Err(format!("registry has no integrity for {version}")),
    }
}

/// Checks a PyPI release document (`/pypi/<name>/<version>/json`): the pinned file must be one
/// of the release's files, not yanked, with the pinned sha256.
pub fn check_pypi(release_doc: &str, version: &str, file: &str, sha256: &str) -> Verdict {
    let doc: Value =
        serde_json::from_str(release_doc).map_err(|e| format!("bad registry response: {e}"))?;
    if doc["info"]["version"].as_str() != Some(version) {
        return Err(format!(
            "registry returned version {}, expected {version}",
            doc["info"]["version"]
        ));
    }
    let files = doc["urls"].as_array().ok_or("registry lists no files")?;
    let Some(f) = files.iter().find(|f| f["filename"].as_str() == Some(file)) else {
        return Err(format!("{version} has no file {file}"));
    };
    if f["yanked"].as_bool() == Some(true) {
        return Err(format!("{file} is yanked"));
    }
    match f["digests"]["sha256"].as_str() {
        Some(h) if h == sha256 => Ok(format!("{file}, sha256 matches")),
        Some(h) => Err(format!("sha256 is {h}, pinned {sha256}")),
        None => Err(format!("registry has no sha256 for {file}")),
    }
}

/// Checks the bytes of a downloaded file against its pinned sha256.
pub fn check_sha256(bytes: &[u8], file: &str, sha256: &str) -> Verdict {
    use sha2::{Digest, Sha256};
    let got = hex::encode(Sha256::digest(bytes));
    if got == sha256 {
        Ok(format!("{file}, sha256 matches"))
    } else {
        Err(format!("{file} has sha256 {got}, pinned {sha256}"))
    }
}

/// Checks a Go module proxy `.info` document. The origin hash is the VCS commit
/// the proxy recorded for the version.
pub fn check_gomod(info: &str, version: &str, origin_hash: &str) -> Verdict {
    let doc: Value = serde_json::from_str(info).map_err(|e| format!("bad proxy response: {e}"))?;
    if doc["Version"].as_str() != Some(version) {
        return Err(format!(
            "proxy returned version {}, expected {version}",
            doc["Version"]
        ));
    }
    match doc["Origin"]["Hash"].as_str() {
        Some(h) if h == origin_hash => Ok(format!("{version} -> {}", short(h))),
        Some(h) => Err(format!("origin hash is {h}, pinned {origin_hash}")),
        None => Err(format!("proxy reports no origin hash for {version}")),
    }
}

/// Checks the `.sha256` companion of a rust channel manifest, which has the
/// form `<hash>  <file name>`.
pub fn check_rust_channel(sha_file: &str, artifact: &str, sha256: &str) -> Verdict {
    let mut parts = sha_file.split_whitespace();
    let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
        return Err(format!("unexpected sha256 file: {sha_file:?}"));
    };
    if name != artifact {
        return Err(format!("sha256 file names {name}, expected {artifact}"));
    }
    if hash != sha256 {
        return Err(format!("sha256 is {hash}, pinned {sha256}"));
    }
    Ok(format!("{artifact} sha256 matches"))
}

/// Checks go.dev's release list (`?mode=json&include=all`).
pub fn check_go_release(releases: &str, version: &str, artifact: &str, sha256: &str) -> Verdict {
    let doc: Value =
        serde_json::from_str(releases).map_err(|e| format!("bad go.dev response: {e}"))?;
    let want = format!("go{version}");
    let Some(release) = doc
        .as_array()
        .ok_or("go.dev response is not a list")?
        .iter()
        .find(|r| r["version"].as_str() == Some(want.as_str()))
    else {
        return Err(format!("{want} is not listed on go.dev"));
    };
    if release["stable"].as_bool() != Some(true) {
        return Err(format!("{want} is not marked stable"));
    }
    let file = release["files"]
        .as_array()
        .and_then(|files| {
            files
                .iter()
                .find(|f| f["filename"].as_str() == Some(artifact))
        })
        .ok_or_else(|| format!("{want} has no file {artifact}"))?;
    match file["sha256"].as_str() {
        Some(h) if h == sha256 => Ok(format!("{artifact} sha256 matches")),
        Some(h) => Err(format!("{artifact} sha256 is {h}, pinned {sha256}")),
        None => Err(format!("{artifact} has no sha256 on go.dev")),
    }
}

/// Checks node's release index and the version's SHASUMS256.txt.
pub fn check_node(
    index: &str,
    shasums: &str,
    version: &str,
    artifact: &str,
    sha256: &str,
) -> Verdict {
    let doc: Value =
        serde_json::from_str(index).map_err(|e| format!("bad nodejs.org index: {e}"))?;
    let want = format!("v{version}");
    let listed = doc
        .as_array()
        .ok_or("nodejs.org index is not a list")?
        .iter()
        .any(|r| r["version"].as_str() == Some(want.as_str()));
    if !listed {
        return Err(format!("{want} is not in the nodejs.org release index"));
    }
    let hash = shasums
        .lines()
        .filter_map(|l| l.split_once("  "))
        .find(|(_, name)| *name == artifact)
        .map(|(h, _)| h)
        .ok_or_else(|| format!("SHASUMS256.txt has no entry for {artifact}"))?;
    if hash != sha256 {
        return Err(format!("{artifact} sha256 is {hash}, pinned {sha256}"));
    }
    Ok(format!("{want}, {artifact} sha256 matches"))
}

/// Checks Flutter's release index (`releases_linux.json`): the version is a stable release
/// whose archive is the pinned artifact with the pinned sha256.
pub fn check_flutter(index: &str, version: &str, artifact: &str, sha256: &str) -> Verdict {
    let doc: Value =
        serde_json::from_str(index).map_err(|e| format!("bad flutter release index: {e}"))?;
    let release = doc["releases"]
        .as_array()
        .ok_or("the flutter release index has no releases")?
        .iter()
        .find(|r| r["version"].as_str() == Some(version) && r["channel"].as_str() == Some("stable"))
        .ok_or_else(|| format!("{version} is no stable release in the flutter release index"))?;
    let archive = release["archive"].as_str().unwrap_or("");
    if archive.rsplit('/').next() != Some(artifact) {
        return Err(format!(
            "{version}'s archive is {archive}, pinned {artifact}"
        ));
    }
    let hash = release["sha256"].as_str().unwrap_or("");
    if hash != sha256 {
        return Err(format!("{artifact} sha256 is {hash}, pinned {sha256}"));
    }
    Ok(format!("{version}, {artifact} sha256 matches"))
}

/// Checks php.net's release document for a version: its source list holds the pinned artifact
/// with the pinned sha256.
pub fn check_php(doc: &str, version: &str, artifact: &str, sha256: &str) -> Verdict {
    let doc: Value =
        serde_json::from_str(doc).map_err(|e| format!("bad php.net release document: {e}"))?;
    let hash = doc["source"]
        .as_array()
        .ok_or_else(|| format!("php.net lists no source files for {version}"))?
        .iter()
        .find(|f| f["filename"].as_str() == Some(artifact))
        .and_then(|f| f["sha256"].as_str())
        .ok_or_else(|| format!("php.net lists no {artifact} for {version}"))?;
    if hash != sha256 {
        return Err(format!("{artifact} sha256 is {hash}, pinned {sha256}"));
    }
    Ok(format!("{version}, {artifact} sha256 matches"))
}

/// Checks the sha256sum file getcomposer.org publishes next to a release's composer.phar.
pub fn check_composer(sums: &str, version: &str, artifact: &str, sha256: &str) -> Verdict {
    let hash = sums
        .lines()
        .filter_map(|l| l.split_once("  "))
        .find(|(_, name)| *name == artifact)
        .map(|(h, _)| h)
        .ok_or_else(|| format!("the composer {version} checksum file has no {artifact}"))?;
    if hash != sha256 {
        return Err(format!("{artifact} sha256 is {hash}, pinned {sha256}"));
    }
    Ok(format!("{version}, {artifact} sha256 matches"))
}

/// Checks a .NET channel's releases.json: an sdk of the pinned version lists the pinned
/// artifact with the pinned sha512, the only hash Microsoft publishes for it.
pub fn check_dotnet(doc: &str, version: &str, artifact: &str, hash: &str) -> Verdict {
    let doc: Value =
        serde_json::from_str(doc).map_err(|e| format!("bad .NET release metadata: {e}"))?;
    let file = doc["releases"]
        .as_array()
        .ok_or("the .NET release metadata has no releases")?
        .iter()
        .flat_map(|r| r["sdks"].as_array().cloned().unwrap_or_default())
        .filter(|sdk| sdk["version"].as_str() == Some(version))
        .flat_map(|sdk| sdk["files"].as_array().cloned().unwrap_or_default())
        .find(|f| {
            f["url"]
                .as_str()
                .is_some_and(|u| u.ends_with(&format!("/{artifact}")))
        })
        .ok_or_else(|| format!("no .NET sdk {version} lists {artifact}"))?;
    let got = file["hash"].as_str().unwrap_or("");
    if got != hash {
        return Err(format!("{artifact} sha512 is {got}, pinned {hash}"));
    }
    Ok(format!("{version}, {artifact} sha512 matches"))
}

/// Checks GitHub's release document for a tag (`/repos/{owner}/{repo}/releases/tags/{tag}`):
/// a published, non-prerelease release that holds the asset, and the sha256 digest GitHub
/// computed for the asset on upload.
pub fn check_github_release(release_doc: &str, tag: &str, asset: &str, sha256: &str) -> Verdict {
    let doc: Value =
        serde_json::from_str(release_doc).map_err(|e| format!("bad github response: {e}"))?;
    if doc["tag_name"].as_str() != Some(tag) {
        return Err(format!(
            "github returned release {}, expected {tag}",
            doc["tag_name"]
        ));
    }
    if doc["draft"].as_bool() != Some(false) {
        return Err(format!("release {tag} is a draft"));
    }
    if doc["prerelease"].as_bool() != Some(false) {
        return Err(format!("release {tag} is a prerelease"));
    }
    let Some(found) = doc["assets"]
        .as_array()
        .ok_or("release has no asset list")?
        .iter()
        .find(|a| a["name"].as_str() == Some(asset))
    else {
        return Err(format!("release {tag} has no asset {asset}"));
    };
    match found["digest"].as_str().map(|d| d.strip_prefix("sha256:")) {
        Some(Some(h)) if h == sha256 => Ok(format!("{asset} sha256 matches")),
        Some(Some(h)) => Err(format!("{asset} sha256 is {h}, pinned {sha256}")),
        Some(None) => Err(format!("{asset} digest is not a sha256 digest")),
        None => Err(format!("github reports no digest for {asset}")),
    }
}

/// Compares the `Docker-Content-Digest` header of a manifest request.
pub fn check_image_digest(header: Option<&str>, digest: &str) -> Verdict {
    match header {
        Some(d) if d == digest => Ok(format!("digest {}", short(d.trim_start_matches("sha256:")))),
        Some(d) => Err(format!("registry digest is {d}, pinned {digest}")),
        None => Err("registry sent no Docker-Content-Digest header".to_string()),
    }
}

/// Checks a release artifact the upstream signs with minisign and publishes no checksum for:
/// the downloaded bytes have the pinned sha256, and the `.minisig` next to them is a valid
/// signature by the pinned key over those bytes, with a trusted comment that names the
/// artifact and is itself signed. Both the legacy (`Ed`) and the prehashed (`ED`, BLAKE2b-512)
/// signature algorithms are read.
pub fn check_minisigned(
    bytes: &[u8],
    minisig: &str,
    public_key: &str,
    artifact: &str,
    sha256: &str,
) -> Verdict {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use blake2::{Blake2b512, Digest as _};
    use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};

    let got = hex::encode(sha2::Sha256::digest(bytes));
    if got != sha256 {
        return Err(format!("{artifact} sha256 is {got}, pinned {sha256}"));
    }
    let key = B64
        .decode(public_key)
        .map_err(|e| format!("the pinned minisign key is not base64: {e}"))?;
    let [b'E', b'd', key_id @ ..] = key.as_slice() else {
        return Err("the pinned minisign key is not an Ed25519 key".to_string());
    };
    if key_id.len() != 8 + 32 {
        return Err("the pinned minisign key has the wrong length".to_string());
    }
    let (key_id, key_bytes) = key_id.split_at(8);
    let vk = VerifyingKey::from_bytes(key_bytes.try_into().expect("32 bytes"))
        .map_err(|e| format!("the pinned minisign key is invalid: {e}"))?;

    let mut lines = minisig.lines();
    let (_untrusted, sig_line, trusted, global) =
        match (lines.next(), lines.next(), lines.next(), lines.next()) {
            (Some(u), Some(s), Some(t), Some(g)) => (u, s, t, g),
            _ => return Err(format!("{artifact}.minisig does not have four lines")),
        };
    let sig = B64
        .decode(sig_line.trim())
        .map_err(|e| format!("{artifact}.minisig signature is not base64: {e}"))?;
    if sig.len() != 2 + 8 + 64 {
        return Err(format!("{artifact}.minisig signature has the wrong length"));
    }
    let (alg, rest) = sig.split_at(2);
    let (sig_key_id, sig_bytes) = rest.split_at(8);
    if sig_key_id != key_id {
        return Err(format!(
            "{artifact}.minisig is signed by key {}, pinned key {}",
            hex::encode(sig_key_id),
            hex::encode(key_id)
        ));
    }
    let signed: Vec<u8> = match alg {
        b"ED" => Blake2b512::digest(bytes).to_vec(),
        b"Ed" => bytes.to_vec(),
        _ => return Err(format!("{artifact}.minisig uses an unknown algorithm")),
    };
    let signature = Signature::from_slice(sig_bytes).map_err(|e| e.to_string())?;
    vk.verify(&signed, &signature)
        .map_err(|_| format!("{artifact}.minisig is not a valid signature of the artifact"))?;

    let comment = trusted
        .strip_prefix("trusted comment: ")
        .ok_or_else(|| format!("{artifact}.minisig has no trusted comment"))?;
    let global = B64
        .decode(global.trim())
        .map_err(|e| format!("{artifact}.minisig global signature is not base64: {e}"))?;
    let global = Signature::from_slice(&global).map_err(|e| e.to_string())?;
    let mut both = sig_bytes.to_vec();
    both.extend_from_slice(comment.as_bytes());
    vk.verify(&both, &global)
        .map_err(|_| format!("{artifact}.minisig trusted comment is not signed"))?;
    if !comment.split('\t').any(|f| f == format!("file:{artifact}")) {
        return Err(format!(
            "{artifact}.minisig trusted comment names another file: {comment}"
        ));
    }
    Ok(format!(
        "{artifact} sha256 matches, minisign signature by key {} verifies",
        hex::encode(key_id)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ls-remote output for stellar-rpc, an annotated tag
    const RPC_LS_REMOTE: &str = "ee5a1a3c978d22ead9d6c27a776219ada07d92ad\trefs/tags/v30.0.0\n\
13e53516633c3ce1281c7edbacb607107d3a4e55\trefs/tags/v30.0.0^{}\n";

    // stellar-core, a lightweight tag
    const CORE_LS_REMOTE: &str = "0e49751aa9ddf892302cca02cea7f3f5d97df9f1\trefs/tags/v30.0.0\n";

    #[test]
    fn annotated_tag_uses_peeled_commit() {
        let ok = check_git_ref(
            RPC_LS_REMOTE,
            "refs/tags/v30.0.0",
            Some("13e53516633c3ce1281c7edbacb607107d3a4e55"),
        );
        assert_eq!(ok, Ok("refs/tags/v30.0.0 -> 13e53516633c".to_string()));
        // the tag object sha is not the commit and must not be accepted
        let err = check_git_ref(
            RPC_LS_REMOTE,
            "refs/tags/v30.0.0",
            Some("ee5a1a3c978d22ead9d6c27a776219ada07d92ad"),
        );
        assert!(
            err.unwrap_err()
                .contains("resolves to 13e53516633c3ce1281c7edbacb607107d3a4e55")
        );
    }

    #[test]
    fn lightweight_tag_matches_and_mismatches() {
        assert!(
            check_git_ref(
                CORE_LS_REMOTE,
                "refs/tags/v30.0.0",
                Some("0e49751aa9ddf892302cca02cea7f3f5d97df9f1")
            )
            .is_ok()
        );
        let err = check_git_ref(
            CORE_LS_REMOTE,
            "refs/tags/v30.0.0",
            Some("0e49751aa9ddf892302cca02cea7f3f5d97df9f0"),
        )
        .unwrap_err();
        assert!(err.contains("pinned 0e49751aa9ddf892302cca02cea7f3f5d97df9f0"));
    }

    #[test]
    fn missing_ref_fails() {
        assert_eq!(
            check_git_ref("", "refs/tags/v30.0.0", None),
            Err("refs/tags/v30.0.0 not found on the remote".to_string())
        );
    }

    #[test]
    fn floating_ref_only_needs_to_exist() {
        let out = "3d3c42e5aac5ba805825da76410c181273ba90b1\trefs/tags/v7\n";
        let ok = check_git_ref(out, "refs/tags/v7", None).unwrap();
        assert!(ok.contains("floating"));
    }

    #[test]
    fn malformed_ls_remote_is_an_error() {
        assert!(check_git_ref("not a ref line", "refs/tags/v7", None).is_err());
    }

    const HEX_INDEX: &str = concat!(
        r#"{"name":"hex","vers":"0.4.1","deps":[],"cksum":"76cdda6bf525062a0c9e8f14ee2b37935c86b8efb6c8b69b3c83dfb518a914af","features":{},"yanked":true}"#,
        "\n",
        r#"{"name":"hex","vers":"0.4.2","deps":[],"cksum":"644f9158b2f133fd50f5fb3242878846d9eb792e445c893805ff0e3824006e35","features":{},"yanked":false}"#,
        "\n",
        r#"{"name":"hex","vers":"0.4.3","deps":[],"cksum":"7f24254aa9a54b5c858eaee2f5bccdb46aaf0e486a595ed5fd8f86ba55232a70","features":{},"yanked":false}"#,
        "\n",
    );

    #[test]
    fn crate_version_matches() {
        let ok = check_crate(
            HEX_INDEX,
            "0.4.3",
            "7f24254aa9a54b5c858eaee2f5bccdb46aaf0e486a595ed5fd8f86ba55232a70",
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn crate_checksum_mismatch_fails() {
        let err = check_crate(
            HEX_INDEX,
            "0.4.3",
            "644f9158b2f133fd50f5fb3242878846d9eb792e445c893805ff0e3824006e35",
        )
        .unwrap_err();
        assert!(err.starts_with("checksum is 7f24254a"));
    }

    #[test]
    fn yanked_crate_fails() {
        let err = check_crate(
            HEX_INDEX,
            "0.4.1",
            "76cdda6bf525062a0c9e8f14ee2b37935c86b8efb6c8b69b3c83dfb518a914af",
        );
        assert_eq!(err, Err("0.4.1 is yanked".to_string()));
    }

    #[test]
    fn unpublished_crate_fails() {
        let err = check_crate(HEX_INDEX, "0.4.9", "00");
        assert_eq!(err, Err("version 0.4.9 is not published".to_string()));
    }

    #[test]
    fn crate_version_with_build_metadata_is_exact() {
        let index = r#"{"name":"toml","vers":"1.2.1+spec-1.1.0","deps":[],"cksum":"d5755c51863cdac711f1956baf8eb9588e060755f7d3c4c979e70ea1e1f32cd7","features":{},"yanked":false}"#;
        assert!(check_crate(index, "1.2.1", "x").is_err());
        assert!(
            check_crate(
                index,
                "1.2.1+spec-1.1.0",
                "d5755c51863cdac711f1956baf8eb9588e060755f7d3c4c979e70ea1e1f32cd7"
            )
            .is_ok()
        );
    }

    const SDK_DOC: &str = r#"{"name":"@stellar/stellar-sdk","version":"17.4.0","license":"Apache-2.0","dist":{"shasum":"13d8db3553b906a0ea4778a29c8b676f44b6acec","tarball":"https://registry.npmjs.org/@stellar/stellar-sdk/-/stellar-sdk-17.4.0.tgz","integrity":"sha512-IX6gi62yWQ+6Oz56LsVEEeSViSOWJL3BkttftwVmVsu+62nLTWNVsCmTWEzwgNLT/q5HGP25OnlU+hhy2E4wwA=="}}"#;

    #[test]
    fn npm_integrity_matches() {
        let ok = check_npm(
            SDK_DOC,
            "17.4.0",
            "sha512-IX6gi62yWQ+6Oz56LsVEEeSViSOWJL3BkttftwVmVsu+62nLTWNVsCmTWEzwgNLT/q5HGP25OnlU+hhy2E4wwA==",
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn npm_integrity_mismatch_and_wrong_version_fail() {
        assert!(
            check_npm(SDK_DOC, "17.4.0", "sha512-AAAA")
                .unwrap_err()
                .contains("pinned sha512-AAAA")
        );
        assert!(
            check_npm(SDK_DOC, "17.3.0", "sha512-AAAA")
                .unwrap_err()
                .contains("expected 17.3.0")
        );
    }

    const SDK_INFO: &str = r#"{"Version":"v0.8.0","Time":"2026-10-20T21:30:21Z","Origin":{"VCS":"git","URL":"https://github.com/stellar/go-stellar-sdk","Hash":"921a5b16af9a72d973ab9ba666e96ad8bf2f6ce4","Ref":"refs/tags/v0.8.0"}}"#;

    #[test]
    fn file_sha256() {
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(check_sha256(b"", "a.jar", empty).is_ok());
        assert!(
            check_sha256(b"x", "a.jar", empty)
                .unwrap_err()
                .contains("pinned")
        );
    }

    #[test]
    fn pypi_file_digest_and_yank() {
        let doc = r#"{"info":{"version":"17.0.1"},"urls":[
            {"filename":"stellar_sdk-17.0.1-py3-none-any.whl","yanked":false,"digests":{"sha256":"aa"}},
            {"filename":"stellar_sdk-17.0.1.tar.gz","yanked":true,"digests":{"sha256":"bb"}}]}"#;
        let wheel = "stellar_sdk-17.0.1-py3-none-any.whl";
        assert!(check_pypi(doc, "17.0.1", wheel, "aa").is_ok());
        assert!(
            check_pypi(doc, "17.0.1", wheel, "cc")
                .unwrap_err()
                .contains("pinned cc")
        );
        assert!(
            check_pypi(doc, "17.0.1", "stellar_sdk-17.0.1.tar.gz", "bb")
                .unwrap_err()
                .contains("yanked")
        );
        assert!(
            check_pypi(doc, "17.0.1", "x.whl", "aa")
                .unwrap_err()
                .contains("no file")
        );
        assert!(
            check_pypi(doc, "16.1.1", wheel, "aa")
                .unwrap_err()
                .contains("expected 16.1.1")
        );
    }

    #[test]
    fn gomod_origin_matches_and_mismatches() {
        assert!(
            check_gomod(
                SDK_INFO,
                "v0.8.0",
                "921a5b16af9a72d973ab9ba666e96ad8bf2f6ce4"
            )
            .is_ok()
        );
        assert!(
            check_gomod(
                SDK_INFO,
                "v0.8.0",
                "83d77301a1966946cc5864856b08694b5a82c662"
            )
            .unwrap_err()
            .contains("origin hash is 921a5b16")
        );
    }

    #[test]
    fn gomod_without_origin_fails() {
        let info = r#"{"Version":"v0.8.0","Time":"2026-10-20T21:30:21Z"}"#;
        assert_eq!(
            check_gomod(info, "v0.8.0", "921a5b16af9a72d973ab9ba666e96ad8bf2f6ce4"),
            Err("proxy reports no origin hash for v0.8.0".to_string())
        );
    }

    #[test]
    fn rust_channel_sha() {
        let file = "fb152be80975c194aa5fc684b2aec3a6e14e2b855efe2faa4e5f8ae6e590f463  channel-rust-1.99.1.toml\n";
        assert!(
            check_rust_channel(
                file,
                "channel-rust-1.99.1.toml",
                "fb152be80975c194aa5fc684b2aec3a6e14e2b855efe2faa4e5f8ae6e590f463"
            )
            .is_ok()
        );
        assert!(check_rust_channel(file, "channel-rust-1.99.0.toml", "x").is_err());
        assert!(check_rust_channel(file, "channel-rust-1.99.1.toml", "00").is_err());
    }

    const GO_RELEASES: &str = r#"[
{"version":"go1.27.4","stable":true,"files":[
 {"filename":"go1.27.4.src.tar.gz","os":"","arch":"","version":"go1.27.4","sha256":"73cfb8571aec0c01177687db87163d46aa20e9779672772891aa17654d9a6099","size":35153471,"kind":"source"},
 {"filename":"go1.27.4.linux-amd64.tar.gz","os":"linux","arch":"amd64","version":"go1.27.4","sha256":"7226736c2339719cda28fbf3a54d71aa63bcefe376e98cef00034268dab24840","size":70680823,"kind":"archive"}]},
{"version":"go1.28rc1","stable":false,"files":[]}
]"#;

    #[test]
    fn go_release_sha() {
        assert!(
            check_go_release(
                GO_RELEASES,
                "1.27.4",
                "go1.27.4.linux-amd64.tar.gz",
                "7226736c2339719cda28fbf3a54d71aa63bcefe376e98cef00034268dab24840"
            )
            .is_ok()
        );
        assert!(
            check_go_release(GO_RELEASES, "1.27.4", "go1.27.4.linux-amd64.tar.gz", "00")
                .unwrap_err()
                .contains("pinned 00")
        );
        assert!(
            check_go_release(GO_RELEASES, "1.27.5", "x", "00")
                .unwrap_err()
                .contains("not listed")
        );
        assert!(
            check_go_release(GO_RELEASES, "1.28rc1", "x", "00")
                .unwrap_err()
                .contains("not marked stable")
        );
    }

    const NODE_INDEX: &str = r#"[{"version":"v27.0.0-alpha.0.1.0","lts":false},{"version":"v26.13.1","date":"2026-10-28","lts":"Lithium","security":false}]"#;
    const NODE_SHASUMS: &str = "89e1853138db51e8b82d3e98ba9c0d3f2956a28e5e55ea1b7e12d25aae92d5dd  node-v26.13.1-linux-arm64.tar.xz\n\
5c063e46bab0fbc1c17cc445d7ff129cbdcd0471be6ed83b6ab864f8143bc864  node-v26.13.1-linux-x64.tar.xz\n";

    #[test]
    fn node_release_sha() {
        assert!(
            check_node(
                NODE_INDEX,
                NODE_SHASUMS,
                "26.13.1",
                "node-v26.13.1-linux-x64.tar.xz",
                "5c063e46bab0fbc1c17cc445d7ff129cbdcd0471be6ed83b6ab864f8143bc864"
            )
            .is_ok()
        );
        assert!(
            check_node(NODE_INDEX, NODE_SHASUMS, "26.13.2", "x", "00")
                .unwrap_err()
                .contains("not in the nodejs.org release index")
        );
        assert!(
            check_node(
                NODE_INDEX,
                NODE_SHASUMS,
                "26.13.1",
                "node-v26.13.1-win-x64.zip",
                "00"
            )
            .unwrap_err()
            .contains("no entry")
        );
    }

    // trimmed from api.github.com/repos/stellar/stellar-cli/releases/tags/v30.0.1
    const CLI_RELEASE: &str = r#"{"tag_name":"v30.0.1","draft":false,"prerelease":false,"assets":[
{"name":"stellar-cli-30.0.1-aarch64-apple-darwin.tar.gz","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},
{"name":"stellar-cli-30.0.1-x86_64-unknown-linux-gnu.tar.gz","digest":"sha256:48c70074020e09af0a3ebbd2bf62d53c9aec47935e49a112d3cd90fb4e37658f","size":26579342}]}"#;
    const CLI_ASSET: &str = "stellar-cli-30.0.1-x86_64-unknown-linux-gnu.tar.gz";
    const CLI_SHA: &str = "48c70074020e09af0a3ebbd2bf62d53c9aec47935e49a112d3cd90fb4e37658f";

    #[test]
    fn github_release_asset_matches() {
        assert_eq!(
            check_github_release(CLI_RELEASE, "v30.0.1", CLI_ASSET, CLI_SHA),
            Ok(format!("{CLI_ASSET} sha256 matches"))
        );
    }

    #[test]
    fn github_release_mismatches_fail() {
        let zero = "0".repeat(64);
        assert!(
            check_github_release(CLI_RELEASE, "v30.0.1", CLI_ASSET, &zero)
                .unwrap_err()
                .contains(&format!("pinned {zero}"))
        );
        assert!(
            check_github_release(CLI_RELEASE, "v30.0.1", "stellar-cli-30.0.1.zip", CLI_SHA)
                .unwrap_err()
                .contains("has no asset")
        );
        assert!(
            check_github_release(CLI_RELEASE, "v30.0.0", CLI_ASSET, CLI_SHA)
                .unwrap_err()
                .contains("expected v30.0.0")
        );
    }

    #[test]
    fn github_prerelease_draft_and_missing_digest_fail() {
        let pre = CLI_RELEASE.replace("\"prerelease\":false", "\"prerelease\":true");
        assert_eq!(
            check_github_release(&pre, "v30.0.1", CLI_ASSET, CLI_SHA),
            Err("release v30.0.1 is a prerelease".to_string())
        );
        let draft = CLI_RELEASE.replace("\"draft\":false", "\"draft\":true");
        assert_eq!(
            check_github_release(&draft, "v30.0.1", CLI_ASSET, CLI_SHA),
            Err("release v30.0.1 is a draft".to_string())
        );
        let undigested = CLI_RELEASE.replace(&format!("\"digest\":\"sha256:{CLI_SHA}\","), "");
        assert_eq!(
            check_github_release(&undigested, "v30.0.1", CLI_ASSET, CLI_SHA),
            Err(format!("github reports no digest for {CLI_ASSET}"))
        );
    }

    #[test]
    fn image_digest() {
        let pinned = "sha256:eb7a7c81a29b8665c8878d55e1b3979f0d7527426350c4ff258a9c7647ae5c51";
        assert!(check_image_digest(Some(pinned), pinned).is_ok());
        assert!(
            check_image_digest(
                Some("sha256:a8cb428ce497695321fdffc8cdb23b8102c1074ebe480f1c9e923fe1052e22ff"),
                pinned
            )
            .is_err()
        );
        assert!(check_image_digest(None, pinned).is_err());
    }

    const FLUTTER_INDEX: &str = r#"{"releases": [
        {"version": "3.50.0", "channel": "stable", "archive": "stable/linux/flutter_linux_3.50.0-stable.tar.xz", "sha256": "aa"},
        {"version": "3.51.0-0.1.pre", "channel": "beta", "archive": "beta/linux/flutter_linux_3.51.0-0.1.pre-beta.tar.xz", "sha256": "bb"}
    ]}"#;

    #[test]
    fn flutter_pin_matches_the_release_index() {
        assert!(
            check_flutter(
                FLUTTER_INDEX,
                "3.50.0",
                "flutter_linux_3.50.0-stable.tar.xz",
                "aa"
            )
            .is_ok()
        );
        assert!(
            check_flutter(
                FLUTTER_INDEX,
                "3.50.0",
                "flutter_linux_3.50.0-stable.tar.xz",
                "ab"
            )
            .is_err()
        );
        assert!(
            check_flutter(
                FLUTTER_INDEX,
                "3.51.0-0.1.pre",
                "flutter_linux_3.51.0-0.1.pre-beta.tar.xz",
                "bb"
            )
            .is_err()
        );
    }

    #[test]
    fn php_and_composer_pins_match_their_checksums() {
        let php = r#"{"source": [{"filename": "php-8.5.12.tar.xz", "sha256": "aa"}]}"#;
        assert!(check_php(php, "8.5.12", "php-8.5.12.tar.xz", "aa").is_ok());
        assert!(check_php(php, "8.5.12", "php-8.5.12.tar.xz", "ab").is_err());
        assert!(check_php(php, "8.5.12", "php-8.5.12.tar.gz", "aa").is_err());
        let sums = "aa  composer.phar\n";
        assert!(check_composer(sums, "2.11.2", "composer.phar", "aa").is_ok());
        assert!(check_composer(sums, "2.11.2", "composer.phar", "ab").is_err());
    }

    #[test]
    fn dotnet_pin_matches_the_release_metadata() {
        let doc = r#"{"releases": [{"sdks": [{"version": "11.0.100", "files": [
            {"url": "https://x/dotnet-sdk-11.0.100-linux-x64.tar.gz", "hash": "aa"}]}]}]}"#;
        let a = "dotnet-sdk-11.0.100-linux-x64.tar.gz";
        assert!(check_dotnet(doc, "11.0.100", a, "aa").is_ok());
        assert!(check_dotnet(doc, "11.0.100", a, "ab").is_err());
        assert!(check_dotnet(doc, "11.0.101", a, "aa").is_err());
    }

    // a minisig made with a fixed test key over a small artifact, the way minisign -S -H writes it
    fn minisig_for(data: &[u8], file: &str, prehash: bool) -> (String, String) {
        use base64::Engine as _;
        use base64::engine::general_purpose::STANDARD as B64;
        use blake2::{Blake2b512, Digest as _};
        use ed25519_dalek::{Signer as _, SigningKey};
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let key_id = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut pk = b"Ed".to_vec();
        pk.extend_from_slice(&key_id);
        pk.extend_from_slice(sk.verifying_key().as_bytes());
        let (alg, msg) = if prehash {
            (b"ED", Blake2b512::digest(data).to_vec())
        } else {
            (b"Ed", data.to_vec())
        };
        let sig = sk.sign(&msg).to_bytes();
        let mut line = alg.to_vec();
        line.extend_from_slice(&key_id);
        line.extend_from_slice(&sig);
        let comment = format!("timestamp:1\tfile:{file}\thashed");
        let mut both = sig.to_vec();
        both.extend_from_slice(comment.as_bytes());
        let global = sk.sign(&both).to_bytes();
        (
            B64.encode(pk),
            format!(
                "untrusted comment: test\n{}\ntrusted comment: {comment}\n{}\n",
                B64.encode(line),
                B64.encode(global)
            ),
        )
    }

    #[test]
    fn minisigned_artifact_verifies_both_algorithms() {
        use sha2::Digest as _;
        let data = b"libsodium release bytes";
        let sha = hex::encode(sha2::Sha256::digest(data));
        for prehash in [true, false] {
            let (key, sig) = minisig_for(data, "a.tar.gz", prehash);
            assert!(check_minisigned(data, &sig, &key, "a.tar.gz", &sha).is_ok());
            // other bytes with the pinned sha256 cannot exist; other bytes fail the hash first
            assert!(check_minisigned(b"other", &sig, &key, "a.tar.gz", &sha).is_err());
            // the comment must name the artifact
            assert!(check_minisigned(data, &sig, &key, "b.tar.gz", &sha).is_err());
        }
    }

    #[test]
    fn minisigned_artifact_refuses_a_tampered_signature_or_comment() {
        use sha2::Digest as _;
        let data = b"libsodium release bytes";
        let sha = hex::encode(sha2::Sha256::digest(data));
        let (key, sig) = minisig_for(data, "a.tar.gz", true);
        let comment = sig.replace("file:a.tar.gz", "file:a.tar.gz\textra:1");
        assert!(check_minisigned(data, &comment, &key, "a.tar.gz", &sha).is_err());
        let (other_key, _) = minisig_for(b"x", "a.tar.gz", true);
        assert_eq!(other_key, key, "one fixed test key");
        let mut lines: Vec<String> = sig.lines().map(str::to_string).collect();
        lines[1] = lines[1].replacen('A', "B", 1);
        let bad = lines.join("\n");
        assert!(check_minisigned(data, &bad, &key, "a.tar.gz", &sha).is_err());
    }
}
