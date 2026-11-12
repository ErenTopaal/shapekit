//! `cap40.signed-payload-signer`: CAP-40 signed-payload signers on an
//! account, as a transaction's only authorisation, as an extra signer, under
//! sponsorship and its revoke, and removed again; plus the two envelopes
//! core refuses for an empty payload.
//!
//! | ledger | content                                                         |
//! |--------|-----------------------------------------------------------------|
//! | 2      | protocol upgrade                                                |
//! | 3      | root funds alice, bob and sponsor                               |
//! | 4 to 6 | alice adds signed-payload signers with 1, 4 and 32 byte payloads |
//! | 7      | sponsor sponsors alice adding a 64 byte payload signer          |
//! | 8      | a payment from alice signed only by the 32 byte payload signer  |
//! | 9      | a payment from alice requiring a 64 byte payload extra signer   |
//! | 10     | sponsor revokes its sponsorship of the 64 byte signer           |
//! | 11     | alice removes the 1 byte signer                                 |
//!
//! The refused envelopes never reach a ledger: an extra signer with an empty
//! payload (`txMALFORMED`, `TransactionFrame::commonValidPreSeqNum`) and a
//! SetOptions adding one (`SET_OPTIONS_BAD_SIGNER`,
//! `SetOptionsOpFrame::doCheckValid`, both stellar-core v30.0.0).

use anyhow::{Context, Result, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::keys::{account_id, muxed_account, signed_payload_signer};
use shapekit_core_driver::tx::{push_signature, sign_signed_payload};
use stellar_xdr::{
    Asset, BeginSponsoringFutureReservesOp, LedgerEntryData, LedgerKey, LedgerKeyAccount,
    OperationBody, PaymentOp, Preconditions, PreconditionsV2, RevokeSponsorshipOp,
    RevokeSponsorshipOpSigner, SetOptionsOp, Signer, SignerKey, TransactionResultResult,
};

use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap40.signed-payload-signer";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Payment amount, 1 XLM.
const AMOUNT: i64 = 10_000_000;

/// The family's recipe at one protocol. Protocol 30 is the current tier;
/// protocol 28 is what the pre-fix wallet-backend commits read.
pub struct SignedPayloadSigner {
    pub protocol: u32,
}

/// `n` deterministic payload bytes for `use_`: SHA-256 blocks of the
/// domain, the use, the length and a block counter, cut to `n`.
pub fn payload(use_: &str, n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n + 32);
    let mut block = 0u8;
    while out.len() < n {
        let mut h = Sha256::new();
        h.update(b"shapekit/cap40/payload/v1");
        h.update(use_.as_bytes());
        h.update((n as u32).to_be_bytes());
        h.update([block]);
        out.extend_from_slice(&h.finalize());
        block += 1;
    }
    out.truncate(n);
    out
}

fn add_signer(key: SignerKey, weight: u32) -> OperationBody {
    OperationBody::SetOptions(SetOptionsOp {
        inflation_dest: None,
        clear_flags: None,
        set_flags: None,
        master_weight: None,
        low_threshold: None,
        med_threshold: None,
        high_threshold: None,
        home_domain: None,
        signer: Some(Signer { key, weight }),
    })
}

fn payment(to: &SigningKey) -> OperationBody {
    OperationBody::Payment(PaymentOp {
        destination: muxed_account(&to.verifying_key()),
        asset: Asset::Native,
        amount: AMOUNT,
    })
}

const SET_OPTIONS_FIELDS: [&str; 2] =
    ["op.set_options.signer", "signer_key.signed_payload.payload"];

impl Recipe for SignedPayloadSigner {
    fn id(&self) -> &str {
        FAMILY
    }

    fn family(&self) -> &str {
        FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["alice", "bob", "sponsor"], BALANCE)?;
        let alice = ctx.account("alice");
        let bob = ctx.account("bob");
        let sponsor = ctx.account("sponsor");
        // the signer keys are not accounts, only keys that sign payloads
        let payload_key = ctx.account("payload-signer");
        let extra_key = ctx.account("extra-signer");
        let pk = payload_key.verifying_key();

        for n in [1usize, 4, 32] {
            let key = signed_payload_signer(&pk, &payload("account", n))?;
            let env = ctx.classic(&alice, vec![add_signer(key, 1)])?;
            let applied = ctx.submit_and_close(&env)?;
            ctx.capture(
                &format!("set-options-payload-{n}"),
                Selector::transaction(&applied),
                Spec::new(format!(
                    "SetOptions adds a signed-payload signer with a {n} byte payload to the source account"
                ))
                .fields(&SET_OPTIONS_FIELDS),
            )?;
        }

        // the 64 byte signer is added under sponsorship, so its reserve and
        // the account's signer sponsoring ids name the sponsor
        let p64 = signed_payload_signer(&pk, &payload("account", 64))?;
        let alice_mux = Some(muxed_account(&alice.verifying_key()));
        let tx = ctx
            .tx(&sponsor)?
            .operation(
                None,
                OperationBody::BeginSponsoringFutureReserves(BeginSponsoringFutureReservesOp {
                    sponsored_id: account_id(&alice.verifying_key()),
                }),
            )
            .operation(alice_mux.clone(), add_signer(p64.clone(), 1))
            .operation(alice_mux, OperationBody::EndSponsoringFutureReserves)
            .build()?;
        let env = ctx.sign(tx, &[&sponsor, &alice])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "sponsored-set-options-payload-64",
            Selector::transaction(&applied),
            Spec::new(
                "a sponsor pays the reserve of a signed-payload signer with a 64 byte payload, inside begin and end sponsoring",
            )
            .fields(&SET_OPTIONS_FIELDS),
        )?;
        let alice_key = LedgerKey::Account(LedgerKeyAccount {
            account_id: account_id(&alice.verifying_key()),
        });
        ctx.capture(
            "account-four-payload-signers",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: alice_key.clone(),
            },
            Spec::new(
                "an account entry holding signed-payload signers with 1, 4, 32 and 64 byte payloads, one of them sponsored",
            )
            .fields(&["signer_key.signed_payload.payload"]),
        )?;
        let signers = ctx
            .driver()
            .node()
            .ledger_entries(std::slice::from_ref(&alice_key))?;
        match signers.entries[0].entry.as_ref().map(|e| &e.data) {
            Some(LedgerEntryData::Account(a)) => ensure!(
                a.signers.len() == 4
                    && a.signers
                        .iter()
                        .all(|s| matches!(s.key, SignerKey::Ed25519SignedPayload(_))),
                "alice should hold four signed-payload signers, has {:?}",
                a.signers
            ),
            other => anyhow::bail!("alice's account is missing: {other:?}"),
        }

        // authorised by the 32 byte payload signer alone: its signature is
        // over the payload, not the transaction, so the envelope is the same
        // under both passphrases
        let p32 = payload("account", 32);
        let tx = ctx.tx(&alice)?.operation(None, payment(&bob)).build()?;
        let mut env = ctx.sign(tx, &[])?;
        push_signature(&mut env, sign_signed_payload(&payload_key, &p32)?)?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "authorised-by-payload-32",
            Selector::ledger_of(&applied),
            Spec::new(
                "a payment whose only signature is a signed-payload signature (hint is the key's last four bytes xor the payload's) for a signer of the source account",
            )
            .fields(&[
                "tx.signatures",
                "tx.signatures.hint",
                "signer_key.signed_payload.payload",
            ]),
        )?;

        let extra_payload = payload("extra", 64);
        let extra = signed_payload_signer(&extra_key.verifying_key(), &extra_payload)?;
        let tx = ctx
            .tx(&alice)?
            .preconditions(Preconditions::V2(PreconditionsV2 {
                time_bounds: None,
                ledger_bounds: None,
                min_seq_num: None,
                min_seq_age: 0.into(),
                min_seq_ledger_gap: 0,
                extra_signers: vec![extra].try_into()?,
            }))
            .operation(None, payment(&bob))
            .build()?;
        let mut env = ctx.sign(tx, &[&alice])?;
        push_signature(&mut env, sign_signed_payload(&extra_key, &extra_payload)?)?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "extra-signer-payload-64",
            Selector::transaction(&applied),
            Spec::new(
                "a payment whose preconditions require a signed-payload extra signer with a 64 byte payload, signed by the source and that signer",
            )
            .fields(&[
                "tx.cond.extra_signers",
                "tx.signatures",
                "tx.signatures.hint",
                "signer_key.signed_payload.payload",
            ]),
        )?;

        let revoke = OperationBody::RevokeSponsorship(RevokeSponsorshipOp::Signer(
            RevokeSponsorshipOpSigner {
                account_id: account_id(&alice.verifying_key()),
                signer_key: p64,
            },
        ));
        let env = ctx.classic(&sponsor, vec![revoke])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "revoke-sponsorship-payload-64",
            Selector::transaction(&applied),
            Spec::new(
                "the sponsor revokes its sponsorship of a signed-payload signer, keyed by the signer key, so the account takes over the reserve",
            )
            .fields(&["op.revoke_sponsorship", "signer_key.signed_payload.payload"]),
        )?;

        let p1 = signed_payload_signer(&pk, &payload("account", 1))?;
        let env = ctx.classic(&alice, vec![add_signer(p1, 0)])?;
        let applied = ctx.submit_and_close(&env)?;
        ctx.capture(
            "remove-signer-payload-1",
            Selector::transaction(&applied),
            Spec::new("SetOptions with weight 0 removes the 1 byte payload signer")
                .fields(&SET_OPTIONS_FIELDS),
        )?;

        let empty = signed_payload_signer(&extra_key.verifying_key(), &[])?;
        let tx = ctx
            .tx(&alice)?
            .preconditions(Preconditions::V2(PreconditionsV2 {
                time_bounds: None,
                ledger_bounds: None,
                min_seq_num: None,
                min_seq_age: 0.into(),
                min_seq_ledger_gap: 0,
                extra_signers: vec![empty.clone()].try_into()?,
            }))
            .operation(None, payment(&bob))
            .build()?;
        let env = ctx.sign(tx, &[&alice])?;
        let r = ctx.capture_rejected(
            "extra-signer-empty-payload",
            &env,
            Spec::new(
                "an extra signer with an empty signed payload, which core refuses as malformed before the transaction reaches a ledger",
            )
            .fields(&["tx.cond.extra_signers", "signer_key.signed_payload.payload"]),
        )?;
        ensure!(
            matches!(r.result, TransactionResultResult::TxMalformed),
            "core answered {:?} for an empty extra signer payload, expected tx_malformed",
            r.result
        );

        let env = ctx.classic(&alice, vec![add_signer(empty, 1)])?;
        let r = ctx.capture_rejected(
            "set-options-empty-payload",
            &env,
            Spec::new(
                "SetOptions adding a signed-payload signer with an empty payload, which core refuses with SET_OPTIONS_BAD_SIGNER before the transaction reaches a ledger",
            )
            .fields(&SET_OPTIONS_FIELDS),
        )?;
        ensure_bad_signer(&r.result)
    }
}

fn ensure_bad_signer(r: &TransactionResultResult) -> Result<()> {
    use stellar_xdr::{OperationResult, OperationResultTr, SetOptionsResult};
    let TransactionResultResult::TxFailed(ops) = r else {
        anyhow::bail!("core answered {r:?} for an empty signer payload, expected tx_failed");
    };
    let op = ops.first().context("tx_failed without operation results")?;
    ensure!(
        matches!(
            op,
            OperationResult::OpInner(OperationResultTr::SetOptions(SetOptionsResult::BadSigner))
        ),
        "core answered {op:?} for an empty signer payload, expected set_options bad_signer"
    );
    Ok(())
}
