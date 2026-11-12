//! `soroban.muxed-source`: Soroban transactions whose source is a muxed
//! account. Core accepts a muxed source on extend-TTL and restore at every
//! protocol, and refuses it on an invocation with `txSOROBAN_INVALID`: from
//! protocol 25 the transaction is invalid (`validateSorobanMemo`,
//! TransactionFrame.cpp), before 25 the queue refuses it (`canAdd`,
//! TransactionQueue.cpp), both stellar-core v30.0.0.
//!
//! Restore needs an archived persistent entry. The node starts with
//! `TESTING_MINIMUM_PERSISTENT_ENTRY_LIFETIME = 10`, so entries created
//! under the genesis settings archive ten ledgers after creation.
//!
//! Protocol 30: testnet's settings replace that minimum before `run`, so the
//! archived entries are the ones created before the settings upgrade, the
//! settings writer's code and instance (`ChainDriver::apply_settings`).
//!
//! | ledger  | content                                                     |
//! |---------|-------------------------------------------------------------|
//! | 3 to 6  | settings writer upload, deploy, store, settings upgrade     |
//! | 7       | root funds alice                                            |
//! | -       | invocation from muxed alice, refused `txSOROBAN_INVALID`     |
//! | 8 to n  | empty ledgers until the writer's code and instance archive  |
//! | n+1     | restore of both from muxed alice                            |
//! | n+2     | extend TTL of both from muxed alice                         |
//!
//! Protocol 28: no settings upgrade, so alice's own echo contract archives.
//! The last ledger holds the wallet-backend#753 shape: an extend-TTL from
//! muxed alice and, in a second transaction from bob, a payment whose
//! operation source is alice's base account.
//!
//! | ledger  | content                                                     |
//! |---------|-------------------------------------------------------------|
//! | 3       | root funds alice and bob                                    |
//! | 4, 5    | alice uploads and deploys echo                              |
//! | -       | invocation from muxed alice, refused `txSOROBAN_INVALID`     |
//! | 6 to n  | empty ledgers until echo's code and instance archive        |
//! | n+1     | restore of both from muxed alice                            |
//! | n+2     | extend TTL from muxed alice, payment with op source alice   |

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::SETTINGS_WRITER_SALT_DOMAIN;
use shapekit_core_driver::contracts::{ECHO_WASM, echo_wasm_for};
use shapekit_core_driver::keys::{
    account_address, account_id, muxed_account, muxed_account_with_id,
};
use shapekit_core_driver::node::{EntryState, SubmitStatus};
use shapekit_core_driver::sim::AuthMode;
use shapekit_core_driver::sim_legacy::AnySimulator;
use shapekit_core_driver::tx::{
    BASE_FEE, TransactionBuilder, assemble_soroban, contract_id_from_address, envelope_hash,
};
use stellar_xdr::{
    Asset, ContractDataDurability, ContractId, ExtendFootprintTtlOp, ExtensionPoint, Hash,
    HostFunction, InvokeContractArgs, InvokeHostFunctionOp, LedgerEntryChange, LedgerKey,
    LedgerKeyContractCode, LedgerKeyContractData, OperationBody, PaymentOp, RestoreFootprintOp,
    ScAddress, ScSymbol, ScVal, SorobanAuthorizationEntry, SorobanCredentials,
    SorobanTransactionData, TransactionEnvelope, TransactionResultResult,
};

use super::cap73::operation_changes;
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "soroban.muxed-source";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Minimum persistent TTL of the genesis settings, core's floor.
const MIN_PERSISTENT_TTL: u32 = 10;

/// Upper bound on the empty ledgers closed while waiting for archival.
const MAX_WAIT: u32 = 3 * MIN_PERSISTENT_TTL;

/// Ledgers past the current one the extend-TTL asks for.
const EXTEND_TO: u32 = 10_000;

const MUX_RESTORE: u64 = 1;
const MUX_EXTEND: u64 = u64::MAX;
const MUX_INVOKE: u64 = 42;

/// One payment, 1 XLM.
const AMOUNT: i64 = 10_000_000;

pub struct MuxedSource {
    pub protocol: u32,
}

fn code_key(wasm: &[u8]) -> LedgerKey {
    LedgerKey::ContractCode(LedgerKeyContractCode {
        hash: Hash(Sha256::digest(wasm).into()),
    })
}

fn instance_key(id: &ContractId) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(id.clone()),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    })
}

fn simulator(ctx: &mut ChainCtx) -> Result<AnySimulator> {
    let node = ctx.driver().node();
    let lctx = node.ledger_context()?;
    Ok(AnySimulator::new(node.snapshot()?, &lctx)?)
}

/// A single-operation Soroban transaction whose source is `source` muxed
/// with `mux_id`, signed by `source`.
fn muxed_soroban(
    ctx: &mut ChainCtx,
    source: &SigningKey,
    mux_id: u64,
    body: OperationBody,
    data: SorobanTransactionData,
    auth: Option<Vec<SorobanAuthorizationEntry>>,
) -> Result<TransactionEnvelope> {
    let seq = ctx
        .driver()
        .node()
        .account_seq(&account_id(&source.verifying_key()))?
        + 1;
    let tx = TransactionBuilder::new(muxed_account_with_id(&source.verifying_key(), mux_id), seq)
        .fee(BASE_FEE)
        .operation(None, body)
        .build()?;
    let tx = assemble_soroban(tx, data, auth)?;
    ctx.sign(tx, &[source])
}

fn states(ctx: &mut ChainCtx, keys: &[LedgerKey]) -> Result<Vec<EntryState>> {
    Ok(ctx
        .driver()
        .node()
        .ledger_entries(keys)?
        .entries
        .iter()
        .map(|e| e.state)
        .collect())
}

/// Closes empty ledgers until every key is archived. Fails if one is
/// missing or if they are still live after `MAX_WAIT` ledgers.
fn wait_archived(ctx: &mut ChainCtx, keys: &[LedgerKey]) -> Result<()> {
    for _ in 0..=MAX_WAIT {
        let s = states(ctx, keys)?;
        ensure!(
            !s.contains(&EntryState::NotFound),
            "an entry to archive is missing: {s:?}"
        );
        if s.iter().all(|s| *s == EntryState::Archived) {
            return Ok(());
        }
        ctx.driver().close_empty()?;
    }
    bail!("entries still live after {MAX_WAIT} empty ledgers")
}

/// The invocation twin: `function` on `contract` simulated for alice's base
/// account, then sent from muxed alice. Core must refuse it.
fn refused_invoke(
    ctx: &mut ChainCtx,
    alice: &SigningKey,
    contract: &ContractId,
    function: &str,
) -> Result<()> {
    let hf = HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: ScAddress::Contract(contract.clone()),
        function_name: ScSymbol(function.try_into()?),
        args: Default::default(),
    });
    let sim = simulator(ctx)?;
    let r = sim.simulate_invoke(
        hf.clone(),
        AuthMode::Record {
            allow_non_root: false,
            address_v2: false,
        },
        &account_id(&alice.verifying_key()),
    )?;
    ensure!(
        r.auth
            .iter()
            .all(|a| matches!(a.credentials, SorobanCredentials::SourceAccount)),
        "the refused invocation should need no signed auth, simulation gave {:?}",
        r.auth
    );
    let body = OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
        host_function: hf,
        auth: Default::default(),
    });
    let env = muxed_soroban(
        ctx,
        alice,
        MUX_INVOKE,
        body,
        r.transaction_data,
        Some(r.auth),
    )?;
    let result = ctx.capture_rejected(
        "invoke-muxed-source",
        &env,
        Spec::new(
            "a contract invocation whose transaction source is a muxed account, refused by core with txSOROBAN_INVALID before it reaches a ledger",
        )
        .fields(&["tx.source_account", "tx.source_account.mux_id"]),
    )?;
    ensure!(
        matches!(result.result, TransactionResultResult::TxSorobanInvalid),
        "core answered {:?} for an invocation from a muxed source, expected tx_soroban_invalid",
        result.result
    );
    Ok(())
}

/// Restores `keys` from muxed alice and checks the meta restored each.
fn restore(ctx: &mut ChainCtx, alice: &SigningKey, keys: &[LedgerKey]) -> Result<()> {
    let data = simulator(ctx)?.simulate_restore(keys)?;
    let body = OperationBody::RestoreFootprint(RestoreFootprintOp {
        ext: ExtensionPoint::V0,
    });
    let env = muxed_soroban(ctx, alice, MUX_RESTORE, body, data, None)?;
    let applied = ctx.submit_and_close(&env)?;
    let restored = operation_changes(&applied.meta)
        .into_iter()
        .filter(|c| matches!(c, LedgerEntryChange::Restored(_)))
        .count();
    // each restored entry comes back with its ttl entry
    ensure!(
        restored == 2 * keys.len(),
        "restore should bring back {} entries with their ttls, meta restored {restored}",
        keys.len()
    );
    // the ledger, not the transaction: a restore's meta carries no muxed
    // account and no arm of the family, the envelope in the tx set does
    ctx.capture(
        "restore-muxed-source",
        Selector::ledger_of(&applied),
        Spec::new(
            "a restore of an archived contract code and instance whose transaction source is a muxed account",
        )
        .fields(&[
            "tx.source_account",
            "tx.source_account.mux_id",
            "op.restore_footprint",
            "tx.soroban.footprint",
        ]),
    )
}

/// An extend-TTL of `keys` from muxed alice, unsigned by anything else.
fn extend(
    ctx: &mut ChainCtx,
    alice: &SigningKey,
    keys: &[LedgerKey],
) -> Result<TransactionEnvelope> {
    let sim = simulator(ctx)?.simulate_extend_ttl(keys, EXTEND_TO)?;
    ensure!(
        sim.restore.is_none(),
        "the extended entries should be live, simulation asks for a restore"
    );
    let body = OperationBody::ExtendFootprintTtl(ExtendFootprintTtlOp {
        ext: ExtensionPoint::V0,
        extend_to: EXTEND_TO,
    });
    muxed_soroban(ctx, alice, MUX_EXTEND, body, sim.transaction_data, None)
}

const EXTEND_FIELDS: [&str; 4] = [
    "tx.source_account",
    "tx.source_account.mux_id",
    "op.extend_footprint_ttl.extend_to",
    "tx.soroban.footprint",
];

impl MuxedSource {
    fn run_p30(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["alice"], BALANCE)?;
        let alice = ctx.account("alice");
        let root = ctx.root().clone();
        let salt: [u8; 32] = Sha256::digest(SETTINGS_WRITER_SALT_DOMAIN).into();
        let writer = contract_id_from_address(
            &ctx.network_id(),
            &account_address(&root.verifying_key()),
            &salt,
        )?;
        let keys = vec![code_key(ECHO_WASM), instance_key(&writer)];
        let s = states(ctx, &keys)?;
        ensure!(
            s.iter().all(|s| *s == EntryState::Live),
            "the settings writer should still be live, states {s:?}"
        );

        refused_invoke(ctx, &alice, &writer, "last")?;
        wait_archived(ctx, &keys)?;
        restore(ctx, &alice, &keys)?;

        let env = extend(ctx, &alice, &keys)?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "extend-ttl-muxed-source",
            Selector::ledger_of(&applied),
            Spec::new(
                "an extend-TTL of contract code and instance whose transaction source is a muxed account",
            )
            .fields(&EXTEND_FIELDS),
        )
    }

    fn run_p28(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["alice", "bob"], BALANCE)?;
        let alice = ctx.account("alice");
        let bob = ctx.account("bob");
        let wasm = echo_wasm_for(self.protocol)
            .with_context(|| format!("no echo build for protocol {}", self.protocol))?;
        let code = ctx.driver().upload(&alice, wasm)?;
        let salt: [u8; 32] = Sha256::digest(b"shapekit/soroban.muxed-source/echo").into();
        let echo = ctx.driver().deploy(&alice, &code, &salt)?;
        let keys = vec![code_key(wasm), instance_key(&echo)];

        refused_invoke(ctx, &alice, &echo, "last")?;
        wait_archived(ctx, &keys)?;
        restore(ctx, &alice, &keys)?;

        // wallet-backend#753: muxed alice as a Soroban source and base alice
        // as an operation source in one ledger. a second transaction from
        // alice herself would be refused by the one transaction per source
        // account rule (HerderImpl::recvTransaction), so bob carries it
        let extend_env = extend(ctx, &alice, &keys)?;
        let tx = ctx
            .tx(&bob)?
            .operation(
                Some(muxed_account(&alice.verifying_key())),
                OperationBody::Payment(PaymentOp {
                    destination: muxed_account(&bob.verifying_key()),
                    asset: Asset::Native,
                    amount: AMOUNT,
                }),
            )
            .build()?;
        let payment_env = ctx.sign(tx, &[&bob, &alice])?;
        let network_id = ctx.network_id();
        let mut hashes = Vec::new();
        for env in [&extend_env, &payment_env] {
            let sub = ctx.driver().node().submit(env)?;
            ensure!(
                sub.status == SubmitStatus::Pending,
                "core did not queue a transaction of the shared ledger: {:?} {:?}",
                sub.status,
                sub.error
            );
            hashes.push(envelope_hash(env, &network_id)?);
        }
        let ledger = ctx.driver().close_empty()?;
        let results = ledger_results(ctx.driver().last_meta());
        for h in &hashes {
            match results.iter().find(|(rh, _)| rh == h) {
                Some((_, ok)) => ensure!(
                    *ok,
                    "ledger {ledger}: transaction {} failed",
                    hex::encode(h)
                ),
                None => bail!(
                    "ledger {ledger}: transaction {} is not in the ledger",
                    hex::encode(h)
                ),
            }
        }
        ensure!(
            results.len() == 2,
            "ledger {ledger} should hold exactly the two transactions, holds {}",
            results.len()
        );
        ctx.capture(
            "muxed-and-base-source-one-ledger",
            Selector::Ledger(ledger),
            Spec::new(
                "one ledger holding an extend-TTL whose source is a muxed account and a payment whose operation source is the same account's base G address",
            )
            .fields(&EXTEND_FIELDS)
            .issues(&["https://github.com/stellar/wallet-backend/pull/753"]),
        )
    }
}

/// Transaction hash and success flag of every transaction in a ledger.
fn ledger_results(lcm: &stellar_xdr::LedgerCloseMeta) -> Vec<([u8; 32], bool)> {
    let ok = |r: &TransactionResultResult| matches!(r, TransactionResultResult::TxSuccess(_));
    match lcm {
        stellar_xdr::LedgerCloseMeta::V0(m) => m
            .tx_processing
            .iter()
            .map(|t| (t.result.transaction_hash.0, ok(&t.result.result.result)))
            .collect(),
        stellar_xdr::LedgerCloseMeta::V1(m) => m
            .tx_processing
            .iter()
            .map(|t| (t.result.transaction_hash.0, ok(&t.result.result.result)))
            .collect(),
        stellar_xdr::LedgerCloseMeta::V2(m) => m
            .tx_processing
            .iter()
            .map(|t| (t.result.transaction_hash.0, ok(&t.result.result.result)))
            .collect(),
    }
}

impl Recipe for MuxedSource {
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
        self.protocol == 30
    }

    fn soroban_limits(&self) -> bool {
        true
    }

    fn min_persistent_ttl(&self) -> Option<u32> {
        Some(MIN_PERSISTENT_TTL)
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        match self.protocol {
            30 => self.run_p30(ctx),
            28 => self.run_p28(ctx),
            p => bail!("{FAMILY} has no chain at protocol {p}"),
        }
    }
}
