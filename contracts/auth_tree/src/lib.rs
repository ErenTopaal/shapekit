//! Builds authorization trees of chosen depth and fan-out.
//!
//! A call to `node` on contract A with `chain = [B, C]` requires the first
//! level's signer, then calls B `fanout` times, and B calls C the same way.
//! Soroban forbids re-entry, so each level of depth is a separate instance of
//! this contract.
#![no_std]

use soroban_sdk::{
    Address, Env, Vec, contract, contracterror, contractimpl, contracttype, panic_with_error,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Level {
    /// Address whose `require_auth` this level calls.
    pub signer: Address,
    /// How many times this level calls the next contract in the chain.
    pub fanout: u32,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// `levels` must have exactly one more element than `chain`.
    Shape = 1,
}

#[contract]
pub struct AuthTree;

#[contractimpl]
impl AuthTree {
    /// Requires `levels[0].signer`, then calls `chain[0]` with the rest of
    /// both lists `levels[0].fanout` times. Returns the number of `node`
    /// invocations in the whole subtree, this one included.
    pub fn node(env: Env, chain: Vec<Address>, levels: Vec<Level>) -> u32 {
        if levels.len() != chain.len() + 1 {
            panic_with_error!(&env, Error::Shape);
        }
        let level = levels.get_unchecked(0);
        level.signer.require_auth();
        let mut count = 1u32;
        if let Some(next) = chain.first() {
            let rest_chain = chain.slice(1..);
            let rest_levels = levels.slice(1..);
            let client = AuthTreeClient::new(&env, &next);
            for _ in 0..level.fanout {
                count += client.node(&rest_chain, &rest_levels);
            }
        }
        count
    }

    /// Calls `require_auth` on `signer` only when `enforce` is true. A
    /// transaction can declare an auth entry for this call and pass false,
    /// leaving the entry unmatched and therefore never checked.
    pub fn guarded(_env: Env, signer: Address, enforce: bool) -> bool {
        if enforce {
            signer.require_auth();
        }
        enforce
    }
}

#[cfg(test)]
mod test {
    extern crate std;

    use super::{AuthTree, AuthTreeClient, Error, Level};
    use soroban_sdk::testutils::{Address as _, AuthorizedFunction, EnvTestConfig};
    use soroban_sdk::{Address, Env, vec};

    fn env() -> Env {
        let env = Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        });
        // levels below the root call require_auth from a nested frame, which
        // plain mock_all_auths refuses as non-root authorization
        env.mock_all_auths_allowing_non_root_auth();
        env
    }

    #[test]
    fn counts_and_records_the_tree() {
        let env = env();
        let a = env.register(AuthTree, ());
        let b = env.register(AuthTree, ());
        let c = env.register(AuthTree, ());
        let (s0, s1, s2) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        let levels = vec![
            &env,
            Level {
                signer: s0.clone(),
                fanout: 2,
            },
            Level {
                signer: s1.clone(),
                fanout: 3,
            },
            Level {
                signer: s2.clone(),
                fanout: 0,
            },
        ];
        let chain = vec![&env, b.clone(), c.clone()];
        let n = AuthTreeClient::new(&env, &a).node(&chain, &levels);
        // 1 root, 2 children, 6 grandchildren
        assert_eq!(n, 9);
        let auths = env.auths();
        // distinct signers never share an entry: every require_auth below the
        // root starts its own non-root entry
        assert_eq!(auths.len(), 9);
        let per = |s: &Address| auths.iter().filter(|(addr, _)| addr == s).count();
        assert_eq!((per(&s0), per(&s1), per(&s2)), (1, 2, 6));
        let root = auths
            .iter()
            .find(|(addr, _)| *addr == s0)
            .expect("root signer");
        assert!(
            matches!(root.1.function, AuthorizedFunction::Contract((ref id, _, _)) if *id == a)
        );
    }

    #[test]
    fn one_signer_gives_one_nested_entry() {
        let env = env();
        let a = env.register(AuthTree, ());
        let b = env.register(AuthTree, ());
        let c = env.register(AuthTree, ());
        let s = Address::generate(&env);
        let levels = vec![
            &env,
            Level {
                signer: s.clone(),
                fanout: 1,
            },
            Level {
                signer: s.clone(),
                fanout: 2,
            },
            Level {
                signer: s.clone(),
                fanout: 0,
            },
        ];
        let chain = vec![&env, b, c];
        assert_eq!(AuthTreeClient::new(&env, &a).node(&chain, &levels), 4);
        let auths = env.auths();
        assert_eq!(auths.len(), 1);
        let root = &auths[0].1;
        assert_eq!(root.sub_invocations.len(), 1);
        assert_eq!(root.sub_invocations[0].sub_invocations.len(), 2);
    }

    #[test]
    fn wrong_shape_fails() {
        let env = env();
        let a = env.register(AuthTree, ());
        let s = Address::generate(&env);
        let levels = vec![
            &env,
            Level {
                signer: s,
                fanout: 1,
            },
        ];
        let chain = vec![&env, a.clone()];
        let r = AuthTreeClient::new(&env, &a).try_node(&chain, &levels);
        assert_eq!(r.err(), Some(Ok(soroban_sdk::Error::from(Error::Shape))));
    }

    #[test]
    fn guarded_only_requires_when_enforced() {
        let env = env();
        let a = env.register(AuthTree, ());
        let client = AuthTreeClient::new(&env, &a);
        let s = Address::generate(&env);
        assert!(!client.guarded(&s, &false));
        assert!(env.auths().is_empty());
        assert!(client.guarded(&s, &true));
        assert_eq!(env.auths().len(), 1);
    }
}
