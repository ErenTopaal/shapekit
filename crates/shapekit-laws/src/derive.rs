//! The hashes and identifiers the network defines, computed from their preimages exactly as
//! stellar-core v30.0.0 computes them. Each function names the core source it follows.

use sha2::{Digest, Sha256};
use stellar_xdr::{
    AccountId, Asset, ClaimableBalanceId, ContractIdPreimage, FeeBumpTransaction,
    GeneralizedTransactionSet, Hash, HashIdPreimage, HashIdPreimageContractId,
    HashIdPreimageOperationId, HashIdPreimageRevokeId, LedgerHeader, Limits,
    LiquidityPoolParameters, MuxedAccount, PoolId, Preconditions, PublicKey, SequenceNumber,
    Transaction, TransactionEnvelope, TransactionExt, TransactionResultPair, TransactionResultSet,
    TransactionSet, TransactionSignaturePayload, TransactionSignaturePayloadTaggedTransaction,
    TransactionV0, TransactionV0Ext, Uint256, WriteXdr,
};

pub type Digest32 = [u8; 32];

/// Encoding a value the decoder produced cannot fail short of a broken encoder, so the error
/// is surfaced as a law violation by the callers rather than unwrapped.
pub type Encoded<T> = Result<T, stellar_xdr::Error>;

pub fn sha256(bytes: &[u8]) -> Digest32 {
    Sha256::digest(bytes).into()
}

fn xdr_sha256(v: &impl WriteXdr) -> Encoded<Digest32> {
    Ok(sha256(&v.to_xdr(Limits::none())?))
}

/// The network id: sha256 of the passphrase.
pub fn network_id(passphrase: &str) -> Digest32 {
    sha256(passphrase.as_bytes())
}

/// The header hash: sha256 of the encoded `LedgerHeader` (LedgerManagerImpl.cpp, `xdrSha256(prevHeader)`).
pub fn header_hash(header: &LedgerHeader) -> Encoded<Digest32> {
    xdr_sha256(header)
}

/// `txSetResultHash`: sha256 of the encoded `TransactionResultSet` in apply order
/// (LedgerManagerImpl.cpp:1884).
pub fn result_set_hash(results: Vec<TransactionResultPair>) -> Encoded<Digest32> {
    let set = TransactionResultSet {
        results: results.try_into()?,
    };
    xdr_sha256(&set)
}

/// The contents hash of a pre-generalized tx set: sha256 over the previous ledger hash followed
/// by each encoded envelope (TxSetFrame.cpp, `computeNonGeneralizedTxSetContentsHash`).
pub fn legacy_tx_set_hash(set: &TransactionSet) -> Encoded<Digest32> {
    let mut h = Sha256::new();
    h.update(set.previous_ledger_hash.0);
    for tx in set.txs.iter() {
        h.update(tx.to_xdr(Limits::none())?);
    }
    Ok(h.finalize().into())
}

/// The contents hash of a generalized tx set: sha256 of its encoding (TxSetFrame.cpp,
/// `TxSetXDRFrame(GeneralizedTransactionSet const&)`).
pub fn generalized_tx_set_hash(set: &GeneralizedTransactionSet) -> Encoded<Digest32> {
    xdr_sha256(set)
}

/// A v0 transaction as core's bridge converts it to v1 (TransactionBridge.cpp, `convertForV13`):
/// the plain ed25519 source, time bounds as `PRECOND_TIME`. Operation ids use its source and
/// sequence number.
pub fn v0_as_v1(tx: &TransactionV0) -> Transaction {
    let TransactionV0Ext::V0 = tx.ext;
    Transaction {
        source_account: MuxedAccount::Ed25519(tx.source_account_ed25519.clone()),
        fee: tx.fee,
        seq_num: tx.seq_num.clone(),
        cond: match &tx.time_bounds {
            Some(tb) => Preconditions::Time(tb.clone()),
            None => Preconditions::None,
        },
        memo: tx.memo.clone(),
        operations: tx.operations.clone(),
        ext: TransactionExt::V0,
    }
}

/// sha256 of the `TransactionSignaturePayload` for a v1 transaction.
pub fn transaction_hash(network: &Digest32, tx: &Transaction) -> Encoded<Digest32> {
    xdr_sha256(&TransactionSignaturePayload {
        network_id: Hash(*network),
        tagged_transaction: TransactionSignaturePayloadTaggedTransaction::Tx(tx.clone()),
    })
}

/// The hash of a v0 transaction as core writes it (TransactionFrame.cpp, `getContentsHash`):
/// sha256 over the network id, `ENVELOPE_TYPE_TX`, a zero word and the encoded `TransactionV0`.
/// The zero word stands in for the `KEY_TYPE_ED25519` discriminant of a muxed source, so the
/// bytes are those of the v1 payload with the bridged transaction.
pub fn v0_transaction_hash(network: &Digest32, tx: &TransactionV0) -> Encoded<Digest32> {
    let mut h = Sha256::new();
    h.update(network);
    h.update(2u32.to_be_bytes());
    h.update(0u32.to_be_bytes());
    h.update(tx.to_xdr(Limits::none())?);
    Ok(h.finalize().into())
}

/// sha256 of the `TransactionSignaturePayload` for a fee bump.
pub fn fee_bump_hash(network: &Digest32, tx: &FeeBumpTransaction) -> Encoded<Digest32> {
    xdr_sha256(&TransactionSignaturePayload {
        network_id: Hash(*network),
        tagged_transaction: TransactionSignaturePayloadTaggedTransaction::TxFeeBump(tx.clone()),
    })
}

/// The hash core writes into `TransactionResultPair.transactionHash` for an envelope: the
/// outer hash, which for a fee bump is the fee bump's own hash.
pub fn envelope_hash(network: &Digest32, env: &TransactionEnvelope) -> Encoded<Digest32> {
    match env {
        TransactionEnvelope::TxV0(e) => v0_transaction_hash(network, &e.tx),
        TransactionEnvelope::Tx(e) => transaction_hash(network, &e.tx),
        TransactionEnvelope::TxFeeBump(e) => fee_bump_hash(network, &e.tx),
    }
}

/// The transaction whose source and sequence number operation ids are derived from: the
/// envelope's own transaction, or a fee bump's inner one.
pub fn inner_transaction(env: &TransactionEnvelope) -> Transaction {
    match env {
        TransactionEnvelope::TxV0(e) => v0_as_v1(&e.tx),
        TransactionEnvelope::Tx(e) => e.tx.clone(),
        TransactionEnvelope::TxFeeBump(e) => {
            let stellar_xdr::FeeBumpTransactionInnerTx::Tx(inner) = &e.tx.inner_tx;
            inner.tx.clone()
        }
    }
}

/// The account behind a muxed account: core's `toAccountID`.
pub fn account_id(m: &MuxedAccount) -> AccountId {
    let key: Uint256 = match m {
        MuxedAccount::Ed25519(k) => k.clone(),
        MuxedAccount::MuxedEd25519(med) => med.ed25519.clone(),
    };
    AccountId(PublicKey::PublicKeyTypeEd25519(key))
}

/// A claimable balance id from `CreateClaimableBalance`: sha256 of `ENVELOPE_TYPE_OP_ID` with the
/// transaction's source, sequence number and the operation index
/// (CreateClaimableBalanceOpFrame.cpp, `getBalanceID`; the source is the transaction's, not the
/// operation's).
pub fn operation_balance_id(
    tx_source: &AccountId,
    seq: &SequenceNumber,
    op_index: u32,
) -> Encoded<ClaimableBalanceId> {
    let h = xdr_sha256(&HashIdPreimage::OpId(HashIdPreimageOperationId {
        source_account: tx_source.clone(),
        seq_num: seq.clone(),
        op_num: op_index,
    }))?;
    Ok(ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash(h)))
}

/// A claimable balance id created when a revocation redeems a pool share: sha256 of
/// `ENVELOPE_TYPE_POOL_REVOKE_OP_ID` (TransactionUtils.cpp, `getRevokeID`).
pub fn revoke_balance_id(
    tx_source: &AccountId,
    seq: &SequenceNumber,
    op_index: u32,
    pool: &PoolId,
    asset: &Asset,
) -> Encoded<ClaimableBalanceId> {
    let h = xdr_sha256(&HashIdPreimage::PoolRevokeOpId(HashIdPreimageRevokeId {
        source_account: tx_source.clone(),
        seq_num: seq.clone(),
        op_num: op_index,
        liquidity_pool_id: pool.clone(),
        asset: asset.clone(),
    }))?;
    Ok(ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash(h)))
}

/// A liquidity pool id: sha256 of the encoded `LiquidityPoolParameters`
/// (OfferExchange.cpp, `getPoolID`).
pub fn pool_id(params: &LiquidityPoolParameters) -> Encoded<PoolId> {
    Ok(PoolId(Hash(xdr_sha256(params)?)))
}

/// A contract id: sha256 of `ENVELOPE_TYPE_CONTRACT_ID` with the network id and the preimage
/// (TransactionUtils.cpp, `getAssetContractID`, and the host's `get_contract_id_hash`).
pub fn contract_id(network: &Digest32, preimage: &ContractIdPreimage) -> Encoded<Digest32> {
    xdr_sha256(&HashIdPreimage::ContractId(HashIdPreimageContractId {
        network_id: Hash(*network),
        contract_id_preimage: preimage.clone(),
    }))
}
