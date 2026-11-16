//! Drives one recorded chain: a node, its recorder and the next ledger to
//! close. Every transaction goes through core's `tx` endpoint and closes in
//! its own ledger, and every close is checked against the meta core wrote,
//! so a step that core refused or failed never passes silently.

use std::path::Path;

use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use stellar_xdr::{
    ConfigUpgradeSet, ContractExecutable, ContractId, CreateContractArgsV2, Hash, HostFunction,
    InvokeContractArgs, InvokeHostFunctionOp, LedgerCloseMeta, LedgerUpgrade, OperationBody,
    ScAddress, ScSymbol, ScVal, SorobanAuthorizationEntry, SorobanCredentials,
    SorobanTransactionData, TransactionEnvelope, TransactionMeta, TransactionResult,
    TransactionResultResult,
};

use crate::config::{NodeConfig, close_time};
use crate::keys::{account_address, account_id, muxed_account, network_id};
use crate::node::{CoreNode, NodeError, SubmitStatus};
use crate::recorder::{ChainRecorder, ChainSummary, FIRST_LEDGER, RecorderError};
use crate::settings::{NetworkSettings, PreparedUpgrade, SettingsError};
use crate::sim::{AuthMode, SimError};
use crate::sim_legacy::AnySimulator;
use crate::tx::{
    BASE_FEE, TransactionBuilder, TxError, assemble_soroban, contract_id_from_address,
    contract_preimage, envelope_hash, sign_for_protocol, sign_transaction,
};

#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    #[error(transparent)]
    Node(#[from] NodeError),
    #[error(transparent)]
    Recorder(#[from] RecorderError),
    #[error(transparent)]
    Sim(#[from] SimError),
    #[error(transparent)]
    Tx(#[from] TxError),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error("core did not queue the transaction ({status:?}): {result:?}")]
    NotQueued {
        status: SubmitStatus,
        result: Option<Box<TransactionResult>>,
    },
    #[error("ledger {ledger}: transaction {hash} is not in the ledger's meta")]
    Missing { ledger: u32, hash: String },
    #[error("ledger {ledger}: transaction {hash} failed: {result:?}")]
    Failed {
        ledger: u32,
        hash: String,
        result: Box<TransactionResultResult>,
    },
    #[error("ledger {ledger}: {what}")]
    Unexpected { ledger: u32, what: String },
    #[error("simulation asks for auth entries that need signatures: {0:?}")]
    NeedsSignatures(Vec<SorobanAuthorizationEntry>),
}

/// One applied transaction as the ledger's meta reports it.
#[derive(Debug, Clone)]
pub struct Applied {
    pub ledger: u32,
    pub hash: [u8; 32],
    pub result: TransactionResult,
    pub meta: TransactionMeta,
}

impl Applied {
    pub fn succeeded(&self) -> bool {
        matches!(
            self.result.result,
            TransactionResultResult::TxSuccess(_)
                | TransactionResultResult::TxFeeBumpInnerSuccess(_)
        )
    }

    /// The host function's return value, for a successful Soroban call.
    pub fn return_value(&self) -> Option<&ScVal> {
        match &self.meta {
            TransactionMeta::V3(m) => m.soroban_meta.as_ref().map(|s| &s.return_value),
            TransactionMeta::V4(m) => m
                .soroban_meta
                .as_ref()
                .and_then(|s| s.return_value.as_ref()),
            _ => None,
        }
    }
}

/// What a settings upgrade left behind, for the caller to report.
#[derive(Debug, Clone)]
pub struct SettingsApplied {
    pub writer: ContractId,
    pub upgrade: PreparedUpgrade,
    pub upgrade_ledger: u32,
    /// `sorobaninfo?format=basic` after the upgrade.
    pub soroban_info: serde_json::Value,
}

/// Salt of the contract instance that stores config upgrade sets. One
/// instance per chain, deployed by the root account.
pub const SETTINGS_WRITER_SALT_DOMAIN: &[u8] = b"shapekit/settings-writer/v1";

pub struct ChainDriver {
    node: CoreNode,
    recorder: ChainRecorder,
    last_closed: u32,
    network_id: [u8; 32],
}

impl std::fmt::Debug for ChainDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainDriver")
            .field("node", &self.node)
            .field("last_closed", &self.last_closed)
            .finish()
    }
}

fn transactions(lcm: &LedgerCloseMeta) -> Vec<(Hash, TransactionResult, TransactionMeta)> {
    match lcm {
        LedgerCloseMeta::V0(m) => m
            .tx_processing
            .iter()
            .map(|t| {
                (
                    t.result.transaction_hash.clone(),
                    t.result.result.clone(),
                    t.tx_apply_processing.clone(),
                )
            })
            .collect(),
        LedgerCloseMeta::V1(m) => m
            .tx_processing
            .iter()
            .map(|t| {
                (
                    t.result.transaction_hash.clone(),
                    t.result.result.clone(),
                    t.tx_apply_processing.clone(),
                )
            })
            .collect(),
        LedgerCloseMeta::V2(m) => m
            .tx_processing
            .iter()
            .map(|t| {
                (
                    t.result.transaction_hash.clone(),
                    t.result.result.clone(),
                    t.tx_apply_processing.clone(),
                )
            })
            .collect(),
    }
}

/// The upgrades a ledger's meta reports as applied.
pub fn upgrades(lcm: &LedgerCloseMeta) -> Vec<LedgerUpgrade> {
    let list = match lcm {
        LedgerCloseMeta::V0(m) => &m.upgrades_processing,
        LedgerCloseMeta::V1(m) => &m.upgrades_processing,
        LedgerCloseMeta::V2(m) => &m.upgrades_processing,
    };
    list.iter().map(|u| u.upgrade.clone()).collect()
}

impl ChainDriver {
    /// Boots a node in `workdir`, starts recording and upgrades ledger 2 to
    /// the config's protocol.
    pub fn start(cfg: NodeConfig, workdir: &Path) -> Result<Self, ChainError> {
        let protocol = cfg.protocol;
        let network_id = network_id(&cfg.passphrase);
        let node = CoreNode::start(cfg, workdir)?;
        let mut recorder = ChainRecorder::new(&node);
        node.upgrade_to(protocol, FIRST_LEDGER, close_time(FIRST_LEDGER))?;
        recorder.collect_through(FIRST_LEDGER)?;
        Ok(Self {
            node,
            recorder,
            last_closed: FIRST_LEDGER,
            network_id,
        })
    }

    pub fn node(&self) -> &CoreNode {
        &self.node
    }

    pub fn recorder(&self) -> &ChainRecorder {
        &self.recorder
    }

    pub fn network_id(&self) -> &[u8; 32] {
        &self.network_id
    }

    pub fn last_closed(&self) -> u32 {
        self.last_closed
    }

    /// Meta of the last closed ledger.
    pub fn last_meta(&self) -> &LedgerCloseMeta {
        &self
            .recorder
            .frames()
            .last()
            .expect("a started chain has recorded ledger 2")
            .meta
    }

    fn closed(&mut self, seq: u32) -> Result<(), ChainError> {
        self.recorder.collect_through(seq)?;
        self.last_closed = seq;
        Ok(())
    }

    /// Closes the next ledger with nothing in it.
    pub fn close_empty(&mut self) -> Result<u32, ChainError> {
        let seq = self.last_closed + 1;
        self.node.close(seq, close_time(seq))?;
        self.closed(seq)?;
        Ok(seq)
    }

    /// Submits `env`, closes the next ledger and returns what core applied,
    /// successful or not.
    pub fn apply(&mut self, env: &TransactionEnvelope) -> Result<Applied, ChainError> {
        let hash = envelope_hash(env, &self.network_id)?;
        let sub = self.node.submit(env)?;
        if sub.status != SubmitStatus::Pending {
            return Err(ChainError::NotQueued {
                status: sub.status,
                result: sub.error.map(Box::new),
            });
        }
        let seq = self.close_empty()?;
        let found = transactions(self.last_meta())
            .into_iter()
            .find(|(h, _, _)| h.0 == hash);
        let Some((_, result, meta)) = found else {
            return Err(ChainError::Missing {
                ledger: seq,
                hash: hex::encode(hash),
            });
        };
        Ok(Applied {
            ledger: seq,
            hash,
            result,
            meta,
        })
    }

    /// Submits `env` without closing a ledger; it waits in core's queue for the next close.
    /// Returns its hash. Fails unless core queued it.
    pub fn queue(&mut self, env: &TransactionEnvelope) -> Result<[u8; 32], ChainError> {
        let hash = envelope_hash(env, &self.network_id)?;
        let sub = self.node.submit(env)?;
        if sub.status != SubmitStatus::Pending {
            return Err(ChainError::NotQueued {
                status: sub.status,
                result: sub.error.map(Box::new),
            });
        }
        Ok(hash)
    }

    /// What the last closed ledger applied for the transaction with `hash`.
    pub fn applied_in_last(&self, hash: &[u8; 32]) -> Result<Applied, ChainError> {
        let ledger = self.last_closed;
        transactions(self.last_meta())
            .into_iter()
            .find(|(h, _, _)| h.0 == *hash)
            .map(|(_, result, meta)| Applied {
                ledger,
                hash: *hash,
                result,
                meta,
            })
            .ok_or_else(|| ChainError::Missing {
                ledger,
                hash: hex::encode(hash),
            })
    }

    /// [`Self::apply`], failing unless the transaction succeeded.
    pub fn apply_ok(&mut self, env: &TransactionEnvelope) -> Result<Applied, ChainError> {
        let applied = self.apply(env)?;
        if !applied.succeeded() {
            return Err(ChainError::Failed {
                ledger: applied.ledger,
                hash: hex::encode(applied.hash),
                result: Box::new(applied.result.result),
            });
        }
        Ok(applied)
    }

    fn next_seq_num(&self, source: &SigningKey) -> Result<i64, ChainError> {
        Ok(self
            .node
            .account_seq(&account_id(&source.verifying_key()))?
            + 1)
    }

    /// One classic transaction from `source` with these operations, base
    /// fee per operation, signed by `source` alone. Below protocol 13 it is a
    /// v0 envelope, since core refuses v1 there.
    pub fn classic(
        &self,
        source: &SigningKey,
        ops: Vec<OperationBody>,
    ) -> Result<TransactionEnvelope, ChainError> {
        let mut b = TransactionBuilder::new(
            muxed_account(&source.verifying_key()),
            self.next_seq_num(source)?,
        );
        for op in ops {
            b = b.operation(None, op);
        }
        let tx = b.build()?;
        Ok(sign_for_protocol(
            tx,
            self.node.config().protocol,
            &self.network_id,
            &[source],
        )?)
    }

    /// Simulates `host_function` against the last closed ledger, with the
    /// host for the ledger's protocol, and returns the assembled, signed
    /// transaction. Only source-account authorization is supported: anything
    /// else would need more signers.
    pub fn soroban(
        &self,
        source: &SigningKey,
        host_function: HostFunction,
    ) -> Result<TransactionEnvelope, ChainError> {
        self.soroban_at(source, host_function, self.next_seq_num(source)?)
    }

    /// [`Self::soroban`] with the sequence number the transaction carries given by the caller,
    /// for a source that already has a transaction waiting for the next close.
    pub fn soroban_at(
        &self,
        source: &SigningKey,
        host_function: HostFunction,
        seq_num: i64,
    ) -> Result<TransactionEnvelope, ChainError> {
        let ctx = self.node.ledger_context()?;
        let sim = AnySimulator::new(self.node.snapshot()?, &ctx)?;
        let source_id = account_id(&source.verifying_key());
        let r = sim.simulate_invoke(
            host_function.clone(),
            AuthMode::Record {
                allow_non_root: false,
                address_v2: false,
            },
            &source_id,
        )?;
        if r.auth
            .iter()
            .any(|a| !matches!(a.credentials, SorobanCredentials::SourceAccount))
        {
            return Err(ChainError::NeedsSignatures(r.auth));
        }
        self.soroban_with_at(source, host_function, r.transaction_data, r.auth, seq_num)
    }

    /// Builds and signs a Soroban transaction with resources chosen by the
    /// caller instead of simulation, for steps simulation cannot price.
    pub fn soroban_with(
        &self,
        source: &SigningKey,
        host_function: HostFunction,
        data: SorobanTransactionData,
        auth: Vec<SorobanAuthorizationEntry>,
    ) -> Result<TransactionEnvelope, ChainError> {
        self.soroban_with_at(
            source,
            host_function,
            data,
            auth,
            self.next_seq_num(source)?,
        )
    }

    /// A simulator over the node's current state, at its ledger's protocol.
    pub fn simulator(&self) -> Result<AnySimulator, ChainError> {
        let ctx = self.node.ledger_context()?;
        Ok(AnySimulator::new(self.node.snapshot()?, &ctx)?)
    }

    /// A signed single-operation Soroban transaction for any Soroban operation (an
    /// invocation with its auth already in the operation, an extend or a restore), with the
    /// resources in `data` and the given sequence number.
    pub fn soroban_op_at(
        &self,
        source: &SigningKey,
        body: OperationBody,
        data: SorobanTransactionData,
        seq_num: i64,
    ) -> Result<TransactionEnvelope, ChainError> {
        let tx = TransactionBuilder::new(muxed_account(&source.verifying_key()), seq_num)
            .fee(BASE_FEE)
            .operation(None, body)
            .build()?;
        let tx = assemble_soroban(tx, data, None)?;
        Ok(TransactionEnvelope::Tx(sign_transaction(
            tx,
            &self.network_id,
            &[source],
        )?))
    }

    /// [`Self::soroban_with`] with the sequence number given by the caller.
    pub fn soroban_with_at(
        &self,
        source: &SigningKey,
        host_function: HostFunction,
        data: SorobanTransactionData,
        auth: Vec<SorobanAuthorizationEntry>,
        seq_num: i64,
    ) -> Result<TransactionEnvelope, ChainError> {
        let tx = TransactionBuilder::new(muxed_account(&source.verifying_key()), seq_num)
            .fee(BASE_FEE)
            .operation(
                None,
                OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                    host_function,
                    auth: Default::default(),
                }),
            )
            .build()?;
        let tx = assemble_soroban(tx, data, Some(auth))?;
        Ok(TransactionEnvelope::Tx(sign_transaction(
            tx,
            &self.network_id,
            &[source],
        )?))
    }

    /// Uploads `wasm` from `source`; returns the code hash.
    pub fn upload(&mut self, source: &SigningKey, wasm: &[u8]) -> Result<Hash, ChainError> {
        let hf = HostFunction::UploadContractWasm(wasm.to_vec().try_into().map_err(TxError::Xdr)?);
        let env = self.soroban(source, hf)?;
        let applied = self.apply_ok(&env)?;
        match applied.return_value() {
            Some(ScVal::Bytes(b)) if b.len() == 32 => {
                Ok(Hash(b.as_slice().try_into().expect("32 bytes")))
            }
            other => Err(ChainError::Unexpected {
                ledger: applied.ledger,
                what: format!("upload returned {other:?}"),
            }),
        }
    }

    /// Creates a contract instance of `code` deployed by `source` with a
    /// salt, no constructor arguments; returns its id, which is checked
    /// against the id derived locally.
    pub fn deploy(
        &mut self,
        source: &SigningKey,
        code: &Hash,
        salt: &[u8; 32],
    ) -> Result<ContractId, ChainError> {
        let deployer = account_address(&source.verifying_key());
        let want = contract_id_from_address(&self.network_id, &deployer, salt)?;
        // create_contract_v2 arrived with protocol 22 (CAP-58); before it, the original
        // create_contract does the same for a contract without a constructor
        let hf = if self.node.ledger_context()?.protocol_version >= 22 {
            HostFunction::CreateContractV2(CreateContractArgsV2 {
                contract_id_preimage: contract_preimage(&deployer, salt),
                executable: ContractExecutable::Wasm(code.clone()),
                constructor_args: Default::default(),
            })
        } else {
            HostFunction::CreateContract(stellar_xdr::CreateContractArgs {
                contract_id_preimage: contract_preimage(&deployer, salt),
                executable: ContractExecutable::Wasm(code.clone()),
            })
        };
        let env = self.soroban(source, hf)?;
        let applied = self.apply_ok(&env)?;
        match applied.return_value() {
            Some(ScVal::Address(ScAddress::Contract(id))) if *id == want => Ok(want),
            other => Err(ChainError::Unexpected {
                ledger: applied.ledger,
                what: format!("deploy returned {other:?}, expected contract {want:?}"),
            }),
        }
    }

    /// Calls `function` on `contract` from `source`.
    pub fn invoke(
        &mut self,
        source: &SigningKey,
        contract: &ContractId,
        function: &str,
        args: Vec<ScVal>,
    ) -> Result<Applied, ChainError> {
        let hf = HostFunction::InvokeContract(InvokeContractArgs {
            contract_address: ScAddress::Contract(contract.clone()),
            function_name: ScSymbol(function.try_into().map_err(TxError::Xdr)?),
            args: args.try_into().map_err(TxError::Xdr)?,
        });
        let env = self.soroban(source, hf)?;
        self.apply_ok(&env)
    }

    /// Applies `settings` through a config upgrade set (CAP-46-09), as
    /// validators do: `deployer` uploads `writer_wasm` (a contract whose
    /// `put_temp(key, value)` stores a temporary entry, the repository's echo
    /// contract), deploys one instance, stores the set under its hash, then
    /// the upgrade is armed and closed. The node must allow these steps
    /// under its current limits, which on a fresh network means
    /// `TESTING_SOROBAN_HIGH_LIMIT_OVERRIDE`. Afterwards core's
    /// `sorobaninfo` must report exactly the set's entries.
    pub fn apply_settings(
        &mut self,
        settings: &NetworkSettings,
        deployer: &SigningKey,
        writer_wasm: &[u8],
    ) -> Result<SettingsApplied, ChainError> {
        let protocol = self.node.config().protocol;
        if protocol != settings.protocol_version {
            return Err(ChainError::Unexpected {
                ledger: self.last_closed,
                what: format!(
                    "settings were captured at protocol {} and only validate there, the chain runs {protocol}",
                    settings.protocol_version
                ),
            });
        }
        let code = self.upload(deployer, writer_wasm)?;
        let salt: [u8; 32] = Sha256::digest(SETTINGS_WRITER_SALT_DOMAIN).into();
        let writer = self.deploy(deployer, &code, &salt)?;
        let (upgrade, seq) = self.apply_upgrade_set(settings.upgrade_set()?, &writer, deployer)?;
        let live = self.node.soroban_upgrade_set()?;
        if live != upgrade.set {
            return Err(ChainError::Unexpected {
                ledger: seq,
                what: "sorobaninfo upgrade_xdr differs from the applied settings".into(),
            });
        }
        Ok(SettingsApplied {
            writer,
            upgrade,
            upgrade_ledger: seq,
            soroban_info: self.node.soroban_info()?,
        })
    }

    /// The settings writer [`Self::apply_settings`] deploys from `deployer`.
    pub fn settings_writer(&self, deployer: &SigningKey) -> Result<ContractId, ChainError> {
        let salt: [u8; 32] = Sha256::digest(SETTINGS_WRITER_SALT_DOMAIN).into();
        Ok(contract_id_from_address(
            &self.network_id,
            &account_address(&deployer.verifying_key()),
            &salt,
        )?)
    }

    /// Applies one config upgrade set (CAP-46-09) on top of the live
    /// settings: `source` stores it through `writer`'s `put_temp`, then the
    /// upgrade is armed and the next ledger closes with it. Fails unless
    /// that ledger's meta reports exactly this upgrade. Returns the prepared
    /// upgrade and the upgrade ledger.
    pub fn apply_upgrade_set(
        &mut self,
        set: ConfigUpgradeSet,
        writer: &ContractId,
        source: &SigningKey,
    ) -> Result<(PreparedUpgrade, u32), ChainError> {
        let upgrade = PreparedUpgrade::new(set, writer.clone())?;
        self.invoke(
            source,
            writer,
            "put_temp",
            vec![upgrade.storage_key()?, upgrade.storage_value()?],
        )?;
        let seq = self.close_with_upgrades(&[("configupgradesetkey", &upgrade.key_base64()?)])?;
        let applied = upgrades(self.last_meta());
        if applied != [LedgerUpgrade::Config(upgrade.key.clone())] {
            return Err(ChainError::Unexpected {
                ledger: seq,
                what: format!("expected exactly the config upgrade, meta has {applied:?}"),
            });
        }
        Ok((upgrade, seq))
    }

    /// Arms the upgrades in `params` (see [`CoreNode::upgrade_with`]) and
    /// closes the next ledger with them. Fails if the ledger's meta reports
    /// no upgrade at all; the caller checks which ones.
    pub fn close_with_upgrades(&mut self, params: &[(&str, &str)]) -> Result<u32, ChainError> {
        let seq = self.last_closed + 1;
        self.node.upgrade_with(params, seq, close_time(seq))?;
        self.closed(seq)?;
        if upgrades(self.last_meta()).is_empty() {
            return Err(ChainError::Unexpected {
                ledger: seq,
                what: format!("core applied none of the armed upgrades {params:?}"),
            });
        }
        Ok(seq)
    }

    /// Submits every envelope, then closes one ledger, so they share its
    /// transaction set. Returns what core applied for each, in the order
    /// given, successful or not. Fails if core refuses one at submission or
    /// leaves one out of the ledger.
    pub fn apply_together(
        &mut self,
        envs: &[TransactionEnvelope],
    ) -> Result<Vec<Applied>, ChainError> {
        let mut hashes = Vec::with_capacity(envs.len());
        for env in envs {
            hashes.push(envelope_hash(env, &self.network_id)?);
            let sub = self.node.submit(env)?;
            if sub.status != SubmitStatus::Pending {
                return Err(ChainError::NotQueued {
                    status: sub.status,
                    result: sub.error.map(Box::new),
                });
            }
        }
        let seq = self.close_empty()?;
        let txs = transactions(self.last_meta());
        hashes
            .into_iter()
            .map(|hash| {
                let (_, result, meta) = txs
                    .iter()
                    .find(|(h, _, _)| h.0 == hash)
                    .cloned()
                    .ok_or_else(|| ChainError::Missing {
                        ledger: seq,
                        hash: hex::encode(hash),
                    })?;
                Ok(Applied {
                    ledger: seq,
                    hash,
                    result,
                    meta,
                })
            })
            .collect()
    }

    /// Stops the node without writing the chain.
    pub fn stop(self) -> Result<(), ChainError> {
        self.node.stop()?;
        Ok(())
    }

    /// Writes the recorded chain into `target` and stops the node.
    pub fn finish(self, target: &Path) -> Result<ChainSummary, ChainError> {
        let summary = self.recorder.finish(&self.node, target)?;
        self.node.stop()?;
        Ok(summary)
    }
}
