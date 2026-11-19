//! `report::merge` puts shard reports back together into the report one whole run writes.

use std::collections::BTreeMap;

use shapekit_harness::report::{Report, merge};

fn whole() -> Report {
    // a committed run of every chain of both networks: any harness input of the findings
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../findings/inputs");
    let mut inputs: Vec<_> = std::fs::read_dir(dir)
        .expect("reading findings/inputs")
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("harness-")
        })
        .collect();
    inputs.sort();
    let path = inputs.first().expect("a committed harness input");
    serde_json::from_slice(&std::fs::read(path).expect("reading the committed run"))
        .expect("parsing it")
}

/// The report shard `index` of `count` of `r` would be: its chains dealt out by name as
/// `harness run --shard` deals them.
fn shard_of(r: &Report, index: usize, count: usize) -> Report {
    let mut names: Vec<&str> = r.chains.iter().map(|c| c.chain.as_str()).collect();
    names.sort();
    names.dedup();
    let keep: Vec<&str> = names
        .iter()
        .enumerate()
        .filter(|(i, _)| i % count == index - 1)
        .map(|(_, c)| *c)
        .collect();
    let mut s = r.clone();
    s.chains.retain(|c| keep.contains(&c.chain.as_str()));
    s.entries.retain(|e| keep.contains(&e.chain.as_str()));
    s.totals.entries = s.entries.len();
    s.totals.by_verdict = BTreeMap::new();
    s.totals.by_reason = BTreeMap::new();
    for e in &s.entries {
        *s.totals
            .by_verdict
            .entry(e.result.verdict.name().to_owned())
            .or_default() += 1;
        if let Some(x) = e.result.reason {
            *s.totals.by_reason.entry(x.name().to_owned()).or_default() += 1;
        }
    }
    s.shard = Some(format!("{index}/{count}"));
    s
}

#[test]
fn shards_merge_into_the_whole_run() {
    let w = whole();
    let shards: Vec<Report> = (1..=3).rev().map(|i| shard_of(&w, i, 3)).collect();
    let m = merge(shards).expect("merging");
    assert_eq!(m.totals, w.totals);
    let key = |c: &shapekit_harness::run::ChainResult| (c.chain.clone(), c.network.clone());
    assert_eq!(m.chains.iter().map(key).collect::<Vec<_>>(), {
        let mut k: Vec<_> = w.chains.iter().map(key).collect();
        k.sort();
        k
    });
    // the entries in the order the whole run wrote them
    assert_eq!(
        m.entries
            .iter()
            .map(|e| (&e.entry, &e.network))
            .collect::<Vec<_>>(),
        w.entries
            .iter()
            .map(|e| (&e.entry, &e.network))
            .collect::<Vec<_>>()
    );
    assert_eq!(m.shard, None);
}

#[test]
fn an_incomplete_or_mixed_set_of_shards_is_refused() {
    let w = whole();
    let err = merge(vec![shard_of(&w, 1, 3), shard_of(&w, 3, 3)]).unwrap_err();
    assert!(format!("{err}").contains("missing"), "{err}");
    let err = merge(vec![shard_of(&w, 1, 2), shard_of(&w, 1, 2)]).unwrap_err();
    assert!(format!("{err}").contains("twice"), "{err}");
    let mut other = shard_of(&w, 2, 2);
    other.adapter_command.push("--cpus".into());
    let err = merge(vec![shard_of(&w, 1, 2), other]).unwrap_err();
    assert!(format!("{err}").contains("not of the same run"), "{err}");
    let err = merge(vec![w.clone()]).unwrap_err();
    assert!(format!("{err}").contains("is not a shard"), "{err}");
}
