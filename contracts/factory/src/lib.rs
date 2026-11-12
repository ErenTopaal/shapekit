//! Deploys contracts from inside a contract, through the create-contract
//! host functions, with and without constructor arguments.
#![no_std]

use soroban_sdk::{
    Address, BytesN, ContractExecutable, Env, Symbol, Val, Vec, contract, contractimpl,
    symbol_short,
};

const OWNER: Symbol = symbol_short!("owner");

#[contract]
pub struct Factory;

#[contractimpl]
impl Factory {
    /// The constructor gives this contract itself a constructor, so a
    /// factory deploying a factory exercises the with-constructor path.
    pub fn __constructor(env: Env, owner: Address) {
        env.storage().instance().set(&OWNER, &owner);
    }

    pub fn owner(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&OWNER)
            .expect("constructor always sets the owner")
    }

    /// Deploys `wasm_hash` with no constructor arguments. The address is
    /// derived from this contract and `salt`.
    pub fn deploy(env: Env, wasm_hash: BytesN<32>, salt: BytesN<32>) -> Address {
        env.deployer()
            .with_current_contract(salt)
            .deploy_contract(ContractExecutable::Wasm(wasm_hash), ())
    }

    /// Deploys `wasm_hash` and passes `args` to its constructor.
    pub fn deploy_with_args(
        env: Env,
        wasm_hash: BytesN<32>,
        salt: BytesN<32>,
        args: Vec<Val>,
    ) -> Address {
        env.deployer()
            .with_current_contract(salt)
            .deploy_contract(ContractExecutable::Wasm(wasm_hash), args)
    }
}

#[cfg(test)]
mod test {
    extern crate std;

    use super::{Factory, FactoryClient};
    use soroban_sdk::testutils::{Address as _, EnvTestConfig};
    use soroban_sdk::{Address, BytesN, Env, IntoVal, Symbol, Val, vec};

    // the committed builds; the factory test deploys real wasm, not a native
    // registration, so the create-contract host path runs
    const ECHO_WASM: &[u8] = include_bytes!("../../wasm/echo.wasm");
    const FACTORY_WASM: &[u8] = include_bytes!("../../wasm/factory.wasm");

    fn env() -> Env {
        Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        })
    }

    #[test]
    fn deploys_without_constructor() {
        let env = env();
        let owner = Address::generate(&env);
        let f = env.register(Factory, (owner.clone(),));
        let client = FactoryClient::new(&env, &f);
        assert_eq!(client.owner(), owner);
        let hash = env.deployer().upload_contract_wasm(ECHO_WASM);
        let salt = BytesN::from_array(&env, &[1; 32]);
        let echo = client.deploy(&hash, &salt);
        let v: Val = 5u32.into_val(&env);
        let back: u32 = env.invoke_contract(&echo, &Symbol::new(&env, "echo"), vec![&env, v]);
        assert_eq!(back, 5);
        // same salt from the same deployer collides
        assert!(client.try_deploy(&hash, &salt).is_err());
    }

    #[test]
    fn deploys_with_constructor() {
        let env = env();
        let owner = Address::generate(&env);
        let inner_owner = Address::generate(&env);
        let f = env.register(Factory, (owner,));
        let client = FactoryClient::new(&env, &f);
        let hash = env.deployer().upload_contract_wasm(FACTORY_WASM);
        let salt = BytesN::from_array(&env, &[2; 32]);
        let args = vec![&env, inner_owner.clone().into_val(&env)];
        let child = client.deploy_with_args(&hash, &salt, &args);
        assert_eq!(FactoryClient::new(&env, &child).owner(), inner_owner);
        // a constructor that takes an argument cannot be called without it
        let other = BytesN::from_array(&env, &[3; 32]);
        assert!(client.try_deploy(&hash, &other).is_err());
    }
}
