//! `<lane>/recipes/chains.toml`: the chains of a lane recorded from recipe IR files rather than
//! from a recipe of this crate. Each row names the chain directory under `<lane>/chains/`, the
//! family of the entries cut from the chain after recording (`shapekit explore entries`), and
//! the recipe file under `<lane>/recipes/`. The rows of `corpus/recipes/chains.toml` are
//! registered recipes: `gen verify-repro` records them again like the recipes of [`super::all`].

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::ir_run::IrRecipe;

/// The index file under a lane's `recipes/`.
pub const CHAINS_INDEX: &str = "chains.toml";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainsIndex {
    pub schema: u32,
    #[serde(default, rename = "chain")]
    pub chains: Vec<IndexedChain>,
}

/// A chain recorded from a recipe IR file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedChain {
    /// The chain directory under the lane's `chains/`.
    pub id: String,
    /// The family of the entries cut from the chain.
    pub family: String,
    /// The recipe, relative to the lane's `recipes/`.
    pub recipe: String,
}

const HEADER: &str = "\
# the chains of this lane recorded from recipe IR files: each chain directory under chains/,
# the family of the entries cut from it, and its recipe under recipes/. `shapekit corpus
# promote` moves a chain and its recipe with the first entry that leaves the lane, and
# `shapekit gen verify-repro` records the chains listed in corpus/recipes/chains.toml again.
";

impl ChainsIndex {
    pub fn path(lane: &Path) -> PathBuf {
        lane.join(super::ir::RECIPES_DIR).join(CHAINS_INDEX)
    }

    /// The lane's index, empty when the lane has none.
    pub fn load(lane: &Path) -> Result<ChainsIndex> {
        let path = Self::path(lane);
        if !path.is_file() {
            return Ok(ChainsIndex {
                schema: 1,
                chains: Vec::new(),
            });
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let index: ChainsIndex =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        ensure!(index.schema == 1, "{}: schema is not 1", path.display());
        let mut ids = BTreeSet::new();
        let mut recipes = BTreeSet::new();
        for c in &index.chains {
            ensure!(
                ids.insert(&c.id),
                "{}: chain {} is listed twice",
                path.display(),
                c.id
            );
            ensure!(
                recipes.insert(&c.recipe),
                "{}: recipe {} records two chains",
                path.display(),
                c.recipe
            );
        }
        Ok(index)
    }

    /// The committed text, rows sorted by chain id.
    pub fn render(&self) -> String {
        let mut out = format!("{HEADER}\nschema = {}\n", self.schema);
        let mut chains = self.chains.clone();
        chains.sort_by(|a, b| a.id.cmp(&b.id));
        for c in &chains {
            let _ = write!(
                out,
                "\n[[chain]]\nid = {}\nfamily = {}\nrecipe = {}\n",
                toml_str(&c.id),
                toml_str(&c.family),
                toml_str(&c.recipe)
            );
        }
        out
    }

    pub fn write(&self, lane: &Path) -> Result<()> {
        let path = Self::path(lane);
        std::fs::write(&path, self.render()).with_context(|| format!("writing {}", path.display()))
    }

    /// The listed chains as recipes, each under its own chain id and family. `root` is the
    /// repository root, which upload paths in a recipe are relative to.
    pub fn recipes(&self, lane: &Path, root: &Path) -> Result<Vec<IrRecipe>> {
        self.chains
            .iter()
            .map(|c| {
                let file = lane.join(super::ir::RECIPES_DIR).join(&c.recipe);
                Ok(IrRecipe::load(&file, root)?.recorded_as(&c.id, &c.family))
            })
            .collect()
    }
}

fn toml_str(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, recipe: &str) -> IndexedChain {
        IndexedChain {
            id: id.into(),
            family: "explore.witnesses".into(),
            recipe: recipe.into(),
        }
    }

    #[test]
    fn the_rendered_index_reads_back_sorted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("recipes")).unwrap();
        let index = ChainsIndex {
            schema: 1,
            chains: vec![
                row("b.chain", "explore.witnesses/b.json"),
                row("a.chain", "explore.witnesses/a.json"),
            ],
        };
        index.write(dir.path()).unwrap();
        let back = ChainsIndex::load(dir.path()).unwrap();
        let ids: Vec<&str> = back.chains.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["a.chain", "b.chain"]);
        assert_eq!(back.render(), index.render());
    }

    #[test]
    fn a_lane_without_an_index_lists_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(ChainsIndex::load(dir.path()).unwrap().chains.is_empty());
    }

    #[test]
    fn a_chain_or_recipe_listed_twice_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("recipes")).unwrap();
        for chains in [
            vec![row("a", "x.json"), row("a", "y.json")],
            vec![row("a", "x.json"), row("b", "x.json")],
        ] {
            let index = ChainsIndex { schema: 1, chains };
            std::fs::write(ChainsIndex::path(dir.path()), index.render()).unwrap();
            assert!(ChainsIndex::load(dir.path()).is_err());
        }
    }
}
