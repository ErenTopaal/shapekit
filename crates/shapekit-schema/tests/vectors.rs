//! Format vectors over the pinned schema.

use shapekit_schema::Schema;
use shapekit_schema::vectors::{Builder, Expect};

#[test]
fn every_definition_has_vectors_the_interpreter_confirms() {
    let schema = Schema::pinned();
    let b = Builder::new(&schema);
    let mut total = 0;
    let mut by_expect = std::collections::BTreeMap::new();
    for d in schema.definitions() {
        if matches!(d, shapekit_schema::Definition::Const(_)) {
            continue;
        }
        assert!(
            b.smallest(d.name()).is_some(),
            "{} has no smallest value",
            d.name()
        );
        let vs = b
            .vectors(d.name())
            .unwrap_or_else(|e| panic!("{}: {e}", d.name()));
        assert!(!vs.is_empty(), "{} has no vector", d.name());
        for v in &vs {
            *by_expect.entry(v.expect).or_insert(0) += 1;
        }
        total += vs.len();
    }
    eprintln!("{total} vectors: {by_expect:?}");
    assert!(by_expect[&Expect::ImplementationDefined] > 0);
}
