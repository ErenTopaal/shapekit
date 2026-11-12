//! Owns CAP-85 executable reference entries and deploys or switches
//! contracts onto them.
#![no_std]

use soroban_sdk::{
    Address, BytesN, ContractExecutable, ContractExecutableRef, Env, String, contract, contractimpl,
};

#[contract]
pub struct ExecRef;

#[contractimpl]
impl ExecRef {
    /// Points the entry keyed by the executable tag `tag` at `wasm_hash`,
    /// creating it on first use. The host refuses a hash whose wasm is not
    /// uploaded.
    pub fn publish(env: Env, tag: String, wasm_hash: BytesN<32>) {
        env.executable_refs().set(&tag, &wasm_hash);
    }

    /// Deploys a contract whose executable is the reference `owner`/`tag`,
    /// with no constructor arguments, from this contract and `salt`.
    pub fn deploy(env: Env, owner: Address, tag: String, salt: BytesN<32>) -> Address {
        env.deployer().with_current_contract(salt).deploy_contract(
            ContractExecutable::ExternalRef(ContractExecutableRef { owner, tag }),
            (),
        )
    }

    /// Replaces this contract's own executable with the reference
    /// `owner`/`tag`, which emits the executable_update system event.
    pub fn adopt(env: Env, owner: Address, tag: String) {
        env.deployer()
            .update_current_contract(ContractExecutable::ExternalRef(ContractExecutableRef {
                owner,
                tag,
            }));
    }
}

#[cfg(test)]
mod test {
    extern crate std;

    use super::{ExecRef, ExecRefClient};
    use soroban_sdk::testutils::EnvTestConfig;
    use soroban_sdk::{BytesN, Env, IntoVal, String, Symbol, Val, vec};

    const ECHO_WASM: &[u8] = include_bytes!("../../wasm/echo.wasm");

    fn env() -> Env {
        Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        })
    }

    #[test]
    fn deploys_onto_a_reference() {
        let env = env();
        let owner = env.register(ExecRef, ());
        let client = ExecRefClient::new(&env, &owner);
        let hash = env.deployer().upload_contract_wasm(ECHO_WASM);
        let tag = String::from_str(&env, "echo");
        client.publish(&tag, &hash);
        let child = client.deploy(&owner, &tag, &BytesN::from_array(&env, &[7; 32]));
        let v: Val = 9u32.into_val(&env);
        let back: u32 = env.invoke_contract(&child, &Symbol::new(&env, "echo"), vec![&env, v]);
        assert_eq!(back, 9);
    }

    #[test]
    fn refuses_a_missing_wasm() {
        let env = env();
        let owner = env.register(ExecRef, ());
        let client = ExecRefClient::new(&env, &owner);
        let tag = String::from_str(&env, "none");
        assert!(
            client
                .try_publish(&tag, &BytesN::from_array(&env, &[1; 32]))
                .is_err()
        );
    }
}
