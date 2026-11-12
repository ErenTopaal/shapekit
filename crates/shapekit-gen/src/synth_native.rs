//! `kind = native-projection` vectors: ScMaps whose keys are distinct ScVals but print the same
//! once converted to a language's native map or object (js-stellar-sdk#1789).
//!
//! Every map here is one the host can hold: keys are sorted with soroban-env-host's comparison
//! and the host converts the map without error, so a contract can emit it in an event or a
//! return value. The rule for a testee is that no entry may be silently lost: returning an
//! error passes, and so does a lossless structure that keeps every entry with its key type;
//! returning fewer entries than the map holds fails.

use anyhow::{Result, ensure};
use serde_json::json;
use shapekit_manifest::EntryKind;
use stellar_xdr as x;

use crate::synth::{LINK_JS_1789, Spec, VectorItem, repr};
use crate::synth_order::{host_accepts, host_sort, map_of};
use crate::synth_xdr::key;

fn sym(s: &str) -> Result<x::ScVal> {
    Ok(x::ScVal::Symbol(x::ScSymbol::try_from(
        s.as_bytes().to_vec(),
    )?))
}

fn string(s: &str) -> Result<x::ScVal> {
    Ok(x::ScVal::String(x::ScString(
        s.as_bytes().to_vec().try_into()?,
    )))
}

struct Set {
    slug: &'static str,
    /// (key, value) in input order; the build sorts them the way the host does.
    entries: Vec<(x::ScVal, x::ScVal)>,
    collides_as: String,
    notes: &'static str,
}

fn sets() -> Result<Vec<Set>> {
    let account = stellar_strkey::ed25519::PublicKey(key(0x3a))
        .to_string()
        .to_string();
    let one = |n: u32| x::ScVal::U32(n);
    Ok(vec![
        Set {
            slug: "map-js1789-u32-symbol",
            entries: vec![(x::ScVal::U32(1), one(10)), (sym("1")?, one(20))],
            collides_as: "1".into(),
            notes: "the js-stellar-sdk#1789 map literally: {u32 1: u32 10, symbol \"1\": u32 20}. scValToNative in 17.2.1 returned { \"1\": 20 }.",
        },
        Set {
            slug: "map-u32-string-symbol",
            entries: vec![
                (x::ScVal::U32(1), one(10)),
                (string("1")?, one(20)),
                (sym("1")?, one(30)),
            ],
            collides_as: "1".into(),
            notes: "u32 1, string \"1\" and symbol \"1\" in one map.",
        },
        Set {
            slug: "map-numeric-types",
            entries: vec![
                (x::ScVal::U32(1), one(1)),
                (x::ScVal::I32(1), one(2)),
                (x::ScVal::U64(1), one(3)),
                (x::ScVal::I64(1), one(4)),
                (x::ScVal::Timepoint(x::TimePoint(1)), one(5)),
                (x::ScVal::Duration(x::Duration(1)), one(6)),
                (x::ScVal::U128(x::UInt128Parts { hi: 0, lo: 1 }), one(7)),
                (x::ScVal::I128(x::Int128Parts { hi: 0, lo: 1 }), one(8)),
                (
                    x::ScVal::U256(x::UInt256Parts {
                        hi_hi: 0,
                        hi_lo: 0,
                        lo_hi: 0,
                        lo_lo: 1,
                    }),
                    one(9),
                ),
                (
                    x::ScVal::I256(x::Int256Parts {
                        hi_hi: 0,
                        hi_lo: 0,
                        lo_hi: 0,
                        lo_lo: 1,
                    }),
                    one(10),
                ),
            ],
            collides_as: "1".into(),
            notes: "the value 1 under every integer type, timepoint and duration: ten distinct keys that all project to the number or string 1.",
        },
        Set {
            slug: "map-bool-and-text",
            entries: vec![
                (x::ScVal::Bool(true), one(10)),
                (string("true")?, one(20)),
                (sym("true")?, one(30)),
            ],
            collides_as: "true".into(),
            notes: "bool true, string \"true\" and symbol \"true\".",
        },
        Set {
            slug: "map-address-and-string",
            entries: vec![
                (
                    x::ScVal::Address(x::ScAddress::Account(x::AccountId(
                        x::PublicKey::PublicKeyTypeEd25519(x::Uint256(key(0x3a))),
                    ))),
                    one(10),
                ),
                (string(&account)?, one(20)),
            ],
            collides_as: account.clone(),
            notes: "an account address and a string holding the same account's strkey.",
        },
        Set {
            slug: "map-bytes-and-string",
            entries: vec![
                (x::ScVal::Bytes(x::ScBytes(vec![0x31].try_into()?)), one(10)),
                (string("1")?, one(20)),
            ],
            collides_as: "1".into(),
            notes: "bytes [0x31] and string \"1\": a projection that turns bytes into a buffer and then into text keys them both as 1.",
        },
        Set {
            slug: "map-vec-and-string",
            entries: vec![
                (
                    x::ScVal::Vec(Some(x::ScVec(vec![x::ScVal::U32(1)].try_into()?))),
                    one(10),
                ),
                (string("1")?, one(20)),
            ],
            collides_as: "1".into(),
            notes: "vec [u32 1] and string \"1\": an array key stringifies to its single element in some languages.",
        },
        Set {
            slug: "map-void-and-text",
            entries: vec![
                (x::ScVal::Void, one(10)),
                (string("null")?, one(20)),
                (sym("null")?, one(30)),
            ],
            collides_as: "null".into(),
            notes: "void, string \"null\" and symbol \"null\": void projects to null, which stringifies to null as a key.",
        },
    ])
}

fn type_name(v: &x::ScVal) -> &'static str {
    v.discriminant().name()
}

pub fn items() -> Result<Vec<VectorItem>> {
    let mut out = Vec::new();
    for set in sets()? {
        let entries: Vec<x::ScMapEntry> = set
            .entries
            .into_iter()
            .map(|(key, val)| x::ScMapEntry { key, val })
            .collect();
        let sorted = host_sort(&entries)?;
        let map = map_of(sorted.clone())?;
        ensure!(
            host_accepts(&map),
            "{}: soroban-env-host refuses the map, so it is not one a contract can emit",
            set.slug
        );
        let keys = sorted
            .iter()
            .map(|e| Ok(json!({ "type": type_name(&e.key), "key": repr(&e.key)? })))
            .collect::<Result<Vec<_>>>()?;
        let cases = vec![json!({
            "op": "to_native",
            "value": repr(&map)?,
            "expected": {
                "rule": "no-entry-lost",
                "entry_count": sorted.len(),
                "keys": keys,
                "collides_as": set.collides_as,
            },
        })];
        let spec = Spec::new("soroban.scval-shapes", set.slug, EntryKind::NativeProjection)
            .flags(&["map-key-collision-after-native"])
            .links(&[LINK_JS_1789])
            .fields(&["op.invoke_host_function.invoke_contract.args"])
            .notes(format!(
                "{} keys are sorted the way soroban-env-host 30.0.0 sorts them and the host converts the map, so a contract can emit it. rule no-entry-lost: an error passes, a structure keeping all {} entries with their key types passes, fewer entries fails.",
                set.notes,
                sorted.len()
            ));
        out.push(VectorItem { spec, cases });
    }
    Ok(out)
}
