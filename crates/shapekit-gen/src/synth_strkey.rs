//! `kind = strkey` vectors: valid and invalid strings for every SEP-23 prefix.
//!
//! Invalid strings are crafted at the binary level (a wrong checksum, a short or long payload,
//! unused trailing bits set, a bad version byte) with a local base32 and CRC16-XMODEM encoder,
//! because a conforming encoder cannot produce them. Every expectation is the verdict of
//! stellar-strkey 1.0.0; the build fails when that verdict disagrees with the intent a case was
//! crafted for, or with the verdict SEP-23 1.4.0 lists for its own test strings.

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use shapekit_manifest::{EntryKind, Validity};
use stellar_strkey::{DecodeError, Strkey, ed25519};

use crate::synth::{Spec, VectorItem};

/// SEP-23 1.4.0 (stellar-protocol at the commit pinned in pins.toml), section Tests.
pub const SEP23_VALID: &[&str] = &[
    "GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVSGZ",
    "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAACJUQ",
    "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVAAAAAAAAAAAAAJLK",
    "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAQACAQDAQCQMBYIBEFAWDANBYHRAEISCMKBKFQXDAMRUGY4DUPB6IBZGM",
    "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAOQCAQDAQCQMBYIBEFAWDANBYHRAEISCMKBKFQXDAMRUGY4DUAAAAFGBU",
    "CA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUWDA",
    "LA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUPJN",
    "BAAD6DBUX6J22DMZOHIEZTEQ64CVCHEDRKWZONFEUL5Q26QD7R76RGR4TU",
    "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAAWWC",
    "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVAAAAAAAAAAAACWJY",
];

/// SEP-23 1.4.0, the invalid test cases in the order the SEP lists them.
pub const SEP23_INVALID: &[&str] = &[
    "GAAAAAAAACGC6",
    "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAACJUR",
    "GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVSGZA",
    "GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUACUSI",
    "G47QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVP2I",
    "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVAAAAAAAAAAAAAJLKA",
    "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVAAAAAAAAAAAAAAV75I",
    "M47QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAACJUQ",
    "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAACJUK===",
    "MA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAACJUO",
    "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAQACAQDAQCQMBYIBEFAWDANBYHRAEISCMKBKFQXDAMRUGY4DUPB6IAAAAAAAAPM",
    "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAOQCAQDAQCQMBYIBEFAWDANBYHRAEISCMKBKFQXDAMRUGY4Z2PQ",
    "PA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAOQCAQDAQCQMBYIBEFAWDANBYHRAEISCMKBKFQXDAMRUGY4DXFH6",
    "BAAD6DBUX6J22DMZOHIEZTEQ64CVCHEDRKWZONFEUL5Q26QD7R76RGR4TV",
    "BAAT6DBUX6J22DMZOHIEZTEQ64CVCHEDRKWZONFEUL5Q26QD7R76RGXACA",
    "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAAWWD",
    "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAAWWCA",
    "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAAAIOUI",
    "W47QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAADXHW",
    "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAAWWA",
];

/// The SEP-23 sample key, `GA7QYNF7...`.
const SEP_KEY: [u8; 32] = [
    0x3f, 0x0c, 0x34, 0xbf, 0x93, 0xad, 0x0d, 0x99, 0x71, 0xd0, 0x4c, 0xcc, 0x90, 0xf7, 0x05, 0x51,
    0x1c, 0x83, 0x8a, 0xad, 0x97, 0x34, 0xa4, 0xa2, 0xfb, 0x0d, 0x7a, 0x03, 0xfc, 0x7f, 0xe8, 0x9a,
];

const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// CRC16-XMODEM, the SEP-23 checksum, little-endian on the wire.
pub fn crc16(data: &[u8]) -> [u8; 2] {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc.to_le_bytes()
}

/// RFC 4648 base32 without padding; `extra_bits` is OR-ed into the unused low bits of the last
/// symbol, which a canonical encoder leaves zero.
pub fn base32(data: &[u8], extra_bits: u8) -> String {
    let mut out = String::new();
    let mut acc: u32 = 0;
    let mut bits = 0;
    for &b in data {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(BASE32[((acc >> bits) & 31) as usize] as char);
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        let unused = 5 - bits;
        let sym = ((acc << unused) as u8 | (extra_bits & ((1 << unused) - 1))) & 31;
        out.push(BASE32[sym as usize] as char);
    }
    out
}

/// Number of unused low bits in the last symbol of a strkey whose binary is `len` bytes.
pub fn unused_bits(len: usize) -> u32 {
    let rem = (len * 8) % 5;
    if rem == 0 { 0 } else { 5 - rem as u32 }
}

fn strkey(version: u8, payload: &[u8]) -> String {
    raw(version, payload, |_| {}, 0)
}

fn raw(version: u8, payload: &[u8], tweak_crc: impl Fn(&mut [u8; 2]), extra_bits: u8) -> String {
    let mut bin = vec![version];
    bin.extend_from_slice(payload);
    let mut crc = crc16(&bin);
    tweak_crc(&mut crc);
    bin.extend_from_slice(&crc);
    base32(&bin, extra_bits)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prefix {
    G,
    M,
    S,
    T,
    X,
    P,
    C,
    L,
    B,
    W,
}

impl Prefix {
    const ALL: [Prefix; 10] = [
        Prefix::G,
        Prefix::M,
        Prefix::S,
        Prefix::T,
        Prefix::X,
        Prefix::P,
        Prefix::C,
        Prefix::L,
        Prefix::B,
        Prefix::W,
    ];

    fn letter(self) -> char {
        match self {
            Prefix::G => 'G',
            Prefix::M => 'M',
            Prefix::S => 'S',
            Prefix::T => 'T',
            Prefix::X => 'X',
            Prefix::P => 'P',
            Prefix::C => 'C',
            Prefix::L => 'L',
            Prefix::B => 'B',
            Prefix::W => 'W',
        }
    }

    /// The SEP-23 version byte (R6).
    fn version(self) -> u8 {
        (match self {
            Prefix::G => 6,
            Prefix::M => 12,
            Prefix::S => 18,
            Prefix::T => 19,
            Prefix::X => 23,
            Prefix::P => 15,
            Prefix::C => 2,
            Prefix::L => 11,
            Prefix::B => 1,
            Prefix::W => 22,
        }) << 3
    }

    fn kind(self) -> &'static str {
        match self {
            Prefix::G => "public_key_ed25519",
            Prefix::M => "muxed_account_ed25519",
            Prefix::S => "private_key_ed25519",
            Prefix::T => "pre_auth_tx",
            Prefix::X => "hash_x",
            Prefix::P => "signed_payload_ed25519",
            Prefix::C => "contract",
            Prefix::L => "liquidity_pool",
            Prefix::B => "claimable_balance",
            Prefix::W => "muxed_contract",
        }
    }

    fn family(self) -> &'static str {
        match self {
            Prefix::G | Prefix::M | Prefix::S => "cap27.muxed-accounts",
            Prefix::T | Prefix::X | Prefix::P => "cap40.signed-payload-signer",
            Prefix::C => "soroban.invoke-basics",
            Prefix::L => "cap38.liquidity-pools",
            Prefix::B => "cap23.claimable-balances",
            Prefix::W => "cap84.muxed-contract",
        }
    }

    fn flags(self) -> &'static [&'static str] {
        match self {
            Prefix::M => &["muxed-account"],
            Prefix::W => &["muxed-contract"],
            Prefix::P => &["signed-payload"],
            _ => &[],
        }
    }

    fn what(self) -> &'static str {
        match self {
            Prefix::G => "ed25519 account public keys (G, 6 << 3)",
            Prefix::M => {
                "muxed accounts (M, 12 << 3, CAP-27): 32 key bytes then a big-endian u64 id"
            }
            Prefix::S => {
                "ed25519 secret seeds (S, 18 << 3); every seed here is a made-up byte pattern"
            }
            Prefix::T => "pre-authorized transaction hashes (T, 19 << 3)",
            Prefix::X => "hash-x signers (X, 23 << 3)",
            Prefix::P => {
                "ed25519 signed-payload signers (P, 15 << 3, CAP-40): 32 key bytes, a u32 length, 1 to 64 payload bytes and zero padding to a multiple of four"
            }
            Prefix::C => "contract ids (C, 2 << 3)",
            Prefix::L => "liquidity pool ids (L, 11 << 3)",
            Prefix::B => {
                "claimable balance ids (B, 1 << 3): a version byte (0 for v0) and 32 hash bytes"
            }
            Prefix::W => {
                "muxed contracts (W, 22 << 3, CAP-84): 32 contract id bytes then a big-endian u64 id"
            }
        }
    }
}

/// Payload bytes for a signed payload: key, length, payload, zero padding.
fn signed_payload(key: [u8; 32], payload: &[u8], declared_len: u32, pad: &[u8]) -> Vec<u8> {
    let mut out = key.to_vec();
    out.extend_from_slice(&declared_len.to_be_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(pad);
    out
}

fn padding(len: usize) -> Vec<u8> {
    vec![0; (4 - len % 4) % 4]
}

fn with_id(key: [u8; 32], id: u64) -> Vec<u8> {
    let mut out = key.to_vec();
    out.extend_from_slice(&id.to_be_bytes());
    out
}

fn counting(n: usize) -> Vec<u8> {
    (1..=n).map(|i| i as u8).collect()
}

/// The canonical payload of each valid case of a prefix, with a label.
fn valid_payloads(p: Prefix) -> Vec<(String, Vec<u8>)> {
    let keys = [
        ("zero key", [0u8; 32]),
        ("0x3a key", [0x3a; 32]),
        ("sep-23 sample key", SEP_KEY),
        ("0xff key", [0xff; 32]),
    ];
    let ids: [(&str, u64); 5] = [
        ("id 0", 0),
        ("id 1", 1),
        ("id 2^53", 1 << 53),
        ("id 2^63", 1 << 63),
        ("id 2^64-1", u64::MAX),
    ];
    match p {
        Prefix::G | Prefix::S | Prefix::T | Prefix::X | Prefix::C | Prefix::L => keys
            .iter()
            .map(|(l, k)| (l.to_string(), k.to_vec()))
            .collect(),
        Prefix::M | Prefix::W => ids
            .iter()
            .map(|(l, id)| (format!("sep-23 sample key, {l}"), with_id(SEP_KEY, *id)))
            .collect(),
        Prefix::B => keys
            .iter()
            .map(|(l, k)| {
                let mut v = vec![0u8];
                v.extend_from_slice(k);
                (format!("v0, {l}"), v)
            })
            .collect(),
        Prefix::P => [1usize, 3, 4, 29, 32, 63, 64]
            .iter()
            .map(|&n| {
                let body = counting(n);
                (
                    format!("{n}-byte payload"),
                    signed_payload(SEP_KEY, &body, n as u32, &padding(n)),
                )
            })
            .collect(),
    }
}

/// A crafted invalid string and why it is invalid.
struct Bad {
    text: String,
    why: String,
}

fn bad(text: String, why: impl Into<String>) -> Bad {
    Bad {
        text,
        why: why.into(),
    }
}

fn invalid_strings(p: Prefix) -> Vec<Bad> {
    let v = p.version();
    let base = valid_payloads(p).remove(0).1;
    let mut out = vec![
        bad(raw(v, &base, |c| c[0] ^= 1, 0), "checksum off by one bit"),
        bad(
            raw(v, &base, |c| c.swap(0, 1), 0),
            "checksum bytes in big-endian order",
        ),
        bad(
            strkey(v, &base[..base.len() - 1]),
            "payload one byte short, with a correct checksum",
        ),
        bad(
            strkey(v, &[base.as_slice(), &[0]].concat()),
            "payload one byte long, with a correct checksum",
        ),
        bad(format!("{}A", strkey(v, &base)), "one extra base32 symbol"),
        bad(strkey(v, &base).to_ascii_lowercase(), "lowercase base32"),
        bad(format!("{}=", strkey(v, &base)), "a trailing = pad"),
        bad(format!(" {} ", strkey(v, &base)), "surrounding spaces"),
        bad(
            strkey(v | 7, &base),
            "algorithm bits of the version byte set to 7",
        ),
        bad(
            strkey(5 << 3, &base),
            "version byte 5 << 3, which no strkey type uses",
        ),
        bad(String::new(), "the empty string"),
        bad(p.letter().to_string(), "the prefix letter alone"),
    ];
    let bin_len = base.len() + 3;
    if unused_bits(bin_len) > 0 {
        out.push(bad(
            raw(v, &base, |_| {}, 1),
            format!(
                "the lowest of the {} unused trailing bits of the last symbol set",
                unused_bits(bin_len)
            ),
        ));
    }
    match p {
        Prefix::M | Prefix::W => {
            out.push(bad(strkey(v, &SEP_KEY), "32 bytes without the 8-byte id"));
        }
        Prefix::G => {
            out.push(bad(
                strkey(Prefix::M.version(), &SEP_KEY),
                "a G payload under the M version byte",
            ));
        }
        Prefix::B => {
            let mut v1 = base.clone();
            v1[0] = 1;
            out.push(bad(strkey(v, &v1), "claimable balance sub-version 1"));
        }
        Prefix::P => {
            out.push(bad(
                strkey(v, &signed_payload(SEP_KEY, &[], 0, &[])),
                "an empty payload: CAP-40 allows 0 to 64 bytes in XDR, but core answers SET_OPTIONS_BAD_SIGNER for an empty one",
            ));
            let body = counting(65);
            out.push(bad(
                strkey(v, &signed_payload(SEP_KEY, &body, 65, &padding(65))),
                "a 65-byte payload, one past opaque payload<64>",
            ));
            let body = counting(5);
            out.push(bad(
                strkey(v, &signed_payload(SEP_KEY, &body, 4, &padding(5))),
                "length prefix 4 with 5 payload bytes",
            ));
            out.push(bad(
                strkey(v, &signed_payload(SEP_KEY, &body, 9, &padding(5))),
                "length prefix 9 with 5 payload bytes and 3 padding bytes, longer than everything after it",
            ));
            out.push(bad(
                strkey(v, &signed_payload(SEP_KEY, &body, 5, &[0, 0, 1])),
                "non-zero padding after a 5-byte payload",
            ));
            out.push(bad(
                strkey(v, &signed_payload(SEP_KEY, &body, 5, &[])),
                "a 5-byte payload without its padding",
            ));
        }
        _ => {}
    }
    out
}

/// A valid strkey with bytes outside the base32 alphabet inserted, and where.
struct Stray {
    text: Vec<u8>,
    why: String,
}

/// Go's encoding/base32 skips `\r` and `\n` anywhere, and with NoPadding its pad rune is -1, so
/// it reads the byte 0xff as padding near the end. go-stellar-sdk strkey before v0.9.0 checked
/// the canonical length on the raw input, so a valid strkey stretched by enough of these bytes
/// to keep a length that check accepts decoded to the strkey's own payload
/// (go-stellar-sdk#6026). each run is that many bytes; the single-byte insertions are invalid
/// for every decoder and are here so a fix that only counts length is still caught.
fn stray_chars(p: Prefix) -> Vec<Stray> {
    let base = strkey(p.version(), &valid_payloads(p).remove(0).1).into_bytes();
    // the fewest bytes that bring the length back to a multiple of 8 symbols
    let n = 8 - base.len() % 8;
    let mid = base.len() / 2;
    let insert = |at: usize, stray: &[u8]| [&base[..at], stray, &base[at..]].concat();
    let run = |b: &[u8]| b.iter().copied().cycle().take(n).collect::<Vec<u8>>();
    let len = base.len();
    vec![
        Stray {
            text: insert(len, &run(b"\n")),
            why: format!("{n} newlines appended, {} symbols in all", len + n),
        },
        Stray {
            text: insert(0, &run(b"\r")),
            why: format!("{n} carriage returns before the prefix letter"),
        },
        Stray {
            text: insert(mid, &run(b"\r\n")),
            why: format!("{n} bytes of alternating carriage return and newline after symbol {mid}"),
        },
        Stray {
            text: insert(mid, b"\n"),
            why: format!("one newline after symbol {mid}"),
        },
        Stray {
            text: insert(len, &run(&[0xff])),
            why: format!("{n} 0xff bytes appended, which go's unpadded decoder reads as padding"),
        },
        Stray {
            text: insert(mid, &[0xff]),
            why: format!("one 0xff byte after symbol {mid}"),
        },
    ]
}

/// The stray-character strings of one prefix as cases. text that is not utf-8 cannot travel as
/// a json string, so it goes as `text_base64`.
fn stray_cases(p: Prefix) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for s in stray_chars(p) {
        let e = match reference_for(p, &s.text) {
            Ok(v) => bail!(
                "{}: stellar-strkey accepts {:?} ({}) as {v}",
                p.letter(),
                String::from_utf8_lossy(&s.text),
                s.why
            ),
            Err(e) => e,
        };
        let mut case = json!({
            "op": "strkey_decode",
            "note": s.why,
            "expected": { "reject": refusal_name(&e) },
        });
        match std::str::from_utf8(&s.text) {
            Ok(t) => case["text"] = json!(t),
            Err(_) => case["text_base64"] = json!(B64.encode(&s.text)),
        }
        out.push(case);
    }
    Ok(out)
}

/// Why the reference refused a string: stellar-strkey's own error, or a string it decoded
/// as another kind than its prefix letter names, which stellar-strkey never reports itself.
#[derive(Debug)]
enum Refusal {
    Strkey(DecodeError),
    OtherKind(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Strkey(e) => write!(f, "{e}"),
            Refusal::OtherKind(k) => write!(f, "decodes as {k}, not the kind its prefix names"),
        }
    }
}

fn refusal_name(e: &Refusal) -> &'static str {
    match e {
        Refusal::Strkey(e) => decode_error_name(e),
        Refusal::OtherKind(_) => "other-kind",
    }
}

fn decode_error_name(e: &DecodeError) -> &'static str {
    match e {
        DecodeError::InvalidBase32 => "invalid-base32",
        DecodeError::TooShort => "too-short",
        DecodeError::TooLong => "too-long",
        DecodeError::ChecksumMismatch => "checksum-mismatch",
        DecodeError::UnsupportedVersion => "unsupported-version",
        DecodeError::UnsupportedClaimableBalanceVersion => "unsupported-claimable-balance-version",
        DecodeError::InvalidPayloadLength => "invalid-payload-length",
        DecodeError::InvalidPadding => "invalid-padding",
        DecodeError::PrivateKey => "private-key",
    }
}

/// stellar-strkey's verdict: the decoded value as a case expects it, or its error. it reads
/// bytes because some invalid inputs are not utf-8, and from_string is from_slice on the
/// string's bytes in 1.0.0.
fn reference(p: Prefix, text: &[u8]) -> std::result::Result<Value, DecodeError> {
    if p == Prefix::S {
        let k = ed25519::PrivateKey::from_slice(text)?;
        return Ok(json!({ "kind": p.kind(), "seed": hex::encode(k.0) }));
    }
    let decoded = Strkey::from_slice(text)?;
    Ok(match decoded {
        Strkey::PublicKeyEd25519(k) => {
            json!({ "kind": "public_key_ed25519", "ed25519": hex::encode(k.0) })
        }
        Strkey::MuxedAccountEd25519(m) => json!({
            "kind": "muxed_account_ed25519",
            "ed25519": hex::encode(m.ed25519),
            "id": m.id.to_string(),
        }),
        Strkey::PreAuthTx(h) => json!({ "kind": "pre_auth_tx", "hash": hex::encode(h.0) }),
        Strkey::HashX(h) => json!({ "kind": "hash_x", "hash": hex::encode(h.0) }),
        Strkey::SignedPayloadEd25519(s) => json!({
            "kind": "signed_payload_ed25519",
            "ed25519": hex::encode(s.ed25519()),
            "payload": hex::encode(s.payload()),
        }),
        Strkey::Contract(c) => json!({ "kind": "contract", "contract": hex::encode(c.0) }),
        Strkey::LiquidityPool(l) => json!({ "kind": "liquidity_pool", "pool": hex::encode(l.0) }),
        Strkey::ClaimableBalance(stellar_strkey::ClaimableBalance::V0(h)) => {
            json!({ "kind": "claimable_balance", "version": 0, "hash": hex::encode(h) })
        }
        Strkey::MuxedContract(m) => json!({
            "kind": "muxed_contract",
            "contract": hex::encode(m.contract_id),
            "id": m.id.to_string(),
        }),
    })
}

// the reference decoder for one prefix also has to agree on which prefix it decoded
fn reference_for(p: Prefix, text: impl AsRef<[u8]>) -> std::result::Result<Value, Refusal> {
    let v = reference(p, text.as_ref()).map_err(Refusal::Strkey)?;
    if v["kind"] != p.kind() {
        return Err(Refusal::OtherKind(
            v["kind"].as_str().unwrap_or("an unknown kind").to_owned(),
        ));
    }
    Ok(v)
}

fn prefix_of(text: &str) -> Result<Prefix> {
    let c = text.chars().next().context("an empty SEP-23 string")?;
    Prefix::ALL
        .into_iter()
        .find(|p| p.letter() == c)
        .with_context(|| format!("SEP-23 string {text} has no known prefix"))
}

pub fn items() -> Result<Vec<VectorItem>> {
    for s in SEP23_VALID.iter().chain(SEP23_INVALID) {
        prefix_of(s)?;
    }
    let mut out = Vec::new();
    for p in Prefix::ALL {
        let mut valid = Vec::new();
        for (label, payload) in valid_payloads(p) {
            let text = strkey(p.version(), &payload);
            let decoded = reference_for(p, &text).map_err(|e| {
                anyhow::anyhow!("{} {label}: stellar-strkey rejects {text}: {e}", p.letter())
            })?;
            // the local encoder must write exactly what stellar-strkey writes back
            let again = match p {
                Prefix::S => {
                    let k = ed25519::PrivateKey::from_string(&text)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    k.as_unredacted().to_string().to_string()
                }
                _ => Strkey::from_string(&text)
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .to_string()
                    .to_string(),
            };
            ensure!(
                again == text,
                "{} {label}: re-encodes to {again}, not {text}",
                p.letter()
            );
            valid.push(json!({
                "op": "strkey_decode",
                "text": text,
                "note": label,
                "expected": decoded,
            }));
        }
        for s in SEP23_VALID.iter().filter(|s| prefix_of(s).ok() == Some(p)) {
            let decoded = reference_for(p, s).map_err(|e| {
                anyhow::anyhow!("stellar-strkey rejects the SEP-23 valid case {s}: {e}")
            })?;
            valid.push(json!({
                "op": "strkey_decode",
                "text": s,
                "note": "SEP-23 1.4.0 valid test case",
                "expected": decoded,
            }));
        }

        let mut invalid = Vec::new();
        for b in invalid_strings(p) {
            match reference_for(p, &b.text) {
                Ok(v) => bail!(
                    "{}: stellar-strkey accepts {:?} ({}) as {v}",
                    p.letter(),
                    b.text,
                    b.why
                ),
                Err(e) => invalid.push(json!({
                    "op": "strkey_decode",
                    "text": b.text,
                    "note": b.why,
                    "expected": { "reject": refusal_name(&e) },
                })),
            }
        }
        for s in SEP23_INVALID
            .iter()
            .filter(|s| prefix_of(s).ok() == Some(p))
        {
            match reference_for(p, s) {
                Ok(v) => bail!("stellar-strkey accepts the SEP-23 invalid case {s} as {v}"),
                Err(e) => invalid.push(json!({
                    "op": "strkey_decode",
                    "text": s,
                    "note": "SEP-23 1.4.0 invalid test case",
                    "expected": { "reject": refusal_name(&e) },
                })),
            }
        }

        let letter = p.letter().to_ascii_lowercase();
        let common = format!(
            "{}. expectations are stellar-strkey 1.0.0's verdicts; SEP-23 1.4.0 says a strkey is valid only if re-encoding its binary gives back the same string, which rules out wrong lengths, set trailing bits, padding and lowercase. the reject reason is stellar-strkey's error and is informational; a testee passes by rejecting.",
            p.what()
        );
        out.push(VectorItem {
            spec: Spec::new(
                p.family(),
                format!("strkey-{letter}-valid"),
                EntryKind::Strkey,
            )
            .flags(p.flags())
            .notes(format!("valid {common}")),
            cases: valid,
        });
        if matches!(p, Prefix::G | Prefix::M | Prefix::P) {
            out.push(VectorItem {
                spec: Spec::new(
                    p.family(),
                    format!("strkey-{letter}-noncanonical-chars"),
                    EntryKind::Strkey,
                )
                .reject(Validity::NotEmittable)
                .flags(p.flags())
                .flags(&["strkey-invalid"])
                .links(&["https://github.com/stellar/go-stellar-sdk/pull/6026"])
                .notes(format!(
                    "{}, each with carriage returns, newlines or 0xff bytes inserted. SEP-23 1.4.0 (Specification, step 6 and the paragraph after it) defines a strkey as RFC 4648 base32 without padding and valid only if re-encoding its binary yields the exact same string, and RFC 4648 section 3.3 says a decoder must reject characters outside the base alphabet unless the referring specification says otherwise, which SEP-23 does not. so every case must be rejected; stellar-strkey 1.0.0 rejects each one. go-stellar-sdk before v0.9.0 decoded the runs of these bytes to the payload of the strkey they were inserted into, because go's encoding/base32 skips carriage returns and newlines and its unpadded decoder reads 0xff as padding; v0.9.0 rejects any byte outside the alphabet first (go-stellar-sdk#6026). cases that are not utf-8 carry the text as text_base64.",
                    p.what()
                )),
                cases: stray_cases(p)?,
            });
        }
        out.push(VectorItem {
            spec: Spec::new(
                p.family(),
                format!("strkey-{letter}-invalid"),
                EntryKind::Strkey,
            )
            .reject(Validity::NotEmittable)
            .flags(p.flags())
            .flags(&["strkey-invalid"])
            .notes(format!("invalid {common}")),
            cases: invalid,
        });
    }
    Ok(out)
}
