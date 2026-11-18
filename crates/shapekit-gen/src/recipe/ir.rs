//! Recipe IR files: loading, and the symbolic references inside their XDR-JSON values.
//!
//! The format is [`shapekit_contracts::RecipeIr`]; its module documents the steps and the
//! reference syntax. This module replaces every reference in a value with what it names on the
//! running chain and decodes the result as the stellar-xdr type the step needs. The
//! interpreter is [`super::ir_run`].

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::de::DeserializeOwned;
use serde_json::Value;
use shapekit_contracts::RecipeIr;
use shapekit_contracts::recipe_ir::SymRef;
use walkdir::WalkDir;

/// Recipes in IR form live under this corpus directory, one directory per family.
pub const RECIPES_DIR: &str = "recipes";

/// Reads and checks one recipe file.
pub fn load(path: &Path) -> Result<RecipeIr> {
    shapekit_contracts::load::<RecipeIr>(path).map_err(|e| anyhow!("{e}"))
}

/// What a recipe pair keeps beside its two recipes: the entries it compares, not a recipe.
pub const PAIR_FILE: &str = "pair.json";

/// Every `*.json` under `<corpus>/recipes/` but a pair's [`PAIR_FILE`], sorted.
pub fn files(corpus: &Path) -> Result<Vec<PathBuf>> {
    let dir = corpus.join(RECIPES_DIR);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for e in WalkDir::new(&dir).sort_by_file_name() {
        let e = e.with_context(|| format!("walking {}", dir.display()))?;
        if e.file_type().is_file()
            && e.path().extension().is_some_and(|x| x == "json")
            && e.file_name() != PAIR_FILE
        {
            out.push(e.into_path());
        }
    }
    Ok(out)
}

/// Replaces every symbolic reference in `value` through `lookup`, which returns the
/// replacement string. `@@x` becomes `@x`.
pub fn resolve(value: &Value, lookup: &mut dyn FnMut(&SymRef) -> Result<String>) -> Result<Value> {
    Ok(match value {
        Value::String(s) => match SymRef::parse(s) {
            None => value.clone(),
            Some(Err(e)) => bail!("{e}"),
            Some(Ok(SymRef::Literal(rest))) => Value::String(rest),
            Some(Ok(r)) => Value::String(lookup(&r).with_context(|| format!("resolving {s}"))?),
        },
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| resolve(v, lookup))
                .collect::<Result<_>>()?,
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), resolve(v, lookup)?)))
                .collect::<Result<_>>()?,
        ),
        other => other.clone(),
    })
}

/// Resolves `value` and decodes it as XDR-JSON of type `T`.
pub fn typed<T: DeserializeOwned>(
    value: &Value,
    what: &str,
    lookup: &mut dyn FnMut(&SymRef) -> Result<String>,
) -> Result<T> {
    let resolved = resolve(value, lookup)?;
    serde_json::from_value(resolved.clone())
        .with_context(|| format!("{what} is not valid XDR-JSON: {resolved}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{Operation, OperationBody};

    #[test]
    fn references_are_replaced_and_literals_unescaped() {
        let v: Value = serde_json::json!({
            "a": "@account:alice",
            "b": ["@seq:alice+5", "@@not-a-ref"],
            "c": 7
        });
        let out = resolve(&v, &mut |r| {
            Ok(match r {
                SymRef::Account(role) => format!("G-{role}"),
                SymRef::Seq { role, add } => format!("{role}+{add}"),
                other => bail!("unexpected {other}"),
            })
        })
        .unwrap();
        assert_eq!(
            out,
            serde_json::json!({"a": "G-alice", "b": ["alice+5", "@not-a-ref"], "c": 7})
        );
    }

    #[test]
    fn a_malformed_reference_is_an_error() {
        let v = Value::String("@nope".into());
        assert!(resolve(&v, &mut |_| Ok(String::new())).is_err());
    }

    #[test]
    fn committed_recipes_load_and_their_expected_results_parse() {
        use shapekit_contracts::recipe_ir::Step;
        use stellar_xdr::TransactionResultResult;
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        let paths = files(&corpus).unwrap();
        assert!(!paths.is_empty(), "no recipe under corpus/recipes");
        for path in paths {
            let ir = load(&path).unwrap();
            for step in &ir.steps {
                let expect = match step {
                    Step::Tx { expect, .. } | Step::Invoke { expect, .. } => expect,
                    _ => continue,
                };
                if let Some(r) = &expect.result {
                    // an expected result may name a role's account (a claimant, a trustor),
                    // which the interpreter replaces on the running chain; any account key
                    // parses the same way here
                    let _: TransactionResultResult = typed(r, "result", &mut |r| match r {
                        SymRef::Account(_) => {
                            Ok("GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".to_owned())
                        }
                        other => bail!(
                            "{} holds a reference other than an account in an expected result: {other}",
                            path.display()
                        ),
                    })
                    .unwrap();
                }
            }
        }
    }

    #[test]
    fn operations_decode_from_xdr_json() {
        let v = serde_json::json!({
            "source_account": null,
            "body": {"bump_sequence": {"bump_to": "@seq:alice+1"}}
        });
        let op: Operation = typed(&v, "operation", &mut |_| Ok("42".into())).unwrap();
        let OperationBody::BumpSequence(b) = op.body else {
            panic!("not a bump")
        };
        assert_eq!(b.bump_to.0, 42);
    }
}
