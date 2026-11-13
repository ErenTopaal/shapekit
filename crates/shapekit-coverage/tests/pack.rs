//! The release archive is a pure function of the corpus bytes.

use std::io::Read;
use std::path::Path;

use shapekit_coverage::pack::{SUMS_FILE, archive_name, pack};
use shapekit_manifest::{
    CoreImage, DecodeExpectation, Entry, EntryKind, EntryProtocol, Expected, FileRef,
    GeneratedWith, Manifest, Source, Validity, Variants, XdrDefinitions, sha256_hex,
};
use shapekit_registry::SourceClass;

const VECTOR: &str = "vectors/strkey/demo.synth-one.json";

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn manifest(vector_sha: &str) -> Manifest {
    Manifest {
        schema_version: 1,
        corpus_version: "9.9.9".into(),
        generated_with: GeneratedWith {
            stellar_xdr: "30.0.0".into(),
            xdr_definitions: XdrDefinitions {
                tag: "v30.0".into(),
                commit: "0".repeat(40),
            },
            core_image: CoreImage {
                repository: "stellar/stellar-core".into(),
                tag: "t".into(),
                digest: "d".into(),
            },
            soroban_env_host: "30.0.0".into(),
        },
        protocol_tiers: vec![],
        entries: vec![Entry {
            id: "demo.synth-one".into(),
            kind: EntryKind::Strkey,
            family: "demo".into(),
            caps: vec![],
            protocol: EntryProtocol {
                introduced: 1,
                generated_at: None,
                valid_min: 1,
                valid_max: None,
            },
            xdr_type: None,
            lcm_version: None,
            tx_meta_version: None,
            source: Source {
                class: SourceClass::Synthetic,
                recipe: None,
                chain: Some("demo.p30".into()),
                ledger_seq: None,
                tx_hash: None,
                tx_index: None,
                upstream: None,
                provenance_url: None,
                normalization: None,
                core_rejection: None,
            },
            synthetic: true,
            passphrase_dependent: false,
            variants: Variants::default(),
            vector: Some(FileRef {
                path: VECTOR.into(),
                sha256: vector_sha.into(),
            }),
            expected: Expected {
                validity: Validity::Valid,
                decode: DecodeExpectation::MustAccept,
                roundtrip: true,
            },
            flags: vec![],
            fields_exercised: vec![],
            linked_issues: vec![],
            notes: None,
        }],
    }
}

// a corpus small enough to pack in a debug build, with a nested path longer than ustar's
// 100-byte name field so the gnu long-name records are exercised too
fn fixture(root: &Path) -> Manifest {
    let vector = b"{\"cases\":[]}\n";
    write(root, "manifest.json", b"{}\n");
    write(root, VECTOR, vector);
    write(root, "entries/demo/demo.synth-one/entry.json", b"{}\n");
    write(root, "coverage/coverage.json", b"{}\n");
    write(root, "coverage/COVERAGE.md", b"# coverage\n");
    write(root, "goldens/core-v30.0.0/index.json", b"{}\n");
    let deep = format!(
        "chains/demo.p30/testnet/ledgers/{}/x.xdr.zst",
        "p".repeat(120)
    );
    write(root, &deep, &[7u8; 5000]);
    write(root, "chains/demo.p30/testnet/chain.json", b"{}\n");
    manifest(&sha256_hex(vector))
}

fn members(archive: &Path) -> Vec<(String, u64, u64, u32, Vec<u8>)> {
    let file = std::fs::File::open(archive).unwrap();
    let decoder = zstd::stream::Decoder::new(file).unwrap();
    let mut tar = tar::Archive::new(decoder);
    let mut out = Vec::new();
    for item in tar.entries().unwrap() {
        let mut item = item.unwrap();
        let h = item.header().clone();
        let path = item.path().unwrap().to_string_lossy().into_owned();
        let mut bytes = Vec::new();
        item.read_to_end(&mut bytes).unwrap();
        out.push((
            path,
            h.mtime().unwrap(),
            h.uid().unwrap() + h.gid().unwrap(),
            h.mode().unwrap(),
            bytes,
        ));
    }
    out
}

#[test]
fn two_runs_give_identical_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let m = fixture(&corpus);
    let a = pack(&corpus, &m, &dir.path().join("a")).unwrap();
    // mtimes on disk must not leak into the archive
    let later = std::fs::File::options()
        .append(true)
        .open(corpus.join("manifest.json"))
        .unwrap();
    later
        .set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1 << 30))
        .unwrap();
    drop(later);
    let b = pack(&corpus, &m, &dir.path().join("b")).unwrap();
    assert_eq!(
        std::fs::read(&a.archive).unwrap(),
        std::fs::read(&b.archive).unwrap()
    );
    assert_eq!(
        std::fs::read(dir.path().join("a").join(SUMS_FILE)).unwrap(),
        std::fs::read(dir.path().join("b").join(SUMS_FILE)).unwrap()
    );
    assert_eq!(a.files, 8);
    let sums = std::fs::read_to_string(&a.sums).unwrap();
    assert_eq!(sums, format!("{}  {}\n", a.sha256, archive_name("9.9.9")));
    assert_eq!(a.sha256, sha256_hex(&std::fs::read(&a.archive).unwrap()));
}

#[test]
fn archive_holds_every_file_with_fixed_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let m = fixture(&corpus);
    let r = pack(&corpus, &m, &dir.path().join("out")).unwrap();
    let members = members(&r.archive);
    let files: Vec<&(String, u64, u64, u32, Vec<u8>)> =
        members.iter().filter(|m| !m.0.ends_with('/')).collect();
    assert_eq!(files.len(), 8);
    for (path, mtime, ids, mode, bytes) in &members {
        assert!(path.starts_with("corpus-9.9.9/"), "{path}");
        assert_eq!((*mtime, *ids), (0, 0), "{path}");
        if path.ends_with('/') {
            assert_eq!(*mode, 0o755, "{path}");
        } else {
            assert_eq!(*mode, 0o644, "{path}");
            let rel = path.strip_prefix("corpus-9.9.9/").unwrap();
            assert_eq!(bytes, &std::fs::read(corpus.join(rel)).unwrap(), "{path}");
        }
    }
    // byte order of paths, parents before children
    let names: Vec<&str> = files.iter().map(|m| m.0.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn refuses_a_file_whose_hash_disagrees_with_the_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let m = fixture(&corpus);
    write(&corpus, VECTOR, b"{\"cases\":[1]}\n");
    let err = pack(&corpus, &m, &dir.path().join("out")).unwrap_err();
    assert!(err.to_string().contains("has sha256"), "{err}");
}

#[test]
fn refuses_strays_outside_the_entry_directories() {
    for stray in [
        "notes.txt",
        "coverage/old.json",
        "goldens/core-v30.0.0/extra.json",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let corpus = dir.path().join("corpus");
        let m = fixture(&corpus);
        write(&corpus, "goldens/agree-exceptions.toml", b"");
        write(&corpus, stray, b"x");
        let err = pack(&corpus, &m, &dir.path().join("out")).unwrap_err();
        assert!(
            err.to_string().contains("belongs to no manifest entry"),
            "{stray}: {err}"
        );
    }
}

#[test]
fn refuses_a_file_named_without_its_hash() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let mut m = fixture(&corpus);
    let rel = "entries/demo/demo.synth-one/testnet.xdr";
    write(&corpus, rel, b"\0\0\0\0");
    m.entries[0].variants.testnet = Some(shapekit_manifest::Variant {
        network_passphrase: None,
        xdr_file: rel.into(),
        xdr_sha256: sha256_hex(b"\0\0\0\0"),
        expected_json_file: Some("entries/demo/demo.synth-one/testnet.json".into()),
        expected_json_sha256: None,
        oracle_file: None,
        oracle_sha256: None,
    });
    let err = pack(&corpus, &m, &dir.path().join("out")).unwrap_err();
    assert!(err.to_string().contains("without its sha256"), "{err}");
}

#[test]
fn refuses_a_path_that_leaves_the_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let mut m = fixture(&corpus);
    write(dir.path(), "outside.json", b"{\"cases\":[]}\n");
    m.entries[0].vector.as_mut().unwrap().path = "../outside.json".into();
    let err = pack(&corpus, &m, &dir.path().join("out")).unwrap_err();
    assert!(err.to_string().contains("not a relative path"), "{err}");
}

#[test]
fn refuses_files_no_entry_vouches_for() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let m = fixture(&corpus);
    write(
        &corpus,
        "entries/demo/demo.leftover/testnet.xdr",
        b"\0\0\0\0",
    );
    let err = pack(&corpus, &m, &dir.path().join("out")).unwrap_err();
    assert!(
        err.to_string().contains("belongs to no manifest entry"),
        "{err}"
    );
}

#[test]
fn refuses_a_corpus_without_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let m = fixture(&corpus);
    std::fs::remove_file(corpus.join("coverage/COVERAGE.md")).unwrap();
    let err = pack(&corpus, &m, &dir.path().join("out")).unwrap_err();
    assert!(err.to_string().contains("coverage/COVERAGE.md"), "{err}");
}
