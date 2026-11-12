//! `soroban.invoke-basics`: the everyday Soroban operations at protocol 30
//! under testnet's settings: a restore of archived entries, wasm upload,
//! contract creation through `CreateContract` and through
//! `CreateContractV2` with a constructor, a contract deploying a contract
//! with constructor arguments, persistent and temporary writes, a TTL
//! extension, and two failed invocations.
//!
//! The restore needs an archived entry. Testnet's minimum persistent TTL
//! is 120960 ledgers, far past what a chain can close, so the node starts
//! with a 16 ledger minimum instead (`min_persistent_ttl`). That only
//! covers what is written before the settings upgrade: the settings writer
//! instance and the echo code the bootstrap uploads. They archive within a
//! few empty ledgers, and the recipe restores them and calls the restored
//! contract.
//!
//! | ledger   | content                                                      |
//! |----------|--------------------------------------------------------------|
//! | 2 to 6   | protocol upgrade, settings bootstrap and settings upgrade    |
//! | 7        | root funds deployer and caller                               |
//! | 8 to n   | empty ledgers until writer instance and echo code archive    |
//! | n+1      | restore of both                                              |
//! | n+2      | echo call on the restored writer                             |
//! | ...      | upload, create v1, create v2, factory deploy, put_temp,      |
//! |          | extend ttl, events discarded, trapped                        |

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use shapekit_core_driver::contracts::ECHO_WASM;
use shapekit_core_driver::keys::account_address;
use shapekit_core_driver::node::EntryState;
use shapekit_core_driver::tx::{contract_id_from_address, contract_preimage};
use stellar_xdr::{
    ContractExecutable, CreateContractArgs, Hash, HostFunction, InvokeHostFunctionResult,
    LedgerEntryChange, ScAddress, ScVal,
};

use super::soroban_steps::{
    FACTORY_WASM, account_val, bytes32, call, code_key, contract_events, create_v2,
    created_instances, ensure_invoke_result, extend_ttl, instance_key, restore, salt, simulate,
    sym, upload, vec_val, with_data,
};
use super::{ChainCtx, Recipe, Selector, Spec};
use shapekit_core_driver::sim::AuthMode;

pub const FAMILY: &str = "soroban.invoke-basics";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// Minimum persistent TTL at genesis, so the bootstrap's entries archive
/// within the chain.
const ARCHIVE_TTL: u32 = 16;

/// Empty ledgers the recipe waits at most for them to archive.
const ARCHIVE_WAIT: u32 = 2 * ARCHIVE_TTL;

/// Ledgers past the current one a TTL extension asks for, about 29 days
/// at testnet's close rate and below its maximum entry TTL of 3110400.
const EXTEND_TO: u32 = 500_000;

pub struct InvokeBasics {
    pub protocol: u32,
}

const INVOKE_FIELDS: [&str; 6] = [
    "tx.ext",
    "tx.soroban.resource_fee",
    "tx.soroban.footprint",
    "op.invoke_host_function.invoke_contract.contract_address",
    "op.invoke_host_function.invoke_contract.function_name",
    "op.invoke_host_function.invoke_contract.args",
];

fn restored_count(changes: &[LedgerEntryChange]) -> usize {
    changes
        .iter()
        .filter(|c| matches!(c, LedgerEntryChange::Restored(_)))
        .count()
}

impl Recipe for InvokeBasics {
    fn id(&self) -> &str {
        FAMILY
    }

    fn family(&self) -> &str {
        FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn soroban_settings(&self) -> bool {
        true
    }

    fn min_persistent_ttl(&self) -> Option<u32> {
        Some(ARCHIVE_TTL)
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["deployer", "caller"], BALANCE)?;
        let deployer = ctx.account("deployer");
        let caller = ctx.account("caller");
        let root = ctx.root().clone();

        // the bootstrap's writer instance and echo code, written under the
        // short genesis ttl
        let writer = ctx.driver().settings_writer(&root)?;
        let echo_hash = Hash(Sha256::digest(ECHO_WASM).into());
        let archived_keys = vec![instance_key(&writer), code_key(&echo_hash)];
        let mut waited = 0;
        loop {
            let got = ctx.driver().node().ledger_entries(&archived_keys)?;
            if got.entries.iter().all(|e| e.state == EntryState::Archived) {
                break;
            }
            ensure!(
                waited < ARCHIVE_WAIT,
                "writer instance and echo code are not archived after {waited} empty ledgers: {:?}",
                got.entries.iter().map(|e| e.state).collect::<Vec<_>>()
            );
            ctx.driver().close_empty()?;
            waited += 1;
        }

        let applied = restore(ctx, &deployer, &archived_keys)?;
        let restored = restored_count(&super::soroban_steps::operation_changes(&applied.meta));
        ensure!(
            restored >= 2,
            "restore meta shows {restored} restored changes, expected the instance and the code"
        );
        ctx.capture(
            "restore-archived-instance-and-code",
            Selector::transaction(&applied),
            Spec::new(
                "RestoreFootprint brings back an archived contract instance and its archived wasm code, the meta reports both as restored",
            )
            .fields(&["op.restore_footprint", "tx.soroban.footprint", "tx.ext"]),
        )?;

        let value = vec_val(vec![sym("restored")?, ScVal::U32(1)])?;
        let applied = ctx
            .driver()
            .invoke(&caller, &writer, "echo", vec![value.clone()])?;
        ensure!(
            applied.return_value() == Some(&value),
            "the restored echo returned {:?}",
            applied.return_value()
        );
        ctx.capture(
            "invoke-restored-contract",
            Selector::transaction(&applied),
            Spec::new(
                "an invocation of the just restored contract that writes a persistent entry and emits a contract event",
            )
            .fields(&INVOKE_FIELDS),
        )?;

        // upload: the code entry carries ContractCodeEntryExt v1 cost inputs
        let ledger_before = ctx.driver().last_closed();
        let factory_hash = upload(ctx, &deployer, FACTORY_WASM)?;
        let upload_ledger = ctx.driver().last_closed();
        ensure!(
            upload_ledger == ledger_before + 1,
            "upload closed no ledger"
        );
        ctx.capture(
            "upload-wasm",
            Selector::Ledger(upload_ledger),
            Spec::new(
                "UploadContractWasm stores the factory contract's code, a contract code entry with v1 cost inputs and its ttl entry",
            )
            .fields(&["op.invoke_host_function.upload_wasm", "tx.ext"]),
        )?;

        // create v1: no constructor arguments can be passed
        let deployer_addr = account_address(&deployer.verifying_key());
        let echo_salt = salt(FAMILY, "echo-v1");
        let hf = HostFunction::CreateContract(CreateContractArgs {
            contract_id_preimage: contract_preimage(&deployer_addr, &echo_salt),
            executable: ContractExecutable::Wasm(echo_hash.clone()),
        });
        let env = ctx.driver().soroban(&deployer, hf)?;
        let applied = ctx.submit_and_close(&env)?;
        let echo = contract_id_from_address(&ctx.network_id(), &deployer_addr, &echo_salt)?;
        ensure!(
            created_instances(&applied.meta)
                == [(
                    ScAddress::Contract(echo.clone()),
                    ContractExecutable::Wasm(echo_hash.clone())
                )],
            "create v1 did not create exactly the echo instance"
        );
        ctx.capture(
            "create-contract-v1",
            Selector::transaction(&applied),
            Spec::new(
                "CreateContract (v1) deploys an instance of uploaded wasm from the source account address and a salt, with no constructor",
            )
            .fields(&["op.invoke_host_function.create_contract.preimage", "tx.ext"]),
        )?;

        // create v2: the factory's constructor stores its owner
        let factory_salt = salt(FAMILY, "factory-v2");
        let hf = create_v2(
            &deployer_addr,
            &factory_salt,
            ContractExecutable::Wasm(factory_hash.clone()),
            vec![account_val(&deployer)],
        )?;
        let env = ctx.driver().soroban(&deployer, hf)?;
        let applied = ctx.submit_and_close(&env)?;
        let factory = contract_id_from_address(&ctx.network_id(), &deployer_addr, &factory_salt)?;
        ensure!(
            created_instances(&applied.meta).len() == 1,
            "create v2 did not create one instance"
        );
        ctx.capture(
            "create-contract-v2-constructor",
            Selector::transaction(&applied),
            Spec::new(
                "CreateContractV2 deploys the factory contract and passes its owner to the constructor, which writes instance storage",
            )
            .fields(&[
                "op.invoke_host_function.create_contract.preimage",
                "op.invoke_host_function.create_contract_v2.constructor_args",
                "tx.ext",
            ]),
        )?;

        // a contract deploys a contract and runs its constructor
        let child_salt = salt(FAMILY, "factory-child");
        let child_args = vec![
            bytes32(&factory_hash.0)?,
            bytes32(&child_salt)?,
            vec_val(vec![account_val(&caller)])?,
        ];
        let applied =
            ctx.driver()
                .invoke(&caller, &factory, "deploy_with_args", child_args.clone())?;
        let child = contract_id_from_address(
            &ctx.network_id(),
            &ScAddress::Contract(factory.clone()),
            &child_salt,
        )?;
        ensure!(
            applied.return_value() == Some(&ScVal::Address(ScAddress::Contract(child.clone()))),
            "deploy_with_args returned {:?}",
            applied.return_value()
        );
        ctx.capture(
            "contract-deploys-with-constructor",
            Selector::transaction(&applied),
            Spec::new(
                "an invocation in which the factory contract deploys another factory instance from its own address and runs its constructor",
            )
            .fields(&INVOKE_FIELDS),
        )?;

        let applied = ctx.driver().invoke(
            &caller,
            &echo,
            "put_temp",
            vec![sym("temp")?, ScVal::U64(u64::MAX)],
        )?;
        ctx.capture(
            "invoke-temporary-entry",
            Selector::transaction(&applied),
            Spec::new("an invocation that writes a temporary contract data entry")
                .fields(&INVOKE_FIELDS),
        )?;

        let keys = vec![instance_key(&echo), code_key(&echo_hash)];
        let applied = extend_ttl(ctx, &caller, &keys, EXTEND_TO)?;
        ctx.capture(
            "extend-ttl",
            Selector::transaction(&applied),
            Spec::new(
                "ExtendFootprintTtl extends a contract instance and its code to live 500000 more ledgers, the meta updates both ttl entries",
            )
            .fields(&[
                "op.extend_footprint_ttl.extend_to",
                "tx.soroban.footprint",
                "tx.ext",
            ]),
        )?;

        // the host runs the call, emits its event, and core then refuses the
        // result because the declared write bytes are too small, so the
        // event is discarded
        let value = sym("discarded")?;
        let hf = call(&echo, "echo", vec![value])?;
        let sim = simulate(
            ctx,
            &caller,
            &hf,
            AuthMode::Record {
                allow_non_root: false,
                address_v2: false,
            },
        )?;
        ensure!(
            sim.contract_events.len() == 1,
            "simulated echo emitted {} events",
            sim.contract_events.len()
        );
        let env = ctx
            .driver()
            .soroban_with(&caller, hf, sim.transaction_data, Vec::new())?;
        let env = with_data(ctx, &env, |d| d.resources.write_bytes = 1, &[&caller])?;
        let applied = ctx.submit_and_close_failing(&env)?;
        ensure_invoke_result(
            &applied,
            |r| matches!(r, InvokeHostFunctionResult::ResourceLimitExceeded),
            "invoke_host_function resource_limit_exceeded",
        )?;
        let events = contract_events(&applied.meta)?;
        ensure!(
            events.is_empty(),
            "the failed invocation kept {} events",
            events.len()
        );
        ctx.capture(
            "failed-events-discarded",
            Selector::transaction(&applied),
            Spec::new(
                "an echo call that emits an event in the host but declares one write byte, so core fails it with RESOURCE_LIMIT_EXCEEDED and the meta carries no contract events",
            )
            .fields(&INVOKE_FIELDS)
            .flags(&["failed-tx"]),
        )?;

        // the constructor of a factory requires an owner; deploying one
        // without arguments traps. resources come from the valid call
        let trap_salt = salt(FAMILY, "factory-trap");
        let good = call(
            &factory,
            "deploy_with_args",
            vec![
                bytes32(&factory_hash.0)?,
                bytes32(&trap_salt)?,
                vec_val(vec![account_val(&caller)])?,
            ],
        )?;
        let sim = simulate(
            ctx,
            &caller,
            &good,
            AuthMode::Record {
                allow_non_root: false,
                address_v2: false,
            },
        )?;
        let bad = call(
            &factory,
            "deploy_with_args",
            vec![
                bytes32(&factory_hash.0)?,
                bytes32(&trap_salt)?,
                vec_val(Vec::new())?,
            ],
        )?;
        let env = ctx
            .driver()
            .soroban_with(&caller, bad, sim.transaction_data, Vec::new())?;
        let applied = ctx.submit_and_close_failing(&env)?;
        ensure_invoke_result(
            &applied,
            |r| matches!(r, InvokeHostFunctionResult::Trapped),
            "invoke_host_function trapped",
        )?;
        ensure!(
            created_instances(&applied.meta).is_empty(),
            "the trapped deployment created an instance"
        );
        ctx.capture(
            "failed-trapped",
            Selector::transaction(&applied),
            Spec::new(
                "a factory call that deploys a contract whose constructor needs an argument it does not get, so the host traps and nothing is created",
            )
            .fields(&INVOKE_FIELDS)
            .flags(&["failed-tx"]),
        )
        .context("capturing the trapped call")
    }
}
