//! adapters/walletbackend/adapter.toml against the cases the go adapter's tests check
//! (adapters/walletbackend/walletbackend_test.go): the mapping must read wallet-backend's rows
//! the way the go mapper does.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use shapekit_mapped::adapter::last_error;
use shapekit_mapped::mapping::Mapping;
use shapekit_mapped::rules::{Lookups, Rules, TxMapping};
use shapekit_mapped::strkey::{encode, version_byte};
use shapekit_mapped::value::{Row, Val};
use shapekit_oracle::network::Network;
use stellar_strkey::Strkey;
use stellar_xdr::{AccountId, AlphaNum4, Asset, AssetCode4, PublicKey, Uint256};

const PASSPHRASE: &str = "Test SDF Network ; September 2015";
const ACCOUNT: &str = "GCCLUSWMBODKXUESRSM35CX545JJAHX2CFYCCN63RSQY5RRHEJCR4DC2";
const SIGNER: &str = "PDDLGS6XTXODPNKK7OP4FHZSNGVJG3OE3GKGMYN47OP3PWM3SSTPCAAAAAQLTTLAOIMJNKCNM54J2UYSG6QYOCGN2XAXHXGHLYGIRPJKRQHFSQH534";
const ISSUER: &str = "GD32JQHFWLIROCNQ564GLCB2M7T4GMQAP5UKHMVT64OQISWL2LHYK2HO";
const OTHER: &str = "GBVGV5HAF2I34PET27RK5KKBMTK2SH2O365ACCDMBSS45KWPGW2UB6C5";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn mapping() -> Mapping {
    Mapping::load(&root().join("adapters/walletbackend/adapter.toml")).unwrap()
}

/// A strkey as wallet-backend's fixed commits store it: version byte, then the payload.
fn stored(s: &str) -> Vec<u8> {
    let (letter, payload): (&str, Vec<u8>) = match Strkey::from_string(s).unwrap() {
        Strkey::PublicKeyEd25519(p) => ("G", p.0.to_vec()),
        Strkey::SignedPayloadEd25519(p) => {
            let mut v = p.ed25519().to_vec();
            v.extend_from_slice(&(p.payload().len() as u32).to_be_bytes());
            v.extend_from_slice(p.payload());
            v.resize(36 + p.payload().len().div_ceil(4) * 4, 0);
            ("P", v)
        }
        _ => unreachable!(),
    };
    let mut out = vec![version_byte(letter).unwrap()];
    out.extend(payload);
    out
}

fn credit(code: &str) -> Asset {
    let key = stellar_strkey::ed25519::PublicKey::from_string(ISSUER).unwrap();
    let mut c = [0u8; 4];
    c[..code.len()].copy_from_slice(code.as_bytes());
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(c),
        issuer: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(key.0))),
    })
}

fn contract_of(a: &Asset) -> Vec<u8> {
    let mut v = vec![version_byte("C").unwrap()];
    v.extend(Network::new(PASSPHRASE).asset_contract_id(a));
    v
}

fn lookups() -> Lookups {
    let net = Network::new(PASSPHRASE);
    let mut m = HashMap::new();
    m.insert(net.asset_contract_id(&Asset::Native), "native".to_owned());
    m.insert(
        net.asset_contract_id(&credit("USD")),
        format!("USD:{ISSUER}"),
    );
    BTreeMap::from([("assets".to_owned(), m)])
}

fn b(v: Vec<u8>) -> Val {
    Val::Bytes(v)
}

fn t(s: &str) -> Val {
    Val::Text(s.to_owned())
}

fn i(v: i64) -> Val {
    Val::Int { v, wide: false }
}

const OP: i64 = (65 << 32) | 1;

fn sc(cat: &str, reason: &str, op: i64, id: i64, cols: Vec<(&str, Val)>) -> Row {
    let mut r = BTreeMap::from([
        ("state_change_category".to_owned(), t(cat)),
        ("state_change_reason".to_owned(), t(reason)),
        ("operation_id".to_owned(), Val::Int { v: op, wide: true }),
        ("state_change_id".to_owned(), Val::Int { v: id, wide: true }),
        (
            "to_id".to_owned(),
            Val::Int {
                v: 65 << 32,
                wide: true,
            },
        ),
    ]);
    for (k, v) in cols {
        r.insert(k.to_owned(), v);
    }
    Row(r)
}

fn map(rows: Vec<Row>) -> (TxMapping, Vec<Value>) {
    let m = mapping();
    let got = Rules::new(&m, lookups()).map_transaction(rows);
    let cs = got
        .changes
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .collect();
    (got, cs)
}

#[test]
fn signer_and_fee() {
    let acct = stored(ACCOUNT);
    let (got, cs) = map(vec![
        sc(
            "SIGNER",
            "ADD",
            OP,
            3,
            vec![
                ("account_id", b(acct.clone())),
                ("signer_account_id", b(stored(SIGNER))),
                ("signer_weight_new", i(1)),
            ],
        ),
        sc(
            "BALANCE",
            "DEBIT",
            0,
            1,
            vec![
                ("account_id", b(acct)),
                ("token_id", b(contract_of(&Asset::Native))),
                ("amount", t("100")),
            ],
        ),
    ]);
    assert!(got.unmapped.is_empty(), "{:?}", got.unmapped);
    assert_eq!(cs.len(), 2);
    let fee = cs.iter().find(|c| c["kind"] == "token-event").unwrap();
    let signer = cs.iter().find(|c| c["kind"] == "signer-added").unwrap();
    assert_eq!(fee["event"], "fee");
    assert_eq!(fee["from"], ACCOUNT);
    assert_eq!(fee["amount"], "100");
    assert_eq!(fee["scope"], "fee-processing");
    assert_eq!(signer["signer"], SIGNER);
    assert_eq!(signer["weight"], 1);
    assert_eq!(signer["op_index"], 0);
    assert_eq!(signer["source"]["category"], "SIGNER");
}

#[test]
fn undecodable_signer_is_null_with_its_bytes() {
    let cut = stored(SIGNER)[..33].to_vec();
    let (got, _) = map(vec![sc(
        "SIGNER",
        "ADD",
        OP,
        3,
        vec![
            ("account_id", b(stored(ACCOUNT))),
            ("signer_account_id", b(cut.clone())),
            ("signer_weight_new", i(1)),
        ],
    )]);
    assert_eq!(got.changes.len(), 1);
    let c = &got.changes[0];
    assert_eq!(c.fields["signer"], Value::Null);
    let raw = &c.raw["signer"];
    assert!(raw.error.is_some());
    assert_eq!(raw.hex, hex::encode(&cut));
}

#[test]
fn transfer_mint_and_burn_pairs() {
    let usd = contract_of(&credit("USD"));
    let (from, to, issuer) = (stored(ACCOUNT), stored(OTHER), stored(ISSUER));
    let row = |reason: &str, id, acct: &Vec<u8>, amount: &str, mux: Option<&str>| {
        let mut cols = vec![
            ("account_id", b(acct.clone())),
            ("token_id", b(usd.clone())),
            ("amount", t(amount)),
        ];
        if let Some(m) = mux {
            cols.push(("to_muxed_id", t(m)));
        }
        sc("BALANCE", reason, OP, id, cols)
    };
    let (got, cs) = map(vec![
        row("DEBIT", 1, &from, "5", None),
        row("CREDIT", 2, &to, "5", Some("7")),
        row("MINT", 3, &issuer, "9", None),
        row("CREDIT", 4, &to, "9", None),
        row("BURN", 5, &issuer, "2", None),
        row("DEBIT", 6, &from, "2", None),
    ]);
    assert!(got.unmapped.is_empty());
    assert_eq!(cs.len(), 3);
    assert_eq!(cs[0]["event"], "transfer");
    assert_eq!(cs[0]["from"], ACCOUNT);
    assert_eq!(cs[0]["to"], OTHER);
    assert_eq!(cs[0]["to_muxed_id"], json!({"type": "u64", "value": "7"}));
    assert_eq!(cs[1]["event"], "mint");
    assert_eq!(cs[1]["to"], OTHER);
    assert_eq!(cs[1]["amount"], "9");
    assert_eq!(cs[2]["event"], "burn");
    assert_eq!(cs[2]["from"], ACCOUNT);
}

// a liquidity-pool leg is one DEBIT row of its own; the next event's DEBIT and CREDIT, with
// the same token and amount, pair with each other and not with it
#[test]
fn balance_pairs_stay_within_one_event() {
    let usd = contract_of(&credit("USD"));
    let row = |reason: &str, id, acct: &str, amount: &str| {
        sc(
            "BALANCE",
            reason,
            OP,
            id,
            vec![
                ("account_id", b(stored(acct))),
                ("token_id", b(usd.clone())),
                ("amount", t(amount)),
            ],
        )
    };
    let (got, cs) = map(vec![
        row("DEBIT", 1, ACCOUNT, "5"),
        row("DEBIT", 2, ISSUER, "5"),
        row("CREDIT", 3, OTHER, "5"),
        // a gap in the ids: not one event, so no pair
        row("DEBIT", 5, ACCOUNT, "1"),
        row("CREDIT", 7, OTHER, "1"),
    ]);
    assert!(got.unmapped.is_empty());
    assert_eq!(cs.len(), 4);
    assert_eq!(cs[0]["from"], ACCOUNT);
    assert!(cs[0].get("to").is_none());
    assert_eq!(cs[1]["from"], ISSUER);
    assert_eq!(cs[1]["to"], OTHER);
    assert!(cs[2].get("to").is_none());
    assert!(cs[3].get("from").is_none());
}

#[test]
fn balance_authorization_holder_neither_g_nor_c_is_unmapped() {
    let usd = contract_of(&credit("USD"));
    let mut muxed = stored(ACCOUNT);
    muxed[0] = version_byte("M").unwrap();
    let mut contract = vec![version_byte("C").unwrap()];
    contract.extend([0u8; 32]);
    let (got, cs) = map(vec![
        sc(
            "BALANCE_AUTHORIZATION",
            "SET",
            OP,
            1,
            vec![("account_id", b(muxed)), ("token_id", b(usd.clone()))],
        ),
        sc(
            "BALANCE_AUTHORIZATION",
            "SET",
            OP,
            2,
            vec![("account_id", b(contract)), ("token_id", b(usd))],
        ),
    ]);
    assert_eq!(got.unmapped.len(), 1);
    assert!(
        got.unmapped[0]
            .note
            .contains("version byte 0x60, neither an account (G) nor a contract (C)"),
        "{}",
        got.unmapped[0].note
    );
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0]["event"], "set_authorized");
    assert_eq!(cs[0]["authorized"], true);
}

#[test]
fn trustline_merges_initial_flags() {
    let usd = contract_of(&credit("USD"));
    let acct = stored(ACCOUNT);
    let (got, cs) = map(vec![
        sc(
            "TRUSTLINE",
            "ADD",
            OP,
            1,
            vec![
                ("account_id", b(acct.clone())),
                ("token_id", b(usd.clone())),
                ("trustline_limit_new", t("922337203685.4775807")),
            ],
        ),
        sc(
            "BALANCE_AUTHORIZATION",
            "SET",
            OP,
            2,
            vec![
                ("account_id", b(acct)),
                ("token_id", b(usd)),
                ("flags", i(1)),
            ],
        ),
    ]);
    assert!(got.unmapped.is_empty());
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0]["kind"], "trustline-created");
    assert_eq!(cs[0]["asset"], format!("USD:{ISSUER}"));
    assert_eq!(cs[0]["limit"], "9223372036854775807");
    assert_eq!(cs[0]["flags"], 1);
}

#[test]
fn flags_and_thresholds_merge_per_account() {
    let acct = stored(ACCOUNT);
    let row = |cat: &str, id| sc(cat, "UPDATE", OP, id, vec![("account_id", b(acct.clone()))]);
    let (got, cs) = map(vec![
        row("SIGNATURE_THRESHOLD", 1),
        row("SIGNATURE_THRESHOLD", 2),
        row("FLAGS", 3),
        row("FLAGS", 4),
    ]);
    assert!(got.unmapped.is_empty());
    let kinds: Vec<&str> = cs.iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, vec!["thresholds", "account-flags"]);
}

#[test]
fn unknown_token_and_blend_rows_are_unmapped() {
    let acct = stored(ACCOUNT);
    let (got, _) = map(vec![
        sc(
            "TRUSTLINE",
            "ADD",
            OP,
            1,
            vec![
                ("account_id", b(acct.clone())),
                ("token_id", b(contract_of(&credit("EUR")))),
                ("trustline_limit_new", t("0.0000001")),
            ],
        ),
        sc(
            "BLEND_SUPPLY",
            "CREDIT",
            OP,
            2,
            vec![("account_id", b(acct.clone()))],
        ),
    ]);
    assert!(got.changes.is_empty());
    assert_eq!(got.unmapped.len(), 2);
    assert_eq!(got.unmapped[0].row["account_id"], hex::encode(&acct));
    assert!(got.unmapped[1].note.starts_with("no vocabulary kind"));
}

#[test]
fn pool_share_asset() {
    let (got, _) = map(vec![sc(
        "TRUSTLINE",
        "REMOVE",
        OP,
        1,
        vec![
            ("account_id", b(stored(ACCOUNT))),
            (
                "liquidity_pool_id",
                t("79d95b12eb2e5c9bc625aa5e059360ec35a1be8a7a4a7fdb90a04c8e5f814ba4"),
            ),
        ],
    )]);
    assert_eq!(got.changes.len(), 1);
    assert_eq!(
        got.changes[0].fields["asset"],
        "pool-share:LB45SWYS5MXFZG6GEWVF4BMTMDWDLIN6RJ5EU763SCQEZDS7QFF2IPOF"
    );
}

#[test]
fn data_entry_and_home_domain() {
    let acct = stored(ACCOUNT);
    let (got, cs) = map(vec![
        sc(
            "DATA_ENTRY",
            "UPDATE",
            OP,
            1,
            vec![
                ("account_id", b(acct.clone())),
                ("data_entry_name", t("k")),
                (
                    "key_value",
                    Val::Json(json!({"old": "AQI=", "new": "AwQ="})),
                ),
            ],
        ),
        sc(
            "HOME_DOMAIN",
            "SET",
            OP,
            2,
            vec![
                ("account_id", b(acct)),
                ("key_value", Val::Json(json!({"new": "example.com"}))),
            ],
        ),
    ]);
    assert!(got.unmapped.is_empty());
    assert_eq!(cs[0]["name_hex"], "6b");
    assert_eq!(cs[0]["action"], "updated");
    assert_eq!(cs[0]["value_before_hex"], "0102");
    assert_eq!(cs[0]["value_after_hex"], "0304");
    assert_eq!(cs[1]["before"], "");
    assert_eq!(cs[1]["after"], "example.com");
}

#[test]
fn last_error_picks_ingestion_failures_only() {
    let m = mapping();
    let logs = [
        r#"time="x" level=info msg="Ingested ledger 3 in 0.07s" pid=1"#,
        r#"time="x" level=warning msg="blend: price snapshot pass failed: instance entry not found" pid=1"#,
        r#"time="x" level=warning msg="Error ingesting data for ledger 4 (attempt 1/5): COPY from stdin failed" pid=1"#,
        r#"time="x" level=info msg="Shutting down servers..." pid=1"#,
    ]
    .join("\n");
    let got = last_error(&m.status.error, &logs).unwrap();
    assert!(got.contains("Error ingesting data for ledger 4"));
    let got = last_error(
        &m.status.error,
        &format!("{logs}\npanic: executing root command: running ingest"),
    )
    .unwrap();
    assert!(got.starts_with("panic: "));
    assert_eq!(
        last_error(
            &m.status.error,
            r#"time="x" level=warning msg="blend: price snapshot pass failed" pid=1"#
        ),
        None
    );
    let crash = regex::Regex::new(m.status.crash.as_deref().unwrap()).unwrap();
    assert!(!crash.is_match("panic: executing root command"));
    assert!(crash.is_match("x\npanic: runtime error: index out of range"));
}

/// Every image the mapping names, and every pinned base, is the one pins.toml verifies.
#[test]
fn images_match_pins() {
    let pins = std::fs::read_to_string(root().join("pins.toml")).unwrap();
    let m = mapping();
    let mut refs: Vec<(String, String)> = m
        .services
        .iter()
        .map(|s| {
            let (rt, _) = s.image.split_once('@').unwrap();
            (rt.split_once(':').unwrap().0.to_owned(), s.image.clone())
        })
        .collect();
    for (from, pinned) in &m.image.bases {
        let repo = from.split_once(':').unwrap().0;
        let repo = if repo.contains('/') {
            repo.to_owned()
        } else {
            format!("library/{repo}")
        };
        let (r, _) = pinned.split_once('@').unwrap();
        assert_eq!(r, format!("docker.io/{repo}"), "{from}");
        let tag = from.split_once(':').unwrap().1;
        let digest = pinned.split_once('@').unwrap().1;
        let entry = format!("repository = \"{repo}\"\ntag = \"{tag}\"\ndigest = \"{digest}\"");
        assert!(
            pins.contains(&entry),
            "pins.toml has no entry with\n{entry}"
        );
    }
    for (repo, image) in refs.drain(..) {
        let (rt, digest) = image.split_once('@').unwrap();
        let tag = rt.split_once(':').unwrap().1;
        for want in [
            format!("repository = \"{repo}\""),
            format!("tag = \"{tag}\""),
            format!("digest = \"{digest}\""),
        ] {
            assert!(pins.contains(&want), "pins.toml has no {want} for {image}");
        }
    }
    // the go adapter builds on the same bases, so both reuse one image per commit
    let go = std::fs::read_to_string(root().join("adapters/walletbackend/docker.go")).unwrap();
    for (from, pinned) in &m.image.bases {
        let re = regex::Regex::new(&format!(
            "\"{}\":\\s*\"{}\"",
            regex::escape(from),
            regex::escape(pinned)
        ))
        .unwrap();
        assert!(re.is_match(&go), "docker.go pins {from} otherwise");
    }
    assert_eq!(encode(version_byte("G").unwrap(), &[0; 32]).len(), 56);
}
