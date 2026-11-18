//! `oracle run` over a small corpus built in a temp dir from real ledgers.

use std::path::Path;

use shapekit_manifest::build;
use shapekit_manifest::xdr::render_json;
use shapekit_manifest::*;
use shapekit_oracle::corpus::run;
use shapekit_oracle::{OracleFile, decode_lcm};
use stellar_xdr::{LedgerCloseMeta, Limits, TransactionPhase, WriteXdr};

const TESTNET: &str = "Test SDF Network ; September 2015";

fn testnet_lcm(name: &str) -> Vec<u8> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = match name {
        "5694308" => dir.join("../shapekit-manifest/tests/fixtures/testnet-5694308.lcm.xdr"),
        _ => dir.join("tests/fixtures/testnet-4437357.lcm.xdr"),
    };
    std::fs::read(path).unwrap()
}

fn entry(id: &str, xdr_type: &str, file: &str, bytes: &[u8], source: Source) -> Entry {
    Entry {
        obligations: Vec::new(),
        id: id.into(),
        kind: EntryKind::Xdr,
        family: "test.family".into(),
        caps: vec![],
        protocol: EntryProtocol {
            introduced: 1,
            generated_at: Some(30),
            valid_min: 1,
            valid_max: None,
        },
        xdr_type: Some(xdr_type.into()),
        lcm_version: None,
        tx_meta_version: None,
        source,
        synthetic: false,
        passphrase_dependent: true,
        variants: Variants {
            testnet: Some(Variant {
                network_passphrase: None,
                xdr_file: file.into(),
                xdr_sha256: sha256_hex(bytes),
                expected_json_file: None,
                expected_json_sha256: None,
                oracle_file: None,
                oracle_sha256: None,
            }),
            mainnet: None,
            any: None,
        },
        vector: None,
        expected: Expected {
            validity: Validity::Valid,
            decode: DecodeExpectation::MustAccept,
            roundtrip: true,
        },
        flags: vec![],
        fields_exercised: vec![],
        arms: Vec::new(),
        linked_issues: vec![],
        notes: None,
    }
}

fn source(chain: Option<&str>, ledger: Option<u32>) -> Source {
    Source {
        recipe_ir: None,
        class: shapekit_registry::SourceClass::TestnetHistory,
        recipe: None,
        chain: chain.map(str::to_string),
        ledger_seq: ledger,
        tx_hash: None,
        tx_index: None,
        upstream: None,
        provenance_url: None,
        normalization: None,
        core_rejection: None,
    }
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn put_entry(root: &Path, dir: &str, e: &Entry) {
    put(
        root,
        &format!("{dir}/entry.json"),
        render_json(&serde_json::to_value(e).unwrap()).as_bytes(),
    );
}

#[test]
fn run_writes_and_records_oracle_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // a ledger-level entry
    let lcm = testnet_lcm("4437357");
    let d1 = "entries/test.family/test.family.ledger";
    put(root, &format!("{d1}/testnet.xdr"), &lcm);
    put_entry(
        root,
        d1,
        &entry(
            "test.family.ledger",
            "LedgerCloseMeta",
            &format!("{d1}/testnet.xdr"),
            &lcm,
            source(None, Some(4437357)),
        ),
    );

    // a transaction-level entry whose ledger lives in a chain stream
    let chain_lcm = testnet_lcm("5694308");
    let decoded = decode_lcm(&chain_lcm).unwrap();
    let LedgerCloseMeta::V2(m) = &decoded else {
        panic!("fixture is meta v2")
    };
    let stellar_xdr::GeneralizedTransactionSet::V1(set) = &m.tx_set;
    let env = set
        .phases
        .iter()
        .find_map(|p| match p {
            TransactionPhase::V1(par) => Some(par.execution_stages[0].0[0].0[0].clone()),
            _ => None,
        })
        .unwrap();
    let env_bytes = env.to_xdr(Limits::none()).unwrap();
    shapekit_pack::write_sep54(
        [Ok(chain_lcm.clone())],
        TESTNET,
        &root.join("chains/test-chain/testnet/ledgers"),
        shapekit_pack::Layout::default(),
    )
    .unwrap();
    put(
        root,
        "chains/test-chain/testnet/chain.json",
        format!("{{\"passphrase\": \"{TESTNET}\"}}").as_bytes(),
    );
    let d2 = "entries/test.family/test.family.tx";
    put(root, &format!("{d2}/testnet.xdr"), &env_bytes);
    put_entry(
        root,
        d2,
        &entry(
            "test.family.tx",
            "TransactionEnvelope",
            &format!("{d2}/testnet.xdr"),
            &env_bytes,
            source(Some("test-chain"), Some(5694308)),
        ),
    );

    // not eligible: a bare ScVal
    let d3 = "entries/test.family/test.family.scval";
    let scval = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../shapekit-manifest/tests/fixtures/testnet-5694308.scval.xdr"),
    )
    .unwrap();
    put(root, &format!("{d3}/testnet.xdr"), &scval);
    put_entry(
        root,
        d3,
        &entry(
            "test.family.scval",
            "ScVal",
            &format!("{d3}/testnet.xdr"),
            &scval,
            source(None, None),
        ),
    );

    let header = GeneratedWith {
        stellar_xdr: "30.0.0".into(),
        xdr_definitions: XdrDefinitions {
            tag: "v30.0".into(),
            commit: "4f524bb".into(),
        },
        core_image: CoreImage {
            repository: "stellar/stellar-core".into(),
            tag: "30.0.0".into(),
            digest: "sha256:0".into(),
        },
        soroban_env_host: "30.0.0".into(),
    };
    let manifest = build::build(root, "0.0.0", header).unwrap();
    std::fs::write(root.join("manifest.json"), build::render(&manifest)).unwrap();

    let summary = run(root).unwrap();
    assert_eq!(summary.written.len(), 2);
    assert_eq!(summary.not_eligible.len(), 1);
    assert_eq!(summary.not_eligible[0].0, "test.family.scval");

    let read_entry = |d: &str| -> Entry {
        serde_json::from_slice(&std::fs::read(root.join(d).join("entry.json")).unwrap()).unwrap()
    };
    let e1 = read_entry(d1);
    let v1 = e1.variants.testnet.as_ref().unwrap();
    let path1 = v1.oracle_file.clone().unwrap();
    assert_eq!(path1, format!("{d1}/testnet.oracle.json"));
    let text1 = std::fs::read(root.join(&path1)).unwrap();
    assert_eq!(
        v1.oracle_sha256.as_deref(),
        Some(sha256_hex(&text1).as_str())
    );
    let o1: OracleFile = serde_json::from_slice(&text1).unwrap();
    assert_eq!(o1.ledgers[0].ledger_seq, 4437357);
    assert!(o1.ledgers[0].transactions.len() > 1);

    let e2 = read_entry(d2);
    let text2 =
        std::fs::read(root.join(e2.variants.testnet.unwrap().oracle_file.unwrap())).unwrap();
    let o2: OracleFile = serde_json::from_slice(&text2).unwrap();
    assert_eq!(o2.network_passphrase, TESTNET);
    assert_eq!(o2.ledgers[0].transactions.len(), 1);
    let net_id = shapekit_oracle::Network::new(TESTNET).id;
    assert_eq!(
        o2.ledgers[0].transactions[0].hash,
        hex::encode(env.hash(net_id).unwrap())
    );

    // the manifest records the same files
    let m = build::load(&root.join("manifest.json")).unwrap();
    let rec = m
        .entries
        .iter()
        .find(|e| e.id == "test.family.ledger")
        .unwrap();
    assert_eq!(
        rec.variants
            .testnet
            .as_ref()
            .unwrap()
            .oracle_file
            .as_deref(),
        Some(path1.as_str())
    );
    assert!(
        m.entries
            .iter()
            .find(|e| e.id == "test.family.scval")
            .unwrap()
            .variants
            .testnet
            .as_ref()
            .unwrap()
            .oracle_file
            .is_none()
    );

    // a second run changes nothing
    let before = std::fs::read(root.join(d1).join("entry.json")).unwrap();
    run(root).unwrap();
    assert_eq!(
        before,
        std::fs::read(root.join(d1).join("entry.json")).unwrap()
    );
    assert_eq!(text1, std::fs::read(root.join(&path1)).unwrap());
}

/// Every transaction-level entry of a committed chain finds its transaction in the chain's
/// lake. The entries were cut from canonical meta, so reading core's raw stream instead
/// fails on every `-meta` entry whose ledger had a reordered change list.
#[test]
fn committed_chain_entries_match_the_canonical_lake() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let family = "cap40.signed-payload-signer";
    let chain = "cap40.signed-payload-signer.p30";
    copy_tree(
        &repo.join("corpus/chains").join(chain),
        &root.join("chains").join(chain),
    );
    let mut copied = 0;
    for item in std::fs::read_dir(repo.join("corpus/entries").join(family)).unwrap() {
        let dir = item.unwrap().path();
        let text = std::fs::read_to_string(dir.join("entry.json")).unwrap();
        let e: Entry = serde_json::from_str(&text).unwrap();
        if e.source.chain.as_deref() != Some(chain) {
            continue;
        }
        copy_tree(
            &dir,
            &root
                .join("entries")
                .join(family)
                .join(dir.file_name().unwrap()),
        );
        copied += 1;
    }
    assert!(copied > 0);
    let summary = run(root).unwrap();
    let meta: Vec<_> = summary
        .written
        .iter()
        .filter(|(id, _, _)| id.ends_with("-meta"))
        .collect();
    // run() fails outright when a transaction is not found, so reaching here means every
    // transaction-level variant matched
    assert!(!meta.is_empty(), "no -meta entry was interpreted");
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for item in std::fs::read_dir(from).unwrap() {
        let p = item.unwrap().path();
        let dst = to.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_tree(&p, &dst);
        } else {
            std::fs::copy(&p, &dst).unwrap();
        }
    }
}

/// A ledger-level entry goes through the conservation check too: a ledger whose fee does
/// not match its balance changes gets no oracle file.
#[test]
fn run_checks_conservation_for_ledger_entries() {
    let mut lcm = decode_lcm(&testnet_lcm("4437357")).unwrap();
    // the list is rebuilt with the first fee one higher, as a VecM has no mutable access
    macro_rules! bump_first_fee {
        ($m:expr) => {{
            let mut txs = $m.tx_processing.to_vec();
            txs[0].result.result.fee_charged += 1;
            $m.tx_processing = txs.try_into().unwrap();
        }};
    }
    match &mut lcm {
        LedgerCloseMeta::V0(m) => bump_first_fee!(m),
        LedgerCloseMeta::V1(m) => bump_first_fee!(m),
        LedgerCloseMeta::V2(m) => bump_first_fee!(m),
    }
    let bytes = lcm.to_xdr(Limits::none()).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let d = "entries/test.family/test.family.ledger";
    put(root, &format!("{d}/testnet.xdr"), &bytes);
    put_entry(
        root,
        d,
        &entry(
            "test.family.ledger",
            "LedgerCloseMeta",
            &format!("{d}/testnet.xdr"),
            &bytes,
            source(None, Some(4437357)),
        ),
    );
    let e = run(root).unwrap_err().to_string();
    assert!(e.contains("feeCharged"), "{e}");
    assert!(!root.join(format!("{d}/testnet.oracle.json")).exists());
}
