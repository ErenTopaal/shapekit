//! The handle a recipe drives its chain through.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::chain::{Applied, ChainDriver};
use shapekit_core_driver::keys::{account_id, derive_key, muxed_account};
use shapekit_core_driver::node::SubmitStatus;
use shapekit_core_driver::recorder::{ChainSummary, FIRST_LEDGER, canonical_frame};
use shapekit_core_driver::tx::{TransactionBuilder, envelope_hash, sign_transaction};
use shapekit_manifest::xdr::{self, result_code};
use shapekit_manifest::{CoreRejection, RejectionStage, Validity, decode_limits};
use stellar_xdr::{
    CreateAccountOp, Limits, OperationBody, Transaction, TransactionEnvelope, TransactionResult,
    WriteXdr,
};

use super::Network;
use super::select::{Selected, Selector, select};

/// What a recipe says about one capture: the fields it exercises, flags,
/// and a note for the entry. Linked issues default to the family's.
#[derive(Debug, Clone, Default)]
pub struct Spec {
    pub notes: String,
    pub fields_exercised: Vec<String>,
    pub flags: Vec<String>,
    pub linked_issues: Option<Vec<String>>,
    /// The bytes do not decode under the shared bounds (`shapekit_manifest::decode_limits`),
    /// so the entry is `decode = must-reject`.
    pub must_reject: bool,
}

impl Spec {
    pub fn new(notes: impl Into<String>) -> Self {
        Self {
            notes: notes.into(),
            ..Self::default()
        }
    }

    /// Field ids from `registry/fields.toml`.
    pub fn fields(mut self, ids: &[&str]) -> Self {
        self.fields_exercised = ids.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn flags(mut self, flags: &[&str]) -> Self {
        self.flags = flags.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn issues(mut self, urls: &[&str]) -> Self {
        self.linked_issues = Some(urls.iter().map(|s| s.to_string()).collect());
        self
    }

    /// For an envelope core refuses because its own parser cannot read it: the shared decode
    /// bounds refuse it as well, so a decoder must too.
    pub fn must_reject(mut self) -> Self {
        self.must_reject = true;
        self
    }
}

/// One captured value under one network, before the variants are merged.
#[derive(Debug, Clone)]
pub struct Captured {
    /// Entry slug without the family, `p30-set-options-payload-1-envelope`.
    pub slug: String,
    pub ledger_seq: u32,
    pub selected: Selected,
    pub validity: Validity,
    pub rejection: Option<CoreRejection>,
    pub spec: Spec,
}

/// A recipe's view of one chain: its keys, the node behind it, and the
/// captures made so far.
pub struct ChainCtx {
    driver: ChainDriver,
    recipe_id: String,
    network: Network,
    protocol: u32,
    root: SigningKey,
    captured: Vec<Captured>,
    slugs: BTreeSet<String>,
}

impl ChainCtx {
    pub(crate) fn new(
        driver: ChainDriver,
        recipe_id: &str,
        network: Network,
        protocol: u32,
    ) -> Self {
        Self {
            driver,
            recipe_id: recipe_id.to_owned(),
            network,
            protocol,
            root: shapekit_core_driver::keys::root_key(network.passphrase()),
            captured: Vec::new(),
            slugs: BTreeSet::new(),
        }
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub fn protocol(&self) -> u32 {
        self.protocol
    }

    /// The network's root account, funded at genesis.
    pub fn root(&self) -> &SigningKey {
        &self.root
    }

    /// The key of `role` in this recipe. The same under both passphrases.
    pub fn account(&self, role: &str) -> SigningKey {
        derive_key(&self.recipe_id, role)
    }

    pub fn network_id(&self) -> [u8; 32] {
        *self.driver.network_id()
    }

    /// The underlying driver, for steps the context has no helper for
    /// (Soroban uploads and invocations, settings).
    pub fn driver(&mut self) -> &mut ChainDriver {
        &mut self.driver
    }

    /// Creates an account for each role from the root account, in one
    /// transaction, `balance` stroops each.
    pub fn fund(&mut self, roles: &[&str], balance: i64) -> Result<Applied> {
        let ops = roles
            .iter()
            .map(|r| {
                OperationBody::CreateAccount(CreateAccountOp {
                    destination: account_id(&self.account(r).verifying_key()),
                    starting_balance: balance,
                })
            })
            .collect();
        let env = self.driver.classic(&self.root, ops)?;
        Ok(self.driver.apply_ok(&env)?)
    }

    /// A transaction builder for `source` carrying its next sequence number.
    pub fn tx(&self, source: &SigningKey) -> Result<TransactionBuilder> {
        let seq = self
            .driver
            .node()
            .account_seq(&account_id(&source.verifying_key()))?;
        Ok(TransactionBuilder::new(
            muxed_account(&source.verifying_key()),
            seq + 1,
        ))
    }

    /// Signs `tx` with each key under this chain's network id.
    pub fn sign(&self, tx: Transaction, signers: &[&SigningKey]) -> Result<TransactionEnvelope> {
        Ok(TransactionEnvelope::Tx(sign_transaction(
            tx,
            self.driver.network_id(),
            signers,
        )?))
    }

    /// One transaction from `source` with `ops`, signed by `source`.
    pub fn classic(
        &self,
        source: &SigningKey,
        ops: Vec<OperationBody>,
    ) -> Result<TransactionEnvelope> {
        Ok(self.driver.classic(source, ops)?)
    }

    /// Submits `env`, closes a ledger with it, and fails unless it succeeded.
    pub fn submit_and_close(&mut self, env: &TransactionEnvelope) -> Result<Applied> {
        Ok(self.driver.apply_ok(env)?)
    }

    /// Submits `env`, closes a ledger with it, and fails unless core
    /// included it and it failed there.
    pub fn submit_and_close_failing(&mut self, env: &TransactionEnvelope) -> Result<Applied> {
        let applied = self.driver.apply(env)?;
        ensure!(
            !applied.succeeded(),
            "ledger {}: transaction {} was expected to fail but succeeded",
            applied.ledger,
            hex::encode(applied.hash)
        );
        Ok(applied)
    }

    fn claim_slug(&mut self, slug: &str) -> Result<String> {
        ensure!(
            !slug.is_empty()
                && slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "capture slug {slug:?} must be lowercase kebab-case"
        );
        // protocol in the id keeps the chains of one family apart
        let full = format!("p{}-{slug}", self.protocol);
        ensure!(
            self.slugs.insert(full.clone()),
            "capture slug {slug} is used twice"
        );
        Ok(full)
    }

    /// Records the bytes `selector` picks from the canonical meta of an
    /// already closed ledger as entry `<family>.p<protocol>-<slug>` (plus
    /// `-envelope` and `-meta` for a transaction).
    pub fn capture(&mut self, slug: &str, selector: Selector, spec: Spec) -> Result<()> {
        ensure!(
            !spec.must_reject,
            "capture {slug}: what a closed ledger holds always decodes"
        );
        let full = self.claim_slug(slug)?;
        let ledger = selector.ledger();
        let frames = self.driver.recorder().frames();
        let frame = ledger
            .checked_sub(FIRST_LEDGER)
            .and_then(|i| frames.get(i as usize))
            .with_context(|| format!("capture {slug}: ledger {ledger} is not recorded yet"))?;
        ensure!(
            frame.ledger_seq == ledger,
            "capture {slug}: recorded frame {} where ledger {ledger} was expected",
            frame.ledger_seq
        );
        let canonical = canonical_frame(frame)?;
        let parts = select(
            &selector,
            &canonical.meta,
            &canonical.xdr,
            &self.network_id(),
        )
        .with_context(|| format!("capture {slug}"))?;
        for selected in parts {
            self.captured.push(Captured {
                slug: format!("{full}{}", selected.suffix),
                ledger_seq: ledger,
                selected,
                validity: Validity::Valid,
                rejection: None,
                spec: spec.clone(),
            });
        }
        Ok(())
    }

    /// Submits `env`, which core must refuse at submission, and records the
    /// envelope as an invalid-by-core entry carrying core's result. Returns
    /// that result. No ledger closes.
    pub fn capture_rejected(
        &mut self,
        slug: &str,
        env: &TransactionEnvelope,
        spec: Spec,
    ) -> Result<TransactionResult> {
        let full = self.claim_slug(slug)?;
        let bytes = env.to_xdr(Limits::none())?;
        let decodes = xdr::decode(
            "TransactionEnvelope",
            &bytes,
            decode_limits("TransactionEnvelope", bytes.len()),
        )
        .is_ok();
        ensure!(
            decodes != spec.must_reject,
            "capture {slug}: the envelope {} under the shared decode limits",
            if decodes {
                "decodes, so it is not must-reject"
            } else {
                "does not decode, so it has to be marked must-reject"
            }
        );
        let sub = self.driver.node().submit(env)?;
        let result = match (sub.status, sub.error) {
            (SubmitStatus::Error, Some(r)) => r,
            (status, _) => bail!(
                "capture {slug}: core answered {status:?}, the envelope was expected to be refused"
            ),
        };
        let hash = envelope_hash(env, self.driver.network_id())?;
        self.captured.push(Captured {
            slug: full,
            ledger_seq: self.driver.last_closed(),
            selected: Selected {
                suffix: "",
                xdr_type: "TransactionEnvelope",
                bytes,
                lcm_version: None,
                tx_meta_version: None,
                tx_index: None,
                tx_hash: Some(hex::encode(hash)),
                normalized: false,
            },
            validity: Validity::InvalidByCore,
            rejection: Some(CoreRejection {
                stage: RejectionStage::Submit,
                result_code: result_code(&result)?,
                result_xdr: result.to_xdr_base64(Limits::none())?,
            }),
            spec,
        });
        Ok(result)
    }

    /// Writes the chain into `target` and stops the node.
    pub(crate) fn finish(self, target: &Path) -> Result<(ChainSummary, Vec<Captured>)> {
        let summary = self
            .driver
            .finish(target)
            .with_context(|| format!("writing the chain to {}", target.display()))?;
        Ok((summary, self.captured))
    }
}
