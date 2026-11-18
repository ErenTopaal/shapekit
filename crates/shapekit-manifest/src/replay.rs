//! The replay digest of a manifest: what a harness run of the corpus depends on.
//!
//! A harness report records it, and `shapekit findings check` compares it with the committed
//! manifest's, so a report stays current while only the entries' descriptions change. It is
//! the sha256 of the manifest's JSON with keys sorted, after leaving out these fields of every
//! entry, which describe an entry and never change what is replayed or how it is judged:
//! [`DESCRIPTIVE_FIELDS`]. Everything else is covered: the header (corpus version, the stack
//! the corpus was generated with, the protocol tiers) and of every entry its id, kind, family,
//! protocol, XDR type, meta versions, source, synthetic and passphrase flags, every variant
//! with its file paths and the sha256 of its bytes, expected JSON and oracle file, the vector
//! file, the expected verdicts and the flags. A change to the bytes, the entry set or an
//! oracle therefore changes the digest.

use serde_json::Value;

use crate::model::Manifest;
use crate::xdr::render_json;

/// The entry fields the replay digest leaves out.
pub const DESCRIPTIVE_FIELDS: [&str; 6] = [
    "caps",
    "arms",
    "notes",
    "fields_exercised",
    "linked_issues",
    "obligations",
];

/// The replay digest of `manifest`, lowercase hex.
pub fn replay_digest(manifest: &Manifest) -> String {
    let mut v = serde_json::to_value(manifest).expect("a manifest serializes");
    if let Some(entries) = v.get_mut("entries").and_then(Value::as_array_mut) {
        for e in entries {
            if let Some(o) = e.as_object_mut() {
                for f in DESCRIPTIVE_FIELDS {
                    o.remove(f);
                }
            }
        }
    }
    crate::sha256_hex(render_json(&v).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::replay_digest;
    use crate::model::Manifest;

    fn sample() -> Manifest {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample/manifest.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn descriptions_keep_the_digest() {
        let m = sample();
        let mut d = m.clone();
        let e = &mut d.entries[0];
        e.caps = vec!["PRE-CAP".into()];
        e.arms = vec!["OperationBody:payment".into()];
        e.notes = Some("retold".into());
        e.fields_exercised.push("x".into());
        e.linked_issues.push("https://example.org/1".into());
        e.obligations.push("gap:x".into());
        assert_eq!(replay_digest(&m), replay_digest(&d));
    }

    #[test]
    fn bytes_entries_and_oracles_change_the_digest() {
        let m = sample();
        let base = replay_digest(&m);
        let mut bytes = m.clone();
        bytes.entries[0]
            .variants
            .testnet
            .as_mut()
            .unwrap()
            .xdr_sha256 = "0".repeat(64);
        assert_ne!(replay_digest(&bytes), base);
        let mut fewer = m.clone();
        fewer.entries.pop();
        assert_ne!(replay_digest(&fewer), base);
        let mut renamed = m.clone();
        renamed.entries[0].id.push('x');
        assert_ne!(replay_digest(&renamed), base);
        let mut oracle = m.clone();
        oracle.entries[0]
            .variants
            .testnet
            .as_mut()
            .unwrap()
            .oracle_sha256 = Some("1".repeat(64));
        assert_ne!(replay_digest(&oracle), base);
        let mut expected = m.clone();
        expected.entries[0].expected.roundtrip = !expected.entries[0].expected.roundtrip;
        assert_ne!(replay_digest(&expected), base);
    }
}
