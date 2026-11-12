//! The oracle's output vocabulary, versioned by [`ORACLE_VOCAB_VERSION`].
//!
//! Every amount, balance, limit, id and sequence number is a decimal string so a JSON
//! reader never routes it through a float. Addresses are canonical strkeys (G, C, L, B);
//! signers keep their own form (G, T, X, P). Mux ids never replace the base address.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stellar_xdr::{Asset, TrustLineAsset};

/// Bumped whenever a kind, field or rule below changes meaning.
///
/// Version 2: `to_muxed_id` keeps the type the event gave it ([`ToMuxedId`]) and no longer
/// adds a mux id to the destination participant, since core fills it with the transaction
/// memo as well as with mux ids.
pub const ORACLE_VOCAB_VERSION: u32 = 2;

/// i128, i64 and u64 values as decimal strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Int(pub i128);

impl From<Int> for String {
    fn from(v: Int) -> String {
        v.0.to_string()
    }
}

impl TryFrom<String> for Int {
    type Error = std::num::ParseIntError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse().map(Int)
    }
}

impl JsonSchema for Int {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Int".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A signed integer of up to 128 bits as a decimal string.",
            "type": "string",
            "pattern": "^(0|-?[1-9][0-9]*)$"
        })
    }
}

impl From<i64> for Int {
    fn from(v: i64) -> Self {
        Int(v.into())
    }
}

impl From<u64> for Int {
    fn from(v: u64) -> Self {
        Int(v.into())
    }
}

impl From<u32> for Int {
    fn from(v: u32) -> Self {
        Int(v.into())
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum AddressKind {
    Account,
    Contract,
    LiquidityPool,
    ClaimableBalance,
}

/// An address an indexer should attribute the transaction to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Participant {
    pub address: String,
    pub kind: AddressKind,
    /// Mux ids this address carried, only from addresses that are muxed in the meta itself:
    /// `SC_ADDRESS_TYPE_MUXED_ACCOUNT` or muxed contract (CAP-84) addresses in event topics.
    /// A token event's `to_muxed_id` never lands here, because core also fills it with the
    /// transaction memo and the two cannot be told apart.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mux_ids: Vec<Int>,
    /// Why the address is a participant, sorted. Actual participants: `fee-source`,
    /// `entry-changed`, `event-address`, `event-contract`. Declared participants use the
    /// envelope field names listed in `declared`.
    pub roles: Vec<String>,
}

/// Where a change came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeScope {
    FeeProcessing,
    TxBefore,
    Operation,
    TxAfter,
    PostApplyFee,
    Upgrade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Created,
    Updated,
    Removed,
    Restored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Thresholds {
    pub low: u8,
    pub medium: u8,
    pub high: u8,
}

/// One normalized state change. `scope` and `op_index` say where in the meta it was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StateChange {
    pub scope: ChangeScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_index: Option<u32>,
    #[serde(flatten)]
    pub change: Change,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Change {
    AccountCreated {
        account: String,
        starting_balance: Int,
    },
    AccountRemoved {
        account: String,
        final_balance: Int,
    },
    /// A change in what an address holds of an asset: account native balance, trustline
    /// balance, claimable balance amount, pool reserve, or a Stellar Asset Contract balance
    /// entry. `holder` is the address whose holding changed.
    BalanceChange {
        holder: String,
        asset: String,
        /// `account`, `trustline`, `claimable-balance`, `liquidity-pool` or `contract`.
        held_in: String,
        before: Int,
        after: Int,
        delta: Int,
    },
    TrustlineCreated {
        account: String,
        asset: String,
        limit: Int,
        flags: u32,
    },
    TrustlineRemoved {
        account: String,
        asset: String,
    },
    TrustlineFlags {
        account: String,
        asset: String,
        before: u32,
        after: u32,
    },
    TrustlineLimit {
        account: String,
        asset: String,
        before: Int,
        after: Int,
    },
    SignerAdded {
        account: String,
        signer: String,
        weight: u32,
    },
    SignerRemoved {
        account: String,
        signer: String,
        weight: u32,
    },
    /// Includes the master key, whose weight is threshold 0 of the account.
    SignerWeight {
        account: String,
        signer: String,
        before: u32,
        after: u32,
    },
    Thresholds {
        account: String,
        before: Thresholds,
        after: Thresholds,
    },
    AccountFlags {
        account: String,
        before: u32,
        after: u32,
    },
    HomeDomain {
        account: String,
        before: String,
        after: String,
    },
    InflationDestination {
        account: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
    },
    Sequence {
        account: String,
        before: Int,
        after: Int,
    },
    /// Data entry name and values as hex, since neither has to be UTF-8.
    DataEntry {
        account: String,
        action: Action,
        name_hex: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value_before_hex: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value_after_hex: Option<String>,
    },
    /// The sponsor of a ledger entry or of one signer changed (created, revoked, transferred,
    /// or the sponsored thing went away). `target` names the entry or signer.
    Sponsorship {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signer: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sponsor_before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sponsor_after: Option<String>,
    },
    ClaimableBalanceCreated {
        balance_id: String,
        asset: String,
        amount: Int,
        claimants: Vec<String>,
    },
    /// Removed by a claim. The operation type tells a claim from a clawback, since the
    /// entry change itself is the same.
    ClaimableBalanceClaimed {
        balance_id: String,
        asset: String,
        amount: Int,
    },
    ClaimableBalanceClawedBack {
        balance_id: String,
        asset: String,
        amount: Int,
    },
    /// A pool-share trustline balance.
    PoolShares {
        account: String,
        pool: String,
        before: Int,
        after: Int,
        delta: Int,
    },
    LiquidityPool {
        pool: String,
        action: Action,
        asset_a: String,
        asset_b: String,
        reserve_a_before: Int,
        reserve_a_after: Int,
        reserve_b_before: Int,
        reserve_b_after: Int,
        shares_before: Int,
        shares_after: Int,
    },
    Offer {
        action: Action,
        offer_id: Int,
        seller: String,
        selling: String,
        buying: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amount_before: Option<Int>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amount_after: Option<Int>,
        price_n: i32,
        price_d: i32,
    },
    /// A contract instance entry appeared. Read from meta, never from auth entries.
    ContractDeployed {
        contract: String,
        /// `wasm:<hex hash>` or `stellar-asset`.
        executable: String,
    },
    ContractCode {
        action: Action,
        hash: String,
    },
    ContractData {
        contract: String,
        action: Action,
        durability: String,
        /// The key `ScVal` as base64 XDR.
        key_xdr: String,
    },
    Ttl {
        action: Action,
        key_hash: String,
        /// The entry the TTL belongs to, when its key is visible in this ledger.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<u32>,
    },
    ConfigSetting {
        action: Action,
        setting: String,
    },
    /// One offer or pool crossed by a manage-offer or path-payment operation, from the
    /// operation result's claim atoms. `counterparty` is the offer's seller or the pool.
    Trade {
        counterparty: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        offer_id: Option<Int>,
        /// What the counterparty gave up.
        sold_asset: String,
        sold_amount: Int,
        /// What the counterparty received.
        bought_asset: String,
        bought_amount: Int,
    },
    /// A consensus contract event that is not a token event (for example a protocol's
    /// own events, such as Blend's supply or borrow), with topics and data as base64 XDR.
    ContractEvent {
        /// `before-all-txs`, `operation`, `after-tx` or `after-all-txs`.
        stage: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        contract: Option<String>,
        topics_xdr: Vec<String>,
        data_xdr: String,
    },
    /// A token event (CAP-67 and SEP-41 shapes): transfer, mint, burn, clawback,
    /// set_authorized, fee, approve.
    TokenEvent(TokenEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TokenEvent {
    pub event: String,
    /// `before-all-txs`, `operation`, `after-tx` or `after-all-txs`.
    pub stage: String,
    pub contract: String,
    /// Set when the contract is the Stellar Asset Contract of this asset, which the oracle
    /// proves by recomputing the contract id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// The SEP-11 asset string a SAC event carries as its last topic, verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_topic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spender: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<Int>,
    /// The data map's `to_muxed_id` as the event typed it: `{"type": "u64", "value":
    /// "<decimal>"}`, `{"type": "string", "value": "<hex>"}` or `{"type": "bytes", "value":
    /// "<hex>"}`. Core writes the destination's mux id here when the destination is muxed
    /// and otherwise the transaction memo (id as u64, text as string, hash and return as
    /// bytes), so a u64 may be either a mux id or a memo id. It is not a participant mux id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_muxed_id: Option<ToMuxedId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorized: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_until_ledger: Option<u32>,
}

/// A CAP-67 `to_muxed_id` value, one variant per `ScVal` type core can put there
/// (`getPossibleMuxedData` and `makeClassicMemoSCVal` in stellar-core v30.0.0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum ToMuxedId {
    /// `SCV_U64`: a mux id or a memo id.
    U64(Int),
    /// `SCV_STRING` as hex: memo text, which core copies without any utf-8 check.
    String(String),
    /// `SCV_BYTES` as hex: memo hash or memo return, indistinguishable in the event.
    Bytes(String),
}

/// What the envelope says, kept apart from what the meta shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declared {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_source: Option<String>,
    pub participants: Vec<Participant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fee {
    /// The account whose balance paid the fee, read from fee processing.
    pub source: String,
    /// `feeCharged` from the result, after any refund.
    pub charged: Int,
    /// Net balance change of the fee source across fee processing and post-apply fee
    /// processing.
    pub balance_delta: Int,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxFacts {
    pub hash: String,
    pub apply_index: u32,
    pub successful: bool,
    /// Result code, for example `tx_success` or `tx_fee_bump_inner_failed`.
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_hash: Option<String>,
    pub fee: Fee,
    pub participants: Vec<Participant>,
    pub changes: Vec<StateChange>,
    pub declared: Declared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeFacts {
    /// The `LedgerUpgrade` as XDR-JSON.
    pub upgrade: serde_json::Value,
    pub changes: Vec<StateChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerFacts {
    pub ledger_seq: u32,
    pub ledger_hash: String,
    pub protocol: u32,
    pub close_time: Int,
    pub transactions: Vec<TxFacts>,
    pub upgrades: Vec<UpgradeFacts>,
    /// Keys evicted at the end of the ledger, described.
    pub evicted: Vec<String>,
}

/// The content of an `.oracle.json` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleFile {
    pub oracle_vocab_version: u32,
    pub network_passphrase: String,
    pub ledgers: Vec<LedgerFacts>,
}

/// SEP-11 style name: `native` or `CODE:ISSUER`.
pub fn asset_id(asset: &Asset) -> String {
    match asset {
        Asset::Native => "native".into(),
        Asset::CreditAlphanum4(a) => format!("{}:{}", a.asset_code, a.issuer),
        Asset::CreditAlphanum12(a) => format!("{}:{}", a.asset_code, a.issuer),
    }
}

pub fn trustline_asset_id(asset: &TrustLineAsset) -> String {
    match asset {
        TrustLineAsset::Native => "native".into(),
        TrustLineAsset::CreditAlphanum4(a) => format!("{}:{}", a.asset_code, a.issuer),
        TrustLineAsset::CreditAlphanum12(a) => format!("{}:{}", a.asset_code, a.issuer),
        TrustLineAsset::PoolShare(p) => format!("pool-share:{p}"),
    }
}
