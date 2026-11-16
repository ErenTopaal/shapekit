//! The next step of an episode: a transaction of one to three operations filled from the
//! schema, or now and then a ledger upgrade.

use std::collections::BTreeSet;

use serde_json::Value;
use shapekit_contracts::recipe_ir::{FeeBump, Step, TxExpect};
use shapekit_schema::Schema;

use crate::fill::Filler;
use crate::rng::Rng;
use crate::world::World;

/// Operations that need a Soroban transaction; the classic search leaves them out.
pub const CONTRACT_OPERATIONS: &[&str] = &[
    "invoke_host_function",
    "extend_footprint_ttl",
    "restore_footprint",
];

/// Operations the search draws from, with how often each has shown something new.
#[derive(Debug, Clone)]
pub struct OpMenu {
    pub arms: Vec<String>,
    pub novelty: Vec<u64>,
}

impl OpMenu {
    pub fn new(schema: &Schema, contracts: bool) -> Self {
        let arms: Vec<String> = Filler::arm_names(schema, "OperationBody")
            .into_iter()
            .filter(|a| contracts || !CONTRACT_OPERATIONS.contains(&a.as_str()))
            .collect();
        let novelty = vec![0; arms.len()];
        Self { arms, novelty }
    }

    /// An operation arm; arms that recently showed something new are drawn more often, and
    /// every arm keeps a floor so none starves.
    /// Arms in `skip` (operations core said it does not support at this protocol) are
    /// never drawn; when every arm is skipped the draw ignores `skip`.
    pub fn pick(&self, rng: &mut Rng, skip: &BTreeSet<String>) -> usize {
        let mut w: Vec<u64> = self
            .arms
            .iter()
            .zip(&self.novelty)
            .map(|(a, n)| {
                if skip.contains(a) {
                    0
                } else {
                    8 + (*n).min(40)
                }
            })
            .collect();
        if w.iter().all(|x| *x == 0) {
            w = self.novelty.iter().map(|n| 8 + (*n).min(40)).collect();
        }
        rng.weighted(&w)
    }

    pub fn credit(&mut self, arm: &str, amount: u64) {
        if let Some(i) = self.arms.iter().position(|a| a == arm) {
            self.novelty[i] = self.novelty[i] / 2 + amount;
        }
    }
}

/// Roles an operation's `source_account` reference names.
fn source_role(op: &Value) -> Option<String> {
    let s = op.get("source_account")?.as_str()?;
    let rest = s
        .strip_prefix("@account:")
        .or_else(|| s.strip_prefix("@muxed:").and_then(|r| r.split(':').next()))?;
    Some(rest.split(':').next()?.to_owned())
}

/// A filled transaction step and the operation arms it uses.
pub fn transaction(
    schema: &Schema,
    world: &World,
    rng: &mut Rng,
    menu: &OpMenu,
    skip: &BTreeSet<String>,
) -> (Step, Vec<String>) {
    // issuers act on their assets (flags, clawback, authorization), so a share of the
    // transactions come from them
    let issuers: Vec<String> = world
        .issuers
        .iter()
        .filter(|i| world.funded.contains(*i))
        .cloned()
        .collect();
    let mut source = if !issuers.is_empty() && rng.chance(3, 10) {
        rng.pick(&issuers).clone()
    } else {
        rng.pick(&world.funded).clone()
    };
    let n = match rng.below(10) {
        0..=6 => 1,
        7 | 8 => 2,
        _ => 3,
    };
    let mut ops = Vec::with_capacity(n);
    let mut arms = Vec::with_capacity(n);
    let mut f = Filler::new(schema, world, rng, &source);
    let mut op_sources = Vec::new();
    for _ in 0..n {
        let arm = menu.arms[menu.pick(f.rng, skip)].clone();
        let body = f
            .union_arm("OperationBody", &arm)
            .expect("the menu holds OperationBody arms");
        let source_account = if f.rng.chance(1, 4) {
            let role = f.role();
            if f.rng.chance(1, 6) {
                Value::String(format!(
                    "@muxed:{role}:{}",
                    *f.rng.pick(&[0u64, 1, u64::MAX])
                ))
            } else {
                Value::String(format!("@account:{role}"))
            }
        } else {
            Value::Null
        };
        let op = serde_json::json!({ "source_account": source_account, "body": body });
        if let Some(r) = source_role(&op)
            && r != source
            && world.funded.contains(&r)
        {
            op_sources.push(r);
        }
        ops.push(op);
        arms.push(arm);
    }
    let memo = f.rng.chance(1, 4).then(|| f.def("Memo"));
    let cond = f.rng.chance(1, 5).then(|| f.def("Preconditions"));
    let named = f.named.clone();
    // a quarter of the time the transaction comes from a role its operations name
    if f.rng.chance(1, 4) {
        let cands: Vec<&String> = named.iter().filter(|r| world.funded.contains(*r)).collect();
        if !cands.is_empty() {
            source = (*f.rng.pick(&cands)).clone();
        }
    }
    let mut signers = Vec::new();
    if !op_sources.is_empty() || rng.chance(1, 12) {
        signers.push(source.clone());
        for r in op_sources {
            if !signers.contains(&r) && rng.chance(6, 7) {
                signers.push(r);
            }
        }
        if rng.chance(1, 3) {
            let extra = rng.pick(&named).clone();
            if !signers.contains(&extra) && world.funded.contains(&extra) {
                signers.push(extra);
            }
        }
        if rng.chance(1, 20) {
            // the source left out: a transaction without its source's signature
            signers.remove(0);
            if signers.is_empty() {
                signers.push(rng.pick(&world.funded).clone());
            }
        }
    }
    // the fee core charges per operation follows the base fee upgrades; a fee of 1 is refused
    let per_op = world.base_fee.max(100);
    let fee = if rng.chance(1, 50) {
        Some(*rng.pick(&[1u32, 10_000]))
    } else if per_op > 100 {
        Some(per_op * n as u32)
    } else {
        None
    };
    // fee bumps wrap v1 envelopes, which core takes from protocol 13 on
    let fee_bump = (world.protocol >= 13 && rng.chance(1, 8)).then(|| {
        let payer = rng.pick(&world.funded).clone();
        let fee = if rng.chance(1, 10) {
            Some(*rng.pick(&[1i64, 1_000_000, i64::MAX]))
        } else if per_op > 100 {
            Some(i64::from(per_op) * (n as i64 + 1))
        } else {
            None
        };
        let signers = if rng.chance(1, 8) {
            vec![rng.pick(&world.funded).clone()]
        } else {
            Vec::new()
        };
        FeeBump {
            source: payer,
            fee,
            signers,
        }
    });
    (
        Step::Tx {
            label: None,
            source,
            // v0 envelopes, below protocol 13, have no muxed source
            source_mux_id: (world.protocol >= 13 && rng.chance(1, 12))
                .then(|| rng.pick(&["0", "1", "18446744073709551615"]).to_string()),
            operations: ops,
            signers,
            memo,
            fee,
            cond,
            fee_bump,
            expect: TxExpect::default(),
        },
        arms,
    )
}

/// A transaction to apply right after `first` in the same ledger: drawn like any other, from
/// an account `first` names when there is one besides its source.
pub fn follower(
    schema: &Schema,
    world: &World,
    rng: &mut Rng,
    menu: &OpMenu,
    skip: &BTreeSet<String>,
    first: &Step,
) -> (Step, Vec<String>) {
    let (mut b, arms) = transaction(schema, world, rng, menu, skip);
    if let (
        Step::Tx {
            source: a_src,
            operations,
            ..
        },
        Step::Tx { source, .. },
    ) = (first, &mut b)
    {
        let text = serde_json::to_string(operations).unwrap_or_default();
        let named: Vec<&String> = world
            .funded
            .iter()
            .filter(|r| *r != a_src && text.contains(&format!(":{r}\"")))
            .collect();
        if !named.is_empty() {
            *source = (*rng.pick(&named)).clone();
        } else if *source == *a_src {
            // core queues one transaction per source account
            let others: Vec<&String> = world.funded.iter().filter(|r| *r != a_src).collect();
            if !others.is_empty() {
                *source = (*rng.pick(&others)).clone();
            }
        }
    }
    (b, arms)
}

/// A ledger upgrade within what this protocol allows to be armed.
pub fn upgrade(world: &World, rng: &mut Rng, max_protocol: u32) -> Step {
    let mut s = Step::Upgrade {
        label: None,
        protocol_version: None,
        base_fee: None,
        base_reserve: None,
        max_tx_set_size: None,
        max_soroban_tx_set_size: None,
        flags: None,
        config: Vec::new(),
    };
    let Step::Upgrade {
        protocol_version,
        base_fee,
        base_reserve,
        max_tx_set_size,
        flags,
        ..
    } = &mut s
    else {
        unreachable!()
    };
    match rng.below(5) {
        0 if world.protocol < max_protocol => *protocol_version = Some(world.protocol + 1),
        1 => *base_fee = Some(*rng.pick(&[50, 100, 1000])),
        2 => *base_reserve = Some(*rng.pick(&[5_000_000, 20_000_000, 100_000_000])),
        3 => *max_tx_set_size = Some(*rng.pick(&[1, 100, 1000])),
        _ => *flags = Some(*rng.pick(&[0, 1, 2, 4, 7])),
    }
    s
}

/// A contract call: a deployed instance, one of its functions, arguments filled from its
/// interface. Calls that name accounts are authorized by those accounts' signatures.
pub fn invoke(
    schema: &Schema,
    world: &World,
    rng: &mut Rng,
    deployed: &[(String, &crate::contract::Wasm)],
    names: &crate::spec::Names,
) -> Option<Step> {
    use shapekit_contracts::recipe_ir::{AuthSpec, ResourceOverride};
    let (label, wasm) = rng.pick(deployed).clone();
    let funcs: Vec<&crate::spec::Function> = wasm
        .iface
        .functions
        .iter()
        .filter(|f| f.name != "__constructor")
        .collect();
    if funcs.is_empty() {
        return None;
    }
    let source = rng.pick(&world.funded).clone();
    let func = (*rng.pick(&funcs)).clone();
    let mut f = Filler::new(schema, world, rng, &source);
    let args: Vec<Value> = func
        .inputs
        .iter()
        .map(|t| crate::spec::value(&mut f, &wasm.iface, names, t, 0))
        .collect();
    let named: Vec<String> = f
        .named
        .iter()
        .filter(|r| world.funded.contains(*r))
        .cloned()
        .collect();
    let text = serde_json::to_string(&args).unwrap_or_default();
    let auth = if named
        .iter()
        .any(|r| text.contains(&format!("@account:{r}\"")))
    {
        AuthSpec::Record {
            allow_non_root: rng.chance(1, 2),
            address_v2: world.protocol >= 27 && rng.chance(1, 2),
            signers: named.iter().map(|r| (r.clone(), Vec::new())).collect(),
        }
    } else {
        AuthSpec::Source
    };
    let resources = rng.chance(1, 12).then(|| match rng.below(3) {
        0 => ResourceOverride {
            instructions: Some(1000),
            disk_read_bytes: None,
            write_bytes: None,
            resource_fee: None,
        },
        1 => ResourceOverride {
            instructions: None,
            disk_read_bytes: None,
            write_bytes: Some(0),
            resource_fee: None,
        },
        _ => ResourceOverride {
            instructions: None,
            disk_read_bytes: None,
            write_bytes: None,
            resource_fee: Some(1),
        },
    });
    Some(Step::Invoke {
        label: None,
        source,
        contract: label,
        function: func.name,
        args,
        auth,
        resources,
        expect: TxExpect::default(),
    })
}

/// A host function written out: a Stellar asset contract for one of the episode's assets, a
/// contract created with either create function from uploaded code, or any value of the type.
pub fn host_function(
    schema: &Schema,
    world: &World,
    rng: &mut Rng,
    names: &crate::spec::Names,
) -> Step {
    use shapekit_contracts::recipe_ir::AuthSpec;
    let source = rng.pick(&world.funded).clone();
    let mut f = Filler::new(schema, world, rng, &source);
    let salt = hex::encode(f.rng.bytes(32));
    let preimage =
        serde_json::json!({ "address": { "address": format!("@account:{source}"), "salt": salt } });
    let wasm = (!names.wasm.is_empty())
        .then(|| serde_json::json!({ "wasm": format!("@wasm:{}", f.rng.pick(&names.wasm)) }));
    let function = match (f.rng.below(5), wasm) {
        (0 | 1, _) => {
            let asset = f.def("Asset");
            serde_json::json!({ "create_contract": { "contract_id_preimage": { "asset": asset }, "executable": "stellar_asset" } })
        }
        (2, Some(w)) => {
            serde_json::json!({ "create_contract": { "contract_id_preimage": preimage, "executable": w } })
        }
        (3, Some(w)) => {
            serde_json::json!({ "create_contract_v2": { "contract_id_preimage": preimage, "executable": w, "constructor_args": [] } })
        }
        _ => f.def("HostFunction"),
    };
    Step::HostFunction {
        label: None,
        source,
        function,
        auth: AuthSpec::Source,
        resources: None,
        expect: TxExpect::default(),
    }
}

/// An extend or a restore over Soroban entries the ledgers showed.
pub fn footprint(world: &World, rng: &mut Rng) -> Option<Step> {
    let keys: Vec<Value> = world.soroban_keys.values().cloned().collect();
    if keys.is_empty() {
        return None;
    }
    let n = 1 + rng.below(keys.len().min(2));
    let picked: Vec<Value> = (0..n).map(|_| rng.pick(&keys).clone()).collect();
    let source = rng.pick(&world.funded).clone();
    Some(if rng.chance(1, 2) {
        Step::ExtendTtl {
            label: None,
            source,
            keys: picked,
            extend_to: *rng.pick(&[1u32, 100, 10_000, 3_000_000]),
            expect: TxExpect::default(),
        }
    } else {
        Step::Restore {
            label: None,
            source,
            keys: picked,
            expect: TxExpect::default(),
        }
    })
}
