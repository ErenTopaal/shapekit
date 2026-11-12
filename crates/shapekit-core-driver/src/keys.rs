//! Deterministic test identities and network ids.
//!
//! Every key the generator signs with is either the network root key or a key
//! derived from a recipe id and a role, so a chain can be rebuilt bit for bit
//! without storing any key material.
//!
//! Derived seed layout, fed to SHA-256 in this order:
//!
//! | bytes            | content                                   |
//! |------------------|-------------------------------------------|
//! | 22               | [`DERIVE_KEY_DOMAIN`], ascii, no length    |
//! | 4                | length of the recipe id, u32 big endian   |
//! | n                | recipe id, utf-8                          |
//! | 4                | length of the role, u32 big endian        |
//! | m                | role, utf-8                               |
//!
//! The 32-byte digest is the ed25519 seed. The length prefixes keep
//! `("ab", "c")` and `("a", "bc")` apart. The passphrase is deliberately not
//! part of the input, so the same accounts exist under testnet and mainnet
//! passphrases and only network-bound signatures differ.

use ed25519_dalek::{SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use stellar_strkey::Unredacted;
use stellar_xdr::{
    AccountId, MuxedAccount, MuxedAccountMed25519, PublicKey, ScAddress, SignerKey,
    SignerKeyEd25519SignedPayload, Uint256,
};

/// Domain string that prefixes every derived seed. Changing it changes every
/// derived account in the corpus.
pub const DERIVE_KEY_DOMAIN: &[u8] = b"shapekit/derive-key/v1";

/// Network id: SHA-256 of the passphrase. Used for transaction hashes and
/// Soroban authorization payloads.
pub fn network_id(passphrase: &str) -> [u8; 32] {
    Sha256::digest(passphrase.as_bytes()).into()
}

/// The genesis root account key. stellar-core seeds it with the network id
/// (`SecretKey::fromSeed(mApp.getNetworkID())` in
/// `LedgerManagerImpl::startNewLedger`, v30.0.0), and the network id is
/// SHA-256 of the passphrase.
pub fn root_key(passphrase: &str) -> SigningKey {
    SigningKey::from_bytes(&network_id(passphrase))
}

/// The seed bytes that [`derive_key`] uses, exposed so the layout can be
/// checked independently.
pub fn derive_seed(recipe_id: &str, role: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(DERIVE_KEY_DOMAIN);
    for part in [recipe_id, role] {
        let len = u32::try_from(part.len()).expect("recipe id or role longer than u32::MAX bytes");
        h.update(len.to_be_bytes());
        h.update(part.as_bytes());
    }
    h.finalize().into()
}

/// Deterministic ed25519 key for one role inside one recipe.
pub fn derive_key(recipe_id: &str, role: &str) -> SigningKey {
    SigningKey::from_bytes(&derive_seed(recipe_id, role))
}

pub fn account_id(key: &VerifyingKey) -> AccountId {
    AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(key.to_bytes())))
}

/// Plain `G` muxed account (no id).
pub fn muxed_account(key: &VerifyingKey) -> MuxedAccount {
    MuxedAccount::Ed25519(Uint256(key.to_bytes()))
}

/// `M` muxed account carrying a mux id.
pub fn muxed_account_with_id(key: &VerifyingKey, id: u64) -> MuxedAccount {
    MuxedAccount::MuxedEd25519(MuxedAccountMed25519 {
        id,
        ed25519: Uint256(key.to_bytes()),
    })
}

pub fn account_address(key: &VerifyingKey) -> ScAddress {
    ScAddress::Account(account_id(key))
}

pub fn ed25519_signer(key: &VerifyingKey) -> SignerKey {
    SignerKey::Ed25519(Uint256(key.to_bytes()))
}

/// CAP-40 signed-payload signer key. Fails when the payload exceeds the
/// 64 bytes the XDR allows.
pub fn signed_payload_signer(
    key: &VerifyingKey,
    payload: &[u8],
) -> Result<SignerKey, stellar_xdr::Error> {
    Ok(SignerKey::Ed25519SignedPayload(
        SignerKeyEd25519SignedPayload {
            ed25519: Uint256(key.to_bytes()),
            payload: payload.to_vec().try_into()?,
        },
    ))
}

/// `G...` strkey of a public key.
pub fn account_strkey(key: &VerifyingKey) -> String {
    stellar_strkey::ed25519::PublicKey(key.to_bytes())
        .to_string()
        .as_str()
        .to_owned()
}

/// `M...` strkey of a public key and mux id.
pub fn muxed_strkey(key: &VerifyingKey, id: u64) -> String {
    stellar_strkey::ed25519::MuxedAccount {
        ed25519: key.to_bytes(),
        id,
    }
    .to_string()
    .as_str()
    .to_owned()
}

/// `S...` strkey of a seed. Only ever used for derived test keys, which carry
/// no value on any real network.
pub fn secret_strkey(key: &SigningKey) -> String {
    let sk = stellar_strkey::ed25519::PrivateKey(key.to_bytes());
    Unredacted(&sk).to_string().as_str().to_owned()
}

/// `P...` strkey of a signed-payload signer. Fails when the payload is not
/// 1 to 64 bytes, the range SEP-23 can encode.
pub fn signed_payload_strkey(
    key: &VerifyingKey,
    payload: &[u8],
) -> Result<String, stellar_strkey::DecodeError> {
    let sp = stellar_strkey::ed25519::SignedPayload::new(key.to_bytes(), payload)?;
    Ok(sp.to_string().as_str().to_owned())
}

/// `C...` strkey of a contract id.
pub fn contract_strkey(id: &stellar_xdr::ContractId) -> String {
    stellar_strkey::Contract(id.0.0)
        .to_string()
        .as_str()
        .to_owned()
}
