//! Signing an edited envelope again. Every signature the probe can make is made over the
//! edited bytes, so core answers about the changed value and not about a stale signature;
//! the one exception is a signature the edit itself changed, which stays as the edit wrote it.

use anyhow::{Context, Result, bail};
use ed25519_dalek::SigningKey;
use serde_json::Value;
use shapekit_core_driver::tx::{
    account_signature, authorization_payload, fee_bump_hash, sign_hash, sign_signed_payload,
    transaction_hash, transaction_v0_hash,
};
use stellar_xdr::{
    DecoratedSignature, ScVal, SignerKey, SorobanAuthorizationEntry, SorobanCredentials,
    TransactionEnvelope,
};

use crate::edit::{get, set};

/// One Soroban authorization entry with address credentials and the keys behind it.
#[derive(Clone)]
pub struct AuthSigner {
    /// JSON pointer to the `SorobanAuthorizationEntry` inside the envelope.
    pub entry: String,
    /// The account keys whose signatures the credentials carry.
    pub keys: Vec<SigningKey>,
    /// JSON pointers to `SorobanDelegateSignature` objects and the account key each one
    /// stands for; a delegate signs the entry's own payload.
    pub delegates: Vec<(String, SigningKey)>,
}

/// Who signs what.
#[derive(Clone, Default)]
pub struct SignPlan {
    /// Ed25519 signatures over the (inner) transaction hash, in order.
    pub tx: Vec<SigningKey>,
    /// CAP-40 signatures: the key and a JSON pointer to the signed-payload `SignerKey` whose
    /// payload it signs. They follow the hash signatures.
    pub payloads: Vec<(SigningKey, String)>,
    /// Signatures of the fee bump's outer transaction, for fee bump envelopes.
    pub fee_bump: Vec<SigningKey>,
    pub auth: Vec<AuthSigner>,
}

/// The pointer of the transaction signatures and, for a fee bump, of the outer ones.
fn signature_slots(env: &Value) -> Result<(String, Option<String>)> {
    if env.get("tx").is_some() {
        Ok(("/tx/signatures".into(), None))
    } else if env.get("tx_v0").is_some() {
        Ok(("/tx_v0/signatures".into(), None))
    } else if env.get("tx_fee_bump").is_some() {
        Ok((
            "/tx_fee_bump/tx/inner_tx/tx/signatures".into(),
            Some("/tx_fee_bump/signatures".into()),
        ))
    } else {
        bail!("not a transaction envelope: {env}")
    }
}

/// Whether the edit at `edit` wrote inside `slot`, so the slot keeps what the edit wrote.
fn edited(edit: Option<&str>, slot: &str) -> bool {
    edit.is_some_and(|e| e == slot || e.starts_with(&format!("{slot}/")))
}

fn envelope(json: &Value) -> Result<TransactionEnvelope> {
    serde_json::from_value(json.clone()).with_context(|| format!("reading the envelope {json}"))
}

/// Signs `json` in place by `plan` and returns the envelope. `edit` is the pointer the single
/// edit wrote at, `None` for the base envelope.
pub fn sign(
    json: &mut Value,
    plan: &SignPlan,
    network_id: &[u8; 32],
    edit: Option<&str>,
) -> Result<TransactionEnvelope> {
    for a in &plan.auth {
        sign_auth(json, a, network_id, edit)?;
    }
    let (inner, outer) = signature_slots(json)?;
    if !edited(edit, &inner) {
        let env = envelope(json)?;
        let hash = match &env {
            TransactionEnvelope::Tx(e) => transaction_hash(&e.tx, network_id)?,
            TransactionEnvelope::TxV0(e) => transaction_v0_hash(&e.tx, network_id)?,
            TransactionEnvelope::TxFeeBump(e) => match &e.tx.inner_tx {
                stellar_xdr::FeeBumpTransactionInnerTx::Tx(i) => {
                    transaction_hash(&i.tx, network_id)?
                }
            },
        };
        let mut sigs: Vec<DecoratedSignature> =
            plan.tx.iter().map(|k| sign_hash(k, &hash)).collect();
        for (k, at) in &plan.payloads {
            let signer: SignerKey = serde_json::from_value(get(json, at)?.clone())
                .with_context(|| format!("reading the signer key at {at}"))?;
            match signer {
                SignerKey::Ed25519SignedPayload(p) => {
                    sigs.push(sign_signed_payload(k, p.payload.as_slice())?)
                }
                // the edit turned the signer into another kind; nothing to sign for it
                _ => continue,
            }
        }
        set(json, &inner, serde_json::to_value(&sigs)?)?;
    }
    if let Some(outer) = outer
        && !edited(edit, &outer)
    {
        let env = envelope(json)?;
        let TransactionEnvelope::TxFeeBump(e) = &env else {
            bail!("a fee bump slot on an envelope that is no fee bump");
        };
        let hash = fee_bump_hash(&e.tx, network_id)?;
        let sigs: Vec<DecoratedSignature> =
            plan.fee_bump.iter().map(|k| sign_hash(k, &hash)).collect();
        set(json, &outer, serde_json::to_value(&sigs)?)?;
    }
    envelope(json)
}

fn sign_auth(
    json: &mut Value,
    a: &AuthSigner,
    network_id: &[u8; 32],
    edit: Option<&str>,
) -> Result<()> {
    // an edit that removed or replaced the entry leaves nothing this signer can sign
    let Some(entry_json) = json.pointer(&a.entry) else {
        return Ok(());
    };
    let Ok(entry) = serde_json::from_value::<SorobanAuthorizationEntry>(entry_json.clone()) else {
        return Ok(());
    };
    let sig_slot = match &entry.credentials {
        SorobanCredentials::Address(_) => format!("{}/credentials/address/signature", a.entry),
        SorobanCredentials::AddressV2(_) => {
            format!("{}/credentials/address_v2/signature", a.entry)
        }
        SorobanCredentials::AddressWithDelegates(_) => format!(
            "{}/credentials/address_with_delegates/address_credentials/signature",
            a.entry
        ),
        SorobanCredentials::SourceAccount => return Ok(()),
    };
    let payload = authorization_payload(&entry, network_id)?;
    if !a.keys.is_empty() && !edited(edit, &sig_slot) {
        let keys: Vec<&SigningKey> = a.keys.iter().collect();
        let sig: ScVal = account_signature(&payload, &keys)?;
        set(json, &sig_slot, serde_json::to_value(&sig)?)?;
    }
    for (at, k) in &a.delegates {
        let slot = format!("{at}/signature");
        if json.pointer(&slot).is_none() || edited(edit, &slot) {
            continue;
        }
        let sig = account_signature(&payload, &[k])?;
        set(json, &slot, serde_json::to_value(&sig)?)?;
    }
    Ok(())
}

/// Signs a typed envelope by a plan of transaction and fee bump signers only, for envelopes
/// XDR-JSON cannot hold.
pub fn sign_typed(
    mut env: TransactionEnvelope,
    plan: &SignPlan,
    network_id: &[u8; 32],
) -> Result<TransactionEnvelope> {
    if !plan.auth.is_empty() {
        bail!("an envelope XDR-JSON cannot hold is not signed for soroban authorization");
    }
    let sigs = |hash: [u8; 32],
                keys: &[SigningKey]|
     -> Result<stellar_xdr::VecM<DecoratedSignature, 20>> {
        let v: Vec<DecoratedSignature> = keys.iter().map(|k| sign_hash(k, &hash)).collect();
        Ok(v.try_into()?)
    };
    // a payload signer signs every signed payload extra signer naming its key
    let payload_sigs = |tx: &stellar_xdr::Transaction| -> Result<Vec<DecoratedSignature>> {
        let mut out = Vec::new();
        let stellar_xdr::Preconditions::V2(v2) = &tx.cond else {
            return Ok(out);
        };
        for (k, _) in &plan.payloads {
            for signer in v2.extra_signers.iter() {
                if let SignerKey::Ed25519SignedPayload(p) = signer
                    && p.ed25519.0 == k.verifying_key().to_bytes()
                {
                    out.push(sign_signed_payload(k, p.payload.as_slice())?);
                }
            }
        }
        Ok(out)
    };
    match &mut env {
        TransactionEnvelope::Tx(e) => {
            let mut v: Vec<DecoratedSignature> =
                sigs(transaction_hash(&e.tx, network_id)?, &plan.tx)?.to_vec();
            v.extend(payload_sigs(&e.tx)?);
            e.signatures = v.try_into()?;
        }
        TransactionEnvelope::TxV0(e) => {
            e.signatures = sigs(transaction_v0_hash(&e.tx, network_id)?, &plan.tx)?
        }
        TransactionEnvelope::TxFeeBump(e) => {
            let stellar_xdr::FeeBumpTransactionInnerTx::Tx(inner) = &mut e.tx.inner_tx;
            inner.signatures = sigs(transaction_hash(&inner.tx, network_id)?, &plan.tx)?;
            e.signatures = sigs(fee_bump_hash(&e.tx, network_id)?, &plan.fee_bump)?;
        }
    }
    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_kept_only_when_the_edit_wrote_inside_it() {
        assert!(edited(Some("/tx/signatures/0/hint"), "/tx/signatures"));
        assert!(edited(Some("/tx/signatures"), "/tx/signatures"));
        assert!(!edited(Some("/tx/tx/fee"), "/tx/signatures"));
        assert!(!edited(Some("/tx/signatures_x"), "/tx/signatures"));
        assert!(!edited(None, "/tx/signatures"));
    }
}
