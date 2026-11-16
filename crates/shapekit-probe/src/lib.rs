//! Asks core which envelope fields it validates.
//!
//! Every attempt starts from the same valid transaction, a payment of one lumen from a funded
//! account to another, changes exactly one field, signs again and submits to a standalone core.
//! Core's answer is one of three: it refuses the transaction at submission (with a result
//! code), it includes it in the next ledger where it fails, or it includes it and it succeeds.
//! Each attempt uses its own source account, so no attempt's sequence number or balance depends
//! on another's.

pub mod variants;

use std::path::Path;

use anyhow::{Context, Result, bail};
use ed25519_dalek::SigningKey;
use shapekit_contracts::validation::{ApplyStatus, ProbeAttempt, SubmitStatus};
use shapekit_core_driver::chain::{ChainDriver, ChainError};
use shapekit_core_driver::config::NodeConfig;
use shapekit_core_driver::keys::{account_id, derive_key, muxed_account};
use shapekit_core_driver::node::SubmitStatus as NodeStatus;
use shapekit_core_driver::recorder::ChainSummary;
use shapekit_core_driver::tx::{BASE_FEE, TransactionBuilder, sign_transaction};
use stellar_xdr::{
    Asset, CreateAccountOp, Limits, OperationBody, PaymentOp, Transaction, TransactionEnvelope,
    TransactionResult, WriteXdr,
};

/// Derives every key the probe uses, so attempts are the same on every run.
pub const KEY_DOMAIN: &str = "shapekit.probe";

/// Starting balance of each probe account, 1,000 XLM.
pub const BALANCE: i64 = 10_000_000_000;

/// The base payment's amount, one lumen.
pub const AMOUNT: i64 = 10_000_000;

/// Create-account operations per funding transaction, core's limit.
const OPS_PER_TX: usize = 100;

/// A transaction under edit and the keys that will sign it.
pub struct Draft {
    pub tx: Transaction,
    pub signers: Vec<SigningKey>,
}

/// What an edit may refer to.
pub struct Ctx<'a> {
    /// The attempt's own source account, which signs the base transaction.
    pub source: &'a SigningKey,
    /// The payment's destination.
    pub destination: &'a SigningKey,
    /// A key that is no signer of any probe account.
    pub stranger: &'a SigningKey,
    pub last_closed: u32,
}

/// One single-field change of the base transaction.
pub struct Variant {
    /// The `registry/fields.toml` id of the changed field.
    pub field: &'static str,
    /// The value, in words.
    pub name: &'static str,
    pub edit: fn(&mut Draft, &Ctx),
}

/// A standalone core with funded probe accounts.
pub struct Probe {
    driver: ChainDriver,
    sources: Vec<SigningKey>,
    used: usize,
    destination: SigningKey,
    stranger: SigningKey,
}

/// The XDR-JSON arm name of a transaction result, `tx_success` or `tx_bad_seq`.
pub fn result_code(result: &TransactionResult) -> Result<String> {
    match serde_json::to_value(&result.result)? {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Object(m) if m.len() == 1 => {
            Ok(m.keys().next().expect("one key").clone())
        }
        other => bail!("unexpected XDR-JSON for a transaction result: {other}"),
    }
}

impl Probe {
    /// Boots core and funds `attempts` source accounts plus the destination.
    pub fn start(cfg: NodeConfig, workdir: &Path, attempts: usize) -> Result<Self> {
        let mut driver = ChainDriver::start(cfg, workdir).context("starting core")?;
        let sources: Vec<SigningKey> = (0..attempts)
            .map(|i| derive_key(KEY_DOMAIN, &format!("source-{i}")))
            .collect();
        let destination = derive_key(KEY_DOMAIN, "destination");
        let stranger = derive_key(KEY_DOMAIN, "stranger");
        let root = shapekit_core_driver::keys::root_key(&driver.node().config().passphrase);
        let funded: Vec<&SigningKey> = sources.iter().chain([&destination]).collect();
        for chunk in funded.chunks(OPS_PER_TX) {
            let ops = chunk
                .iter()
                .map(|k| {
                    OperationBody::CreateAccount(CreateAccountOp {
                        destination: account_id(&k.verifying_key()),
                        starting_balance: BALANCE,
                    })
                })
                .collect();
            let env = driver.classic(&root, ops)?;
            driver
                .apply_ok(&env)
                .context("funding the probe accounts")?;
        }
        Ok(Self {
            driver,
            sources,
            used: 0,
            destination,
            stranger,
        })
    }

    /// The valid transaction every attempt changes: `source` pays the destination one lumen
    /// at the base fee, with no memo and no preconditions.
    fn base(&self, source: &SigningKey) -> Result<Transaction> {
        let seq = self
            .driver
            .node()
            .account_seq(&account_id(&source.verifying_key()))?;
        Ok(
            TransactionBuilder::new(muxed_account(&source.verifying_key()), seq + 1)
                .fee(BASE_FEE)
                .operation(
                    None,
                    OperationBody::Payment(PaymentOp {
                        destination: muxed_account(&self.destination.verifying_key()),
                        asset: Asset::Native,
                        amount: AMOUNT,
                    }),
                )
                .build()?,
        )
    }

    /// Submits `variant` and, when core queues it, closes a ledger with it.
    pub fn attempt(&mut self, variant: &Variant) -> Result<(ProbeAttempt, TransactionEnvelope)> {
        let source = self
            .sources
            .get(self.used)
            .context("every funded probe account is used; start the probe with more")?
            .clone();
        self.used += 1;
        let mut draft = Draft {
            tx: self.base(&source)?,
            signers: vec![source.clone()],
        };
        let ctx = Ctx {
            source: &source,
            destination: &self.destination,
            stranger: &self.stranger,
            last_closed: self.driver.last_closed(),
        };
        (variant.edit)(&mut draft, &ctx);
        let keys: Vec<&SigningKey> = draft.signers.iter().collect();
        let env =
            TransactionEnvelope::Tx(sign_transaction(draft.tx, self.driver.network_id(), &keys)?);
        let attempt = match self.driver.apply(&env) {
            Ok(applied) => ProbeAttempt {
                variant: variant.name.to_owned(),
                submission: SubmitStatus::Pending,
                result_code: Some(result_code(&applied.result)?),
                result_xdr: Some(applied.result.to_xdr_base64(Limits::none())?),
                applied: if applied.succeeded() {
                    ApplyStatus::Succeeded
                } else {
                    ApplyStatus::Failed
                },
                witnesses: Vec::new(),
                carrier: None,
            },
            Err(ChainError::NotQueued { status, result }) => ProbeAttempt {
                variant: variant.name.to_owned(),
                submission: match status {
                    NodeStatus::Pending => SubmitStatus::Pending,
                    NodeStatus::Duplicate => SubmitStatus::Duplicate,
                    NodeStatus::Error => SubmitStatus::Error,
                    NodeStatus::TryAgainLater => SubmitStatus::TryAgainLater,
                    NodeStatus::Filtered => SubmitStatus::Filtered,
                },
                result_code: result.as_deref().map(result_code).transpose()?,
                result_xdr: result
                    .as_deref()
                    .map(|r| r.to_xdr_base64(Limits::none()))
                    .transpose()?,
                applied: ApplyStatus::NotApplied,
                witnesses: Vec::new(),
                carrier: None,
            },
            Err(e) => return Err(e).with_context(|| format!("attempt {:?}", variant.name)),
        };
        Ok((attempt, env))
    }

    pub fn driver(&self) -> &ChainDriver {
        &self.driver
    }

    /// Writes the chain the attempts were closed in to `target` and stops the node. Dropping
    /// the probe stops the node without writing anything.
    pub fn finish(self, target: &Path) -> Result<ChainSummary> {
        Ok(self.driver.finish(target)?)
    }
}
