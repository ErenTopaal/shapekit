//! `kind = ordering` vectors: liquidity pool asset pairs and ScMap key sets.
//!
//! Asset order is computed by an implementation of core's rule (R5): `isPoolShareAssetValid`
//! requires `assetA < assetB`, and that `<` is the xdrpp operator generated for the Asset
//! union, which compares the discriminant, then the arm's fields in order, with opaque arrays
//! compared as unsigned bytes (xdrpp `xarray<uint8_t>` is a `std::array`). Every pair is also
//! checked against stellar-xdr's derived `Ord`, which follows the same field order, and the
//! py-stellar-base#1246 pair against its real testnet pool id.
//!
//! ScMap order comes from soroban-env-host 30.0.0 itself: keys are sorted with its
//! `Compare<ScVal>` and the result is fed back through the host's own ScVal to host-object
//! conversion, which refuses unsorted maps.

use std::cmp::Ordering;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use shapekit_manifest::{EntryKind, sha256_hex};
use soroban_env_host::budget::Budget;
use soroban_env_host::{Compare, Host, TryFromVal, Val};
use stellar_xdr as x;

use crate::synth::{LINK_DOTNET_257, LINK_JS_1766, LINK_PY_1246, Spec, VectorItem, repr};
use crate::synth_xdr::{enc, key};

/// The real pool of py-stellar-base#1246 on testnet (R14).
pub const PY_1246_POOL_ID: &str =
    "c2a730127d006897b021d6fcb279da68b4f65f9b6a408c74eb2cdd85ea862d5e";
pub const R14_ZERO_ISSUER: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
pub const R14_COLON_ISSUER: &str = "GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ";
const PY_1246_A: &str = "GDRQL6EF772CANMYX55HXMU463OLAX46ZRCPCWS4KVL66ZS4BYA7OYDL";
const PY_1246_B: &str = "GD32JQHFWLIROCNQ564GLCB2M7T4GMQAP5UKHMVT64OQISWL2LHYK2HO";
const DOTNET_257_A: &str = "GBVGV5HAF2I34PET27RK5KKBMTK2SH2O365ACCDMBSS45KWPGW2UB6C5";
const DOTNET_257_B: &str = "GB5Z5P4R6AA7HM3BXWFTNPJILIF5LCMV7KABR2GWXRSBSYY2QMQL3K7W";

/// `LIQUIDITY_POOL_FEE_V18`, the only fee core accepts.
pub const POOL_FEE: i32 = 30;

pub fn items() -> Result<Vec<VectorItem>> {
    let mut out = asset_pairs()?;
    out.extend(map_sets()?);
    Ok(out)
}

/// Core's asset order: type, then the code bytes with their zero padding, then the issuer's
/// raw 32 key bytes. Never the strkey text.
pub fn core_asset_cmp(a: &x::Asset, b: &x::Asset) -> Ordering {
    fn parts(a: &x::Asset) -> (i32, &[u8], Option<&[u8; 32]>) {
        fn issuer(id: &x::AccountId) -> &[u8; 32] {
            let x::PublicKey::PublicKeyTypeEd25519(k) = &id.0;
            &k.0
        }
        match a {
            x::Asset::Native => (0, &[], None),
            x::Asset::CreditAlphanum4(c) => (1, &c.asset_code.0, Some(issuer(&c.issuer))),
            x::Asset::CreditAlphanum12(c) => (2, &c.asset_code.0, Some(issuer(&c.issuer))),
        }
    }
    let (ta, ca, ia) = parts(a);
    let (tb, cb, ib) = parts(b);
    ta.cmp(&tb).then(ca.cmp(cb)).then(ia.cmp(&ib))
}

/// The pool id core derives: sha256 of the XDR of the constant-product parameters.
pub fn pool_id(a: &x::Asset, b: &x::Asset) -> Result<[u8; 32]> {
    let params = x::LiquidityPoolParameters::LiquidityPoolConstantProduct(
        x::LiquidityPoolConstantProductParameters {
            asset_a: a.clone(),
            asset_b: b.clone(),
            fee: POOL_FEE,
        },
    );
    let hex_id = sha256_hex(&enc(&params)?);
    let mut out = [0u8; 32];
    hex::decode_to_slice(&hex_id, &mut out).context("pool id hex")?;
    Ok(out)
}

fn account_from_strkey(s: &str) -> Result<x::AccountId> {
    let pk = stellar_strkey::ed25519::PublicKey::from_string(s)
        .map_err(|e| anyhow::anyhow!("{s}: {e}"))?;
    Ok(x::AccountId(x::PublicKey::PublicKeyTypeEd25519(
        x::Uint256(pk.0),
    )))
}

fn strkey_of(id: &x::AccountId) -> String {
    let x::PublicKey::PublicKeyTypeEd25519(k) = &id.0;
    stellar_strkey::ed25519::PublicKey(k.0)
        .to_string()
        .to_string()
}

fn raw_account(n: u8) -> x::AccountId {
    x::AccountId(x::PublicKey::PublicKeyTypeEd25519(x::Uint256(key(n))))
}

fn credit(code: &str, issuer: x::AccountId) -> Result<x::Asset> {
    let bytes = code.as_bytes();
    Ok(match bytes.len() {
        1..=4 => {
            let mut c = [0u8; 4];
            c[..bytes.len()].copy_from_slice(bytes);
            x::Asset::CreditAlphanum4(x::AlphaNum4 {
                asset_code: x::AssetCode4(c),
                issuer,
            })
        }
        5..=12 => {
            let mut c = [0u8; 12];
            c[..bytes.len()].copy_from_slice(bytes);
            x::Asset::CreditAlphanum12(x::AlphaNum12 {
                asset_code: x::AssetCode12(c),
                issuer,
            })
        }
        n => anyhow::bail!("asset code {code:?} has {n} bytes"),
    })
}

fn asset_text(a: &x::Asset) -> String {
    let code = |c: &[u8]| {
        String::from_utf8_lossy(c)
            .trim_end_matches('\0')
            .to_string()
    };
    match a {
        x::Asset::Native => "native".into(),
        x::Asset::CreditAlphanum4(c) => {
            format!("{}:{}", code(&c.asset_code.0), strkey_of(&c.issuer))
        }
        x::Asset::CreditAlphanum12(c) => {
            format!("{}:{}", code(&c.asset_code.0), strkey_of(&c.issuer))
        }
    }
}

fn asset_repr(a: &x::Asset) -> Result<Value> {
    let mut v = repr(a)?;
    v["text"] = json!(asset_text(a));
    Ok(v)
}

fn order_name(o: Ordering) -> &'static str {
    match o {
        Ordering::Less => "less",
        Ordering::Equal => "equal",
        Ordering::Greater => "greater",
    }
}

fn pool_case(a: &x::Asset, b: &x::Asset) -> Result<Value> {
    let expected = match core_asset_cmp(a, b) {
        Ordering::Less => {
            let id = pool_id(a, b)?;
            json!({
                "pool_id": hex::encode(id),
                "strkey": stellar_strkey::Strkey::LiquidityPool(stellar_strkey::LiquidityPool(id)).to_string().to_string(),
            })
        }
        Ordering::Equal => json!({ "reject": "assets-equal" }),
        Ordering::Greater => json!({ "reject": "assets-out-of-order" }),
    };
    Ok(json!({
        "op": "pool_id",
        "a": asset_repr(a)?,
        "b": asset_repr(b)?,
        "fee": POOL_FEE,
        "expected": expected,
    }))
}

struct Pair {
    slug: &'static str,
    a: x::Asset,
    b: x::Asset,
    links: &'static [&'static str],
    notes: &'static str,
}

fn asset_pairs() -> Result<Vec<VectorItem>> {
    let zero = account_from_strkey(R14_ZERO_ISSUER)?;
    let colon = account_from_strkey(R14_COLON_ISSUER)?;
    ensure!(
        zero == raw_account(0x00) && colon == raw_account(0x3a),
        "the R14 issuer strkeys are not 32 zero bytes and 32 bytes of 0x3a"
    );
    let py_a = credit("USD", account_from_strkey(PY_1246_A)?)?;
    let py_b = credit("USD", account_from_strkey(PY_1246_B)?)?;
    let (py_lo, py_hi) = if core_asset_cmp(&py_a, &py_b) == Ordering::Less {
        (py_a.clone(), py_b.clone())
    } else {
        (py_b.clone(), py_a.clone())
    };
    let real = hex::encode(pool_id(&py_lo, &py_hi)?);
    ensure!(
        real == PY_1246_POOL_ID,
        "the py-stellar-base#1246 assets in core order hash to pool {real}, not the testnet pool {PY_1246_POOL_ID}"
    );

    let pairs = vec![
        Pair {
            slug: "assets-js1766-issuers",
            a: credit("USD", colon.clone())?,
            b: credit("USD", zero.clone())?,
            links: &[LINK_JS_1766],
            notes: "the js-stellar-sdk#1766 issuers: 32 bytes of 0x3a sort first as strkey text (GA5D... before GAAA...), 32 zero bytes sort first as key bytes, which is what core uses. same code on both sides, so the issuer decides.",
        },
        Pair {
            slug: "assets-py1246-testnet-pool",
            a: py_hi,
            b: py_lo,
            links: &[LINK_PY_1246],
            notes: "the py-stellar-base#1246 pair from testnet pool c2a730127d006897b021d6fcb279da68b4f65f9b6a408c74eb2cdd85ea862d5e (transaction 0126748091930959a3aefac9296e1773b947f964a9b73f37ce856c4df32d346f, ledger 5521432). the pool id computed from the pair in core order equals the real pool id, which the build checks.",
        },
        Pair {
            slug: "assets-dotnet257",
            a: credit("USD", account_from_strkey(DOTNET_257_A)?)?,
            b: credit("USD", account_from_strkey(DOTNET_257_B)?)?,
            links: &[LINK_DOTNET_257],
            notes: "the Beans-BV/dotnet-stellar-sdk#257 issuer pair (the issue is open at the time of writing). the issue names only the issuers; the code USD is the same on both sides so the issuer decides.",
        },
        Pair {
            slug: "assets-type-native",
            a: credit("A", raw_account(0x01))?,
            b: x::Asset::Native,
            links: &[],
            notes: "native against a one-letter alphanum4: the type decides and native is lowest.",
        },
        Pair {
            slug: "assets-type-4-vs-12",
            a: credit("AAAAA", raw_account(0x01))?,
            b: credit("ZZZZ", raw_account(0x01))?,
            links: &[],
            notes: "alphanum12 AAAAA against alphanum4 ZZZZ with the same issuer: the type decides, so ZZZZ is lower although its text sorts after.",
        },
        Pair {
            slug: "assets-code-case",
            a: credit("usd", raw_account(0x00))?,
            b: credit("USD", raw_account(0xff))?,
            links: &[],
            notes: "codes usd and USD: raw bytes put U (0x55) before u (0x75). a case-insensitive or locale comparison calls the codes equal and falls through to the issuer, which points the other way here.",
        },
        Pair {
            slug: "assets-code-padding-length",
            a: credit("ZZ", raw_account(0x01))?,
            b: credit("AAA", raw_account(0x01))?,
            links: &[],
            notes: "alphanum4 ZZ (5a 5a 00 00) against AAA (41 41 41 00): the zero-padded bytes decide, so AAA is lower. comparing code length first gets it backwards.",
        },
        Pair {
            slug: "assets-code-padding-prefix",
            a: credit("ABC", raw_account(0xff))?,
            b: credit("ABCD", raw_account(0x00))?,
            links: &[],
            notes: "alphanum4 ABC against ABCD: the padding byte 00 is below every code character, so the prefix is lower, and the issuers, which point the other way, never matter.",
        },
        Pair {
            slug: "assets-issuer-high-bit",
            a: credit("USD", raw_account(0x80))?,
            b: credit("USD", raw_account(0x7f))?,
            links: &[],
            notes: "issuers of 32 bytes 0x80 and 32 bytes 0x7f: bytes compare unsigned, so 0x7f is lower. a language with signed bytes that compares them as numbers gets it backwards.",
        },
        Pair {
            slug: "assets-equal",
            a: credit("USD", raw_account(0x01))?,
            b: credit("USD", raw_account(0x01))?,
            links: &[],
            notes: "the same asset twice. core requires assetA < assetB strictly, so a pool of one asset with itself is malformed.",
        },
    ];

    let mut out = Vec::new();
    for p in pairs {
        let core = core_asset_cmp(&p.a, &p.b);
        ensure!(
            core == p.a.cmp(&p.b),
            "{}: core's rule says {core:?} but stellar-xdr's derived Ord says {:?}",
            p.slug,
            p.a.cmp(&p.b)
        );
        let text = asset_text(&p.a).cmp(&asset_text(&p.b));
        let mut flags = vec!["asset-pair"];
        if text != core {
            flags.push("strkey-order-differs");
        }
        if let (x::Asset::CreditAlphanum4(a), x::Asset::CreditAlphanum4(b)) = (&p.a, &p.b)
            && a.asset_code == b.asset_code
            && strkey_of(&a.issuer).cmp(&strkey_of(&b.issuer)) != core
        {
            flags.push("issuer-order-differs");
        }
        let cases = vec![
            json!({
                "op": "asset_order",
                "a": asset_repr(&p.a)?,
                "b": asset_repr(&p.b)?,
                "expected": order_name(core),
            }),
            json!({
                "op": "asset_order",
                "a": asset_repr(&p.b)?,
                "b": asset_repr(&p.a)?,
                "expected": order_name(core.reverse()),
            }),
            pool_case(&p.a, &p.b)?,
            pool_case(&p.b, &p.a)?,
        ];
        let spec = Spec::new("cap38.liquidity-pools", p.slug, EntryKind::Ordering)
            .flags(&flags)
            .links(p.links)
            .fields(&["op.change_trust.line.liquidity_pool"])
            .notes(format!(
                "{} expected order computed with core's rule (asset type, code bytes with zero padding, issuer key bytes; isPoolShareAssetValid in stellar-core v30.0.0 src/util/types.cpp and the xdrpp union and xarray comparisons it uses), checked against stellar-xdr's derived Ord. pool_id cases expect the id core derives (sha256 of LiquidityPoolParameters with fee 30) when a < b, and a refusal otherwise, because core answers CHANGE_TRUST_MALFORMED for parameters that are not strictly ordered.",
                p.notes
            ));
        out.push(VectorItem { spec, cases });
    }
    Ok(out)
}

fn sym(s: &str) -> Result<x::ScVal> {
    Ok(x::ScVal::Symbol(x::ScSymbol::try_from(
        s.as_bytes().to_vec(),
    )?))
}

fn bytes(b: &[u8]) -> Result<x::ScVal> {
    Ok(x::ScVal::Bytes(x::ScBytes(b.to_vec().try_into()?)))
}

fn i128v(v: i128) -> x::ScVal {
    x::ScVal::I128(x::Int128Parts {
        hi: (v >> 64) as i64,
        lo: v as u64,
    })
}

fn vec_u32(items: &[u32]) -> Result<x::ScVal> {
    let v: Vec<x::ScVal> = items.iter().map(|&i| x::ScVal::U32(i)).collect();
    Ok(x::ScVal::Vec(Some(x::ScVec(v.try_into()?))))
}

/// Sorts map entries with soroban-env-host's own comparison.
pub fn host_sort(entries: &[x::ScMapEntry]) -> Result<Vec<x::ScMapEntry>> {
    let budget = Budget::default();
    let mut sorted: Vec<x::ScMapEntry> = Vec::with_capacity(entries.len());
    for e in entries {
        let mut at = sorted.len();
        for (i, s) in sorted.iter().enumerate() {
            let o = budget
                .compare(&e.key, &s.key)
                .map_err(|err| anyhow::anyhow!("host comparison failed: {err:?}"))?;
            if o == Ordering::Less {
                at = i;
                break;
            }
        }
        sorted.insert(at, e.clone());
    }
    Ok(sorted)
}

/// Whether soroban-env-host converts the value into a host object, which for maps means the
/// keys are strictly increasing under its comparison.
pub fn host_accepts(value: &x::ScVal) -> bool {
    let host = Host::default();
    Val::try_from_val(&host, value).is_ok()
}

pub fn map_of(entries: Vec<x::ScMapEntry>) -> Result<x::ScVal> {
    Ok(x::ScVal::Map(Some(x::ScMap(entries.try_into()?))))
}

struct MapSet {
    slug: &'static str,
    keys: Vec<x::ScVal>,
    /// The sorted order R14 states, where it states one; checked against the host.
    r14_sorted: Option<Vec<x::ScVal>>,
    notes: &'static str,
}

fn map_sets() -> Result<Vec<VectorItem>> {
    let account = x::ScVal::Address(x::ScAddress::Account(raw_account(0xff)));
    let contract = x::ScVal::Address(x::ScAddress::Contract(x::ContractId(x::Hash(key(0x00)))));
    let sets = vec![
        MapSet {
            slug: "map-keys-bytes",
            keys: vec![bytes(&[10])?, bytes(&[9])?, bytes(&[1, 2])?, bytes(&[2])?],
            r14_sorted: Some(vec![
                bytes(&[1, 2])?,
                bytes(&[2])?,
                bytes(&[9])?,
                bytes(&[10])?,
            ]),
            notes: "the js-stellar-sdk#1766 bytes keys [10], [9], [1,2], [2]. the host compares content bytewise, so [1,2] comes before [2]; the XDR encodings start with the length, so sorting the encoded keys puts [2] first.",
        },
        MapSet {
            slug: "map-keys-symbols",
            keys: vec![sym("b")?, sym("aa")?, sym("a")?],
            r14_sorted: Some(vec![sym("a")?, sym("aa")?, sym("b")?]),
            notes: "the js-stellar-sdk#1766 symbol keys b, aa, a; bytewise content order puts aa between a and b, while the length-prefixed encodings put both one-letter symbols first.",
        },
        MapSet {
            slug: "map-keys-i128",
            keys: vec![i128v(1), i128v(-1), i128v(0), i128v(-200)],
            r14_sorted: Some(vec![i128v(-200), i128v(-1), i128v(0), i128v(1)]),
            notes: "the js-stellar-sdk#1766 i128 keys 1, -1, 0, -200: numeric order. the two's-complement encodings of negative values sort after the positive ones as raw bytes.",
        },
        MapSet {
            slug: "map-keys-mixed-types",
            keys: vec![sym("a")?, x::ScVal::U32(1), x::ScVal::Bool(true)],
            r14_sorted: Some(vec![x::ScVal::Bool(true), x::ScVal::U32(1), sym("a")?]),
            notes: "the js-stellar-sdk#1766 mixed keys: different types order by ScValType discriminant, so bool (0) before u32 (3) before symbol (15).",
        },
        MapSet {
            slug: "map-keys-vecs",
            keys: vec![vec_u32(&[2])?, vec_u32(&[1, 5])?, vec_u32(&[1])?],
            r14_sorted: Some(vec![vec_u32(&[1])?, vec_u32(&[1, 5])?, vec_u32(&[2])?]),
            notes: "vec keys [1], [1,5], [2] of u32: element-wise lexicographic, a prefix before its extensions. length-prefixed encodings put both one-element vecs first.",
        },
        MapSet {
            slug: "map-keys-u64-small-and-object",
            keys: vec![
                x::ScVal::U64(u64::MAX),
                x::ScVal::U64(1),
                x::ScVal::U64(1 << 56),
            ],
            r14_sorted: None,
            notes: "u64 keys 2^64-1, 1 and 2^56: the host keeps small values inline and large ones as objects, but compares by value either way.",
        },
        MapSet {
            slug: "map-keys-addresses",
            keys: vec![contract, account],
            r14_sorted: None,
            notes: "an account address with key bytes ff..ff and a contract address with hash 00..00: ScAddressType decides first (account 0 before contract 1), so the account is lower although its bytes are higher.",
        },
    ];

    let mut out = Vec::new();
    for set in sets {
        let entries: Vec<x::ScMapEntry> = set
            .keys
            .iter()
            .enumerate()
            .map(|(i, k)| x::ScMapEntry {
                key: k.clone(),
                val: x::ScVal::U32(i as u32),
            })
            .collect();
        let sorted = host_sort(&entries)?;
        if let Some(r14) = &set.r14_sorted {
            let got: Vec<&x::ScVal> = sorted.iter().map(|e| &e.key).collect();
            ensure!(
                got == r14.iter().collect::<Vec<_>>(),
                "{}: the host sorts the keys differently from R14",
                set.slug
            );
        }
        let order: Vec<u32> = sorted
            .iter()
            .map(|e| match e.val {
                x::ScVal::U32(i) => Ok(i),
                _ => anyhow::bail!("map values are indices"),
            })
            .collect::<Result<_>>()?;
        let sorted_map = map_of(sorted.clone())?;
        let input_map = map_of(entries.clone())?;
        ensure!(
            host_accepts(&sorted_map),
            "{}: the host refuses the map it sorted itself",
            set.slug
        );
        let input_sorted = order.iter().enumerate().all(|(i, &o)| i as u32 == o);
        ensure!(
            !input_sorted,
            "{}: the input order is already sorted, so it tests nothing",
            set.slug
        );
        ensure!(
            !host_accepts(&input_map),
            "{}: the host accepts the unsorted input map",
            set.slug
        );
        // stellar-xdr's own sorted constructor must agree with the host
        let xdr_sorted = x::ScMap::sorted_from(entries.clone())
            .map_err(|e| anyhow::anyhow!("{}: ScMap::sorted_from: {e}", set.slug))?;
        ensure!(
            xdr_sorted.0.to_vec() == sorted,
            "{}: stellar-xdr ScMap::sorted_from disagrees with the host",
            set.slug
        );
        let mut by_bytes = entries
            .iter()
            .map(|e| Ok((enc(&e.key)?, e.clone())))
            .collect::<Result<Vec<_>>>()?;
        by_bytes.sort_by(|a, b| a.0.cmp(&b.0));
        let by_bytes: Vec<x::ScMapEntry> = by_bytes.into_iter().map(|(_, e)| e).collect();
        let mut flags = vec!["map-key-order"];
        if by_bytes != sorted {
            flags.push("xdr-byte-order-differs");
        }
        let entry_reprs = entries
            .iter()
            .map(|e| Ok(json!({ "key": repr(&e.key)?, "val": repr(&e.val)? })))
            .collect::<Result<Vec<Value>>>()?;
        let cases = vec![
            json!({
                "op": "map_sort",
                "entries": entry_reprs,
                "expected": { "order": order, "map": repr(&sorted_map)? },
            }),
            json!({ "op": "map_validate", "map": repr(&input_map)?, "expected": "reject" }),
            json!({ "op": "map_validate", "map": repr(&sorted_map)?, "expected": "accept" }),
        ];
        let spec = Spec::new("soroban.scval-shapes", set.slug, EntryKind::Ordering)
            .flags(&flags)
            .links(&[LINK_JS_1766])
            .fields(&["op.invoke_host_function.invoke_contract.args"])
            .notes(format!(
                "{} map_sort expects the entry indices in the order soroban-env-host 30.0.0 sorts them (Compare<ScVal> for Budget, src/host/comparison.rs), and the sorted map; map_validate expects the host's verdict on converting the map to a host object, which refuses keys that are not strictly increasing (\"ScMap was not sorted by key for conversion to host object\").",
                set.notes
            ));
        out.push(VectorItem { spec, cases });
    }
    Ok(out)
}
