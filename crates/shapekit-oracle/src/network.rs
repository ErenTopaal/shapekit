//! Network identity: the passphrase hash that keys transaction hashes, the genesis root
//! account and Stellar Asset Contract ids.

use sha2::{Digest, Sha256};
use stellar_xdr::{
    AccountId, Asset, ContractIdPreimage, Hash, HashIdPreimage, HashIdPreimageContractId, Limits,
    PublicKey, Uint256, WriteXdr,
};

/// Passphrase of the private network core's unit tests run on, and so of every core golden.
pub const CORE_TEST_PASSPHRASE: &str = "(V) (;,,;) (V)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    pub passphrase: String,
    pub id: [u8; 32],
}

impl Network {
    pub fn new(passphrase: &str) -> Self {
        Network {
            passphrase: passphrase.to_string(),
            id: Sha256::digest(passphrase.as_bytes()).into(),
        }
    }

    /// The genesis root account. Core derives its secret seed from the network id
    /// (`SecretKey::fromSeed(getNetworkID())` in `LedgerManagerImpl::startNewLedger`).
    pub fn root_account(&self) -> AccountId {
        let signing = ed25519_public_from_seed(&self.id);
        AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(signing)))
    }

    /// The contract id of the Stellar Asset Contract for `asset` on this network, computed
    /// the way core's `getAssetContractID` does.
    pub fn asset_contract_id(&self, asset: &Asset) -> [u8; 32] {
        let preimage = HashIdPreimage::ContractId(HashIdPreimageContractId {
            network_id: Hash(self.id),
            contract_id_preimage: ContractIdPreimage::Asset(asset.clone()),
        });
        let bytes = preimage
            .to_xdr(Limits::none())
            .expect("a contract id preimage always encodes");
        Sha256::digest(bytes).into()
    }
}

fn ed25519_public_from_seed(seed: &[u8; 32]) -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(seed)
        .verifying_key()
        .to_bytes()
}
