//! The repository's test contracts as the explorer uses them: which build a ledger at a
//! protocol can run, their interfaces, and the steps that upload and deploy them.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use shapekit_contracts::recipe_ir::Step;
use shapekit_core_driver::contracts::interface_protocol;

use crate::fill::Filler;
use crate::spec::{self, Interface, Names};

/// Where the builds live, relative to the repository root.
pub const WASM_DIR: &str = "contracts/wasm";

#[derive(Debug, Clone)]
pub struct Wasm {
    /// The contract's name, `echo`.
    pub name: String,
    /// Relative to the repository root.
    pub path: String,
    pub sha256: String,
    /// The protocol its `contractenvmetav0` declares; a ledger below it refuses the code.
    pub interface: u32,
    pub iface: Interface,
}

/// Every committed build: the current ones and the older-sdk ones under `p<N>/`.
pub fn load_all(root: &Path) -> Result<Vec<Wasm>> {
    let mut out = Vec::new();
    let base = root.join(WASM_DIR);
    let mut dirs = vec![(base.clone(), String::new())];
    for e in std::fs::read_dir(&base).with_context(|| format!("reading {}", base.display()))? {
        let p = e?.path();
        if p.is_dir() {
            let sub = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            dirs.push((p, format!("{sub}/")));
        }
    }
    for (dir, rel) in dirs {
        let mut files: Vec<_> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "wasm"))
            .collect();
        files.sort();
        for f in files {
            let bytes = std::fs::read(&f).with_context(|| format!("reading {}", f.display()))?;
            let name = f
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let interface = interface_protocol(&bytes)
                .with_context(|| format!("{} declares no interface protocol", f.display()))?;
            out.push(Wasm {
                path: format!("{WASM_DIR}/{rel}{name}.wasm"),
                sha256: hex::encode(Sha256::digest(&bytes)),
                iface: Interface::of(&bytes).with_context(|| format!("{}", f.display()))?,
                name,
                interface,
            });
        }
    }
    Ok(out)
}

/// Per contract, the newest build a ledger at `protocol` runs.
pub fn for_protocol(all: &[Wasm], protocol: u32) -> Vec<&Wasm> {
    let mut best: std::collections::BTreeMap<&str, &Wasm> = std::collections::BTreeMap::new();
    for w in all.iter().filter(|w| w.interface <= protocol) {
        match best.get(w.name.as_str()) {
            Some(b) if b.interface >= w.interface => {}
            _ => {
                best.insert(&w.name, w);
            }
        }
    }
    best.into_values().collect()
}

/// The deploy label of a contract's first instance.
pub fn instance_label(name: &str) -> String {
    format!("{}-1", name.replace('_', "-"))
}

pub fn code_label(name: &str) -> String {
    name.replace('_', "-")
}

/// The salt a contract's first instance is deployed with.
pub fn salt(name: &str) -> String {
    hex::encode(Sha256::digest(format!("shapekit/explore/{name}")))
}

/// Uploads each contract and deploys one instance of it, constructor arguments filled from
/// its interface. Each submission gets a ledger of its own.
pub fn setup_steps(wasms: &[&Wasm], f: &mut Filler<'_>, source: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut names = Names::default();
    for w in wasms {
        steps.push(Step::Upload {
            label: code_label(&w.name),
            source: source.to_owned(),
            wasm: w.path.clone(),
            sha256: w.sha256.clone(),
        });
        steps.push(Step::Close {
            label: None,
            order: Vec::new(),
        });
        names.wasm.push(code_label(&w.name));
    }
    for w in wasms {
        let args: Vec<Value> = w
            .iface
            .constructor()
            .map(|c| {
                c.inputs
                    .iter()
                    .map(|t| spec::value(f, &w.iface, &names, t, 0))
                    .collect()
            })
            .unwrap_or_default();
        steps.push(Step::Deploy {
            label: instance_label(&w.name),
            source: source.to_owned(),
            code: code_label(&w.name),
            salt: salt(&w.name),
            constructor_args: args,
        });
        steps.push(Step::Close {
            label: None,
            order: Vec::new(),
        });
        names.contracts.push(instance_label(&w.name));
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_protocol_gets_the_newest_build_it_runs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let all = load_all(&root).unwrap();
        let at = |p: u32| -> Vec<(String, u32)> {
            for_protocol(&all, p)
                .iter()
                .map(|w| (w.name.clone(), w.interface))
                .collect()
        };
        assert!(at(20).is_empty());
        assert_eq!(at(21), [("echo".to_owned(), 21)]);
        assert!(at(22).contains(&("echo".into(), 22)));
        assert!(at(26).contains(&("echo".into(), 23)));
        assert!(at(30).contains(&("echo".into(), 29)));
        assert!(at(30).iter().any(|(n, _)| n == "delegating_account"));
    }
}
