//! Transaction building and signing on `stellar-xdr` types.
//!
//! The builder only enforces what the XDR itself enforces (vector bounds,
//! one Soroban operation for assembly). It does not apply core's semantic
//! validation, because the corpus needs envelopes core rejects as well.

use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use stellar_xdr::{
    DecoratedSignature, FeeBumpTransaction, FeeBumpTransactionEnvelope, FeeBumpTransactionExt,
    FeeBumpTransactionInnerTx, Hash, HashIdPreimage, HashIdPreimageSorobanAuthorization,
    HashIdPreimageSorobanAuthorizationWithAddress, Limits, Memo, MuxedAccount, Operation,
    OperationBody, Preconditions, ScAddress, ScBytes, ScMap, ScMapEntry, ScSymbol, ScVal, ScVec,
    SequenceNumber, Signature, SignatureHint, SorobanAddressCredentials, SorobanAuthorizationEntry,
    SorobanCredentials, SorobanResources, SorobanResourcesExtV0, SorobanTransactionData,
    SorobanTransactionDataExt, Transaction, TransactionEnvelope, TransactionExt,
    TransactionSignaturePayload, TransactionSignaturePayloadTaggedTransaction,
    TransactionV1Envelope, WriteXdr,
};

/// Fee core charges per operation at genesis and on both public networks.
pub const BASE_FEE: u32 = 100;

#[derive(Debug, thiserror::Error)]
pub enum TxError {
    #[error("xdr: {0}")]
    Xdr(#[from] stellar_xdr::Error),
    #[error("signed payload is {0} bytes, the xdr allows at most 64")]
    PayloadTooLong(usize),
    #[error("transaction v0 envelopes have no signature payload since protocol 13")]
    UnsupportedEnvelope,
    #[error("soroban assembly needs exactly one operation, the transaction has {0}")]
    NotSingleOperation(usize),
    #[error("auth entries can only be attached to an invoke host function operation")]
    NotInvokeHostFunction,
    #[error("fee overflow: inclusion fee {inclusion} plus resource fee {resource}")]
    FeeOverflow { inclusion: u32, resource: i64 },
    #[error("authorization entry uses source account credentials, there is nothing to sign")]
    SourceAccountCredentials,
    #[error("authorization address is not an account, its signature format is contract defined")]
    NotAnAccountAddress,
    #[error("no signers given")]
    NoSigners,
}

/// Last four bytes of a public key, the hint of a plain ed25519 signature.
pub fn hint(public_key: &[u8; 32]) -> SignatureHint {
    SignatureHint(last_four(public_key))
}

/// CAP-40 hint: last four bytes of the key XOR the last four bytes of the
/// payload, where a payload shorter than four bytes is zero padded at the end
/// (core `getHint` copies short input to the front of a zeroed hint).
pub fn signed_payload_hint(public_key: &[u8; 32], payload: &[u8]) -> SignatureHint {
    let k = last_four(public_key);
    let p = last_four(payload);
    SignatureHint([k[0] ^ p[0], k[1] ^ p[1], k[2] ^ p[2], k[3] ^ p[3]])
}

fn last_four(bytes: &[u8]) -> [u8; 4] {
    let mut out = [0u8; 4];
    if bytes.len() < 4 {
        out[..bytes.len()].copy_from_slice(bytes);
    } else {
        out.copy_from_slice(&bytes[bytes.len() - 4..]);
    }
    out
}

/// Signature payload hash of a v1 transaction under a network id.
pub fn transaction_hash(tx: &Transaction, network_id: &[u8; 32]) -> Result<[u8; 32], TxError> {
    payload_hash(
        network_id,
        TransactionSignaturePayloadTaggedTransaction::Tx(tx.clone()),
    )
}

/// Signature payload hash of a fee-bump transaction under a network id.
pub fn fee_bump_hash(tx: &FeeBumpTransaction, network_id: &[u8; 32]) -> Result<[u8; 32], TxError> {
    payload_hash(
        network_id,
        TransactionSignaturePayloadTaggedTransaction::TxFeeBump(tx.clone()),
    )
}

/// Hash of the outermost transaction in an envelope, which is what core
/// reports as the transaction hash.
pub fn envelope_hash(
    env: &TransactionEnvelope,
    network_id: &[u8; 32],
) -> Result<[u8; 32], TxError> {
    match env {
        TransactionEnvelope::Tx(e) => transaction_hash(&e.tx, network_id),
        TransactionEnvelope::TxFeeBump(e) => fee_bump_hash(&e.tx, network_id),
        TransactionEnvelope::TxV0(_) => Err(TxError::UnsupportedEnvelope),
    }
}

fn payload_hash(
    network_id: &[u8; 32],
    tagged: TransactionSignaturePayloadTaggedTransaction,
) -> Result<[u8; 32], TxError> {
    let payload = TransactionSignaturePayload {
        network_id: Hash(*network_id),
        tagged_transaction: tagged,
    };
    Ok(Sha256::digest(payload.to_xdr(Limits::none())?).into())
}

/// Ed25519 signature over a 32-byte transaction hash with the key's hint.
pub fn sign_hash(key: &SigningKey, hash: &[u8; 32]) -> DecoratedSignature {
    decorated(
        hint(&key.verifying_key().to_bytes()),
        &key.sign(hash).to_bytes(),
    )
}

/// CAP-40 signature: ed25519 over the raw payload bytes, not hashed and not
/// combined with the network id, so it is identical under every passphrase.
pub fn sign_signed_payload(
    key: &SigningKey,
    payload: &[u8],
) -> Result<DecoratedSignature, TxError> {
    if payload.len() > 64 {
        return Err(TxError::PayloadTooLong(payload.len()));
    }
    let h = signed_payload_hint(&key.verifying_key().to_bytes(), payload);
    Ok(decorated(h, &key.sign(payload).to_bytes()))
}

/// Hash-x signature: the preimage itself, hinted by the last four bytes of
/// its SHA-256.
pub fn sign_hash_x(preimage: &[u8]) -> Result<DecoratedSignature, TxError> {
    let digest: [u8; 32] = Sha256::digest(preimage).into();
    Ok(DecoratedSignature {
        hint: hint(&digest),
        signature: Signature(preimage.to_vec().try_into()?),
    })
}

fn decorated(hint: SignatureHint, sig: &[u8; 64]) -> DecoratedSignature {
    DecoratedSignature {
        hint,
        signature: Signature(
            sig.to_vec()
                .try_into()
                .expect("a 64 byte signature fits signature<64>"),
        ),
    }
}

/// Builder for a v1 `Transaction`.
#[derive(Clone, Debug)]
pub struct TransactionBuilder {
    source: MuxedAccount,
    seq_num: i64,
    fee: u32,
    cond: Preconditions,
    memo: Memo,
    operations: Vec<Operation>,
    soroban_data: Option<SorobanTransactionData>,
}

impl TransactionBuilder {
    /// `seq_num` is the sequence number the transaction carries, which is
    /// the source account's current sequence plus one.
    pub fn new(source: MuxedAccount, seq_num: i64) -> Self {
        Self {
            source,
            seq_num,
            fee: 0,
            cond: Preconditions::None,
            memo: Memo::None,
            operations: Vec::new(),
            soroban_data: None,
        }
    }

    /// Total fee. When never set, `build` uses `BASE_FEE` per operation.
    pub fn fee(mut self, fee: u32) -> Self {
        self.fee = fee;
        self
    }

    pub fn preconditions(mut self, cond: Preconditions) -> Self {
        self.cond = cond;
        self
    }

    pub fn memo(mut self, memo: Memo) -> Self {
        self.memo = memo;
        self
    }

    pub fn operation(mut self, source: Option<MuxedAccount>, body: OperationBody) -> Self {
        self.operations.push(Operation {
            source_account: source,
            body,
        });
        self
    }

    pub fn soroban_data(mut self, data: SorobanTransactionData) -> Self {
        self.soroban_data = Some(data);
        self
    }

    pub fn build(self) -> Result<Transaction, TxError> {
        let fee = if self.fee == 0 {
            let ops = u32::try_from(self.operations.len()).unwrap_or(u32::MAX);
            BASE_FEE.saturating_mul(ops.max(1))
        } else {
            self.fee
        };
        Ok(Transaction {
            source_account: self.source,
            fee,
            seq_num: SequenceNumber(self.seq_num),
            cond: self.cond,
            memo: self.memo,
            operations: self.operations.try_into()?,
            ext: match self.soroban_data {
                Some(d) => TransactionExt::V1(d),
                None => TransactionExt::V0,
            },
        })
    }
}

/// Signs a transaction with each key in order and wraps it in a v1 envelope.
pub fn sign_transaction(
    tx: Transaction,
    network_id: &[u8; 32],
    signers: &[&SigningKey],
) -> Result<TransactionV1Envelope, TxError> {
    let hash = transaction_hash(&tx, network_id)?;
    let signatures: Vec<DecoratedSignature> = signers.iter().map(|k| sign_hash(k, &hash)).collect();
    Ok(TransactionV1Envelope {
        tx,
        signatures: signatures.try_into()?,
    })
}

/// Appends a signature to whichever transaction the envelope's outer layer
/// carries. Fails once the 20 signature limit is reached.
pub fn push_signature(
    env: &mut TransactionEnvelope,
    sig: DecoratedSignature,
) -> Result<(), TxError> {
    let sigs = match env {
        TransactionEnvelope::Tx(e) => &mut e.signatures,
        TransactionEnvelope::TxFeeBump(e) => &mut e.signatures,
        TransactionEnvelope::TxV0(e) => &mut e.signatures,
    };
    let mut v = sigs.to_vec();
    v.push(sig);
    *sigs = v.try_into()?;
    Ok(())
}

/// Wraps a signed inner transaction in a fee bump. `fee` is the outer total
/// fee, which core requires to cover the inner fee rate over one more
/// operation than the inner transaction has.
pub fn fee_bump(
    inner: TransactionV1Envelope,
    fee_source: MuxedAccount,
    fee: i64,
) -> FeeBumpTransaction {
    FeeBumpTransaction {
        fee_source,
        fee,
        inner_tx: FeeBumpTransactionInnerTx::Tx(inner),
        ext: FeeBumpTransactionExt::V0,
    }
}

pub fn sign_fee_bump(
    tx: FeeBumpTransaction,
    network_id: &[u8; 32],
    signers: &[&SigningKey],
) -> Result<FeeBumpTransactionEnvelope, TxError> {
    let hash = fee_bump_hash(&tx, network_id)?;
    let signatures: Vec<DecoratedSignature> = signers.iter().map(|k| sign_hash(k, &hash)).collect();
    Ok(FeeBumpTransactionEnvelope {
        tx,
        signatures: signatures.try_into()?,
    })
}

/// Soroban transaction data from simulated resources. A non-empty
/// `archived_indices` selects the v1 extension that marks footprint entries
/// for automatic restore.
pub fn soroban_transaction_data(
    resources: SorobanResources,
    resource_fee: i64,
    archived_indices: &[u32],
) -> Result<SorobanTransactionData, TxError> {
    Ok(SorobanTransactionData {
        ext: if archived_indices.is_empty() {
            SorobanTransactionDataExt::V0
        } else {
            SorobanTransactionDataExt::V1(SorobanResourcesExtV0 {
                archived_soroban_entries: archived_indices.to_vec().try_into()?,
            })
        },
        resources,
        resource_fee,
    })
}

/// Turns an unassembled single-operation transaction into a Soroban one:
/// attaches the transaction data, sets the fee to the transaction's current
/// fee (the inclusion fee) plus the resource fee, and, when `auth` is given,
/// replaces the invoke operation's auth entries.
pub fn assemble_soroban(
    mut tx: Transaction,
    data: SorobanTransactionData,
    auth: Option<Vec<SorobanAuthorizationEntry>>,
) -> Result<Transaction, TxError> {
    if tx.operations.len() != 1 {
        return Err(TxError::NotSingleOperation(tx.operations.len()));
    }
    let overflow = TxError::FeeOverflow {
        inclusion: tx.fee,
        resource: data.resource_fee,
    };
    let total = i64::from(tx.fee)
        .checked_add(data.resource_fee)
        .and_then(|f| u32::try_from(f).ok())
        .ok_or(overflow)?;
    if let Some(auth) = auth {
        let mut ops = tx.operations.to_vec();
        match &mut ops[0].body {
            OperationBody::InvokeHostFunction(op) => op.auth = auth.try_into()?,
            _ => return Err(TxError::NotInvokeHostFunction),
        }
        tx.operations = ops.try_into()?;
    }
    tx.fee = total;
    tx.ext = TransactionExt::V1(data);
    Ok(tx)
}

/// The preimage a signer of this entry authenticates. Address credentials use
/// the plain preimage; address-v2 and with-delegates credentials (CAP-71) bind
/// the address too, matching `get_signature_payload` in soroban-env-host
/// 30.0.0 `auth.rs`.
pub fn authorization_preimage(
    entry: &SorobanAuthorizationEntry,
    network_id: &[u8; 32],
) -> Result<HashIdPreimage, TxError> {
    let (creds, with_address) = match &entry.credentials {
        SorobanCredentials::SourceAccount => return Err(TxError::SourceAccountCredentials),
        SorobanCredentials::Address(c) => (c, false),
        SorobanCredentials::AddressV2(c) => (c, true),
        SorobanCredentials::AddressWithDelegates(c) => (&c.address_credentials, true),
    };
    Ok(if with_address {
        HashIdPreimage::SorobanAuthorizationWithAddress(
            HashIdPreimageSorobanAuthorizationWithAddress {
                network_id: Hash(*network_id),
                nonce: creds.nonce,
                signature_expiration_ledger: creds.signature_expiration_ledger,
                address: creds.address.clone(),
                invocation: entry.root_invocation.clone(),
            },
        )
    } else {
        HashIdPreimage::SorobanAuthorization(HashIdPreimageSorobanAuthorization {
            network_id: Hash(*network_id),
            nonce: creds.nonce,
            signature_expiration_ledger: creds.signature_expiration_ledger,
            invocation: entry.root_invocation.clone(),
        })
    })
}

/// SHA-256 of the authorization preimage: the 32 bytes the host passes to
/// signature verification. Delegate signatures in CAP-71 entries sign this
/// same payload.
pub fn authorization_payload(
    entry: &SorobanAuthorizationEntry,
    network_id: &[u8; 32],
) -> Result<[u8; 32], TxError> {
    let preimage = authorization_preimage(entry, network_id)?;
    Ok(Sha256::digest(preimage.to_xdr(Limits::none())?).into())
}

/// The signature value a classic account expects in address credentials:
/// a vec of `{public_key: bytes32, signature: bytes64}` maps sorted by
/// public key, as `check_account_authentication` in soroban-env-host 30.0.0
/// decodes it (`AccountEd25519Signature`, strictly increasing keys).
pub fn account_signature(payload: &[u8; 32], signers: &[&SigningKey]) -> Result<ScVal, TxError> {
    if signers.is_empty() {
        return Err(TxError::NoSigners);
    }
    let mut sigs: Vec<([u8; 32], [u8; 64])> = signers
        .iter()
        .map(|k| (k.verifying_key().to_bytes(), k.sign(payload).to_bytes()))
        .collect();
    sigs.sort_by_key(|a| a.0);
    let mut items = Vec::with_capacity(sigs.len());
    for (pk, sig) in sigs {
        let map = ScMap(
            vec![
                ScMapEntry {
                    key: ScVal::Symbol(ScSymbol("public_key".try_into()?)),
                    val: ScVal::Bytes(ScBytes(pk.to_vec().try_into()?)),
                },
                ScMapEntry {
                    key: ScVal::Symbol(ScSymbol("signature".try_into()?)),
                    val: ScVal::Bytes(ScBytes(sig.to_vec().try_into()?)),
                },
            ]
            .try_into()?,
        );
        items.push(ScVal::Map(Some(map)));
    }
    Ok(ScVal::Vec(Some(ScVec(items.try_into()?))))
}

/// Signs the address credentials of an entry for a classic account address:
/// sets the expiration ledger, then fills the signature. For with-delegates
/// credentials only the primary signature is filled; delegate signatures
/// are built with [`account_signature`] over [`authorization_payload`].
pub fn sign_authorization_entry(
    mut entry: SorobanAuthorizationEntry,
    network_id: &[u8; 32],
    signature_expiration_ledger: u32,
    signers: &[&SigningKey],
) -> Result<SorobanAuthorizationEntry, TxError> {
    let creds: &mut SorobanAddressCredentials = match &mut entry.credentials {
        SorobanCredentials::SourceAccount => return Err(TxError::SourceAccountCredentials),
        SorobanCredentials::Address(c) | SorobanCredentials::AddressV2(c) => c,
        SorobanCredentials::AddressWithDelegates(c) => &mut c.address_credentials,
    };
    if !matches!(creds.address, ScAddress::Account(_)) {
        return Err(TxError::NotAnAccountAddress);
    }
    creds.signature_expiration_ledger = signature_expiration_ledger;
    let payload = authorization_payload(&entry, network_id)?;
    let signature = account_signature(&payload, signers)?;
    match &mut entry.credentials {
        SorobanCredentials::Address(c) | SorobanCredentials::AddressV2(c) => {
            c.signature = signature
        }
        SorobanCredentials::AddressWithDelegates(c) => c.address_credentials.signature = signature,
        SorobanCredentials::SourceAccount => return Err(TxError::SourceAccountCredentials),
    }
    Ok(entry)
}
