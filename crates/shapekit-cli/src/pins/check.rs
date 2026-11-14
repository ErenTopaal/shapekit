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
}
