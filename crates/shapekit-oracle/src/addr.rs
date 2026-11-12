//! Address helpers shared by the interpreter and the declared-participant walker.

use std::collections::BTreeMap;

use stellar_xdr::{
    AccountId, ClaimableBalanceId, ContractId, Hash, LedgerEntry, LedgerEntryData, LedgerKey,
    MuxedAccount, PoolId, PublicKey, ScAddress, Uint256,
};

use crate::vocab::{AddressKind, Int, Participant};

pub fn account(a: &AccountId) -> String {
    a.to_string()
}

/// The base G address of a muxed account and its mux id, if any.
pub fn muxed(m: &MuxedAccount) -> (String, Option<u64>) {
    match m {
        MuxedAccount::Ed25519(k) => (g(k), None),
        MuxedAccount::MuxedEd25519(mm) => (g(&mm.ed25519), Some(mm.id)),
    }
}

fn g(k: &Uint256) -> String {
    AccountId(PublicKey::PublicKeyTypeEd25519(k.clone())).to_string()
}

pub fn contract(id: &ContractId) -> String {
    ScAddress::Contract(id.clone()).to_string()
}

pub fn contract_hash(h: &[u8; 32]) -> String {
    contract(&ContractId(Hash(*h)))
}

pub fn pool(p: &PoolId) -> String {
    p.to_string()
}

pub fn balance_id(b: &ClaimableBalanceId) -> String {
    b.to_string()
}

/// Base address, kind and mux id of an `ScAddress`. Muxed accounts and muxed contracts
/// (CAP-84) are reduced to their base address, the mux id kept apart.
pub fn sc_address(a: &ScAddress) -> (String, AddressKind, Option<u64>) {
    match a {
        ScAddress::Account(acc) => (account(acc), AddressKind::Account, None),
        ScAddress::MuxedAccount(m) => (g(&m.ed25519), AddressKind::Account, Some(m.id)),
        ScAddress::Contract(c) => (contract(c), AddressKind::Contract, None),
        ScAddress::MuxedContract(m) => {
            (contract(&m.contract_id), AddressKind::Contract, Some(m.id))
        }
        ScAddress::ClaimableBalance(b) => (balance_id(b), AddressKind::ClaimableBalance, None),
        ScAddress::LiquidityPool(p) => (pool(p), AddressKind::LiquidityPool, None),
    }
}

/// The address that owns a ledger entry, if the entry belongs to one.
pub fn key_owner(key: &LedgerKey) -> Option<(String, AddressKind)> {
    match key {
        LedgerKey::Account(k) => Some((account(&k.account_id), AddressKind::Account)),
        LedgerKey::Trustline(k) => Some((account(&k.account_id), AddressKind::Account)),
        LedgerKey::Offer(k) => Some((account(&k.seller_id), AddressKind::Account)),
        LedgerKey::Data(k) => Some((account(&k.account_id), AddressKind::Account)),
        LedgerKey::ClaimableBalance(k) => {
            Some((balance_id(&k.balance_id), AddressKind::ClaimableBalance))
        }
        LedgerKey::LiquidityPool(k) => {
            Some((pool(&k.liquidity_pool_id), AddressKind::LiquidityPool))
        }
        LedgerKey::ContractData(k) => {
            let (a, kind, _) = sc_address(&k.contract);
            Some((a, kind))
        }
        LedgerKey::ContractCode(_) | LedgerKey::ConfigSetting(_) | LedgerKey::Ttl(_) => None,
    }
}

/// The sponsor recorded on an entry, if any.
pub fn sponsor(e: &LedgerEntry) -> Option<String> {
    match &e.ext {
        stellar_xdr::LedgerEntryExt::V0 => None,
        stellar_xdr::LedgerEntryExt::V1(v1) => v1.sponsoring_id.0.as_ref().map(account),
    }
}

pub fn entry_owner(e: &LedgerEntry) -> Option<(String, AddressKind)> {
    match &e.data {
        LedgerEntryData::ContractCode(_)
        | LedgerEntryData::ConfigSetting(_)
        | LedgerEntryData::Ttl(_) => None,
        _ => key_owner(&e.to_key()),
    }
}

/// Collects participants with roles and mux ids, then renders them sorted by address.
#[derive(Default)]
pub struct ParticipantSet {
    map: BTreeMap<
        String,
        (
            AddressKind,
            std::collections::BTreeSet<u64>,
            std::collections::BTreeSet<String>,
        ),
    >,
}

impl ParticipantSet {
    pub fn add(&mut self, address: String, kind: AddressKind, mux: Option<u64>, role: &str) {
        let slot = self
            .map
            .entry(address)
            .or_insert_with(|| (kind, Default::default(), Default::default()));
        if let Some(m) = mux {
            slot.1.insert(m);
        }
        slot.2.insert(role.to_string());
    }

    pub fn into_vec(self) -> Vec<Participant> {
        self.map
            .into_iter()
            .map(|(address, (kind, mux, roles))| Participant {
                address,
                kind,
                mux_ids: mux.into_iter().map(Int::from).collect(),
                roles: roles.into_iter().collect(),
            })
            .collect()
    }
}
