//! A CAP-71 custom account that authenticates only through the delegated
//! signers attached to its auth entry, each of which must be registered at
//! construction. A delegate may itself be an instance of this contract, so
//! delegation nests.
#![no_std]

use soroban_sdk::auth::{Context, CustomAccountInterface};
use soroban_sdk::crypto::Hash;
use soroban_sdk::{Address, Env, Vec, contract, contracterror, contractimpl, contracttype};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// The auth entry attached no delegated signers.
    NoDelegates = 1,
    /// A delegated signer is not registered with this account.
    UnknownDelegate = 2,
}

#[contracttype]
enum Key {
    Signer(Address),
}

#[contract]
pub struct DelegatingAccount;

#[contractimpl]
impl DelegatingAccount {
    pub fn __constructor(env: Env, signers: Vec<Address>) {
        for s in signers.iter() {
            env.storage().persistent().set(&Key::Signer(s), &());
        }
    }
}

#[contractimpl]
impl CustomAccountInterface for DelegatingAccount {
    type Signature = ();
    type Error = Error;

    fn __check_auth(
        env: Env,
        _payload: Hash<32>,
        _signature: (),
        _contexts: Vec<Context>,
    ) -> Result<(), Error> {
        let delegates = env.custom_account().get_delegated_signers();
        if delegates.is_empty() {
            return Err(Error::NoDelegates);
        }
        for d in delegates.iter() {
            if !env.storage().persistent().has(&Key::Signer(d.clone())) {
                return Err(Error::UnknownDelegate);
            }
        }
        for d in delegates.iter() {
            env.custom_account().delegate_auth(&d);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    extern crate std;

    use super::DelegatingAccount;
    use soroban_sdk::testutils::{Address as _, EnvTestConfig};
    use soroban_sdk::{Address, Env, vec};

    #[test]
    fn registers_its_signers() {
        let env = Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        });
        let s = Address::generate(&env);
        let id = env.register(DelegatingAccount, (vec![&env, s.clone()],));
        let has: bool = env.as_contract(&id, || {
            env.storage()
                .persistent()
                .has(&super::Key::Signer(s.clone()))
        });
        assert!(has);
    }
}
