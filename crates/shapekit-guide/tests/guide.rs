//! Renders the guide regions from the repository's real registry and corpus manifest and
//! checks that `check` catches each kind of drift.

use std::path::{Path, PathBuf};

use shapekit_guide::{
    CORPUS_BY_FIELD, ChecklistFile, CorpusLinks, FIELD_TABLE, MeasuredRow, REGIONS, Sources,
    UnknownField,
};
use shapekit_manifest::Manifest;
use shapekit_registry::{Registry, ValidatedByCore};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Fixture {
    registry: Registry,
    manifest: Manifest,
    checklist: ChecklistFile,
}

fn fixture() -> Fixture {
    let registry = Registry::load(&repo().join("registry")).unwrap();
    let manifest = shapekit_manifest::build::load(&repo().join("corpus/manifest.json")).unwrap();
    let checklist =
        ChecklistFile::load(&repo().join("registry").join(shapekit_guide::CHECKLIST_FILE)).unwrap();
    Fixture {
        registry,
        manifest,
        checklist,
    }
}

fn skeleton() -> String {
    let mut doc = String::from("---\ntitle: t\n---\n\n## a\n\n");
    for name in REGIONS {
        doc.push_str(&format!(
            "{{/* shapekit:begin {name} */}}\n{{/* shapekit:end {name} */}}\n\nprose\n\n"
        ));
    }
    doc
}

/// Every row of the field table, measured as agreeing.
fn agreeing(r: &Registry) -> Vec<MeasuredRow> {
    r.fields
        .fields
        .iter()
        .map(|f| MeasuredRow {
            row: f.id.clone(),
            agrees: true,
            settled: false,
        })
        .collect()
}

#[test]
fn the_field_table_must_stand_against_the_measured_verdicts() {
    let f = fixture();
    let mut rows = agreeing(&f.registry);
    let ok = shapekit_guide::measured(&f.registry, &rows);
    assert!(ok.problems.is_empty(), "{:?}", ok.problems);
    assert_eq!((ok.rows, ok.agree, ok.settled), (rows.len(), rows.len(), 0));

    // a settled disagreement passes
    rows[0].agrees = false;
    rows[0].settled = true;
    let m = shapekit_guide::measured(&f.registry, &rows);
    assert!(m.problems.is_empty(), "{:?}", m.problems);
    assert_eq!(m.settled, 1);

    // an open disagreement, a stale settlement, a missing row and an unknown row fail
    rows[0].settled = false;
    rows[1].settled = true;
    let missing = rows.pop().unwrap().row;
    rows.push(MeasuredRow {
        row: "tx.not_a_row".into(),
        agrees: true,
        settled: false,
    });
    let m = shapekit_guide::measured(&f.registry, &rows);
    let first = &rows[0].row;
    let second = &rows[1].row;
    assert!(
        m.problems
            .iter()
            .any(|p| p.starts_with(&format!("{first}: the verdicts measured")))
    );
    assert!(
        m.problems
            .iter()
            .any(|p| p.starts_with(&format!("{second}: the measured verdicts agree")))
    );
    assert!(
        m.problems
            .iter()
            .any(|p| p == &format!("{missing}: no measured verdict compares this row"))
    );
    assert!(
        m.problems
            .iter()
            .any(|p| p.starts_with("tx.not_a_row: compared, but not"))
    );
    assert_eq!(m.problems.len(), 4, "{:?}", m.problems);
}

fn links() -> CorpusLinks {
    CorpusLinks::Relative("../../corpus".into())
}

#[test]
fn applied_guide_is_current_and_lists_every_field() {
    let f = fixture();
    let links = links();
    let src = Sources {
        registry: &f.registry,
        manifest: &f.manifest,
        checklist: &f.checklist,
        links: &links,
    };
    let doc = shapekit_guide::apply(&skeleton(), &src).unwrap();
    let report = shapekit_guide::check(&doc, &src, &agreeing(&f.registry)).unwrap();
    assert!(report.stale.is_empty(), "{:?}", report.stale);
    assert!(report.unknown.is_empty(), "{:?}", report.unknown);
    assert!(report.checklist.is_empty(), "{:?}", report.checklist);
    assert_eq!(report.table_fields, f.registry.fields.fields.len());
    // applying twice changes nothing
    assert_eq!(shapekit_guide::apply(&doc, &src).unwrap(), doc);
    // every lint rule appears with the id and name the linter prints
    for rule in &f.registry.lint_rules.rules {
        assert!(doc.contains(&format!("`{}` `{}`", rule.id, rule.name)));
    }
    assert!(!doc.contains('\u{2014}'), "no em dashes in rendered text");
}

#[test]
fn edited_region_is_stale_and_dropped_row_is_unknown() {
    let f = fixture();
    let links = links();
    let src = Sources {
        registry: &f.registry,
        manifest: &f.manifest,
        checklist: &f.checklist,
        links: &links,
    };
    let doc = shapekit_guide::apply(&skeleton(), &src).unwrap();
    let exercised = f
        .manifest
        .entries
        .iter()
        .find_map(|e| {
            e.fields_exercised
                .first()
                .map(|x| (e.id.clone(), x.clone()))
        })
        .expect("the corpus exercises at least one field");
    let edited: String = doc
        .lines()
        .filter(|l| !l.starts_with(&format!("| `{}`", exercised.1)))
        .map(|l| format!("{l}\n"))
        .collect();
    let report = shapekit_guide::check(&edited, &src, &agreeing(&f.registry)).unwrap();
    assert_eq!(report.stale, [FIELD_TABLE]);
    assert!(report.unknown.contains(&UnknownField {
        entry: exercised.0,
        field: exercised.1,
    }));
    assert!(!report.passed());
}

#[test]
fn field_without_entries_is_uncovered() {
    let mut f = fixture();
    let target = f
        .registry
        .fields
        .fields
        .iter()
        .find(|x| x.validated_by_core == ValidatedByCore::No)
        .unwrap()
        .id
        .clone();
    for e in &mut f.manifest.entries {
        e.fields_exercised.retain(|x| x != &target);
    }
    let links = links();
    let src = Sources {
        registry: &f.registry,
        manifest: &f.manifest,
        checklist: &f.checklist,
        links: &links,
    };
    let doc = shapekit_guide::apply(&skeleton(), &src).unwrap();
    let report = shapekit_guide::check(&doc, &src, &agreeing(&f.registry)).unwrap();
    assert!(report.uncovered.iter().any(|u| u.field == target));
    let region = &shapekit_guide::render(&src).unwrap()[1];
    assert_eq!(region.0, CORPUS_BY_FIELD);
    assert!(region.1.contains(&format!("`{target}`")));
}

#[test]
fn unknown_field_in_manifest_is_reported() {
    let mut f = fixture();
    f.manifest.entries[0]
        .fields_exercised
        .push("tx.not_a_field".into());
    let links = links();
    let src = Sources {
        registry: &f.registry,
        manifest: &f.manifest,
        checklist: &f.checklist,
        links: &links,
    };
    let doc = shapekit_guide::apply(&skeleton(), &src).unwrap();
    let report = shapekit_guide::check(&doc, &src, &agreeing(&f.registry)).unwrap();
    assert_eq!(
        report.unknown,
        [UnknownField {
            entry: f.manifest.entries[0].id.clone(),
            field: "tx.not_a_field".into()
        }]
    );
}

#[test]
fn link_modes() {
    let f = fixture();
    let entry = f
        .manifest
        .entries
        .iter()
        .find(|e| !e.fields_exercised.is_empty())
        .unwrap();
    let path = shapekit_guide::entry_path(entry).unwrap();
    for (links, expected) in [
        (links(), format!("](../../corpus/{path})")),
        (
            CorpusLinks::Url("https://example.org/corpus/".into()),
            format!("](https://example.org/corpus/{path})"),
        ),
        (CorpusLinks::Plain, format!("`{}`", entry.id)),
    ] {
        let src = Sources {
            registry: &f.registry,
            manifest: &f.manifest,
            checklist: &f.checklist,
            links: &links,
        };
        let regions = shapekit_guide::render(&src).unwrap();
        assert!(regions[1].1.contains(&expected), "{links:?}");
    }
}

#[test]
fn checklist_reference_to_missing_row_is_a_problem() {
    let mut f = fixture();
    f.checklist.items[0].fields.push("tx.gone".into());
    let problems = f.checklist.problems(&f.registry);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].message.contains("tx.gone"));
}
