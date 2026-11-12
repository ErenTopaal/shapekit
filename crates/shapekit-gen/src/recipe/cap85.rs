//! `cap85.executable-tag`: contracts whose executable is an external
//! reference (`CONTRACT_EXECUTABLE_EXTERNAL_REF`) and the `SCV_EXECUTABLE_TAG`
//! value, from transactions core applies at protocol 30.
//!
//! An owner contract (`contracts/exec_ref`) publishes a persistent entry
//! keyed by an executable tag that holds the hash of the uploaded echo wasm.
//! The host only lets a contract create such a key (`create_executable_tag`),
//! so the entry comes from an invocation. Then:
//!
//! - `CreateContractV2` with an external-ref executable deploys an instance
//!   that runs echo through the reference,
//! - that instance echoes an `SCV_EXECUTABLE_TAG` value supplied as a call
//!   argument, which lands in its storage, its event and its return value,
//! - the owner deploys an external-ref instance from inside a contract,
//! - a second owner instance switches its own executable to the reference,
//!   which emits the executable_update system event,
//! - a `CreateContractV2` naming a tag the owner never published traps.

use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use shapekit_core_driver::contracts::ECHO_WASM;
use shapekit_core_driver::keys::account_address;
use shapekit_core_driver::sim::AuthMode;
use shapekit_core_driver::tx::contract_id_from_address;
use stellar_xdr::{
    ContractDataDurability, ContractExecutable, ContractExecutableExternalRef, Hash, HostFunction,
    InvokeHostFunctionResult, LedgerKey, LedgerKeyContractData, ScAddress, ScString, ScVal,
    SorobanAuthorizedFunction,
};

use super::soroban_steps::{
    EXEC_REF_WASM, bytes32, contract_address, create_v2, created_instances, ensure_invoke_result,
    salt, simulate, string, upload,
};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "cap85.executable-tag";

/// Starting balance of every funded account, 10,000 XLM.
const BALANCE: i64 = 100_000_000_000;

/// The tag the owner publishes.
const TAG: &str = "echo";

/// A tag the owner never publishes.
const MISSING_TAG: &str = "never-published";

pub struct ExecutableTag {
    pub protocol: u32,
}

fn tag_string(tag: &str) -> Result<ScString> {
    Ok(ScString(tag.as_bytes().to_vec().try_into()?))
}

fn external_ref(owner: &ScAddress, tag: &str) -> Result<ContractExecutable> {
    Ok(ContractExecutable::ExternalRef(
        ContractExecutableExternalRef {
            executable_owner: owner.clone(),
            tag: tag_string(tag)?,
        },
    ))
}

/// The owner's persistent entry keyed by `tag`.
fn tag_key(owner: &ScAddress, tag: &str) -> Result<LedgerKey> {
    Ok(LedgerKey::ContractData(LedgerKeyContractData {
        contract: owner.clone(),
        key: ScVal::ExecutableTag(tag_string(tag)?),
        durability: ContractDataDurability::Persistent,
    }))
}

const RECORD: AuthMode = AuthMode::Record {
    allow_non_root: false,
    address_v2: false,
};

const INVOKE_FIELDS: [&str; 3] = [
    "op.invoke_host_function.invoke_contract.contract_address",
    "op.invoke_host_function.invoke_contract.function_name",
    "op.invoke_host_function.invoke_contract.args",
];

impl Recipe for ExecutableTag {
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

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        ctx.fund(&["source"], BALANCE)?;
        let source = ctx.account("source");
        let source_addr = account_address(&source.verifying_key());
        // the bootstrap uploaded echo, so its code is already live
        let echo_hash = Hash(Sha256::digest(ECHO_WASM).into());
        let owner_code = upload(ctx, &source, EXEC_REF_WASM)?;
        let owner = ctx
            .driver()
            .deploy(&source, &owner_code, &salt(FAMILY, "owner"))?;
        let owner_addr = contract_address(&owner);

        let applied = ctx.driver().invoke(
            &source,
            &owner,
            "publish",
            vec![string(TAG)?, bytes32(&echo_hash.0)?],
        )?;
        let entry = ctx
            .driver()
            .node()
            .ledger_entries(&[tag_key(&owner_addr, TAG)?])?;
        ensure!(
            entry.entries[0].entry.is_some(),
            "the owner has no entry under the executable tag after publish"
        );
        ctx.capture(
            "publish-executable-tag",
            Selector::ledger_of(&applied),
            Spec::new(
                "the owner contract publishes a persistent entry keyed by an executable tag that holds the echo wasm hash",
            )
            .fields(&INVOKE_FIELDS),
        )?;
        ctx.capture(
            "executable-tag-entry",
            Selector::LedgerEntry {
                ledger: applied.ledger,
                key: tag_key(&owner_addr, TAG)?,
            },
            Spec::new(
                "a contract data entry whose key is an SCV_EXECUTABLE_TAG value and whose value is a wasm hash",
            ),
        )?;

        let ref_salt = salt(FAMILY, "external-ref");
        let hf = create_v2(
            &source_addr,
            &ref_salt,
            external_ref(&owner_addr, TAG)?,
            Vec::new(),
        )?;
        let env = ctx.driver().soroban(&source, hf)?;
        let applied = ctx.submit_and_close(&env)?;
        let instance = contract_id_from_address(&ctx.network_id(), &source_addr, &ref_salt)?;
        ensure!(
            created_instances(&applied.meta)
                == [(
                    ScAddress::Contract(instance.clone()),
                    external_ref(&owner_addr, TAG)?
                )],
            "the deployment did not create one external-ref instance"
        );
        ctx.capture(
            "create-external-ref",
            Selector::transaction(&applied),
            Spec::new(
                "CreateContractV2 with an external-ref executable naming the owner contract and its tag; the instance entry stores the reference, not a wasm hash",
            )
            .fields(&["op.invoke_host_function.create_contract.preimage"]),
        )?;

        let tag_value = ScVal::ExecutableTag(tag_string(TAG)?);
        let applied = ctx
            .driver()
            .invoke(&source, &instance, "echo", vec![tag_value.clone()])?;
        ensure!(
            applied.return_value() == Some(&tag_value),
            "the external-ref instance returned {:?}",
            applied.return_value()
        );
        ctx.capture(
            "echo-executable-tag-value",
            Selector::transaction(&applied),
            Spec::new(
                "a call to the external-ref instance, which runs echo through the reference, with an SCV_EXECUTABLE_TAG argument that it stores, emits and returns",
            )
            .fields(&INVOKE_FIELDS),
        )?;

        let child_salt = salt(FAMILY, "owner-deploys");
        let applied = ctx.driver().invoke(
            &source,
            &owner,
            "deploy",
            vec![
                ScVal::Address(owner_addr.clone()),
                string(TAG)?,
                bytes32(&child_salt)?,
            ],
        )?;
        let child = contract_id_from_address(&ctx.network_id(), &owner_addr, &child_salt)?;
        ensure!(
            created_instances(&applied.meta)
                == [(ScAddress::Contract(child), external_ref(&owner_addr, TAG)?)],
            "the owner's deploy did not create one external-ref instance"
        );
        ctx.capture(
            "contract-deploys-external-ref",
            Selector::transaction(&applied),
            Spec::new(
                "the owner contract deploys an instance whose executable is its own executable reference",
            )
            .fields(&INVOKE_FIELDS),
        )?;

        let adopter = ctx
            .driver()
            .deploy(&source, &owner_code, &salt(FAMILY, "adopter"))?;
        let applied = ctx.driver().invoke(
            &source,
            &adopter,
            "adopt",
            vec![ScVal::Address(owner_addr.clone()), string(TAG)?],
        )?;
        ctx.capture(
            "update-to-external-ref",
            Selector::transaction(&applied),
            Spec::new(
                "a wasm contract replaces its own executable with the external reference; the instance entry changes and the executable_update system event names both executables",
            )
            .fields(&INVOKE_FIELDS),
        )?;

        // the valid deployment prices the failing one; the missing tag's
        // key is added so the host finds no entry rather than a key outside
        // the footprint
        let miss_salt = salt(FAMILY, "missing-tag");
        let good = create_v2(
            &source_addr,
            &miss_salt,
            external_ref(&owner_addr, TAG)?,
            Vec::new(),
        )?;
        let sim = simulate(ctx, &source, &good, RECORD)?;
        let bad = create_v2(
            &source_addr,
            &miss_salt,
            external_ref(&owner_addr, MISSING_TAG)?,
            Vec::new(),
        )?;
        let HostFunction::CreateContractV2(bad_args) = &bad else {
            anyhow::bail!("create_v2 built another host function");
        };
        let mut auth = sim.auth.clone();
        ensure!(
            auth.len() == 1,
            "the valid deployment recorded {} auth entries",
            auth.len()
        );
        auth[0].root_invocation.function =
            SorobanAuthorizedFunction::CreateContractV2HostFn(bad_args.clone());
        let mut data = sim.transaction_data;
        let mut ro = data.resources.footprint.read_only.to_vec();
        ro.push(tag_key(&owner_addr, MISSING_TAG)?);
        data.resources.footprint.read_only = ro.try_into()?;
        let env = ctx.driver().soroban_with(&source, bad, data, auth)?;
        let applied = ctx.submit_and_close_failing(&env)?;
        ensure_invoke_result(
            &applied,
            |r| matches!(r, InvokeHostFunctionResult::Trapped),
            "invoke_host_function trapped",
        )?;
        ensure!(
            created_instances(&applied.meta).is_empty(),
            "the deployment from a missing tag created an instance"
        );
        ctx.capture(
            "create-external-ref-missing-tag",
            Selector::ledger_of(&applied),
            Spec::new(
                "CreateContractV2 naming an executable tag the owner never published; the host finds no entry and traps, nothing is created",
            )
            .fields(&["op.invoke_host_function.create_contract.preimage"])
            .flags(&["failed-tx"]),
        )
    }
}
