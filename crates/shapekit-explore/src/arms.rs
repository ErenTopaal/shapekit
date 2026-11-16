//! Union arms: which ones a ledger holds, which ones the corpus already has from core, and
//! which of those belong to smart contracts.
//!
//! An arm is named as the coverage report names it, `<Type>:<arm>` anchored at the union's
//! type (`PaymentResult:no_trust`); see `shapekit_manifest::signature`.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use shapekit_manifest::signature::{reachable_arms, signature_of};
use stellar_xdr::{LedgerCloseMeta, Type};

/// The coverage report's root type, whose reachable arms are the denominator.
pub const ROOT_TYPE: &str = "LedgerCloseMeta";

/// Unions that only smart contract transactions, their settings or their ledger entries
/// produce.
const CONTRACT_UNIONS: &[&str] = &[
    "ConfigSettingEntry",
    "ContractCodeEntryExt",
    "ContractEventBody",
    "ContractExecutable",
    "ContractIdPreimage",
    "ExtendFootprintTtlResult",
    "HostFunction",
    "InvokeHostFunctionResult",
    "RestoreFootprintResult",
    "ScAddress",
    "ScError",
    "ScVal",
    "SorobanAuthorizedFunction",
    "SorobanCredentials",
    "SorobanTransactionDataExt",
    "SorobanTransactionMetaExt",
];

/// Arms of shared unions that only smart contracts produce: the three Soroban operations,
/// the four Soroban ledger entry types, the two Soroban ledger upgrades and the
/// transaction extension that carries `SorobanTransactionData`.
const CONTRACT_ARMS: &[&str] = &[
    "OperationBody:invoke_host_function",
    "OperationBody:extend_footprint_ttl",
    "OperationBody:restore_footprint",
    "OperationResultTr:invoke_host_function",
    "OperationResultTr:extend_footprint_ttl",
    "OperationResultTr:restore_footprint",
    "LedgerEntryData:contract_data",
    "LedgerEntryData:contract_code",
    "LedgerEntryData:ttl",
    "LedgerEntryData:config_setting",
    "LedgerKey:contract_data",
    "LedgerKey:contract_code",
    "LedgerKey:ttl",
    "LedgerKey:config_setting",
    "LedgerUpgrade:config",
    "LedgerUpgrade:max_soroban_tx_set_size",
    "TransactionExt:v1",
];

/// Whether an arm belongs to smart contracts rather than to classic transactions.
///
/// `ScVal`, `ScAddress` and `ContractEventBody` count as smart contract arms even though
/// classic operations emit CAP-67 events built from some of them: the classic set is kept to
/// the unions classic transactions themselves are made of.
pub fn is_contract_arm(arm: &str) -> bool {
    let union = arm.split(':').next().unwrap_or(arm);
    CONTRACT_UNIONS.contains(&union) || CONTRACT_ARMS.contains(&arm)
}

/// Every arm reachable from [`ROOT_TYPE`].
pub fn reachable() -> Result<BTreeSet<String>> {
    reachable_arms(ROOT_TYPE).map_err(|e| anyhow::anyhow!("reachable arms of {ROOT_TYPE}: {e}"))
}

/// The arms `corpus/coverage/coverage.json` lists as held by a core-emitted entry.
pub fn core_witnessed(coverage_json: &Path) -> Result<BTreeSet<String>> {
    let text = std::fs::read_to_string(coverage_json)
        .with_context(|| format!("reading {}", coverage_json.display()))?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing {}", coverage_json.display()))?;
    let Some(covered) = v.pointer("/arms/covered").and_then(|c| c.as_array()) else {
        bail!("{} has no arms.covered list", coverage_json.display());
    };
    covered
        .iter()
        .map(|a| {
            a.get("arm")
                .and_then(|s| s.as_str())
                .map(str::to_owned)
                .with_context(|| {
                    format!(
                        "{}: an arms.covered row has no arm",
                        coverage_json.display()
                    )
                })
        })
        .collect()
}

/// The full signature of a ledger: every path, anchored arm and enum value it holds.
pub fn ledger_signature(lcm: &LedgerCloseMeta) -> Result<BTreeSet<String>> {
    signature_of(&Type::LedgerCloseMeta(Box::new(lcm.clone())))
        .map_err(|e| anyhow::anyhow!("signature of a ledger: {e}"))
}

/// The reachable arms a signature holds.
pub fn arms_in(signature: &BTreeSet<String>, reachable: &BTreeSet<String>) -> BTreeSet<String> {
    signature.intersection(reachable).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_splits_the_committed_report() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let arms = core_witnessed(&root.join("corpus/coverage/coverage.json")).unwrap();
        let reachable = reachable().unwrap();
        assert!(arms.is_subset(&reachable));
        let classic = arms.iter().filter(|a| !is_contract_arm(a)).count();
        assert!(classic > 0 && classic < arms.len());
        for a in CONTRACT_ARMS {
            assert!(reachable.contains(*a), "{a} is not a reachable arm");
        }
        for u in CONTRACT_UNIONS {
            assert!(
                reachable.iter().any(|a| a.starts_with(&format!("{u}:"))),
                "{u} has no reachable arm"
            );
        }
    }
}
