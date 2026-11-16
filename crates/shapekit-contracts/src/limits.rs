//! Limits, `registry/generated/limits.json`: the largest value core accepts for a quantity and
//! the first it refuses, found by bisection on a running core.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Contract, CoreRef, Stage, parse_decimal, sorted_unique, witness};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LimitsTable {
    pub schema_version: u32,
    pub core: CoreRef,
    /// Sorted by (path, measure), each pair once.
    pub limits: Vec<Limit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    /// The field the quantity belongs to, as `shapekit schema` names it.
    pub path: String,
    /// What was varied when it is not the field's own value, `invocations in one auth tree
    /// level` for example.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<String>,
    /// Decimal integer: the largest value core accepted and applied successfully.
    pub max_accepted: String,
    /// Decimal integer: the next value, which core refused or failed.
    pub first_rejected: String,
    pub rejection: LimitRejection,
    /// The network settings in force, setting name to decimal value, for limits that follow
    /// from them (`contract_compute_v0.tx_max_instructions` for example).
    #[serde(default)]
    pub network_settings: BTreeMap<String, String>,
    /// The entries of the accepted and the refused value.
    pub witnesses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LimitRejection {
    pub stage: Stage,
    /// The XDR-JSON name of the result: the transaction result arm, `tx_malformed`, or for a
    /// failed operation the operation result, `invoke_host_function.resource_limit_exceeded`.
    /// Present for `submit` and `apply`, absent for `decode`, where core returned no result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_code: Option<String>,
}

impl Contract for LimitsTable {
    const SCHEMA_FILE: &'static str = "limits.schema.json";
    const VERSION: u32 = 1;
    const TITLE: &'static str = "shapekit limits";
    const DESCRIPTION: &'static str = "registry/generated/limits.json: for each bounded quantity, the largest value core accepts, the first it refuses with core's result code, and the network settings in force.";

    fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.core.problems(&mut out);
        sorted_unique(
            "limit",
            &self.limits,
            |l| (l.path.clone(), l.measure.clone()),
            &mut out,
        );
        for l in &self.limits {
            let what = match &l.measure {
                Some(m) => format!("limit {} ({m})", l.path),
                None => format!("limit {}", l.path),
            };
            match (parse_decimal(&l.max_accepted), parse_decimal(&l.first_rejected)) {
                (Some(a), Some(r)) if a.checked_add(1) == Some(r) => {}
                (Some(_), Some(_)) => out.push(format!(
                    "{what}: first_rejected {} is not max_accepted {} plus one, so the bisection did not finish",
                    l.first_rejected, l.max_accepted
                )),
                _ => out.push(format!("{what}: bounds must be decimal integers")),
            }
            for (k, v) in &l.network_settings {
                if parse_decimal(v).is_none() {
                    out.push(format!(
                        "{what}: setting {k} = {v:?} is not a decimal integer"
                    ));
                }
            }
            let code = l.rejection.result_code.as_deref();
            match (l.rejection.stage, code) {
                (Stage::Decode, Some(_)) => {
                    out.push(format!("{what}: a decode rejection has no result code"))
                }
                (Stage::Submit | Stage::Apply, c) if c.is_none_or(|c| c.trim().is_empty()) => {
                    out.push(format!("{what} has no rejection result code"))
                }
                _ => {}
            }
            if l.witnesses.len() < 2 {
                out.push(format!(
                    "{what} needs a witness for the accepted and for the refused value"
                ));
            }
            witness::check_all(&what, &l.witnesses, &mut out);
        }
        out
    }
}
