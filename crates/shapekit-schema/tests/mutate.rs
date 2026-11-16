//! Schema mutants against the format vectors.

use std::collections::BTreeMap;

use shapekit_schema::mutate::{check, mutants};
use shapekit_schema::vectors::Builder;
use shapekit_schema::{Definition, Schema};

#[test]
fn every_mutant_is_killed() {
    let schema = Schema::pinned();
    let b = Builder::new(&schema);
    let names: Vec<&str> = schema
        .definitions()
        .iter()
        .filter(|d| !matches!(d, Definition::Const(_)))
        .map(|d| d.name())
        .collect();
    let vectors: BTreeMap<String, _> = names
        .iter()
        .map(|n| (n.to_string(), b.vectors(n).unwrap()))
        .collect();
    let ms = mutants(&schema, &names);
    let out = check(&schema, &ms, &vectors);
    let survivors: Vec<_> = out.iter().filter(|o| o.killed_by.is_none()).collect();
    let mut by_op: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &survivors {
        *by_op.entry(s.operator).or_default() += 1;
    }
    let described: Vec<&str> = survivors.iter().map(|s| s.description.as_str()).collect();
    assert!(survivors.is_empty(), "{by_op:?}: {described:?}");
    assert!(out.len() > 2000, "{} mutants", out.len());
}
