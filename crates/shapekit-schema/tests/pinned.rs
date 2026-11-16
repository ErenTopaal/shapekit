//! The pinned definitions against the stellar-xdr crate built from the same .x files, and the
//! counts the schema stats command reports.

use std::collections::BTreeSet;

use shapekit_schema::{Definition, Schema};

#[test]
fn files_match_the_stellar_xdr_crate() {
    let schema = Schema::pinned();
    let ours: Vec<(&str, &str)> = schema
        .files()
        .iter()
        .map(|f| (f.name.as_str(), f.sha256.as_str()))
        .collect();
    assert_eq!(ours, stellar_xdr::XDR_FILES_SHA256.to_vec());
    assert!(schema.resolved_features().is_empty());
}

#[test]
fn every_type_has_a_stellar_xdr_type_variant() {
    // the rust generator renames SCPBallot to ScpBallot and uint32 to Uint32, which lowercasing
    // undoes, and the overlay struct Error to SError, so it does not shadow the crate's own Error
    let schema = Schema::pinned();
    let ours: BTreeSet<String> = schema
        .definitions()
        .iter()
        .filter(|d| !matches!(d, Definition::Const(_)))
        .map(|d| match d.name() {
            "Error" => "serror".to_string(),
            n => n.to_lowercase(),
        })
        .collect();
    let theirs: BTreeSet<String> = stellar_xdr::TypeVariant::VARIANTS
        .iter()
        .map(|v| v.name().to_lowercase())
        .collect();
    let only_ours: Vec<_> = ours.difference(&theirs).collect();
    let only_theirs: Vec<_> = theirs.difference(&ours).collect();
    assert!(
        only_ours.is_empty() && only_theirs.is_empty(),
        "only here: {only_ours:?}, only in stellar-xdr: {only_theirs:?}"
    );
    assert_eq!(ours.len(), 477);
}

#[test]
fn whole_schema_counts() {
    let schema = Schema::pinned();
    let s = schema.graph().stats(None).unwrap();
    assert_eq!(s.types, 495);
    assert_eq!(s.kinds["struct"], 237);
    assert_eq!(s.kinds["union"], 115);
    assert_eq!(s.kinds["enum"], 85);
    assert_eq!(s.kinds["typedef"], 40);
    assert_eq!(s.kinds["const"], 18);
    assert_eq!(s.union_cases, 583);
    assert_eq!(s.enum_values, 605);
    assert_eq!(s.struct_fields, 741);
}

#[test]
fn ledger_close_meta_reach() {
    let schema = Schema::pinned();
    let s = schema.graph().stats(Some("LedgerCloseMeta")).unwrap();
    assert_eq!(
        (s.types, s.union_cases, s.enum_values, s.struct_fields),
        (362, 499, 411, 535)
    );
}

#[test]
fn transaction_envelope_reach() {
    let schema = Schema::pinned();
    let s = schema.graph().stats(Some("TransactionEnvelope")).unwrap();
    assert_eq!(
        (s.types, s.union_cases, s.enum_values, s.struct_fields),
        (156, 142, 169, 203)
    );
}

#[test]
fn six_recursive_groups() {
    let schema = Schema::pinned();
    let groups = schema.graph().recursive_groups();
    let firsts: Vec<&str> = groups.iter().map(|g| g[0]).collect();
    assert_eq!(groups.len(), 6, "{groups:?}");
    assert!(groups.iter().any(|g| g.contains(&"SCVal")));
    assert!(groups.iter().any(|g| g == &vec!["ClaimPredicate"]));
    assert!(groups.iter().any(|g| g == &vec!["SCPQuorumSet"]));
    assert!(firsts.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn unknown_root_is_an_error() {
    let schema = Schema::pinned();
    assert!(schema.graph().stats(Some("NoSuchType")).is_err());
}

#[test]
fn dangling_reference_is_rejected() {
    let text = r#"{"version":1,"files":[],"resolved_features":[],"definitions":[
        {"kind":"typedef","name":"A","source":"","file_index":0,"type":{"kind":"ref","name":"B"}}]}"#;
    let err = Schema::from_json(text).unwrap_err().to_string();
    assert_eq!(err, "A refers to B, which is not defined");
}

#[test]
fn union_arm_lookup() {
    let schema = Schema::pinned();
    let Some(Definition::Union(u)) = schema.get("TransactionEnvelope") else {
        panic!("TransactionEnvelope is a union");
    };
    assert_eq!(u.case_values().collect::<Vec<_>>(), vec![0, 2, 5]);
    assert_eq!(u.arm(5).and_then(|a| a.name.as_deref()), Some("feeBump"));
    assert!(u.arm(1).is_none());
}

#[test]
fn typedefs_resolve() {
    let schema = Schema::pinned();
    let Some(Definition::Struct(s)) = schema.get("SCPBallot") else {
        panic!("SCPBallot is a struct");
    };
    let counter = &s.fields[0].ty;
    assert_eq!(*schema.resolve(counter), shapekit_schema::Type::UnsignedInt);
}
