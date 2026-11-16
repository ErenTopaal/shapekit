//! The chain every attempt runs on: a standalone core with invariant checks, a few bank
//! accounts that fund the attempts' own accounts, a shared credit issuer and, at protocol 30,
//! testnet's Soroban settings and the repository's test contracts.

use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::{Applied, ChainDriver, ChainError};
use shapekit_core_driver::config::NodeConfig;
use shapekit_core_driver::keys::account_address;
use shapekit_core_driver::keys::{account_id, derive_key, muxed_account, root_key};
use shapekit_core_driver::settings::NetworkSettings;
use shapekit_core_driver::tx::{BASE_FEE, TransactionBuilder, sign_for_protocol};
use shapekit_core_driver::tx::{contract_id_from_address, contract_preimage};
use stellar_xdr::{
    AlphaNum4, AlphaNum12, Asset, AssetCode4, AssetCode12, ConfigSettingEntry, ConfigUpgradeSet,
    ContractExecutable, ContractId, ContractIdPreimage, CreateAccountOp, CreateContractArgs,
    CreateContractArgsV2, Hash, HostFunction, Operation, OperationBody, ScAddress, ScBytes,
    ScString, ScVal, ScVec, Transaction, TransactionEnvelope, TransactionV1Envelope,
};

use crate::KEY_DOMAIN;

/// Accounts one bank creates per transaction, core's operation limit.
const OPS_PER_TX: usize = 100;

/// Bank accounts, each funded with [`BANK_BALANCE`].
const BANKS: usize = 1;

const BANK_BALANCE: i64 = 1_000_000_000_000_000;

/// Starting balance of every attempt account, 1,000 XLM.
pub const BALANCE: i64 = 10_000_000_000;

/// The accounts of one attempt: the transaction's source and three helpers, all funded.
pub struct Subject {
    pub source: SigningKey,
    pub peers: [SigningKey; 3],
    /// Derives further keys that are not funded, for accounts an attempt creates itself.
    pub tag: String,
}

impl Subject {
    pub fn new(tag: String) -> Self {
        let k = |role: &str| derive_key(KEY_DOMAIN, &format!("{tag}/{role}"));
        Self {
            source: k("source"),
            peers: [k("peer-0"), k("peer-1"), k("peer-2")],
            tag,
        }
    }

    /// A key no one has funded.
    pub fn fresh(&self, role: &str) -> SigningKey {
        derive_key(KEY_DOMAIN, &format!("{}/fresh-{role}", self.tag))
    }

    pub fn funded(&self) -> Vec<&SigningKey> {
        std::iter::once(&self.source).chain(&self.peers).collect()
    }
}

/// The chains carriers run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorldKind {
    /// Protocol 30 under testnet's Soroban settings, with the test contracts deployed.
    Current,
    /// Protocol 12, the last that accepts v0 envelopes.
    V0,
    /// Protocol 30 under testnet's settings except state archival: persistent entries live
    /// [`ARCHIVAL_MIN_PERSISTENT_TTL`] ledgers, so an attempt can archive its own entry.
    Archival,
}

impl WorldKind {
    /// The suffix of the chain id its attempts name.
    pub fn chain(self) -> &'static str {
        match self {
            WorldKind::Current => "p30",
            WorldKind::V0 => "p12",
            WorldKind::Archival => "p30-archival",
        }
    }

    pub fn protocol(self) -> u32 {
        match self {
            WorldKind::Current | WorldKind::Archival => 30,
            WorldKind::V0 => 12,
        }
    }
}

/// Persistent entry lifetime of the archival world, cap66.archival's value.
pub const ARCHIVAL_MIN_PERSISTENT_TTL: u32 = 10;

/// Echo instances the archival world deploys before it lowers the lifetime, so they stay
/// live while the value each attempt stores in one of them archives.
const ARCHIVAL_INSTANCES: usize = 24;

pub const ECHO_WASM: &[u8] = shapekit_core_driver::contracts::ECHO_WASM;
pub const AUTH_TREE_WASM: &[u8] = include_bytes!("../../../contracts/wasm/auth_tree.wasm");
pub const DELEGATING_ACCOUNT_WASM: &[u8] =
    include_bytes!("../../../contracts/wasm/delegating_account.wasm");
pub const EXEC_REF_WASM: &[u8] = include_bytes!("../../../contracts/wasm/exec_ref.wasm");
pub const FACTORY_WASM: &[u8] = include_bytes!("../../../contracts/wasm/factory.wasm");

/// The contracts of a Soroban world.
pub struct Contracts {
    pub deployer: SigningKey,
    pub echo_code: Hash,
    pub echo: ContractId,
    pub auth_tree: ContractId,
    /// A second auth_tree instance, the level below `auth_tree` (the host forbids re-entry).
    pub auth_tree_child: ContractId,
    /// The only delegate the delegating account accepts.
    pub delegate: SigningKey,
    pub delegating: ContractId,
    /// Owns the executable tag `echo`, which names the echo wasm.
    pub exec_ref: ContractId,
    pub factory_code: Hash,
    pub native_sac: ContractId,
    /// Archival world only: echo instances with long lived instance entries, one per
    /// attempt that needs an archived value.
    pub spare: Vec<ContractId>,
}

pub struct World {
    pub driver: ChainDriver,
    pub kind: WorldKind,
    pub protocol: u32,
    pub root: SigningKey,
    banks: Vec<SigningKey>,
    /// Issues [`World::credit4`] and [`World::credit12`] with no flags set.
    pub issuer: SigningKey,
    pub contracts: Option<Contracts>,
}

impl World {
    /// Boots core for `kind` and funds the banks and the issuer; a Soroban world also gets
    /// testnet's settings and the test contracts.
    pub fn start(kind: WorldKind, cfg: NodeConfig, workdir: &Path) -> Result<Self> {
        ensure!(
            cfg.protocol == kind.protocol(),
            "{kind:?} runs protocol {}",
            kind.protocol()
        );
        let protocol = cfg.protocol;
        let root = root_key(&cfg.passphrase);
        let driver = ChainDriver::start(cfg, workdir).context("starting core")?;
        let banks: Vec<SigningKey> = (0..BANKS)
            .map(|i| derive_key(KEY_DOMAIN, &format!("bank-{i}")))
            .collect();
        let issuer = derive_key(KEY_DOMAIN, "issuer");
        let mut w = Self {
            driver,
            kind,
            protocol,
            root,
            banks,
            issuer,
            contracts: None,
        };
        let writer = if kind == WorldKind::V0 {
            None
        } else {
            let root = w.root.clone();
            let applied = w
                .driver
                .apply_settings(&NetworkSettings::testnet()?, &root, ECHO_WASM)
                .context("installing testnet's soroban settings")?;
            Some(applied.writer)
        };
        let deployer = derive_key(KEY_DOMAIN, "deployer");
        let delegate = derive_key(KEY_DOMAIN, "delegate");
        let ops = w
            .banks
            .iter()
            .map(|k| create(k, BANK_BALANCE))
            .chain([&w.issuer, &deployer, &delegate].map(|k| create(k, BALANCE)))
            .collect();
        let root = w.root.clone();
        w.classic_ok(&root, ops, &[]).context("funding the banks")?;
        if let Some(writer) = writer {
            w.contracts = Some(w.deploy_contracts(deployer, delegate, &writer)?);
        }
        Ok(w)
    }

    fn deploy_contracts(
        &mut self,
        deployer: SigningKey,
        delegate: SigningKey,
        writer: &ContractId,
    ) -> Result<Contracts> {
        let d = &mut self.driver;
        let salt =
            |what: &str| -> [u8; 32] { Sha256::digest(format!("shapekit.probe/{what}")).into() };
        let echo_code = d.upload(&deployer, ECHO_WASM)?;
        let echo = d.deploy(&deployer, &echo_code, &salt("echo"))?;
        let tree_code = d.upload(&deployer, AUTH_TREE_WASM)?;
        let auth_tree = d.deploy(&deployer, &tree_code, &salt("auth-tree"))?;
        let auth_tree_child = d.deploy(&deployer, &tree_code, &salt("auth-tree-child"))?;
        let ref_code = d.upload(&deployer, EXEC_REF_WASM)?;
        let exec_ref = d.deploy(&deployer, &ref_code, &salt("exec-ref"))?;
        d.invoke(
            &deployer,
            &exec_ref,
            "publish",
            vec![
                ScVal::String(ScString("echo".try_into()?)),
                ScVal::Bytes(ScBytes(echo_code.0.to_vec().try_into()?)),
            ],
        )?;
        let factory_code = d.upload(&deployer, FACTORY_WASM)?;
        let delegating_code = d.upload(&deployer, DELEGATING_ACCOUNT_WASM)?;
        let deployer_addr = account_address(&deployer.verifying_key());
        let dsalt = salt("delegating-account");
        let delegating = contract_id_from_address(d.network_id(), &deployer_addr, &dsalt)?;
        let hf = HostFunction::CreateContractV2(CreateContractArgsV2 {
            contract_id_preimage: contract_preimage(&deployer_addr, &dsalt),
            executable: ContractExecutable::Wasm(delegating_code),
            constructor_args: vec![ScVal::Vec(Some(ScVec(
                vec![ScVal::Address(account_address(&delegate.verifying_key()))].try_into()?,
            )))]
            .try_into()?,
        });
        let env = d.soroban(&deployer, hf)?;
        d.apply_ok(&env)
            .context("deploying the delegating account")?;
        let hf = HostFunction::CreateContract(CreateContractArgs {
            contract_id_preimage: ContractIdPreimage::Asset(Asset::Native),
            executable: ContractExecutable::StellarAsset,
        });
        let env = d.soroban(&deployer, hf)?;
        let applied = d
            .apply_ok(&env)
            .context("deploying the native asset contract")?;
        let native_sac = match applied.return_value() {
            Some(ScVal::Address(ScAddress::Contract(id))) => id.clone(),
            other => bail!("the native asset contract deploy returned {other:?}"),
        };
        let mut spare = Vec::new();
        if self.kind == WorldKind::Archival {
            for i in 0..ARCHIVAL_INSTANCES {
                spare.push(self.driver.deploy(
                    &deployer,
                    &echo_code,
                    &salt(&format!("spare-{i}")),
                )?);
            }
            let ConfigSettingEntry::StateArchival(mut archival) = self
                .driver
                .node()
                .soroban_upgrade_set()?
                .updated_entry
                .iter()
                .find(|e| matches!(e, ConfigSettingEntry::StateArchival(_)))
                .cloned()
                .context("the live settings have no state archival entry")?
            else {
                unreachable!("matched above")
            };
            archival.min_persistent_ttl = ARCHIVAL_MIN_PERSISTENT_TTL;
            let set = ConfigUpgradeSet {
                updated_entry: vec![ConfigSettingEntry::StateArchival(archival)].try_into()?,
            };
            self.driver.apply_upgrade_set(set, writer, &deployer)?;
        }
        Ok(Contracts {
            deployer,
            echo_code,
            echo,
            auth_tree,
            auth_tree_child,
            delegate,
            delegating,
            exec_ref,
            factory_code,
            native_sac,
            spare,
        })
    }

    pub fn contracts(&self) -> Result<&Contracts> {
        self.contracts
            .as_ref()
            .context("this world runs no soroban contracts")
    }

    pub fn network_id(&self) -> [u8; 32] {
        *self.driver.network_id()
    }

    pub fn credit4(&self) -> Asset {
        Asset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(*b"PRB\0"),
            issuer: account_id(&self.issuer.verifying_key()),
        })
    }

    pub fn credit12(&self) -> Asset {
        Asset::CreditAlphanum12(AlphaNum12 {
            asset_code: AssetCode12(*b"PROBELONG\0\0\0"),
            issuer: account_id(&self.issuer.verifying_key()),
        })
    }

    /// Creates `keys` with [`BALANCE`] each, a hundred per ledger: the ledger's transaction set
    /// holds at most 100 operations (`maxTxSetSize` from genesis), so a larger batch would leave
    /// a funding transaction out.
    pub fn fund(&mut self, keys: &[&SigningKey]) -> Result<()> {
        let bank = self.banks[0].clone();
        for chunk in keys.chunks(OPS_PER_TX) {
            let ops = chunk.iter().map(|k| create(k, BALANCE)).collect();
            self.classic_ok(&bank, ops, &[])
                .context("funding probe accounts")?;
        }
        Ok(())
    }

    /// Pays `amount` stroops from the bank to `to`.
    pub fn top_up(&mut self, to: &SigningKey, amount: i64) -> Result<()> {
        let bank = self.banks[0].clone();
        self.classic_ok(
            &bank,
            vec![OperationBody::Payment(stellar_xdr::PaymentOp {
                destination: muxed_account(&to.verifying_key()),
                asset: Asset::Native,
                amount,
            })],
            &[],
        )
        .context("topping up a probe account")?;
        Ok(())
    }

    /// The source's next sequence number.
    pub fn next_seq(&self, source: &SigningKey) -> Result<i64> {
        Ok(self
            .driver
            .node()
            .account_seq(&account_id(&source.verifying_key()))?
            + 1)
    }

    /// A builder for a transaction from `source` with its next sequence number.
    pub fn builder(&self, source: &SigningKey) -> Result<TransactionBuilder> {
        Ok(TransactionBuilder::new(
            muxed_account(&source.verifying_key()),
            self.next_seq(source)?,
        ))
    }

    /// Operations from `source` (each may name its own source among `others`), base fee per
    /// operation, signed by `source` and `others`; fails unless it succeeds.
    pub fn classic_ok(
        &mut self,
        source: &SigningKey,
        ops: Vec<OperationBody>,
        others: &[&SigningKey],
    ) -> Result<Applied> {
        let ops = ops.into_iter().map(|b| (None, b)).collect();
        self.ops_ok(source, ops, others)
    }

    /// [`Self::classic_ok`] with an operation source per operation.
    pub fn ops_ok(
        &mut self,
        source: &SigningKey,
        ops: Vec<(Option<&SigningKey>, OperationBody)>,
        others: &[&SigningKey],
    ) -> Result<Applied> {
        let mut b = self.builder(source)?;
        for (s, body) in ops {
            b = b.operation(s.map(|k| muxed_account(&k.verifying_key())), body);
        }
        let tx = b.build()?;
        let mut keys = vec![source];
        keys.extend_from_slice(others);
        let env = sign_for_protocol(tx, self.protocol, &self.network_id(), &keys)?;
        Ok(self.driver.apply_ok(&env)?)
    }

    /// Submits and closes; whatever core answered.
    pub fn apply(&mut self, env: &TransactionEnvelope) -> std::result::Result<Applied, ChainError> {
        self.driver.apply(env)
    }

    /// The unsigned v1 envelope of `tx`.
    pub fn unsigned(tx: Transaction) -> TransactionEnvelope {
        TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: Default::default(),
        })
    }

    /// One unsigned v1 transaction from `source` carrying `ops`.
    pub fn tx(
        &self,
        source: &SigningKey,
        ops: Vec<(Option<&SigningKey>, OperationBody)>,
    ) -> Result<Transaction> {
        let mut b = self.builder(source)?;
        for (s, body) in ops {
            b = b.operation(s.map(|k| muxed_account(&k.verifying_key())), body);
        }
        Ok(b.fee(0).build()?)
    }
}

pub fn create(k: &SigningKey, balance: i64) -> OperationBody {
    OperationBody::CreateAccount(CreateAccountOp {
        destination: account_id(&k.verifying_key()),
        starting_balance: balance,
    })
}

/// An operation without its own source.
pub fn op(body: OperationBody) -> Operation {
    Operation {
        source_account: None,
        body,
    }
}

/// The base fee per operation, for transactions built by hand.
pub fn fee_for(ops: usize) -> u32 {
    BASE_FEE * u32::try_from(ops.max(1)).unwrap_or(u32::MAX)
}

/// Fails unless `applied` succeeded, naming the step.
pub fn ensure_ok(applied: &Applied, what: &str) -> Result<()> {
    if !applied.succeeded() {
        bail!("{what} failed: {:?}", applied.result.result);
    }
    Ok(())
}
