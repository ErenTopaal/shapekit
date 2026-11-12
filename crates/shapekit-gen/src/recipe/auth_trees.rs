//! exploration build, replaced before commit
use anyhow::Result;
use sha2::{Digest, Sha256};
use shapekit_core_driver::contracts::ECHO_WASM;
use shapekit_core_driver::keys::account_address;
use shapekit_core_driver::sim::AuthMode;
use shapekit_core_driver::tx::sign_authorization_entry;
use stellar_xdr::{
    ContractId, Hash, InvokeContractArgs, Limits, ScAddress, ScMap, ScMapEntry, ScSymbol, ScVal,
    SorobanAuthorizationEntry, SorobanAuthorizedFunction, SorobanAuthorizedInvocation,
    SorobanCredentials, WriteXdr,
};

use super::soroban_steps::{
    AUTH_TREE_WASM, authorized, call, contract_address, expiration, salt, simulate, sym, upload,
    vec_val,
};
use super::{ChainCtx, Recipe};

pub const FAMILY: &str = "soroban.auth-trees";

pub struct AuthTrees {
    pub protocol: u32,
}

pub fn level(signer: &ScAddress, fanout: u32) -> Result<ScVal> {
    Ok(ScVal::Map(Some(ScMap(
        vec![
            ScMapEntry {
                key: sym("fanout")?,
                val: ScVal::U32(fanout),
            },
            ScMapEntry {
                key: sym("signer")?,
                val: ScVal::Address(signer.clone()),
            },
        ]
        .try_into()?,
    ))))
}

pub fn node_args(chain: &[ContractId], levels: &[(ScAddress, u32)]) -> Result<Vec<ScVal>> {
    let c = chain
        .iter()
        .map(|c| ScVal::Address(contract_address(c)))
        .collect();
    let l = levels
        .iter()
        .map(|(s, f)| level(s, *f))
        .collect::<Result<Vec<_>>>()?;
    Ok(vec![vec_val(c)?, vec_val(l)?])
}

fn minimal(
    c: &ContractId,
    children: Vec<SorobanAuthorizedInvocation>,
) -> Result<SorobanAuthorizedInvocation> {
    Ok(SorobanAuthorizedInvocation {
        function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
            contract_address: contract_address(c),
            function_name: ScSymbol("node".try_into()?),
            args: Default::default(),
        }),
        sub_invocations: children.try_into()?,
    })
}

fn chain_of(c: &ContractId, depth: u32) -> Result<Vec<SorobanAuthorizedInvocation>> {
    let mut cur: Vec<SorobanAuthorizedInvocation> = Vec::new();
    for _ in 0..depth {
        cur = vec![minimal(c, cur)?];
    }
    Ok(cur)
}

impl Recipe for AuthTrees {
    fn id(&self) -> &str {
        FAMILY
    }
    fn family(&self) -> &str {
        FAMILY
    }
    fn protocol(&self) -> u32 {
        self.protocol
    }
    fn soroban_settings(&self) -> bool {
        true
    }
    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["source", "alice"], 100_000_000_000)?;
        let source = ctx.account("source");
        let alice = ctx.account("alice");
        let alice_addr = account_address(&alice.verifying_key());
        let code = upload(ctx, &source, AUTH_TREE_WASM)?;
        let _ = Hash(Sha256::digest(ECHO_WASM).into());
        let n: u32 = std::env::var("P5_N")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        let mut ts = Vec::new();
        for i in 0..n {
            let s = salt(FAMILY, &format!("tree-{i}"));
            ts.push(ctx.driver().deploy(&source, &code, &s)?);
        }
        let sign = |ctx: &mut ChainCtx,
                    e: SorobanAuthorizationEntry|
         -> Result<SorobanAuthorizationEntry> {
            if matches!(e.credentials, SorobanCredentials::SourceAccount) {
                return Ok(e);
            }
            let exp = expiration(ctx);
            Ok(sign_authorization_entry(
                e,
                &ctx.network_id(),
                exp,
                &[&alice],
            )?)
        };
        let mut keys: Vec<stellar_xdr::LedgerKey> =
            ts.iter().map(super::soroban_steps::instance_key).collect();
        keys.push(super::soroban_steps::code_key(&code));
        for k in [&source, &alice] {
            keys.push(stellar_xdr::LedgerKey::Account(
                stellar_xdr::LedgerKeyAccount {
                    account_id: shapekit_core_driver::keys::account_id(&k.verifying_key()),
                },
            ));
        }
        for d in [
            120u32, 240, 245, 246, 247, 248, 249, 250, 255, 300, 400, 490, 493, 494, 500,
        ] {
            let levels = vec![(alice_addr.clone(), 0)];
            let hf = call(&ts[0], "node", node_args(&[], &levels)?)?;
            let rec = simulate(
                ctx,
                &source,
                &hf,
                AuthMode::Record {
                    allow_non_root: false,
                    address_v2: false,
                },
            )?;
            let mut e = rec.auth[0].clone();
            e.root_invocation.sub_invocations = chain_of(&ts[1], d)?.try_into()?;
            let e = sign(ctx, e)?;
            let sim = simulate(ctx, &source, &hf, AuthMode::Enforce(vec![e.clone()]));
            let data = match sim {
                Ok(s) => s.transaction_data,
                Err(err) => {
                    eprintln!("EXPLORE declared {d}: enforce sim failed: {err:#}");
                    let mut d2 = rec.transaction_data.clone();
                    d2.resource_fee += 2_000_000;
                    d2.resources.instructions += 20_000_000;
                    d2
                }
            };
            let env = ctx.driver().soroban_with(&source, hf, data, vec![e])?;
            let len = env.to_xdr(Limits::none())?.len();
            match ctx.driver().apply(&env) {
                Ok(a) => eprintln!(
                    "EXPLORE declared {d}: applied ledger {} len {len} result {:?}",
                    a.ledger, a.result.result
                ),
                Err(err) => eprintln!("EXPLORE declared {d}: not applied len {len}: {err}"),
            }
        }
        for f in (300u32..=400).step_by(10) {
            let levels = vec![(alice_addr.clone(), f), (alice_addr.clone(), 0)];
            let hf = call(&ts[0], "node", node_args(&ts[1..2], &levels)?)?;
            let t = std::time::Instant::now();
            match super::soroban_steps::authorized_prefetched(
                ctx,
                &source,
                hf,
                AuthMode::Record {
                    allow_non_root: false,
                    address_v2: false,
                },
                &keys,
                sign,
            ) {
                Ok((env, _)) => eprintln!(
                    "EXPLORE fanout {f}: ok {} bytes {:?}",
                    env.to_xdr(Limits::none())?.len(),
                    t.elapsed()
                ),
                Err(e) => {
                    eprintln!("EXPLORE fanout {f}: failed {e:#}");
                    break;
                }
            }
        }
        for d in [30u32, 31, 32] {
            let levels: Vec<(ScAddress, u32)> = (0..d)
                .map(|i| (alice_addr.clone(), if i + 1 == d { 0 } else { 1 }))
                .collect();
            let hf = call(&ts[0], "node", node_args(&ts[1..d as usize], &levels)?)?;
            let t = std::time::Instant::now();
            match super::soroban_steps::authorized_prefetched(
                ctx,
                &source,
                hf,
                AuthMode::Record {
                    allow_non_root: false,
                    address_v2: false,
                },
                &keys,
                sign,
            ) {
                Ok((env, _)) => {
                    let data = super::soroban_steps::soroban_data(&env)?;
                    eprintln!(
                        "EXPLORE matched {d}: ok {} bytes {:?} instr {} fp {}",
                        env.to_xdr(Limits::none())?.len(),
                        t.elapsed(),
                        data.resources.instructions,
                        data.resources.footprint.read_only.len()
                            + data.resources.footprint.read_write.len()
                    );
                }
                Err(e) => eprintln!("EXPLORE matched {d}: failed {e:#} {:?}", t.elapsed()),
            }
        }
        anyhow::bail!("exploration only")
    }
}
