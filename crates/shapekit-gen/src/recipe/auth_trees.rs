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
        let roles: Vec<String> = (0..60).map(|i| format!("probe-{i}")).collect();
        let role_refs: Vec<&str> = roles.iter().map(|s| s.as_str()).collect();
        ctx.fund(&role_refs, 1_000_000_000)?;
        let mut next = 0usize;
        let mut fresh = |ctx: &mut ChainCtx| { let k = ctx.account(&roles[next]); next += 1; k };
        let mut base: Option<stellar_xdr::SorobanTransactionData> = None;
        for d in [240u32, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 252, 300, 490, 491, 492, 493, 494, 495, 496] {
            let p = fresh(ctx);
            let pa = account_address(&p.verifying_key());
            let levels = vec![(pa.clone(), 0)];
            let hf = call(&ts[0], "node", node_args(&[], &levels)?)?;
            let root = SorobanAuthorizedInvocation { function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs { contract_address: contract_address(&ts[0]), function_name: ScSymbol("node".try_into()?), args: node_args(&[], &levels)?.try_into()? }), sub_invocations: chain_of(&ts[1], d)?.try_into()? };
            let e = SorobanAuthorizationEntry { credentials: SorobanCredentials::Address(stellar_xdr::SorobanAddressCredentials { address: pa.clone(), nonce: 7, signature_expiration_ledger: 0, signature: ScVal::Void }), root_invocation: root };
            let exp = expiration(ctx);
            let e = sign_authorization_entry(e, &ctx.network_id(), exp, &[&p])?;
            let data = match simulate(ctx, &source, &hf, AuthMode::Enforce(vec![e.clone()])) {
                Ok(s) => { base = Some(s.transaction_data.clone()); s.transaction_data },
                Err(err) => {
                    eprintln!("EXPLORE declared {d}: enforce sim failed: {err:#}");
                    let mut b = base.clone().ok_or_else(|| anyhow::anyhow!("no base"))?;
                    // same footprint shape, but the nonce key belongs to this signer
                    let mut rw = b.resources.footprint.read_write.to_vec();
                    for k in rw.iter_mut() {
                        if let stellar_xdr::LedgerKey::ContractData(cd) = k { if matches!(cd.key, ScVal::LedgerKeyNonce(_)) { cd.contract = pa.clone(); } }
                    }
                    b.resources.footprint.read_write = rw.try_into()?;
                    let mut ro = b.resources.footprint.read_only.to_vec();
                    for k in ro.iter_mut() {
                        if let stellar_xdr::LedgerKey::Account(a) = k { if a.account_id != shapekit_core_driver::keys::account_id(&source.verifying_key()) { a.account_id = shapekit_core_driver::keys::account_id(&p.verifying_key()); } }
                    }
                    b.resources.footprint.read_only = ro.try_into()?;
                    b.resource_fee *= 3;
                    b
                }
            };
            let env = ctx.driver().soroban_with(&source, hf, data, vec![e])?;
            let len = env.to_xdr(Limits::none())?.len();
            match ctx.driver().apply(&env) {
                Ok(a) => eprintln!("EXPLORE declared {d}: applied ledger {} len {len} result {:?}", a.ledger, a.result.result),
                Err(err) => eprintln!("EXPLORE declared {d}: not applied len {len}: {err}"),
            }
        }
        for f in 321u32..=339 {
            let p = fresh(ctx);
            let pa = account_address(&p.verifying_key());
            let levels = vec![(pa.clone(), f), (pa.clone(), 0)];
            let hf = call(&ts[0], "node", node_args(&ts[1..2], &levels)?)?;
            let signp = |ctx: &mut ChainCtx, e: SorobanAuthorizationEntry| -> Result<SorobanAuthorizationEntry> { let exp = expiration(ctx); Ok(sign_authorization_entry(e, &ctx.network_id(), exp, &[&p])?) };
            match super::soroban_steps::authorized_prefetched(ctx, &source, hf, AuthMode::Record { allow_non_root: false, address_v2: false }, &keys, signp) {
                Ok((env, _)) => { let dd = super::soroban_steps::soroban_data(&env)?; eprintln!("EXPLORE fanout {f}: ok {} bytes instr {}", env.to_xdr(Limits::none())?.len(), dd.resources.instructions) },
                Err(e) => { eprintln!("EXPLORE fanout {f}: failed {e:#}"); break; }
            }
        }
        anyhow::bail!("exploration only")
    }
}
