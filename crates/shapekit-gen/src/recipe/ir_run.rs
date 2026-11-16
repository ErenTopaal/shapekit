//! The recipe IR interpreter: a [`RecipeIr`] as a [`Recipe`], so an IR file records its
//! chain, captures its entries and is checked for reproducibility through the same
//! [`super::generate`] and [`super::verify_repro`] as a recipe written in Rust.
//!
//! Step semantics, as the format documents them: `fund`, `tx`, `upload`, `deploy` and
//! `invoke` submit a transaction and leave it in core's queue; the next `close` or `upgrade`
//! closes one ledger with everything queued, and only then is each queued transaction checked
//! against its expectation. A step expected to be `rejected` is submitted, must be refused,
//! and is never queued.
//!
//! Choices the format leaves to the interpreter:
//!
//! - fee: [`shapekit_core_driver::tx::BASE_FEE`] per operation, the same as
//!   [`ChainDriver::classic`](shapekit_core_driver::chain::ChainDriver::classic);
//! - preconditions: none;
//! - a fee bump's outer fee: the base fee for each inner operation plus one;
//! - envelope: v0 below protocol 13 and v1 from then on, as [`ChainCtx::sign`] does;
//! - sequence numbers: the account's sequence number on the last closed ledger plus one, or
//!   one more than the role's last transaction still waiting in the queue;
//! - Soroban steps take their footprint and resources from simulation against the last
//!   closed ledger and support source-account authorization only;
//! - an `upgrade` with `config` entries stores the set through a settings writer contract,
//!   which needs a ledger of its own before the upgrade ledger (and on first use two more to
//!   upload and deploy the writer), exactly as
//!   [`ChainDriver::apply_settings`](shapekit_core_driver::chain::ChainDriver::apply_settings)
//!   does. Nothing may be queued when such an upgrade starts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use serde_json::Value;
use sha2::{Digest, Sha256};
use shapekit_contracts::RecipeIr;
use shapekit_contracts::recipe_ir::{FeeBump, Outcome, Selection, Step, SymRef, TxExpect};
use shapekit_core_driver::chain::{Applied, ChainError, SETTINGS_WRITER_SALT_DOMAIN, upgrades};
use shapekit_core_driver::contracts::echo_wasm_for;
use shapekit_core_driver::keys::{
    account_address, account_id, account_strkey, contract_strkey, muxed_account,
    muxed_account_with_id, muxed_strkey,
};
use shapekit_core_driver::node::SubmitStatus;
use shapekit_core_driver::settings::PreparedUpgrade;
use shapekit_core_driver::tx::{
    BASE_FEE, TransactionBuilder, contract_id_from_address, contract_preimage, envelope_hash,
    fee_bump, sign_fee_bump,
};
use stellar_xdr::{
    ConfigSettingEntry, ConfigUpgradeSet, ContractExecutable, ContractId, CreateAccountOp,
    CreateContractArgsV2, Hash, HostFunction, InvokeContractArgs, LedgerKey, LedgerUpgrade, Memo,
    Operation, OperationBody, Preconditions, ScAddress, ScSymbol, ScVal, TransactionEnvelope,
    TransactionResult, TransactionResultResult,
};

use super::{ChainCtx, Recipe, Selector, Spec};

/// A recipe read from an IR file.
pub struct IrRecipe {
    ir: RecipeIr,
    /// The repository root, which `upload` paths are relative to.
    root: PathBuf,
}

impl IrRecipe {
    pub fn new(ir: RecipeIr, root: &Path) -> Self {
        Self {
            ir,
            root: root.to_path_buf(),
        }
    }

    /// Reads and checks `path`; `root` is the repository root.
    pub fn load(path: &Path, root: &Path) -> Result<Self> {
        Ok(Self::new(super::ir::load(path)?, root))
    }

    pub fn ir(&self) -> &RecipeIr {
        &self.ir
    }
}

impl Recipe for IrRecipe {
    fn id(&self) -> &str {
        &self.ir.id
    }

    fn family(&self) -> &str {
        &self.ir.family
    }

    fn protocol(&self) -> u32 {
        self.ir.protocol
    }

    fn soroban_settings(&self) -> bool {
        self.ir.node.soroban_settings
    }

    fn soroban_limits(&self) -> bool {
        self.ir.node.soroban_limits || self.ir.node.soroban_settings
    }

    fn min_persistent_ttl(&self) -> Option<u32> {
        self.ir.node.min_persistent_ttl
    }

    fn emit_classic_events(&self) -> bool {
        self.ir.node.emit_classic_events
    }

    fn backfill_stellar_asset_events(&self) -> bool {
        self.ir.node.backfill_stellar_asset_events
    }

    fn below_protocol_valid(&self) -> bool {
        self.ir.below_protocol_valid
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        let mut interp = Session::new(ctx, &self.root, self.ir.node.soroban_settings)?;
        for (i, step) in self.ir.steps.iter().enumerate() {
            interp
                .step(ctx, i, step)
                .with_context(|| format!("recipe {} step {i}", self.ir.id))?;
        }
        ensure!(
            interp.pending.is_empty(),
            "recipe {}: {} transactions are still queued after the last step; end with a close",
            self.ir.id,
            interp.pending.len()
        );
        Ok(())
    }
}

/// What core did with a submitting step run in learning mode.
#[derive(Debug, Clone, PartialEq)]
pub struct Observed {
    /// Index of the step, as passed to [`Session::step`].
    pub step: usize,
    pub outcome: Outcome,
    pub result: TransactionResult,
    /// The ledger it was applied in, or the last closed ledger when it was refused.
    pub ledger: u32,
}

#[derive(Debug, Clone)]
enum Kind {
    Tx,
    Upload(Hash),
    Deploy(ContractId),
}

struct Pending {
    step: usize,
    label: Option<String>,
    hash: [u8; 32],
    expect: TxExpect,
    kind: Kind,
}

struct Rejected {
    env: TransactionEnvelope,
    result: TransactionResult,
    ledger: u32,
}

/// Runs the steps of a recipe one at a time on a chain. [`IrRecipe`] runs a whole file
/// through it; the explorer drives it step by step, in learning mode.
pub struct Session<'a> {
    root: &'a Path,
    /// Learning mode: a submitting step's `expect` is not checked; what core did is
    /// recorded in `observed` instead.
    learn: bool,
    observed: Vec<Observed>,
    /// Steps core queued and then left out of the ledger it closed, in learning mode.
    left_out: Vec<usize>,
    current: usize,
    pending: Vec<Pending>,
    applied: BTreeMap<String, Applied>,
    closed: BTreeMap<String, u32>,
    rejected: BTreeMap<String, Rejected>,
    wasm: BTreeMap<String, Hash>,
    contracts: BTreeMap<String, ContractId>,
    /// sequence number of each role's last transaction waiting in the queue
    queued_seq: BTreeMap<String, i64>,
    writer: Option<ContractId>,
}

fn key_of(ctx: &ChainCtx, role: &str) -> SigningKey {
    if role == "root" {
        ctx.root().clone()
    } else {
        ctx.account(role)
    }
}

fn ledger_seq_on_chain(ctx: &mut ChainCtx, role: &str) -> Result<i64> {
    let key = key_of(ctx, role);
    Ok(ctx
        .driver()
        .node()
        .account_seq(&account_id(&key.verifying_key()))?)
}

impl<'a> Session<'a> {
    /// A session on `ctx`. `soroban_settings` says the chain was started with the captured
    /// settings, whose writer contract the session then reuses.
    pub fn new(ctx: &mut ChainCtx, root: &'a Path, soroban_settings: bool) -> Result<Self> {
        let writer = if soroban_settings {
            // the settings applied before the first step deployed it
            let root = ctx.root().clone();
            Some(ctx.driver().settings_writer(&root)?)
        } else {
            None
        };
        Ok(Session {
            root,
            learn: false,
            observed: Vec::new(),
            left_out: Vec::new(),
            current: 0,
            pending: Vec::new(),
            applied: BTreeMap::new(),
            closed: BTreeMap::new(),
            rejected: BTreeMap::new(),
            wasm: BTreeMap::new(),
            contracts: BTreeMap::new(),
            queued_seq: BTreeMap::new(),
            writer,
        })
    }

    /// Learning mode: submitting steps are not held to their `expect`; what core answered
    /// is returned by [`Self::take_observed`] once known (at submission for a refusal, at
    /// the next close otherwise). Upload and deploy steps are still checked.
    pub fn set_learning(&mut self, on: bool) {
        self.learn = on;
    }

    /// What core did with each submitting step since the last call, in step order.
    pub fn take_observed(&mut self) -> Vec<Observed> {
        std::mem::take(&mut self.observed)
    }

    /// Steps core queued but left out of the ledger it then closed, since the last call.
    pub fn take_left_out(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.left_out)
    }

    /// Transactions queued and not yet closed.
    pub fn queued(&self) -> usize {
        self.pending.len()
    }

    fn lookup(&self, ctx: &mut ChainCtx, r: &SymRef) -> Result<String> {
        Ok(match r {
            SymRef::Account(role) => account_strkey(&key_of(ctx, role).verifying_key()),
            SymRef::Muxed { role, id } => muxed_strkey(&key_of(ctx, role).verifying_key(), *id),
            SymRef::Contract(label) => contract_strkey(
                self.contracts
                    .get(label)
                    .with_context(|| format!("no deploy step labelled {label} has run"))?,
            ),
            SymRef::Wasm(label) => hex::encode(
                self.wasm
                    .get(label)
                    .with_context(|| format!("no upload step labelled {label} has run"))?
                    .0,
            ),
            SymRef::Seq { role, add } => ledger_seq_on_chain(ctx, role)?
                .checked_add(*add)
                .with_context(|| format!("sequence number of {role} plus {add} overflows"))?
                .to_string(),
            SymRef::LedgerStart(n) => {
                let ledger = i64::from(ctx.driver().last_closed()) + i64::from(*n);
                (ledger << 32).to_string()
            }
            SymRef::Literal(s) => s.clone(),
        })
    }

    fn typed<T: serde::de::DeserializeOwned>(
        &self,
        ctx: &mut ChainCtx,
        value: &Value,
        what: &str,
    ) -> Result<T> {
        super::ir::typed(value, what, &mut |r| self.lookup(ctx, r))
    }

    /// The sequence number the next transaction of `role` carries.
    fn next_seq(&self, ctx: &mut ChainCtx, role: &str) -> Result<i64> {
        let on_chain = ledger_seq_on_chain(ctx, role)?;
        let last = self
            .queued_seq
            .get(role)
            .copied()
            .map_or(on_chain, |q| q.max(on_chain));
        last.checked_add(1)
            .with_context(|| format!("{role}'s sequence number cannot grow"))
    }

    fn queue(
        &mut self,
        ctx: &mut ChainCtx,
        (role, seq): (&str, i64),
        label: &Option<String>,
        env: &TransactionEnvelope,
        expect: &TxExpect,
        kind: Kind,
    ) -> Result<()> {
        if self.learn && matches!(kind, Kind::Tx) {
            let sub = ctx.driver().node().submit(env)?;
            match (sub.status, sub.error) {
                (SubmitStatus::Error, Some(result)) => {
                    let ledger = ctx.driver().last_closed();
                    self.observed.push(Observed {
                        step: self.current,
                        outcome: Outcome::Rejected,
                        result: result.clone(),
                        ledger,
                    });
                    if let Some(label) = label {
                        self.rejected.insert(
                            label.clone(),
                            Rejected {
                                env: env.clone(),
                                result,
                                ledger,
                            },
                        );
                    }
                    return Ok(());
                }
                (SubmitStatus::Pending, _) => {}
                (status, _) => bail!("core answered {status:?} to the submission"),
            }
            let hash = envelope_hash(env, ctx.driver().network_id())?;
            self.queued_seq.insert(role.to_owned(), seq);
            self.pending.push(Pending {
                step: self.current,
                label: label.clone(),
                hash,
                expect: expect.clone(),
                kind,
            });
            return Ok(());
        }
        if expect.outcome == Outcome::Rejected {
            let result = ctx.submit_rejected(env)?;
            self.check_result(ctx, &result.result, expect)?;
            if let Some(label) = label {
                let ledger = ctx.driver().last_closed();
                self.rejected.insert(
                    label.clone(),
                    Rejected {
                        env: env.clone(),
                        result,
                        ledger,
                    },
                );
            }
            return Ok(());
        }
        let hash = ctx.driver().queue(env)?;
        self.queued_seq.insert(role.to_owned(), seq);
        self.pending.push(Pending {
            step: self.current,
            label: label.clone(),
            hash,
            expect: expect.clone(),
            kind,
        });
        Ok(())
    }

    fn check_result(
        &self,
        ctx: &mut ChainCtx,
        got: &TransactionResultResult,
        expect: &TxExpect,
    ) -> Result<()> {
        if let Some(want) = &expect.result {
            let want: TransactionResultResult = self.typed(ctx, want, "expected result")?;
            ensure!(
                *got == want,
                "core answered {}, the recipe expects {}",
                serde_json::to_string(got)?,
                serde_json::to_string(&want)?
            );
        }
        Ok(())
    }

    /// Checks every queued transaction against the ledger that just closed.
    fn settle(&mut self, ctx: &mut ChainCtx) -> Result<()> {
        for p in std::mem::take(&mut self.pending) {
            let applied = match ctx.driver().applied_in_last(&p.hash) {
                Ok(a) => a,
                // core can drop a queued transaction from the set it closes, for one when
                // its time bounds have passed by the close time
                Err(ChainError::Missing { .. }) if self.learn && matches!(p.kind, Kind::Tx) => {
                    self.left_out.push(p.step);
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            let what = p.label.as_deref().unwrap_or("an unlabelled step");
            if self.learn && matches!(p.kind, Kind::Tx) {
                self.observed.push(Observed {
                    step: p.step,
                    outcome: if applied.succeeded() {
                        Outcome::Success
                    } else {
                        Outcome::Failed
                    },
                    result: applied.result.clone(),
                    ledger: applied.ledger,
                });
                if let Some(label) = p.label {
                    self.applied.insert(label, applied);
                }
                continue;
            }
            match p.expect.outcome {
                Outcome::Success => ensure!(
                    applied.succeeded(),
                    "{what}: ledger {} answered {}, expected success",
                    applied.ledger,
                    serde_json::to_string(&applied.result.result)?
                ),
                Outcome::Failed => ensure!(
                    !applied.succeeded(),
                    "{what}: ledger {} applied it successfully, expected failure",
                    applied.ledger
                ),
                Outcome::Rejected => unreachable!("rejected steps are never queued"),
            }
            self.check_result(ctx, &applied.result.result, &p.expect)
                .with_context(|| what.to_owned())?;
            match &p.kind {
                Kind::Tx => {}
                Kind::Upload(hash) => match applied.return_value() {
                    Some(ScVal::Bytes(b)) if b.as_slice() == hash.0.as_slice() => {}
                    other => bail!("{what}: upload returned {other:?}, expected the code hash"),
                },
                Kind::Deploy(id) => match applied.return_value() {
                    Some(ScVal::Address(ScAddress::Contract(got))) if got == id => {}
                    other => bail!("{what}: deploy returned {other:?}, expected {id:?}"),
                },
            }
            if let Some(label) = p.label {
                self.applied.insert(label, applied);
            }
        }
        self.queued_seq.clear();
        Ok(())
    }

    /// Runs one step; `index` is its position in the recipe, reported back in learning mode.
    pub fn step(&mut self, ctx: &mut ChainCtx, index: usize, step: &Step) -> Result<()> {
        self.current = index;
        match step {
            Step::Fund {
                label,
                roles,
                balance,
            } => {
                let balance: i64 = balance
                    .parse()
                    .with_context(|| format!("balance {balance:?}"))?;
                let root = ctx.root().clone();
                let seq = self.next_seq(ctx, "root")?;
                let mut b = TransactionBuilder::new(muxed_account(&root.verifying_key()), seq);
                for role in roles {
                    b = b.operation(
                        None,
                        OperationBody::CreateAccount(CreateAccountOp {
                            destination: account_id(&ctx.account(role).verifying_key()),
                            starting_balance: balance,
                        }),
                    );
                }
                let env = ctx.sign(b.build()?, &[&root])?;
                self.queue(
                    ctx,
                    ("root", seq),
                    label,
                    &env,
                    &TxExpect::default(),
                    Kind::Tx,
                )
            }
            Step::Tx {
                label,
                source,
                source_mux_id,
                operations,
                signers,
                memo,
                fee,
                cond,
                expect,
                fee_bump,
            } => {
                let key = key_of(ctx, source);
                let muxed = match source_mux_id {
                    None => muxed_account(&key.verifying_key()),
                    Some(id) => muxed_account_with_id(
                        &key.verifying_key(),
                        id.parse()
                            .with_context(|| format!("source_mux_id {id:?}"))?,
                    ),
                };
                let seq = self.next_seq(ctx, source)?;
                let mut b = TransactionBuilder::new(muxed, seq);
                if let Some(fee) = fee {
                    b = b.fee(*fee);
                }
                if let Some(memo) = memo {
                    b = b.memo(self.typed::<Memo>(ctx, memo, "memo")?);
                }
                if let Some(cond) = cond {
                    b = b.preconditions(self.typed::<Preconditions>(ctx, cond, "cond")?);
                }
                for (i, op) in operations.iter().enumerate() {
                    let op: Operation = self.typed(ctx, op, &format!("operation {i}"))?;
                    b = b.operation(op.source_account, op.body);
                }
                let keys: Vec<SigningKey> = if signers.is_empty() {
                    vec![key]
                } else {
                    signers.iter().map(|r| key_of(ctx, r)).collect()
                };
                let refs: Vec<&SigningKey> = keys.iter().collect();
                let mut env = ctx.sign(b.build()?, &refs)?;
                if let Some(bump) = fee_bump {
                    env = self.fee_bump(ctx, env, bump, operations.len())?;
                }
                self.queue(ctx, (source, seq), label, &env, expect, Kind::Tx)
            }
            Step::Close { label } => {
                let seq = ctx.driver().close_empty()?;
                self.settle(ctx)?;
                if let Some(label) = label {
                    self.closed.insert(label.clone(), seq);
                }
                Ok(())
            }
            Step::Upgrade {
                label,
                protocol_version,
                base_fee,
                base_reserve,
                max_tx_set_size,
                max_soroban_tx_set_size,
                flags,
                config,
            } => {
                // the contract has this upgrade; this interpreter does not arm it yet
                ensure!(
                    max_soroban_tx_set_size.is_none(),
                    "an upgrade step with max_soroban_tx_set_size: this interpreter does not arm it yet"
                );
                let mut params: Vec<(&str, String)> = Vec::new();
                let mut want = Vec::new();
                if let Some(v) = protocol_version {
                    params.push(("protocolversion", v.to_string()));
                    want.push(LedgerUpgrade::Version(*v));
                }
                if let Some(v) = base_fee {
                    params.push(("basefee", v.to_string()));
                    want.push(LedgerUpgrade::BaseFee(*v));
                }
                if let Some(v) = base_reserve {
                    params.push(("basereserve", v.to_string()));
                    want.push(LedgerUpgrade::BaseReserve(*v));
                }
                if let Some(v) = max_tx_set_size {
                    params.push(("maxtxsetsize", v.to_string()));
                    want.push(LedgerUpgrade::MaxTxSetSize(*v));
                }
                if let Some(v) = flags {
                    params.push(("flags", v.to_string()));
                    want.push(LedgerUpgrade::Flags(*v));
                }
                if !config.is_empty() {
                    let key = self.store_config(ctx, config)?;
                    params.push(("configupgradesetkey", key.key_base64()?));
                    want.push(LedgerUpgrade::Config(key.key.clone()));
                }
                let pairs: Vec<(&str, &str)> =
                    params.iter().map(|(k, v)| (*k, v.as_str())).collect();
                let seq = ctx.driver().close_with_upgrades(&pairs)?;
                if let Some(v) = protocol_version {
                    ctx.set_protocol(*v);
                }
                let applied = upgrades(ctx.driver().last_meta());
                for w in &want {
                    ensure!(
                        applied.contains(w),
                        "ledger {seq} applied {applied:?}, which lacks {w:?}"
                    );
                }
                self.settle(ctx)?;
                if let Some(label) = label {
                    self.closed.insert(label.clone(), seq);
                }
                Ok(())
            }
            Step::Upload {
                label,
                source,
                wasm,
                sha256,
            } => {
                let path = self.root.join(wasm);
                let bytes =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
                let digest: [u8; 32] = Sha256::digest(&bytes).into();
                ensure!(
                    hex::encode(digest) == *sha256,
                    "{wasm} has sha256 {}, the recipe pins {sha256}",
                    hex::encode(digest)
                );
                let key = key_of(ctx, source);
                let seq = self.next_seq(ctx, source)?;
                let hf = HostFunction::UploadContractWasm(bytes.try_into()?);
                let env = ctx.driver().soroban_at(&key, hf, seq)?;
                let hash = Hash(digest);
                self.wasm.insert(label.clone(), hash.clone());
                let label = Some(label.clone());
                self.queue(
                    ctx,
                    (source, seq),
                    &label,
                    &env,
                    &TxExpect::default(),
                    Kind::Upload(hash),
                )
            }
            Step::Deploy {
                label,
                source,
                code,
                salt,
                constructor_args,
            } => {
                let key = key_of(ctx, source);
                let code_hash = self
                    .wasm
                    .get(code)
                    .with_context(|| format!("no upload step labelled {code} has run"))?
                    .clone();
                let salt: [u8; 32] = hex::decode(salt)?
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("salt is not 32 bytes"))?;
                let deployer = account_address(&key.verifying_key());
                let id = contract_id_from_address(ctx.driver().network_id(), &deployer, &salt)?;
                let mut args = Vec::with_capacity(constructor_args.len());
                for (i, a) in constructor_args.iter().enumerate() {
                    args.push(self.typed::<ScVal>(ctx, a, &format!("constructor argument {i}"))?);
                }
                let hf = HostFunction::CreateContractV2(CreateContractArgsV2 {
                    contract_id_preimage: contract_preimage(&deployer, &salt),
                    executable: ContractExecutable::Wasm(code_hash),
                    constructor_args: args.try_into()?,
                });
                let seq = self.next_seq(ctx, source)?;
                let env = ctx.driver().soroban_at(&key, hf, seq)?;
                self.contracts.insert(label.clone(), id.clone());
                let label = Some(label.clone());
                self.queue(
                    ctx,
                    (source, seq),
                    &label,
                    &env,
                    &TxExpect::default(),
                    Kind::Deploy(id),
                )
            }
            Step::Invoke {
                label,
                source,
                contract,
                function,
                args,
                auth,
                resources,
                expect,
            } => {
                // the contract has auth modes and resource overrides; this interpreter does not
                // build them yet
                ensure!(
                    auth.is_source() && resources.is_none(),
                    "an invoke step with auth or resources: this interpreter does not build them yet"
                );
                let key = key_of(ctx, source);
                let deployed = match SymRef::parse(contract) {
                    None => contract.clone(),
                    Some(Ok(SymRef::Contract(l))) => l,
                    Some(Ok(other)) => bail!("invoke needs a contract, {other} names none"),
                    Some(Err(e)) => bail!("{e}"),
                };
                let id = self
                    .contracts
                    .get(&deployed)
                    .with_context(|| format!("no deploy step labelled {deployed} has run"))?
                    .clone();
                let mut xdr_args = Vec::with_capacity(args.len());
                for (i, a) in args.iter().enumerate() {
                    xdr_args.push(self.typed::<ScVal>(ctx, a, &format!("argument {i}"))?);
                }
                let hf = HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(id),
                    function_name: ScSymbol(function.as_str().try_into()?),
                    args: xdr_args.try_into()?,
                });
                let seq = self.next_seq(ctx, source)?;
                let env = ctx.driver().soroban_at(&key, hf, seq)?;
                self.queue(ctx, (source, seq), label, &env, expect, Kind::Tx)
            }
            Step::HostFunction { .. } | Step::ExtendTtl { .. } | Step::Restore { .. } => {
                bail!(
                    "host-function, extend-ttl and restore steps: this interpreter does not run them yet"
                )
            }
            Step::Capture {
                slug,
                select,
                notes,
                fields,
                flags,
                issues,
                must_reject,
            } => {
                let spec = Spec {
                    notes: notes.clone(),
                    fields_exercised: fields.clone(),
                    flags: flags.clone(),
                    linked_issues: issues.clone(),
                    must_reject: *must_reject,
                };
                let of = select.of();
                let ledger_of = |this: &Self| -> Result<u32> {
                    if let Some(a) = this.applied.get(of) {
                        return Ok(a.ledger);
                    }
                    this.closed
                        .get(of)
                        .copied()
                        .with_context(|| format!("{of} has not closed in a ledger yet"))
                };
                match select {
                    Selection::Ledger { .. } => {
                        let ledger = ledger_of(self)?;
                        ctx.capture(slug, Selector::Ledger(ledger), spec)
                    }
                    Selection::Transaction { .. } => {
                        let applied = self
                            .applied
                            .get(of)
                            .with_context(|| format!("{of} has no applied transaction"))?;
                        ctx.capture(slug, Selector::transaction(applied), spec)
                    }
                    Selection::LedgerEntry { key, .. } => {
                        let ledger = ledger_of(self)?;
                        let key: LedgerKey = self.typed(ctx, key, "ledger key")?;
                        ctx.capture(slug, Selector::LedgerEntry { ledger, key }, spec)
                    }
                    Selection::Rejected { .. } => {
                        let r = self
                            .rejected
                            .get(of)
                            .with_context(|| format!("{of} was not refused at submission"))?;
                        let (env, result, ledger) = (r.env.clone(), r.result.clone(), r.ledger);
                        ctx.record_rejected(slug, &env, &result, ledger, spec)
                    }
                }
            }
        }
    }

    /// Wraps a signed transaction in a fee bump. The outer fee defaults to the base fee for
    /// each operation plus one, the least core accepts at the base fee rate.
    fn fee_bump(
        &self,
        ctx: &mut ChainCtx,
        inner: TransactionEnvelope,
        bump: &FeeBump,
        operations: usize,
    ) -> Result<TransactionEnvelope> {
        let TransactionEnvelope::Tx(inner) = inner else {
            bail!(
                "a fee bump wraps a v1 envelope, and protocol {} signs v0 envelopes",
                ctx.protocol()
            );
        };
        let payer = key_of(ctx, &bump.source);
        let fee = match bump.fee {
            Some(f) => f,
            None => i64::from(BASE_FEE) * (i64::try_from(operations)? + 1),
        };
        let tx = fee_bump(inner, muxed_account(&payer.verifying_key()), fee);
        let keys: Vec<SigningKey> = if bump.signers.is_empty() {
            vec![payer]
        } else {
            bump.signers.iter().map(|r| key_of(ctx, r)).collect()
        };
        let refs: Vec<&SigningKey> = keys.iter().collect();
        Ok(TransactionEnvelope::TxFeeBump(sign_fee_bump(
            tx,
            ctx.driver().network_id(),
            &refs,
        )?))
    }

    /// Stores a config upgrade set through the settings writer, deploying the writer first if
    /// the chain has none. Returns the prepared upgrade, armed by the caller.
    fn store_config(&mut self, ctx: &mut ChainCtx, config: &[Value]) -> Result<PreparedUpgrade> {
        ensure!(
            self.pending.is_empty(),
            "an upgrade with config entries needs ledgers of its own, but {} transactions are queued",
            self.pending.len()
        );
        let mut entries = Vec::with_capacity(config.len());
        for (i, c) in config.iter().enumerate() {
            entries.push(self.typed::<ConfigSettingEntry>(ctx, c, &format!("config entry {i}"))?);
        }
        let root = ctx.root().clone();
        let writer = match &self.writer {
            Some(w) => w.clone(),
            None => {
                let protocol = ctx.protocol();
                let wasm = echo_wasm_for(protocol).with_context(|| {
                    format!("no settings writer contract builds for protocol {protocol}")
                })?;
                let code = ctx.driver().upload(&root, wasm)?;
                let salt: [u8; 32] = Sha256::digest(SETTINGS_WRITER_SALT_DOMAIN).into();
                let w = ctx.driver().deploy(&root, &code, &salt)?;
                self.writer = Some(w.clone());
                w
            }
        };
        let upgrade = PreparedUpgrade::new(
            ConfigUpgradeSet {
                updated_entry: entries.try_into()?,
            },
            writer.clone(),
        )?;
        ctx.driver().invoke(
            &root,
            &writer,
            "put_temp",
            vec![upgrade.storage_key()?, upgrade.storage_value()?],
        )?;
        Ok(upgrade)
    }
}
