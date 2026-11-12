//! Soroban network settings captured from a real network, and the config
//! upgrade set (CAP-46-09) that applies them to a standalone network.
//!
//! A fresh standalone network starts with core's initial Soroban settings,
//! which are too small to upload a contract, or with
//! `TESTING_SOROBAN_HIGH_LIMIT_OVERRIDE`, which is far above any real
//! network. Real limits and fees come from `data/testnet-config-settings.json`
//! (getLedgerEntries on soroban-testnet, provenance beside it). Core applies
//! them the way validators do: the `ConfigUpgradeSet` XDR is written as a
//! temporary contract data entry, keyed by `ScVal::Bytes(sha256(set))`, by a
//! contract invocation, and the `upgrades` command is armed with the
//! `ConfigUpgradeSetKey` naming that contract and hash
//! (`ConfigUpgradeSetFrame::makeFromKey` and `getLedgerKey`,
//! src/herder/Upgrades.cpp, v30.0.0; core's own tooling is
//! `get-settings-upgrade-txs` in src/main/SettingsUpgradeUtils.cpp).

use serde::Deserialize;
use sha2::{Digest, Sha256};
use stellar_xdr::{
    ConfigSettingEntry, ConfigSettingId, ConfigUpgradeSet, ConfigUpgradeSetKey,
    ContractDataDurability, ContractId, Hash, LedgerEntryData, LedgerKey, LedgerKeyContractData,
    Limits, ReadXdr, ScAddress, ScBytes, ScVal, WriteXdr,
};

/// The captured testnet settings, committed with their provenance.
pub const TESTNET_SETTINGS_JSON: &str = include_str!("../data/testnet-config-settings.json");

/// SHA-256 of [`TESTNET_SETTINGS_JSON`] as recorded in the provenance file.
pub const TESTNET_SETTINGS_SHA256: &str =
    "5c785afb8e0b8ecca6dcee779b93aa161857411ea9d6313841c02e618d04b295";

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("settings json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("settings entry {id}: {source}")]
    Xdr { id: u32, source: stellar_xdr::Error },
    #[error("settings entry {id}: data is not a CONFIG_SETTING entry with that id")]
    WrongEntry { id: u32 },
    #[error("config setting {0:?} appears twice")]
    Duplicate(ConfigSettingId),
    #[error("encoding the upgrade set: {0}")]
    Encode(stellar_xdr::Error),
    #[error("the upgrade set holds no upgradeable entries")]
    Empty,
}

#[derive(Deserialize)]
struct CapturedFile {
    network_passphrase: String,
    protocol_version: u32,
    latest_ledger: u32,
    entries: Vec<CapturedEntry>,
}

#[derive(Deserialize)]
struct CapturedEntry {
    config_setting_id: u32,
    data_xdr: String,
}

/// Every CONFIG_SETTING entry of one network at one ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSettings {
    pub network_passphrase: String,
    /// Protocol the network ran when captured. The cost parameter entries
    /// hold one value per cost type of that protocol, so the set only
    /// validates on a ledger running exactly this protocol.
    pub protocol_version: u32,
    pub ledger: u32,
    /// Sorted by setting id, each id once.
    pub entries: Vec<ConfigSettingEntry>,
}

/// Settings core sets itself and refuses in an upgrade
/// (`SorobanNetworkConfig::isNonUpgradeableConfigSettingEntry`, v30.0.0):
/// they describe the live state of this ledger, not policy.
pub fn is_upgradeable(id: ConfigSettingId) -> bool {
    !matches!(
        id,
        ConfigSettingId::LiveSorobanStateSizeWindow
            | ConfigSettingId::EvictionIterator
            | ConfigSettingId::FrozenLedgerKeys
            | ConfigSettingId::FreezeBypassTxs
    )
}

impl NetworkSettings {
    /// Testnet's settings from the committed capture.
    pub fn testnet() -> Result<Self, SettingsError> {
        Self::from_json(TESTNET_SETTINGS_JSON)
    }

    pub fn from_json(text: &str) -> Result<Self, SettingsError> {
        let f: CapturedFile = serde_json::from_str(text)?;
        let mut entries = Vec::with_capacity(f.entries.len());
        for e in f.entries {
            let id = e.config_setting_id;
            let data = LedgerEntryData::from_xdr_base64(&e.data_xdr, Limits::none())
                .map_err(|source| SettingsError::Xdr { id, source })?;
            let LedgerEntryData::ConfigSetting(setting) = data else {
                return Err(SettingsError::WrongEntry { id });
            };
            if setting.discriminant() as u32 != id {
                return Err(SettingsError::WrongEntry { id });
            }
            entries.push(setting);
        }
        entries.sort_by_key(|e| e.discriminant() as i32);
        if let Some(w) = entries
            .windows(2)
            .find(|w| w[0].discriminant() == w[1].discriminant())
        {
            return Err(SettingsError::Duplicate(w[0].discriminant()));
        }
        Ok(Self {
            network_passphrase: f.network_passphrase,
            protocol_version: f.protocol_version,
            ledger: f.latest_ledger,
            entries,
        })
    }

    /// The upgrade set: every upgradeable entry, in increasing id order with
    /// no repeats, which is what `ConfigUpgradeSetFrame::isValidXDR` demands.
    pub fn upgrade_set(&self) -> Result<ConfigUpgradeSet, SettingsError> {
        let updated: Vec<ConfigSettingEntry> = self
            .entries
            .iter()
            .filter(|e| is_upgradeable(e.discriminant()))
            .cloned()
            .collect();
        if updated.is_empty() {
            return Err(SettingsError::Empty);
        }
        Ok(ConfigUpgradeSet {
            updated_entry: updated.try_into().map_err(SettingsError::Encode)?,
        })
    }

    pub fn get(&self, id: ConfigSettingId) -> Option<&ConfigSettingEntry> {
        self.entries.iter().find(|e| e.discriminant() == id)
    }
}

/// An upgrade set ready to be written by a contract, with its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedUpgrade {
    pub set: ConfigUpgradeSet,
    /// The XDR of `set`, the value the contract stores.
    pub set_xdr: Vec<u8>,
    pub key: ConfigUpgradeSetKey,
}

impl PreparedUpgrade {
    pub fn new(set: ConfigUpgradeSet, contract: ContractId) -> Result<Self, SettingsError> {
        let set_xdr = set.to_xdr(Limits::none()).map_err(SettingsError::Encode)?;
        let content_hash = Hash(Sha256::digest(&set_xdr).into());
        Ok(Self {
            set,
            set_xdr,
            key: ConfigUpgradeSetKey {
                contract_id: contract,
                content_hash,
            },
        })
    }

    /// `ScVal::Bytes(content hash)`, the storage key core looks the set up by.
    pub fn storage_key(&self) -> Result<ScVal, SettingsError> {
        Ok(ScVal::Bytes(ScBytes(
            self.key
                .content_hash
                .0
                .to_vec()
                .try_into()
                .map_err(SettingsError::Encode)?,
        )))
    }

    /// `ScVal::Bytes(set xdr)`, the value stored under [`Self::storage_key`].
    pub fn storage_value(&self) -> Result<ScVal, SettingsError> {
        Ok(ScVal::Bytes(ScBytes(
            self.set_xdr
                .clone()
                .try_into()
                .map_err(SettingsError::Encode)?,
        )))
    }

    /// The temporary contract data entry core loads
    /// (`ConfigUpgradeSetFrame::getLedgerKey`).
    pub fn ledger_key(&self) -> Result<LedgerKey, SettingsError> {
        Ok(LedgerKey::ContractData(LedgerKeyContractData {
            contract: ScAddress::Contract(self.key.contract_id.clone()),
            key: self.storage_key()?,
            durability: ContractDataDurability::Temporary,
        }))
    }

    /// Base64 of the key, as the `upgrades` command takes it.
    pub fn key_base64(&self) -> Result<String, SettingsError> {
        self.key
            .to_xdr_base64(Limits::none())
            .map_err(SettingsError::Encode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_matches_its_provenance() {
        let digest = hex::encode(Sha256::digest(TESTNET_SETTINGS_JSON.as_bytes()));
        assert_eq!(digest, TESTNET_SETTINGS_SHA256);
        let prov: serde_json::Value = serde_json::from_str(include_str!(
            "../data/testnet-config-settings.provenance.json"
        ))
        .unwrap();
        assert_eq!(prov["sha256"], TESTNET_SETTINGS_SHA256);
        let s = NetworkSettings::testnet().unwrap();
        assert_eq!(prov["requests"][0]["result_latest_ledger"], s.ledger);
        assert_eq!(s.network_passphrase, "Test SDF Network ; September 2015");
        assert_eq!(s.protocol_version, 30);
    }

    #[test]
    fn upgrade_set_is_what_core_accepts_structurally() {
        let s = NetworkSettings::testnet().unwrap();
        let set = s.upgrade_set().unwrap();
        let ids: Vec<i32> = set
            .updated_entry
            .iter()
            .map(|e| e.discriminant() as i32)
            .collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "{ids:?}");
        // 19 captured, 4 of them describe live state and are not upgradeable
        assert_eq!(s.entries.len(), 19);
        assert_eq!(ids.len(), 15);
        assert!(!ids.contains(&(ConfigSettingId::EvictionIterator as i32)));

        let p = PreparedUpgrade::new(set, ContractId(Hash([9; 32]))).unwrap();
        assert_eq!(
            p.key.content_hash.0,
            <[u8; 32]>::from(Sha256::digest(&p.set_xdr))
        );
        let LedgerKey::ContractData(k) = p.ledger_key().unwrap() else {
            panic!("not contract data");
        };
        assert_eq!(k.durability, ContractDataDurability::Temporary);
    }
}
