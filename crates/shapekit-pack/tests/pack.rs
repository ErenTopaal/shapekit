//! The committed form against a recorded chain: the checkpoint chain in
//! `tests/fixtures/checkpoints.p30/testnet` (empty ledgers 2 to 127, written by
//! `shapekit gen checkpoints --network testnet --last 127` with the pinned core image) and
//! the corpus chains.

use std::path::{Path, PathBuf};

use shapekit_manifest::framing::frame_records;
use shapekit_pack::archive::{WELL_KNOWN, for_each_member};
use shapekit_pack::chain::{
    ARCHIVE_DIR, CANONICAL_STREAM_FILE, CHAIN_FILE, ChainFile, LEDGERS_DIR, open_lake,
};
use shapekit_pack::lake::CONFIG_FILE;
use shapekit_pack::{Lake, Layout, Packed, archive_at, lake_digest, pack_chain, verify_chain};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/checkpoints.p30/testnet")
}

fn corpus_chains() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/chains");
    let mut out = Vec::new();
    for c in std::fs::read_dir(root).unwrap() {
        for n in std::fs::read_dir(c.unwrap().path()).unwrap() {
            let n = n.unwrap().path();
            if n.join(CHAIN_FILE).is_file() {
                out.push(n);
            }
        }
    }
    out.sort();
    assert!(!out.is_empty());
    out
}

fn ledgers(dir: &Path) -> Vec<Vec<u8>> {
    let chain = ChainFile::read(dir).unwrap();
    open_lake(dir, &chain)
        .unwrap()
        .ledgers()
        .map(|l| l.unwrap().xdr)
        .collect()
}

/// Rebuilds what the recorder writes from a committed chain: the canonical stream, the
/// archive directory and a schema 1 chain.json.
fn unpack(committed: &Path, out: &Path) {
    let chain = ChainFile::read(committed).unwrap();
    std::fs::create_dir_all(out).unwrap();
    let all = ledgers(committed);
    let stream = frame_records(all.iter().map(Vec::as_slice)).unwrap();
    std::fs::write(out.join(CANONICAL_STREAM_FILE), stream).unwrap();
    std::fs::copy(
        committed.join(&chain.stream_file),
        out.join(&chain.stream_file),
    )
    .unwrap();
    let archive = out.join(ARCHIVE_DIR);
    for_each_member(&committed.join(&chain.archive.file), |path, bytes| {
        let p = archive.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
        Ok(())
    })
    .unwrap();
    let mut v = serde_json::to_value(&chain).unwrap();
    let o = v.as_object_mut().unwrap();
    o.insert("schema".into(), 1.into());
    o.remove("lake");
    o.remove("archive");
    o.insert("canonical_stream_file".into(), CANONICAL_STREAM_FILE.into());
    std::fs::write(out.join(CHAIN_FILE), serde_json::to_vec_pretty(&v).unwrap()).unwrap();
}

#[test]
fn committed_chains_verify() {
    let mut dirs = corpus_chains();
    dirs.push(fixture());
    for d in dirs {
        let problems = verify_chain(&d).unwrap();
        assert!(problems.is_empty(), "{}: {problems:?}", d.display());
    }
}

/// Packing the recorder's output again reproduces the committed chain byte for byte, so
/// the lake, the tar and chain.json do not depend on the run or the machine.
#[test]
fn repacking_reproduces_the_committed_bytes() {
    let mut dirs = vec![fixture()];
    dirs.extend(corpus_chains().into_iter().take(4));
    for committed in dirs {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("chain");
        unpack(&committed, &work);
        let chain = ChainFile::read(&committed).unwrap();
        assert_eq!(
            pack_chain(&work, chain.layout().unwrap()).unwrap(),
            Packed::Converted {
                ledgers: chain.ledgers.len(),
                batches: chain.lake.batches.len()
            }
        );
        assert_eq!(
            pack_chain(&work, chain.layout().unwrap()).unwrap(),
            Packed::AlreadyPacked
        );
        for f in [CHAIN_FILE, "archive.tar", "ledgers.xdr"] {
            assert_eq!(
                std::fs::read(work.join(f)).unwrap(),
                std::fs::read(committed.join(f)).unwrap(),
                "{}: {f}",
                committed.display()
            );
        }
        assert_eq!(
            lake_digest(&work.join(LEDGERS_DIR)).unwrap(),
            lake_digest(&committed.join(LEDGERS_DIR)).unwrap()
        );
        assert!(!work.join(CANONICAL_STREAM_FILE).exists());
        assert!(!work.join(ARCHIVE_DIR).exists());
        assert!(verify_chain(&work).unwrap().is_empty());
    }
}

/// Every file under `dir`, relative, sorted: what a failed pack must leave unchanged.
fn listing(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p.clone());
            }
            out.push(p.strip_prefix(dir).unwrap().display().to_string());
        }
    }
    out.sort();
    out
}

/// A pack that fails after writing its outputs (here: chain.json lists a checkpoint the
/// archive lacks, found once the lake and the tar are written) removes them again, so the
/// same command succeeds once the input is right.
#[test]
fn a_failed_pack_cleans_up_and_a_rerun_works() {
    let committed = fixture();
    let chain = ChainFile::read(&committed).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("chain");
    unpack(&committed, &work);
    let good = std::fs::read(work.join(CHAIN_FILE)).unwrap();
    let mut v: serde_json::Value = serde_json::from_slice(&good).unwrap();
    v["checkpoints"].as_array_mut().unwrap().push(9_999.into());
    std::fs::write(
        work.join(CHAIN_FILE),
        serde_json::to_vec_pretty(&v).unwrap(),
    )
    .unwrap();
    let before = listing(&work);

    for _ in 0..2 {
        let e = pack_chain(&work, chain.layout().unwrap()).unwrap_err();
        assert!(e.to_string().contains("checkpoint 9999"), "{e}");
        assert_eq!(listing(&work), before, "a failed pack left files behind");
    }

    std::fs::write(work.join(CHAIN_FILE), &good).unwrap();
    assert!(matches!(
        pack_chain(&work, chain.layout().unwrap()).unwrap(),
        Packed::Converted { .. }
    ));
    assert!(verify_chain(&work).unwrap().is_empty());
    assert_eq!(
        std::fs::read(work.join("archive.tar")).unwrap(),
        std::fs::read(committed.join("archive.tar")).unwrap()
    );
}

/// Outputs nobody's pack left (a schema 1 chain is not packed yet) are never overwritten.
#[test]
fn pack_refuses_foreign_outputs() {
    let committed = fixture();
    let chain = ChainFile::read(&committed).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("chain");
    unpack(&committed, &work);
    std::fs::write(work.join("archive.tar"), b"not mine").unwrap();
    let e = pack_chain(&work, chain.layout().unwrap()).unwrap_err();
    assert!(e.to_string().contains("already exists"), "{e}");
    assert_eq!(
        std::fs::read(work.join("archive.tar")).unwrap(),
        b"not mine"
    );
    assert!(!work.join(LEDGERS_DIR).exists());
    assert!(!work.join(".ledgers.partial").exists());
}

#[test]
fn config_is_what_galexie_writes() {
    let cfg = std::fs::read_to_string(fixture().join(LEDGERS_DIR).join(CONFIG_FILE)).unwrap();
    assert_eq!(
        cfg,
        r#"{"networkPassphrase":"Test SDF Network ; September 2015","version":"1.0","compression":"zstd","ledgersPerBatch":1,"batchesPerPartition":64000}"#
    );
}

/// Batches of 64: the first batch's key covers 0 to 63 but it holds 2 to 63, and its
/// startSequence says so.
#[test]
fn batches_of_64_round_trip() {
    let all = ledgers(&fixture());
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("lake");
    let layout = Layout::new(64, 1).unwrap();
    let summary = shapekit_pack::write_sep54(
        all.iter().cloned().map(Ok),
        "Test SDF Network ; September 2015",
        &dir,
        layout,
    )
    .unwrap();
    let keys: Vec<(&str, u32, u32)> = summary
        .batches
        .iter()
        .map(|b| (b.key.as_str(), b.first, b.last))
        .collect();
    assert_eq!(
        keys,
        vec![
            ("FFFFFFFF--0-63.xdr.zst", 2, 63),
            ("FFFFFFBF--64-127.xdr.zst", 64, 127)
        ]
    );
    let lake = Lake::open(&dir).unwrap();
    assert_eq!(lake.range().unwrap(), (2, 127));
    let first = lake.read_batch(&lake.batches()[0]).unwrap();
    assert_eq!((first.start, first.end), (2, 63));
    let back: Vec<Vec<u8>> = lake.ledgers().map(|l| l.unwrap().xdr).collect();
    assert_eq!(back, all);

    // and with partitions of 8 batches of 2
    let dir2 = tmp.path().join("lake2");
    let s2 = shapekit_pack::write_sep54(
        all.iter().cloned().map(Ok),
        "Test SDF Network ; September 2015",
        &dir2,
        Layout::new(2, 8).unwrap(),
    )
    .unwrap();
    assert_eq!(s2.batches[0].key, "FFFFFFFF--0-15/FFFFFFFD--2-3.xdr.zst");
    assert_eq!(s2.batches[8].key, "FFFFFFEF--16-31/FFFFFFED--18-19.xdr.zst");
    let back2: Vec<Vec<u8>> = Lake::open(&dir2)
        .unwrap()
        .ledgers()
        .map(|l| l.unwrap().xdr)
        .collect();
    assert_eq!(back2, all);
}

#[test]
fn writer_refuses_gaps() {
    let all = ledgers(&fixture());
    let tmp = tempfile::tempdir().unwrap();
    let gappy = vec![Ok(all[0].clone()), Ok(all[2].clone())];
    let err = shapekit_pack::write_sep54(gappy, "x", &tmp.path().join("l"), Layout::default())
        .unwrap_err();
    assert!(err.to_string().contains("consecutive"), "{err}");
}

#[test]
fn verify_finds_a_changed_batch() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("chain");
    unpack(&fixture(), &work);
    pack_chain(&work, Layout::default()).unwrap();
    let chain = ChainFile::read(&work).unwrap();
    let key = &chain.lake.batches[5].key;
    let other = &chain.lake.batches[6].key;
    std::fs::copy(
        work.join(LEDGERS_DIR).join(other),
        work.join(LEDGERS_DIR).join(key),
    )
    .unwrap();
    let err = verify_chain(&work);
    // the swapped object decodes as the wrong ledger, which the reader refuses outright
    match err {
        Ok(problems) => assert!(problems.iter().any(|p| p.contains(key)), "{problems:?}"),
        Err(e) => assert!(e.to_string().contains(key.as_str()), "{e}"),
    }
}

#[test]
fn archive_at_a_checkpoint() {
    let tar = fixture().join("archive.tar");
    let tmp = tempfile::tempdir().unwrap();
    let at63 = tmp.path().join("63");
    let files = archive_at(&tar, 63, &at63).unwrap();
    assert!(files.contains(&"ledger/00/00/00/ledger-0000003f.xdr.gz".to_string()));
    assert!(!files.iter().any(|f| f.contains("0000007f")), "{files:?}");
    assert_eq!(
        std::fs::read(at63.join(WELL_KNOWN)).unwrap(),
        std::fs::read(at63.join("history/00/00/00/history-0000003f.json")).unwrap()
    );
    let has: serde_json::Value =
        serde_json::from_slice(&std::fs::read(at63.join(WELL_KNOWN)).unwrap()).unwrap();
    assert_eq!(has["currentLedger"], 63);
    for b in shapekit_pack::archive::has_buckets(&has) {
        let p = at63.join(format!(
            "bucket/{}/{}/{}/bucket-{b}.xdr.gz",
            &b[0..2],
            &b[2..4],
            &b[4..6]
        ));
        assert!(p.is_file(), "{}", p.display());
    }

    let at127 = tmp.path().join("127");
    let files = archive_at(&tar, 127, &at127).unwrap();
    assert!(files.iter().any(|f| f.contains("0000007f")));

    let err = archive_at(&tar, 64, &tmp.path().join("64")).unwrap_err();
    assert!(
        err.to_string().contains("no state for checkpoint 64"),
        "{err}"
    );
}
