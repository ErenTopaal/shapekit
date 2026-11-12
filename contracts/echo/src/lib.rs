//! Puts caller-chosen values into ledger entries, events and return values.
#![no_std]

use soroban_sdk::{Env, Symbol, Val, Vec, contract, contractimpl, symbol_short, vec};

#[cfg(not(feature = "sdk22"))]
use soroban_sdk::events::Event;

const LAST: Symbol = symbol_short!("last");

// a hand-written event so the caller decides the exact topics and data;
// the contractevent macro would wrap data in a map and fix the topic types.
// soroban-sdk 22 has neither, its `publish` makes the same host call
#[cfg(not(feature = "sdk22"))]
struct Raw {
    topics: Vec<Val>,
    data: Val,
}

#[cfg(not(feature = "sdk22"))]
impl Event for Raw {
    fn topics(&self, _env: &Env) -> Vec<Val> {
        self.topics.clone()
    }

    fn data(&self, _env: &Env) -> Val {
        self.data
    }
}

#[contract]
pub struct Echo;

#[contractimpl]
impl Echo {
    /// Stores `value` in persistent storage, emits it as the data of an
    /// `echo` event, and returns it unchanged.
    pub fn echo(env: Env, value: Val) -> Val {
        env.storage().persistent().set(&LAST, &value);
        let topics = vec![&env, LAST.to_val()];
        #[cfg(not(feature = "sdk22"))]
        env.events().publish_event(&Raw {
            topics,
            data: value,
        });
        #[cfg(feature = "sdk22")]
        env.events().publish(topics, value);
        value
    }

    /// Emits one event with exactly these topics and data. soroban-env-host
    /// 30.0.0 records topics without checking their types, so vec and map
    /// topics reach the ledger even though the sdk docs advise against them.
    pub fn emit(env: Env, topics: Vec<Val>, data: Val) {
        #[cfg(not(feature = "sdk22"))]
        env.events().publish_event(&Raw { topics, data });
        #[cfg(feature = "sdk22")]
        env.events().publish(topics, data);
    }

    /// Stores `value` under `key` in temporary storage, for entries that
    /// should expire quickly.
    pub fn put_temp(env: Env, key: Val, value: Val) {
        env.storage().temporary().set(&key, &value);
    }

    /// The value the last `echo` call stored, if any.
    pub fn last(env: Env) -> Option<Val> {
        env.storage().persistent().get(&LAST)
    }
}

#[cfg(test)]
mod test {
    extern crate std;

    use super::{Echo, EchoClient};
    use soroban_sdk::testutils::{EnvTestConfig, Events};
    use soroban_sdk::xdr::ScVal;
    use soroban_sdk::{
        Bytes, Env, IntoVal, Map, String, Symbol, TryFromVal, Val, symbol_short, vec,
    };

    // no json snapshots written next to the sources on drop
    fn env() -> Env {
        Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        })
    }

    #[test]
    fn echo_returns_stores_and_emits() {
        let env = env();
        let id = env.register(Echo, ());
        let client = EchoClient::new(&env, &id);
        let mut m: Map<Val, Val> = Map::new(&env);
        m.set(
            1u32.into_val(&env),
            String::from_str(&env, "1").into_val(&env),
        );
        m.set(
            Symbol::new(&env, "one").into_val(&env),
            (-5i128).into_val(&env),
        );
        let value: Val = vec![&env, m.to_val(), u64::MAX.into_val(&env)].into_val(&env);

        let want = ScVal::try_from_val(&env, &value).unwrap();
        let back = client.echo(&value);
        assert_eq!(ScVal::try_from_val(&env, &back).unwrap(), want);
        let events = env.events().all();
        let all = events.events();
        assert_eq!(all.len(), 1);
        let soroban_sdk::xdr::ContractEventBody::V0(body) = &all[0].body;
        assert_eq!(body.data, want);
        assert_eq!(body.topics.len(), 1);
        let stored = client.last().unwrap();
        assert_eq!(ScVal::try_from_val(&env, &stored).unwrap(), want);
    }

    #[test]
    fn emit_passes_topics_through() {
        let env = env();
        let id = env.register(Echo, ());
        let client = EchoClient::new(&env, &id);
        let topics = vec![
            &env,
            symbol_short!("a").to_val(),
            7u32.into_val(&env),
            Bytes::from_array(&env, &[1, 2, 3]).into_val(&env),
        ];
        client.emit(&topics, &true.into_val(&env));
        let events = env.events().all();
        let all = events.events();
        assert_eq!(all.len(), 1);
        let soroban_sdk::xdr::ContractEventBody::V0(body) = &all[0].body;
        assert_eq!(body.topics.len(), 3);
        assert_eq!(body.data, soroban_sdk::xdr::ScVal::Bool(true));
    }

    #[test]
    fn emit_accepts_a_vec_topic() {
        let env = env();
        let id = env.register(Echo, ());
        let client = EchoClient::new(&env, &id);
        let inner: Val = vec![&env, 1u32].into_val(&env);
        let topics = vec![&env, inner];
        client.emit(&topics, &().into_val(&env));
        let events = env.events().all();
        let soroban_sdk::xdr::ContractEventBody::V0(body) = &events.events()[0].body;
        assert!(matches!(
            body.topics[0],
            soroban_sdk::xdr::ScVal::Vec(Some(_))
        ));
    }

    #[test]
    fn temp_storage() {
        let env = env();
        let id = env.register(Echo, ());
        let client = EchoClient::new(&env, &id);
        client.put_temp(&1u32.into_val(&env), &2u32.into_val(&env));
        let got: Option<u32> = env.as_contract(&id, || env.storage().temporary().get(&1u32));
        assert_eq!(got, Some(2));
        assert!(client.last().is_none());
    }
}
