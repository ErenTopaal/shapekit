//! Known answers for the Soroban, settings and unified-events families: the oracle's facts
//! for the committed chains equal what the recipe in `crates/shapekit-gen/src/recipe/` did.
//! Contract ids, wasm hashes, ttl values, upgrade set hashes and event payloads are rebuilt
//! here from the recipe's roles, salts and constants, never read from an oracle file, and
//! every test runs both network variants.

mod common;

use common::*;
use sha2::{Digest, Sha256};
use shapekit_core_driver::chain::SETTINGS_WRITER_SALT_DOMAIN;
use shapekit_core_driver::contracts::ECHO_WASM;
use shapekit_core_driver::keys::{
    account_address, contract_strkey, derive_key, network_id, root_key,
};
use shapekit_core_driver::settings::{NetworkSettings, PreparedUpgrade, is_upgradeable};
use shapekit_core_driver::tx::contract_id_from_address;
use shapekit_oracle::*;
use stellar_xdr::{
    AlphaNum4, Asset, AssetCode4, ConfigSettingEntry, ConfigSettingId, ConfigUpgradeSet,
    ContractDataDurability, ContractEvent, ContractEventBody, ContractEventType, ContractEventV0,
    ContractId, ContractIdPreimage, Duration, EncodedLedgerKey, ExtensionPoint,
    FrozenLedgerKeysDelta, Hash, HashIdPreimage, HashIdPreimageContractId, Int128Parts,
    Int256Parts, LedgerKey, LedgerKeyContractCode, LedgerKeyContractData, LedgerKeyTrustLine,
    Limits, LiquidityPoolConstantProductParameters, LiquidityPoolParameters, ReadXdr, ScAddress,
    ScBytes, ScMap, ScMapEntry, ScString, ScSymbol, ScVal, ScVec, TimePoint, TrustLineAsset,
    UInt128Parts, UInt256Parts, WriteXdr,
};

const FACTORY_WASM: &[u8] = include_bytes!("../../../contracts/wasm/factory.wasm");
const EXEC_REF_WASM: &[u8] = include_bytes!("../../../contracts/wasm/exec_ref.wasm");

fn net(n: &str) -> [u8; 32] {
    network_id(passphrase(n))
}

fn addr_of(recipe: &str, role: &str) -> ScAddress {
    account_address(&derive_key(recipe, role).verifying_key())
}

fn contract_from(n: &str, deployer: &ScAddress, salt: &[u8; 32]) -> ContractId {
    contract_id_from_address(&net(n), deployer, salt).unwrap()
}

/// `soroban_steps::salt`.
fn soroban_salt(recipe: &str, use_: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"shapekit/soroban-salt/v1");
    h.update(recipe.as_bytes());
    h.update([0]);
    h.update(use_.as_bytes());
    h.finalize().into()
}

/// `cap63::salt`, shared by cap63 and cap66.
fn recipe_salt(recipe: &str, name: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"shapekit/recipe-salt/v1");
    h.update(recipe.as_bytes());
    h.update([0]);
    h.update(name.as_bytes());
    h.finalize().into()
}

/// The echo instance `scval::deploy_echo` deploys for `use_` from `role`.
fn echo_instance(n: &str, recipe: &str, role: &str, use_: &str) -> ContractId {
    let salt: [u8; 32] = Sha256::digest(format!("shapekit/echo-instance/{use_}")).into();
    contract_from(n, &addr_of(recipe, role), &salt)
}

/// The settings writer the Soroban bootstrap deploys from root.
fn writer(n: &str) -> ContractId {
    let salt: [u8; 32] = Sha256::digest(SETTINGS_WRITER_SALT_DOMAIN).into();
    let root = account_address(&root_key(passphrase(n)).verifying_key());
    contract_from(n, &root, &salt)
}

fn sac(n: &str, asset: &Asset) -> String {
    let preimage = HashIdPreimage::ContractId(HashIdPreimageContractId {
        network_id: Hash(net(n)),
        contract_id_preimage: ContractIdPreimage::Asset(asset.clone()),
    });
    let id: [u8; 32] = Sha256::digest(preimage.to_xdr(Limits::none()).unwrap()).into();
    format!("{}", stellar_strkey::Contract(id))
}

fn wasm_hash(wasm: &[u8]) -> [u8; 32] {
    Sha256::digest(wasm).into()
}

fn b64<T: WriteXdr>(v: &T) -> String {
    v.to_xdr_base64(Limits::none()).unwrap()
}

fn sym(s: &str) -> ScVal {
    ScVal::Symbol(ScSymbol(s.try_into().unwrap()))
}

fn vec_of(items: Vec<ScVal>) -> ScVal {
    ScVal::Vec(Some(ScVec(items.try_into().unwrap())))
}

fn instance_key(id: &ContractId) -> LedgerKey {
    data_key(
        id,
        ScVal::LedgerKeyContractInstance,
        ContractDataDurability::Persistent,
    )
}

fn data_key(id: &ContractId, key: ScVal, durability: ContractDataDurability) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(id.clone()),
        key,
        durability,
    })
}

fn code_key(hash: [u8; 32]) -> LedgerKey {
    LedgerKey::ContractCode(LedgerKeyContractCode { hash: Hash(hash) })
}

fn ttl_hash(key: &LedgerKey) -> String {
    hex::encode(Sha256::digest(key.to_xdr(Limits::none()).unwrap()))
}

fn testnet_archival() -> stellar_xdr::StateArchivalSettings {
    match NetworkSettings::testnet()
        .unwrap()
        .get(ConfigSettingId::StateArchival)
    {
        Some(ConfigSettingEntry::StateArchival(s)) => s.clone(),
        other => panic!("testnet state archival setting: {other:?}"),
    }
}

/// The live-until ledger core gives an entry written in `ledger` under minimum ttl `min`.
fn live_until(ledger: u32, min: u32) -> u32 {
    ledger + min - 1
}

fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The upgrade json the oracle writes for a config upgrade set stored by the writer.
fn config_upgrade_json(n: &str, set: ConfigUpgradeSet) -> serde_json::Value {
    let w = writer(n);
    let key = PreparedUpgrade::new(set, w.clone()).unwrap().key;
    serde_json::json!({"config": {
        "content_hash": hex::encode(key.content_hash.0),
        "contract_id": contract_strkey(&w),
    }})
}

fn upgrade_settings(u: &UpgradeFacts) -> Vec<(Action, String)> {
    u.changes
        .iter()
        .map(|c| {
            assert_eq!(c.scope, ChangeScope::Upgrade);
            match &c.change {
                Change::ConfigSetting { action, setting } => (*action, setting.clone()),
                other => panic!("upgrade change {other:?}"),
            }
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct Data {
    contract: String,
    action: Action,
    durability: String,
    key_xdr: String,
}

fn contract_data(tx: &TxFacts) -> Vec<Data> {
    op_changes(tx)
        .into_iter()
        .filter_map(|c| match c {
            Change::ContractData {
                contract,
                action,
                durability,
                key_xdr,
            } => Some(Data {
                contract: contract.clone(),
                action: *action,
                durability: durability.clone(),
                key_xdr: key_xdr.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn data(contract: &ContractId, action: Action, durability: &str, key: &ScVal) -> Data {
    Data {
        contract: contract_strkey(contract),
        action,
        durability: durability.into(),
        key_xdr: b64(key),
    }
}

/// `(key hash, action, before, after)` of every ttl change at operation scope, sorted.
fn ttls(tx: &TxFacts) -> Vec<(String, Action, Option<u32>, Option<u32>)> {
    let mut out: Vec<_> = op_changes(tx)
        .into_iter()
        .filter_map(|c| match c {
            Change::Ttl {
                action,
                key_hash,
                before,
                after,
                ..
            } => Some((key_hash.clone(), *action, *before, *after)),
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn deployed(tx: &TxFacts) -> Vec<(String, String)> {
    tx.changes
        .iter()
        .filter_map(|c| match &c.change {
            Change::ContractDeployed {
                contract,
                executable,
            } => Some((contract.clone(), executable.clone())),
            _ => None,
        })
        .collect()
}

fn code_changes(tx: &TxFacts) -> Vec<(Action, String)> {
    op_changes(tx)
        .into_iter()
        .filter_map(|c| match c {
            Change::ContractCode { action, hash } => Some((*action, hash.clone())),
            _ => None,
        })
        .collect()
}

/// `(stage, contract, topics, data)` of every contract event that is not a token event.
fn events(tx: &TxFacts) -> Vec<(String, Option<String>, Vec<String>, String)> {
    tx.changes
        .iter()
        .filter_map(|c| match &c.change {
            Change::ContractEvent {
                stage,
                contract,
                topics_xdr,
                data_xdr,
            } => Some((
                stage.clone(),
                contract.clone(),
                topics_xdr.clone(),
                data_xdr.clone(),
            )),
            _ => None,
        })
        .collect()
}

/// The single event echo's `echo(value)` emits: topic `last`, the value as data.
fn echo_event(
    contract: &ContractId,
    value: &ScVal,
) -> (String, Option<String>, Vec<String>, String) {
    (
        "operation".into(),
        Some(contract_strkey(contract)),
        vec![b64(&sym("last"))],
        b64(value),
    )
}

/// A failed Soroban call: no effect beyond the fee and the sequence number, and none of the
/// events the host may have emitted before failing.
fn assert_fee_only(tx: &TxFacts, what: &str) {
    assert!(!tx.successful, "{what}");
    assert_eq!(tx.result, "tx_failed", "{what}");
    assert!(
        effect_kinds(tx).is_empty(),
        "{what}: {:?}",
        effect_kinds(tx)
    );
    assert!(events(tx).is_empty(), "{what}");
    assert!(deployed(tx).is_empty(), "{what}");
}

/// soroban.invoke-basics: restore of the archived writer instance and echo code, a call of
/// the restored writer, an upload, create v1 and v2, a factory deploy, a temporary write, a
/// ttl extension to 500000 ledgers and two failed calls.
#[test]
fn invoke_basics_known_answers() {
    let fam = "soroban.invoke-basics";
    let min_p = testnet_archival().min_persistent_ttl;
    let min_t = testnet_archival().min_temporary_ttl;
    let echo_hash = wasm_hash(ECHO_WASM);
    let factory_hash = wasm_hash(FACTORY_WASM);
    let deployer = account(fam, "deployer");
    let caller = account(fam, "caller");
    for n in NETWORKS {
        let id = |slug: &str| format!("{fam}.p30-{slug}");
        let w = writer(n);
        let dep = addr_of(fam, "deployer");
        let echo = contract_from(n, &dep, &soroban_salt(fam, "echo-v1"));
        let factory = contract_from(n, &dep, &soroban_salt(fam, "factory-v2"));
        let child = contract_from(
            n,
            &ScAddress::Contract(factory.clone()),
            &soroban_salt(fam, "factory-child"),
        );

        let slug = id("restore-archived-instance-and-code-meta");
        let tx = entry_tx(fam, &slug, n);
        let seq = entry_ledger(fam, &slug, n).ledger_seq;
        assert!(tx.successful);
        assert_eq!(tx.fee.source, deployer);
        assert_eq!(
            code_changes(&tx),
            vec![(Action::Restored, hex::encode(echo_hash))]
        );
        assert_eq!(
            contract_data(&tx),
            vec![data(
                &w,
                Action::Restored,
                "persistent",
                &ScVal::LedgerKeyContractInstance
            )],
            "{n}"
        );
        let restored_until = live_until(seq, min_p);
        let mut want = vec![
            (
                ttl_hash(&code_key(echo_hash)),
                Action::Restored,
                Some(restored_until),
                Some(restored_until),
            ),
            (
                ttl_hash(&instance_key(&w)),
                Action::Restored,
                Some(restored_until),
                Some(restored_until),
            ),
        ];
        want.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(ttls(&tx), want, "{n}");

        let slug = id("invoke-restored-contract-meta");
        let tx = entry_tx(fam, &slug, n);
        let seq = entry_ledger(fam, &slug, n).ledger_seq;
        let value = vec_of(vec![sym("restored"), ScVal::U32(1)]);
        assert_eq!(tx.fee.source, caller);
        assert_eq!(
            contract_data(&tx),
            vec![data(&w, Action::Created, "persistent", &sym("last"))]
        );
        assert_eq!(events(&tx), vec![echo_event(&w, &value)], "{n}");
        let last = data_key(&w, sym("last"), ContractDataDurability::Persistent);
        assert_eq!(
            ttls(&tx),
            vec![(
                ttl_hash(&last),
                Action::Created,
                None,
                Some(live_until(seq, min_p))
            )]
        );

        let slug = id("upload-wasm");
        let tx = only_tx(fam, &slug, n);
        let seq = entry_ledger(fam, &slug, n).ledger_seq;
        assert!(tx.successful);
        assert_eq!(
            code_changes(&tx),
            vec![(Action::Created, hex::encode(factory_hash))]
        );
        assert_eq!(
            ttls(&tx),
            vec![(
                ttl_hash(&code_key(factory_hash)),
                Action::Created,
                None,
                Some(live_until(seq, min_p))
            )]
        );
        assert!(deployed(&tx).is_empty());

        let slug = id("create-contract-v1-meta");
        let tx = entry_tx(fam, &slug, n);
        let echo_created = entry_ledger(fam, &slug, n).ledger_seq;
        assert_eq!(
            deployed(&tx),
            vec![(
                contract_strkey(&echo),
                format!("wasm:{}", hex::encode(echo_hash))
            )],
            "{n}"
        );
        assert_eq!(
            contract_data(&tx),
            vec![data(
                &echo,
                Action::Created,
                "persistent",
                &ScVal::LedgerKeyContractInstance
            )]
        );

        let tx = entry_tx(fam, &id("create-contract-v2-constructor-meta"), n);
        assert_eq!(
            deployed(&tx),
            vec![(
                contract_strkey(&factory),
                format!("wasm:{}", hex::encode(factory_hash))
            )],
            "{n}"
        );
        // the constructor's owner lives in instance storage, so the instance is the only entry
        assert_eq!(
            contract_data(&tx),
            vec![data(
                &factory,
                Action::Created,
                "persistent",
                &ScVal::LedgerKeyContractInstance
            )]
        );

        let tx = entry_tx(fam, &id("contract-deploys-with-constructor-meta"), n);
        assert_eq!(tx.fee.source, caller);
        assert_eq!(
            deployed(&tx),
            vec![(
                contract_strkey(&child),
                format!("wasm:{}", hex::encode(factory_hash))
            )],
            "{n}"
        );

        let slug = id("invoke-temporary-entry-meta");
        let tx = entry_tx(fam, &slug, n);
        let seq = entry_ledger(fam, &slug, n).ledger_seq;
        assert_eq!(
            contract_data(&tx),
            vec![data(&echo, Action::Created, "temporary", &sym("temp"))]
        );
        let temp = data_key(&echo, sym("temp"), ContractDataDurability::Temporary);
        assert_eq!(
            ttls(&tx),
            vec![(
                ttl_hash(&temp),
                Action::Created,
                None,
                Some(live_until(seq, min_t))
            )]
        );
        // put_temp emits nothing
        assert!(events(&tx).is_empty());

        let slug = id("extend-ttl-meta");
        let tx = entry_tx(fam, &slug, n);
        let seq = entry_ledger(fam, &slug, n).ledger_seq;
        let mut want = vec![
            (
                ttl_hash(&instance_key(&echo)),
                Action::Updated,
                Some(live_until(echo_created, min_p)),
                Some(seq + 500_000),
            ),
            (
                ttl_hash(&code_key(echo_hash)),
                Action::Updated,
                Some(restored_until),
                Some(seq + 500_000),
            ),
        ];
        want.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(ttls(&tx), want, "{n}");
        assert!(contract_data(&tx).is_empty());

        // the host emitted the echo event, core then failed the call on its write bytes
        assert_fee_only(
            &entry_tx(fam, &id("failed-events-discarded-meta"), n),
            "discarded",
        );
        assert_fee_only(&entry_tx(fam, &id("failed-trapped-meta"), n), "trapped");
    }
}

/// cap46.config-upgrades: the bootstrap's testnet settings upgrade set, then a
/// `maxsorobantxsetsize` upgrade to 1000 that core applies to the execution lanes.
#[test]
fn cap46_config_upgrades_name_the_settings() {
    let fam = "cap46.config-upgrades";
    let testnet = NetworkSettings::testnet().unwrap();
    let upgradeable: Vec<String> = testnet
        .entries
        .iter()
        .map(|e| e.discriminant())
        .filter(|id| is_upgradeable(*id))
        .map(|id| snake(id.name()))
        .collect();
    for n in NETWORKS {
        let slug = format!("{fam}.p30-testnet-settings-upgrade");
        let l = entry_ledger(fam, &slug, n);
        assert!(l.transactions.is_empty());
        assert_eq!(l.upgrades.len(), 1);
        let up = &l.upgrades[0];
        assert_eq!(
            up.upgrade,
            config_upgrade_json(n, testnet.upgrade_set().unwrap()),
            "{n}"
        );
        let wrote = upgrade_settings(up);
        assert!(
            wrote.iter().all(|(a, _)| *a == Action::Updated),
            "{wrote:?}"
        );
        let names: Vec<&str> = wrote.iter().map(|(_, s)| s.as_str()).collect();
        for s in &upgradeable {
            assert!(names.contains(&s.as_str()), "{n}: {s} not written");
        }
        // core resizes the state size window itself when the archival setting changes it
        for s in &names {
            assert!(
                upgradeable.iter().any(|u| u == s) || *s == "live_soroban_state_size_window",
                "{n}: {s}"
            );
        }
        for e in ["cost-params-cpu", "cost-params-memory"] {
            let entry = entry(fam, &format!("{fam}.p30-{e}"));
            assert_eq!(entry.source.ledger_seq, Some(l.ledger_seq), "{e}");
        }

        let l = entry_ledger(
            fam,
            &format!("{fam}.p30-max-soroban-tx-set-size-upgrade"),
            n,
        );
        assert_eq!(l.upgrades.len(), 1);
        assert_eq!(
            l.upgrades[0].upgrade,
            serde_json::json!({"max_soroban_tx_set_size": 1000})
        );
        assert_eq!(
            upgrade_settings(&l.upgrades[0]),
            vec![(Action::Updated, "contract_execution_lanes".to_string())]
        );

        // the classic header upgrades the recipe arms together in the last ledger
        let l = entry_ledger(fam, &format!("{fam}.p30-header-upgrades"), n);
        assert!(l.transactions.is_empty(), "{n}");
        let got: Vec<&serde_json::Value> = l.upgrades.iter().map(|u| &u.upgrade).collect();
        for want in [
            serde_json::json!({"base_fee": 200}),
            serde_json::json!({"base_reserve": 50_000_000}),
            serde_json::json!({"max_tx_set_size": 1000}),
            serde_json::json!({"flags": 1}),
        ] {
            assert!(got.contains(&&want), "{n}: {want} not in {got:?}");
        }
        assert_eq!(got.len(), 4, "{n}");
    }
}

/// cap70.scp-timing: a config upgrade set holding only the scp timing setting with the
/// close time target lowered to 4000 ms, the rest kept at testnet's values.
#[test]
fn cap70_scp_timing_upgrade() {
    let fam = "cap70.scp-timing";
    let Some(ConfigSettingEntry::ScpTiming(before)) = NetworkSettings::testnet()
        .unwrap()
        .get(ConfigSettingId::ScpTiming)
        .cloned()
    else {
        panic!("testnet has no scp timing setting");
    };
    let mut want = before.clone();
    want.ledger_target_close_time_milliseconds = 4000;
    let set = ConfigUpgradeSet {
        updated_entry: vec![ConfigSettingEntry::ScpTiming(want)]
            .try_into()
            .unwrap(),
    };
    for n in NETWORKS {
        let l = entry_ledger(fam, &format!("{fam}.p30-scp-timing-upgrade"), n);
        assert_eq!(l.upgrades.len(), 1);
        assert_eq!(
            l.upgrades[0].upgrade,
            config_upgrade_json(n, set.clone()),
            "{n}"
        );
        assert_eq!(
            upgrade_settings(&l.upgrades[0]),
            vec![(Action::Updated, "scp_timing".to_string())]
        );
        let e = entry(fam, &format!("{fam}.p30-scp-timing-entry"));
        assert_eq!(e.source.ledger_seq, Some(l.ledger_seq));
    }
}

/// cap63.parallel-txset: s1 to s4 call `echo(i + 1)` on instances a, b, c and a in one
/// ledger, s5 calls `last` on instance d, which nothing ever stored into.
#[test]
fn cap63_every_parallel_call_has_its_effect() {
    let fam = "cap63.parallel-txset";
    let s1 = addr_of(fam, "s1");
    for n in NETWORKS {
        let inst: Vec<ContractId> = ["a", "b", "c", "d"]
            .iter()
            .map(|name| contract_from(n, &s1, &recipe_salt(fam, name)))
            .collect();
        let l = entry_ledger(fam, &format!("{fam}.p30-five-calls-parallel-phase"), n);
        assert_eq!(l.transactions.len(), 5, "{n}");
        assert!(l.transactions.iter().all(|t| t.successful));
        let by = |role: &str| {
            let a = account(fam, role);
            l.transactions
                .iter()
                .find(|t| t.fee.source == a)
                .unwrap_or_else(|| panic!("{n}: {role}'s call"))
        };
        let plan = [
            ("s1", 0usize, 1u32),
            ("s2", 1, 2),
            ("s3", 2, 3),
            ("s4", 0, 4),
        ];
        let mut on_a = Vec::new();
        for (role, i, v) in plan {
            let tx = by(role);
            assert_eq!(
                events(tx),
                vec![echo_event(&inst[i], &ScVal::U32(v))],
                "{n} {role}"
            );
            let d = contract_data(tx);
            assert_eq!(d.len(), 1, "{n} {role}: {d:?}");
            assert_eq!(d[0].contract, contract_strkey(&inst[i]));
            assert_eq!(d[0].key_xdr, b64(&sym("last")));
            if i == 0 {
                on_a.push(d[0].action);
            } else {
                assert_eq!(d[0].action, Action::Created, "{n} {role}");
            }
        }
        // s1 and s4 share a cluster, so whichever ran first created the value
        on_a.sort_by_key(|a| *a != Action::Created);
        assert_eq!(on_a, vec![Action::Created, Action::Updated], "{n}");
        let read = by("s5");
        assert!(op_changes(read).is_empty(), "{n}: {:?}", op_changes(read));
        assert!(events(read).is_empty());
    }
}

/// cap66.archival: three echo instances store U32 1, 2 and 3 under a 10 ledger minimum
/// persistent ttl, evict also stores a temporary `scratch`; bob restores the restore
/// instance's value, carol's `echo(4)` on auto restores it on the way, and the scan evicts.
#[test]
fn cap66_restores_and_evictions() {
    let fam = "cap66.archival";
    let alice = addr_of(fam, "alice");
    let min_p = 10;
    for n in NETWORKS {
        let restore_id = contract_from(n, &alice, &recipe_salt(fam, "restore"));
        let auto_id = contract_from(n, &alice, &recipe_salt(fam, "auto"));
        let evict_id = contract_from(n, &alice, &recipe_salt(fam, "evict"));
        let last = |id: &ContractId| data_key(id, sym("last"), ContractDataDurability::Persistent);
        let temp = data_key(&evict_id, sym("scratch"), ContractDataDurability::Temporary);
        let describe = |k: &LedgerKey| match k {
            LedgerKey::ContractData(d) => format!(
                "contract data of {} key {}",
                match &d.contract {
                    ScAddress::Contract(c) => contract_strkey(c),
                    other => panic!("{other:?}"),
                },
                hex::encode(d.key.to_xdr(Limits::none()).unwrap())
            ),
            other => panic!("{other:?}"),
        };
        let ttl_of = |k: &LedgerKey| format!("ttl {}", ttl_hash(k));

        let slug = format!("{fam}.p30-restore-footprint");
        let l = entry_ledger(fam, &slug, n);
        let tx = only_tx(fam, &slug, n);
        assert!(tx.successful);
        assert_eq!(tx.fee.source, account(fam, "bob"));
        assert_eq!(
            contract_data(&tx),
            vec![data(
                &restore_id,
                Action::Restored,
                "persistent",
                &sym("last")
            )],
            "{n}"
        );
        let until = live_until(l.ledger_seq, min_p);
        assert_eq!(
            ttls(&tx),
            vec![(
                ttl_hash(&last(&restore_id)),
                Action::Restored,
                Some(until),
                Some(until)
            )]
        );

        let slug = format!("{fam}.p30-auto-restore-invoke-meta");
        let tx = entry_tx(fam, &slug, n);
        let seq = entry_ledger(fam, &slug, n).ledger_seq;
        assert_eq!(tx.fee.source, account(fam, "carol"));
        assert_eq!(
            contract_data(&tx),
            vec![
                data(&auto_id, Action::Restored, "persistent", &sym("last")),
                data(&auto_id, Action::Updated, "persistent", &sym("last")),
            ],
            "{n}"
        );
        let until = live_until(seq, min_p);
        assert_eq!(
            ttls(&tx),
            vec![(
                ttl_hash(&last(&auto_id)),
                Action::Restored,
                Some(until),
                Some(until)
            )]
        );
        assert_eq!(events(&tx), vec![echo_event(&auto_id, &ScVal::U32(4))]);

        let l = entry_ledger(fam, &format!("{fam}.p30-evicted-persistent"), n);
        assert!(!l.evicted.contains(&describe(&temp)), "{n}");
        let ours: Vec<LedgerKey> = [&restore_id, &auto_id, &evict_id].map(last).to_vec();
        let hits: Vec<&LedgerKey> = ours
            .iter()
            .filter(|k| l.evicted.contains(&describe(k)))
            .collect();
        assert!(!hits.is_empty(), "{n}: {:?}", l.evicted);
        for k in hits {
            assert!(
                l.evicted.contains(&ttl_of(k)),
                "{n}: the ttl goes with the entry"
            );
        }

        let l = entry_ledger(fam, &format!("{fam}.p30-evicted-temporary"), n);
        assert!(l.evicted.contains(&describe(&temp)), "{n}: {:?}", l.evicted);
        assert!(l.evicted.contains(&ttl_of(&temp)));

        // the recipe waits until the scan has evicted all four values
        let c = chain("cap66.archival.p30", n);
        for k in ours.iter().chain([&temp]) {
            assert!(
                c.ledgers
                    .values()
                    .any(|l| l.evicted.contains(&describe(k)) && l.evicted.contains(&ttl_of(k))),
                "{n}: {} never evicted",
                describe(k)
            );
        }
    }
}

fn unif(recipe: &str) -> Asset {
    let issuer = derive_key(recipe, "issuer").verifying_key();
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*b"UNIF"),
        issuer: shapekit_core_driver::keys::account_id(&issuer),
    })
}

#[derive(Debug, PartialEq, Eq)]
struct Ev {
    event: String,
    asset: String,
    from: Option<String>,
    to: Option<String>,
    amount: Option<i128>,
    to_muxed_id: Option<ToMuxedId>,
    authorized: Option<bool>,
}

fn ev(event: &str, asset: &str, from: Option<&str>, to: Option<&str>, amount: Option<i64>) -> Ev {
    Ev {
        event: event.into(),
        asset: asset.into(),
        from: from.map(Into::into),
        to: to.map(Into::into),
        amount: amount.map(i128::from),
        to_muxed_id: None,
        authorized: None,
    }
}

/// Operation-scope token events, with the contract checked to be the asset's SAC.
fn op_token_events(n: &str, tx: &TxFacts, assets: &[(String, Asset)]) -> Vec<Ev> {
    tx.changes
        .iter()
        .filter(|c| c.scope == ChangeScope::Operation)
        .filter_map(|c| match &c.change {
            Change::TokenEvent(t) => Some(t),
            _ => None,
        })
        .map(|t| {
            let asset = t.asset.clone().expect("a classic event names its asset");
            let (_, a) = assets
                .iter()
                .find(|(name, _)| *name == asset)
                .unwrap_or_else(|| panic!("unexpected asset {asset}"));
            assert_eq!(t.contract, sac(n, a), "{asset}");
            assert_eq!(t.stage, "operation");
            Ev {
                event: t.event.clone(),
                asset,
                from: t.from.clone(),
                to: t.to.clone(),
                amount: t.amount.map(|a| a.0),
                to_muxed_id: t.to_muxed_id.clone(),
                authorized: t.authorized,
            }
        })
        .collect()
}

/// cap67.unified-events: the same classic steps on three chains: p30 with classic events,
/// p22 with events off and p22 with `BACKFILL_STELLAR_ASSET_EVENTS`. The emitting chains
/// report each step's token event with the recipe's amounts and the typed to_muxed_id; the
/// p22 chain without backfill reports the same balance effects and no events at all.
#[test]
fn cap67_classic_token_events() {
    const UNIT: i64 = 10_000_000;
    const BALANCE: i64 = 100_000_000_000;
    let modes = [
        ("cap67.unified-events", "p30", true),
        ("cap67.unified-events", "p22", false),
        ("cap67.unified-events-backfill", "p22-backfill", true),
    ];
    let fam = "cap67.unified-events";
    for (recipe, prefix, emits) in modes {
        let alice = account(recipe, "alice");
        let bob = account(recipe, "bob");
        let carol = account(recipe, "carol");
        let asset = unif(recipe);
        let unif_s = format!("UNIF:{}", account(recipe, "issuer"));
        let assets = [
            ("native".to_string(), Asset::Native),
            (unif_s.clone(), asset.clone()),
        ];
        let params = LiquidityPoolParameters::LiquidityPoolConstantProduct(
            LiquidityPoolConstantProductParameters {
                asset_a: Asset::Native,
                asset_b: asset.clone(),
                fee: 30,
            },
        );
        let pool = format!(
            "{}",
            stellar_strkey::LiquidityPool(
                Sha256::digest(params.to_xdr(Limits::none()).unwrap()).into(),
            )
        );
        let memo_hash: [u8; 32] = Sha256::digest(b"shapekit/cap67/memo-hash").into();
        for n in NETWORKS {
            let tx_of = |slug: &str| only_tx(fam, &format!("{fam}.{prefix}-{slug}"), n);
            let created = tx_of("create-claimable-balance");
            let cb = op_changes(&created)
                .into_iter()
                .find_map(|c| match c {
                    Change::ClaimableBalanceCreated {
                        balance_id,
                        asset,
                        amount,
                        claimants,
                    } => {
                        assert_eq!(*asset, unif_s);
                        assert_eq!(amount.0, i128::from(20 * UNIT));
                        assert_eq!(*claimants, vec![bob.clone()]);
                        Some(balance_id.clone())
                    }
                    _ => None,
                })
                .expect("the created claimable balance");
            let clawed = tx_of("clawback-claimable-balance");
            let clawed_cb = op_changes(&clawed)
                .into_iter()
                .find_map(|c| match c {
                    Change::ClaimableBalanceClawedBack {
                        balance_id, amount, ..
                    } => {
                        assert_eq!(amount.0, i128::from(4 * UNIT));
                        Some(balance_id.clone())
                    }
                    _ => None,
                })
                .expect("the clawed back claimable balance");
            let merge = tx_of("account-merge-muxed-destination");
            // carol's merge is her only transaction, so she merges her balance less its fee
            let merged = BALANCE - i64::try_from(merge.fee.charged.0).unwrap();

            let muxed = |mut e: Ev, id: ToMuxedId| {
                e.to_muxed_id = Some(id);
                e
            };
            let authorized = |b: bool| Ev {
                authorized: Some(b),
                ..ev("set_authorized", &unif_s, None, Some(&bob), None)
            };
            let cases: Vec<(&str, Vec<Ev>)> = vec![
                (
                    "payment-mint",
                    vec![ev("mint", &unif_s, None, Some(&alice), Some(1000 * UNIT))],
                ),
                (
                    "payment-transfer",
                    vec![ev(
                        "transfer",
                        &unif_s,
                        Some(&alice),
                        Some(&bob),
                        Some(100 * UNIT),
                    )],
                ),
                (
                    "payment-burn",
                    vec![ev("burn", &unif_s, Some(&bob), None, Some(10 * UNIT))],
                ),
                (
                    "payment-to-self",
                    vec![ev(
                        "transfer",
                        "native",
                        Some(&alice),
                        Some(&alice),
                        Some(UNIT),
                    )],
                ),
                (
                    "payment-muxed-destination",
                    vec![muxed(
                        ev(
                            "transfer",
                            "native",
                            Some(&alice),
                            Some(&bob),
                            Some(5 * UNIT),
                        ),
                        ToMuxedId::U64((1u64 << 53).into()),
                    )],
                ),
                (
                    "payment-memo-id",
                    vec![muxed(
                        ev("transfer", "native", Some(&alice), Some(&bob), Some(UNIT)),
                        ToMuxedId::U64(u64::MAX.into()),
                    )],
                ),
                (
                    "payment-memo-text",
                    vec![muxed(
                        ev("transfer", "native", Some(&alice), Some(&bob), Some(UNIT)),
                        ToMuxedId::String(hex::encode("unified events memo")),
                    )],
                ),
                (
                    "payment-memo-hash",
                    vec![muxed(
                        ev("transfer", "native", Some(&alice), Some(&bob), Some(UNIT)),
                        ToMuxedId::Bytes(hex::encode(memo_hash)),
                    )],
                ),
                (
                    "path-payment-muxed-destination",
                    vec![muxed(
                        ev(
                            "transfer",
                            &unif_s,
                            Some(&alice),
                            Some(&bob),
                            Some(3 * UNIT),
                        ),
                        ToMuxedId::U64(7u64.into()),
                    )],
                ),
                (
                    "clawback",
                    vec![ev("clawback", &unif_s, Some(&alice), None, Some(5 * UNIT))],
                ),
                ("set-authorized-false", vec![authorized(false)]),
                ("set-authorized-true", vec![authorized(true)]),
                (
                    "create-claimable-balance",
                    vec![ev(
                        "transfer",
                        &unif_s,
                        Some(&alice),
                        Some(&cb),
                        Some(20 * UNIT),
                    )],
                ),
                (
                    "claim-claimable-balance",
                    vec![ev(
                        "transfer",
                        &unif_s,
                        Some(&cb),
                        Some(&bob),
                        Some(20 * UNIT),
                    )],
                ),
                (
                    "clawback-claimable-balance",
                    vec![ev(
                        "clawback",
                        &unif_s,
                        Some(&clawed_cb),
                        None,
                        Some(4 * UNIT),
                    )],
                ),
                (
                    "liquidity-pool-deposit",
                    vec![
                        ev(
                            "transfer",
                            "native",
                            Some(&alice),
                            Some(&pool),
                            Some(100 * UNIT),
                        ),
                        ev(
                            "transfer",
                            &unif_s,
                            Some(&alice),
                            Some(&pool),
                            Some(100 * UNIT),
                        ),
                    ],
                ),
                (
                    "account-merge-muxed-destination",
                    vec![muxed(
                        ev("transfer", "native", Some(&carol), Some(&bob), Some(merged)),
                        ToMuxedId::U64(u64::MAX.into()),
                    )],
                ),
            ];
            for (slug, want) in cases {
                let what = format!("{prefix} {n} {slug}");
                let tx = tx_of(slug);
                assert!(tx.successful, "{what}");
                if emits {
                    let mut got = op_token_events(n, &tx, &assets);
                    let mut want = want;
                    // the deposit's two events may come in either asset order
                    got.sort_by(|a, b| a.asset.cmp(&b.asset));
                    want.sort_by(|a, b| a.asset.cmp(&b.asset));
                    assert_eq!(got, want, "{what}");
                    let fees: Vec<_> = token_events(&tx)
                        .into_iter()
                        .filter(|t| t.event == "fee")
                        .map(|t| (t.stage.as_str(), t.from.clone(), t.amount))
                        .collect();
                    assert_eq!(
                        fees,
                        vec![(
                            "before-all-txs",
                            Some(tx.fee.source.clone()),
                            Some(tx.fee.charged)
                        )],
                        "{what}"
                    );
                } else {
                    // protocol 22 in its day: v3 meta with no events of any kind
                    assert!(
                        token_events(&tx).is_empty(),
                        "{what}: {:?}",
                        token_events(&tx)
                    );
                    assert!(events(&tx).is_empty(), "{what}");
                }
            }

            // a self payment changes no entry, events or not
            assert!(
                op_changes(&tx_of("payment-to-self"))
                    .iter()
                    .all(|c| matches!(c, Change::TokenEvent(_)))
            );
            // the mint's effect is there in every mode
            let mint = tx_of("payment-mint");
            assert!(
                op_changes(&mint).iter().any(|c| matches!(c,
                    Change::BalanceChange { holder, asset, held_in, delta, .. }
                        if *holder == alice && *asset == unif_s && held_in == "trustline"
                            && delta.0 == i128::from(1000 * UNIT))),
                "{prefix} {n}"
            );
        }
    }

    // p30 only: the fee refund stage of a Soroban call and a SAC mint to a muxed contract
    let recipe = "cap67.unified-events";
    let issuer = account(recipe, "issuer");
    let alice = account(recipe, "alice");
    let unif_s = format!("UNIF:{issuer}");
    for n in NETWORKS {
        let echo = echo_instance(n, recipe, "alice", fam);
        let tx = only_tx(fam, &format!("{fam}.p30-soroban-fee-refund"), n);
        assert!(tx.successful);
        assert_eq!(events(&tx), vec![echo_event(&echo, &ScVal::U32(67))], "{n}");
        let fees: Vec<_> = token_events(&tx)
            .into_iter()
            .filter(|t| t.event == "fee")
            .collect();
        assert_eq!(
            fees.iter().map(|t| t.stage.as_str()).collect::<Vec<_>>(),
            vec!["before-all-txs", "after-all-txs"],
            "{n}"
        );
        assert!(
            fees.iter()
                .all(|t| t.from.as_deref() == Some(alice.as_str()))
        );
        let charge = fees[0].amount.unwrap().0;
        let refund = fees[1].amount.unwrap().0;
        assert!(charge > 0 && refund < 0, "{n}: {charge} {refund}");
        assert_eq!(charge + refund, tx.fee.charged.0, "{n}");

        let tx = only_tx(
            fam,
            &format!("{fam}.p30-sac-issuer-transfer-to-muxed-contract"),
            n,
        );
        let assets = [(unif_s.clone(), unif(recipe))];
        let want = Ev {
            to_muxed_id: Some(ToMuxedId::U64(67u64.into())),
            ..ev(
                "mint",
                &unif_s,
                None,
                Some(&contract_strkey(&echo)),
                Some(10_000_000),
            )
        };
        assert_eq!(op_token_events(n, &tx, &assets), vec![want], "{n}");
    }
}

fn nonce_kind(key_xdr: &str) -> bool {
    matches!(
        ScVal::from_xdr_base64(key_xdr, Limits::none()),
        Ok(ScVal::LedgerKeyNonce(_))
    )
}

/// cap71.credentials: alice authorises `guarded(alice, true)` with address-v2 credentials,
/// the outer delegating account does the same through nested delegates, and the twin with
/// its delegates reversed is refused by the host.
#[test]
fn cap71_credentials_consume_the_authorizers_nonce() {
    let fam = "cap71.credentials";
    let source = account(fam, "source");
    let alice = account(fam, "alice");
    for n in NETWORKS {
        let outer = contract_from(
            n,
            &addr_of(fam, "source"),
            &soroban_salt(fam, "outer-account"),
        );
        let cases = [
            ("address-v2", alice.clone()),
            ("with-delegates-nested", contract_strkey(&outer)),
        ];
        for (slug, owner) in cases {
            let tx = only_tx(fam, &format!("{fam}.p30-{slug}"), n);
            assert!(tx.successful, "{n} {slug}");
            assert_eq!(tx.fee.source, source);
            // one nonce, for the address the entry names; delegates consume none
            let d = contract_data(&tx);
            assert_eq!(d.len(), 1, "{n} {slug}: {d:?}");
            assert_eq!(d[0].contract, owner, "{n} {slug}");
            assert_eq!(
                (d[0].action, d[0].durability.as_str()),
                (Action::Created, "temporary")
            );
            assert!(nonce_kind(&d[0].key_xdr), "{n} {slug}: {}", d[0].key_xdr);
            // guarded emits no event
            assert!(events(&tx).is_empty());
        }
        let tx = only_tx(fam, &format!("{fam}.p30-with-delegates-unsorted"), n);
        assert_eq!(tx.fee.source, source);
        assert_fee_only(&tx, "unsorted delegates");
    }
}

/// cap77.frozen-keys: a config upgrade freezing holder's USD trustline, a claim, deposit
/// and withdrawal that fail on it, then a config upgrade bypassing one USD payment, which
/// applies in the next ledger.
#[test]
fn cap77_freeze_and_bypass() {
    let fam = "cap77.frozen-keys";
    let holder = account(fam, "holder");
    let issuer = account(fam, "issuer");
    let usd = AlphaNum4 {
        asset_code: AssetCode4(*b"USD\0"),
        issuer: shapekit_core_driver::keys::account_id(&derive_key(fam, "issuer").verifying_key()),
    };
    let trustline = LedgerKey::Trustline(LedgerKeyTrustLine {
        account_id: shapekit_core_driver::keys::account_id(
            &derive_key(fam, "holder").verifying_key(),
        ),
        asset: TrustLineAsset::CreditAlphanum4(usd.clone()),
    });
    let freeze = ConfigUpgradeSet {
        updated_entry: vec![ConfigSettingEntry::FrozenLedgerKeysDelta(
            FrozenLedgerKeysDelta {
                keys_to_freeze: vec![EncodedLedgerKey(
                    trustline
                        .to_xdr(Limits::none())
                        .unwrap()
                        .try_into()
                        .unwrap(),
                )]
                .try_into()
                .unwrap(),
                keys_to_unfreeze: Default::default(),
            },
        )]
        .try_into()
        .unwrap(),
    };
    let usd_s = format!("USD:{issuer}");
    for n in NETWORKS {
        let l = entry_ledger(fam, &format!("{fam}.p30-freeze-upgrade"), n);
        assert_eq!(l.upgrades.len(), 1);
        assert_eq!(
            l.upgrades[0].upgrade,
            config_upgrade_json(n, freeze.clone()),
            "{n}"
        );
        assert_eq!(
            upgrade_settings(&l.upgrades[0]),
            vec![(Action::Updated, "frozen_ledger_keys".to_string())]
        );
        let e = entry(fam, &format!("{fam}.p30-frozen-keys-entry"));
        assert_eq!(e.source.ledger_seq, Some(l.ledger_seq));

        for slug in [
            "claim-into-frozen-trustline",
            "deposit-frozen-trustline",
            "withdraw-frozen-trustline",
        ] {
            let tx = only_tx(fam, &format!("{fam}.p30-{slug}"), n);
            assert!(!tx.successful, "{n} {slug}");
            assert_eq!(tx.fee.source, holder);
            assert!(
                effect_kinds(&tx).is_empty(),
                "{n} {slug}: {:?}",
                effect_kinds(&tx)
            );
        }

        let l = entry_ledger(fam, &format!("{fam}.p30-bypass-upgrade"), n);
        assert_eq!(
            upgrade_settings(&l.upgrades[0]),
            vec![(Action::Updated, "freeze_bypass_txs".to_string())]
        );
        // the bypassed payment is the next ledger's only transaction; its hash is what the
        // recipe put in the bypass delta
        let c = chain("cap77.frozen-keys.p30", n);
        let next = &c.ledgers[&(l.ledger_seq + 1)];
        assert_eq!(next.transactions.len(), 1, "{n}");
        let pay = &next.transactions[0];
        assert!(pay.successful, "{n}");
        assert_eq!(pay.fee.source, holder);
        let bypass = ConfigUpgradeSet {
            updated_entry: vec![ConfigSettingEntry::FreezeBypassTxsDelta(
                stellar_xdr::FreezeBypassTxsDelta {
                    add_txs: vec![Hash(hex::decode(&pay.hash).unwrap().try_into().unwrap())]
                        .try_into()
                        .unwrap(),
                    remove_txs: Default::default(),
                },
            )]
            .try_into()
            .unwrap(),
        };
        assert_eq!(l.upgrades[0].upgrade, config_upgrade_json(n, bypass), "{n}");
        // a payment to the issuer burns
        let burns: Vec<_> = token_events(pay)
            .into_iter()
            .filter(|t| t.event != "fee")
            .map(|t| (t.event.clone(), t.from.clone(), t.asset.clone(), t.amount))
            .collect();
        assert_eq!(
            burns,
            vec![(
                "burn".to_string(),
                Some(holder.clone()),
                Some(usd_s.clone()),
                Some(Int(100_000_000))
            )],
            "{n}"
        );
    }
}

/// cap85.executable-tag: the owner publishes tag `echo` holding the echo wasm hash, an
/// external-ref instance is created, echoes an executable tag value, the owner deploys one
/// itself, an adopter switches to the reference, and a never published tag traps.
#[test]
fn cap85_external_ref_executables() {
    let fam = "cap85.executable-tag";
    let source = addr_of(fam, "source");
    let tag = ScVal::ExecutableTag(ScString("echo".as_bytes().to_vec().try_into().unwrap()));
    for n in NETWORKS {
        let owner = contract_from(n, &source, &soroban_salt(fam, "owner"));
        let owner_s = contract_strkey(&owner);
        let instance = contract_from(n, &source, &soroban_salt(fam, "external-ref"));
        let child = contract_from(
            n,
            &ScAddress::Contract(owner.clone()),
            &soroban_salt(fam, "owner-deploys"),
        );
        let adopter = contract_from(n, &source, &soroban_salt(fam, "adopter"));
        let exec = format!("external-ref:{owner_s}:{}", hex::encode("echo"));

        let tx = only_tx(fam, &format!("{fam}.p30-publish-executable-tag"), n);
        assert_eq!(
            contract_data(&tx),
            vec![data(&owner, Action::Created, "persistent", &tag)],
            "{n}"
        );

        let tx = entry_tx(fam, &format!("{fam}.p30-create-external-ref-meta"), n);
        assert_eq!(
            deployed(&tx),
            vec![(contract_strkey(&instance), exec.clone())],
            "{n}"
        );
        // the reference runs echo, but no code is uploaded or touched
        assert!(code_changes(&tx).is_empty());

        let tx = entry_tx(fam, &format!("{fam}.p30-echo-executable-tag-value-meta"), n);
        assert_eq!(events(&tx), vec![echo_event(&instance, &tag)], "{n}");
        assert_eq!(
            contract_data(&tx),
            vec![data(&instance, Action::Created, "persistent", &sym("last"))]
        );

        let tx = entry_tx(
            fam,
            &format!("{fam}.p30-contract-deploys-external-ref-meta"),
            n,
        );
        assert_eq!(
            deployed(&tx),
            vec![(contract_strkey(&child), exec.clone())],
            "{n}"
        );

        let tx = entry_tx(fam, &format!("{fam}.p30-update-to-external-ref-meta"), n);
        assert_eq!(
            contract_data(&tx),
            vec![data(
                &adopter,
                Action::Updated,
                "persistent",
                &ScVal::LedgerKeyContractInstance
            )],
            "{n}"
        );
        // an update is not a deployment
        assert!(deployed(&tx).is_empty());
        let ev = events(&tx);
        assert_eq!(ev.len(), 1, "{n}: {ev:?}");
        assert_eq!(ev[0].1.as_deref(), Some(contract_strkey(&adopter).as_str()));
        assert_eq!(ev[0].2[0], b64(&sym("executable_update")));
        let old = vec_of(vec![
            sym("Wasm"),
            ScVal::Bytes(ScBytes(
                wasm_hash(EXEC_REF_WASM).to_vec().try_into().unwrap(),
            )),
        ]);
        assert_eq!(
            ev[0].2[1],
            b64(&old),
            "{n}: the old executable is the owner wasm"
        );

        let tx = only_tx(
            fam,
            &format!("{fam}.p30-create-external-ref-missing-tag"),
            n,
        );
        assert_fee_only(&tx, "missing tag");
    }
}

fn bytes_val(n: usize) -> ScVal {
    let b: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
    ScVal::Bytes(ScBytes(b.try_into().unwrap()))
}

/// `events_max::counted`: what core counts for one event plus the return value.
fn counted(contract: &ContractId, topics: &[ScVal], data: &ScVal, ret: &ScVal) -> usize {
    let e = ContractEvent {
        ext: ExtensionPoint::V0,
        contract_id: Some(contract.clone()),
        type_: ContractEventType::Contract,
        body: ContractEventBody::V0(ContractEventV0 {
            topics: topics.to_vec().try_into().unwrap(),
            data: data.clone(),
        }),
    };
    e.to_xdr(Limits::none()).unwrap().len() + ret.to_xdr(Limits::none()).unwrap().len()
}

/// `events_max::fill`.
fn fill(limit: usize, count: impl Fn(usize) -> usize) -> usize {
    let mut n = 0;
    while count(n + 1) <= limit {
        n += 1;
    }
    assert_eq!(count(n), limit);
    n
}

/// soroban.events-max: `echo(bytes)` and `emit(200 u32 topics, bytes)` sized so events plus
/// return value hit txMaxContractEventsSizeBytes exactly, and twins four bytes over.
#[test]
fn events_max_carry_the_whole_payload() {
    let fam = "soroban.events-max";
    let limit = match NetworkSettings::testnet()
        .unwrap()
        .get(ConfigSettingId::ContractEventsV0)
    {
        Some(ConfigSettingEntry::ContractEventsV0(c)) => {
            c.tx_max_contract_events_size_bytes as usize
        }
        other => panic!("{other:?}"),
    };
    let topics: Vec<ScVal> = (0..200).map(ScVal::U32).collect();
    for n in NETWORKS {
        let echo = echo_instance(n, fam, "caller", fam);
        let last = [sym("last")];
        let k = fill(limit, |k| {
            counted(&echo, &last, &bytes_val(k), &bytes_val(k))
        });
        let tx = only_tx(fam, &format!("{fam}.p30-echo-bytes-at-limit"), n);
        assert!(tx.successful);
        assert_eq!(events(&tx), vec![echo_event(&echo, &bytes_val(k))], "{n}");

        let k = fill(limit, |k| {
            counted(&echo, &topics, &bytes_val(k), &ScVal::Void)
        });
        let tx = only_tx(fam, &format!("{fam}.p30-emit-topics-bytes-at-limit"), n);
        assert!(tx.successful);
        let want = (
            "operation".to_string(),
            Some(contract_strkey(&echo)),
            topics.iter().map(b64).collect::<Vec<_>>(),
            b64(&bytes_val(k)),
        );
        assert_eq!(events(&tx), vec![want], "{n}");
        // emit stores nothing
        assert!(contract_data(&tx).is_empty());

        for slug in [
            "echo-bytes-at-limit-over",
            "emit-topics-bytes-at-limit-over",
        ] {
            assert_fee_only(&only_tx(fam, &format!("{fam}.p30-{slug}"), n), slug);
        }
    }
}

fn string_val(b: &[u8]) -> ScVal {
    ScVal::String(ScString(b.to_vec().try_into().unwrap()))
}

fn bytes_of(b: &[u8]) -> ScVal {
    ScVal::Bytes(ScBytes(b.to_vec().try_into().unwrap()))
}

fn map_entry(key: ScVal, name: &str) -> ScMapEntry {
    ScMapEntry {
        key,
        val: sym(name),
    }
}

/// `scval::integer_extremes`.
fn integer_extremes() -> ScVal {
    vec_of(vec![
        ScVal::U32(u32::MAX),
        ScVal::I32(i32::MIN),
        ScVal::I32(i32::MAX),
        ScVal::U64(u64::MAX),
        ScVal::I64(i64::MIN),
        ScVal::I64(i64::MAX),
        ScVal::Timepoint(TimePoint(u64::MAX)),
        ScVal::Duration(Duration(u64::MAX)),
        ScVal::U128(UInt128Parts { hi: 0, lo: 0 }),
        ScVal::U128(UInt128Parts {
            hi: u64::MAX,
            lo: u64::MAX,
        }),
        ScVal::I128(Int128Parts {
            hi: i64::MIN,
            lo: 0,
        }),
        ScVal::I128(Int128Parts {
            hi: i64::MAX,
            lo: u64::MAX,
        }),
        ScVal::U256(UInt256Parts {
            hi_hi: 0,
            hi_lo: 0,
            lo_hi: 0,
            lo_lo: 0,
        }),
        ScVal::U256(UInt256Parts {
            hi_hi: u64::MAX,
            hi_lo: u64::MAX,
            lo_hi: u64::MAX,
            lo_lo: u64::MAX,
        }),
        ScVal::I256(Int256Parts {
            hi_hi: i64::MIN,
            hi_lo: 0,
            lo_hi: 0,
            lo_lo: 0,
        }),
        ScVal::I256(Int256Parts {
            hi_hi: i64::MAX,
            hi_lo: u64::MAX,
            lo_hi: u64::MAX,
            lo_lo: u64::MAX,
        }),
    ])
}

/// `scval::colliding_entries`, sorted the way the recipe sorts them.
fn colliding_map() -> ScVal {
    let one256 = UInt256Parts {
        hi_hi: 0,
        hi_lo: 0,
        lo_hi: 0,
        lo_lo: 1,
    };
    let entries = vec![
        map_entry(ScVal::U32(1), "u32"),
        map_entry(ScVal::I32(1), "i32"),
        map_entry(ScVal::U64(1), "u64"),
        map_entry(ScVal::I64(1), "i64"),
        map_entry(ScVal::U128(UInt128Parts { hi: 0, lo: 1 }), "u128"),
        map_entry(ScVal::I128(Int128Parts { hi: 0, lo: 1 }), "i128"),
        map_entry(ScVal::U256(one256.clone()), "u256"),
        map_entry(
            ScVal::I256(Int256Parts {
                hi_hi: 0,
                hi_lo: 0,
                lo_hi: 0,
                lo_lo: 1,
            }),
            "i256",
        ),
        map_entry(bytes_of(b"1"), "bytes"),
        map_entry(string_val(b"1"), "string"),
        map_entry(sym("1"), "symbol"),
    ];
    ScVal::Map(Some(ScMap::sorted_from(entries).unwrap()))
}

/// `scval::mixed_key_entries`, already in host order.
fn mixed_map(addr: ScAddress) -> ScVal {
    let entries = vec![
        map_entry(ScVal::Bool(true), "bool"),
        map_entry(ScVal::Void, "void"),
        map_entry(ScVal::Error(stellar_xdr::ScError::Contract(7)), "error"),
        map_entry(ScVal::U32(7), "u32"),
        map_entry(ScVal::I32(-7), "i32"),
        map_entry(ScVal::U64(7), "u64"),
        map_entry(ScVal::I64(-7), "i64"),
        map_entry(ScVal::Timepoint(TimePoint(7)), "timepoint"),
        map_entry(ScVal::Duration(Duration(7)), "duration"),
        map_entry(ScVal::U128(UInt128Parts { hi: 0, lo: 7 }), "u128"),
        map_entry(
            ScVal::I128(Int128Parts {
                hi: -1,
                lo: u64::MAX - 6,
            }),
            "i128",
        ),
        map_entry(
            ScVal::U256(UInt256Parts {
                hi_hi: 0,
                hi_lo: 0,
                lo_hi: 0,
                lo_lo: 7,
            }),
            "u256",
        ),
        map_entry(
            ScVal::I256(Int256Parts {
                hi_hi: -1,
                hi_lo: u64::MAX,
                lo_hi: u64::MAX,
                lo_lo: u64::MAX - 6,
            }),
            "i256",
        ),
        map_entry(bytes_of(&[7]), "bytes"),
        map_entry(string_val(b"seven"), "string"),
        map_entry(sym("seven"), "symbol"),
        map_entry(vec_of(vec![ScVal::U32(7)]), "vec"),
        map_entry(
            ScVal::Map(Some(ScMap(
                vec![ScMapEntry {
                    key: ScVal::U32(7),
                    val: ScVal::U32(7),
                }]
                .try_into()
                .unwrap(),
            ))),
            "map",
        ),
        map_entry(ScVal::Address(addr), "address"),
    ];
    ScVal::Map(Some(ScMap(entries.try_into().unwrap())))
}

/// Levels of single-element nesting around the leaf, if every level is `unwrap`'s kind.
fn nesting(mut v: ScVal, unwrap: fn(ScVal) -> Option<ScVal>) -> (u32, ScVal) {
    let mut levels = 0;
    while let Some(inner) = unwrap(v.clone()) {
        levels += 1;
        v = inner;
    }
    (levels, v)
}

fn unwrap_vec(v: ScVal) -> Option<ScVal> {
    match v {
        ScVal::Vec(Some(items)) if items.len() == 1 => Some(items[0].clone()),
        _ => None,
    }
}

fn unwrap_map(v: ScVal) -> Option<ScVal> {
    match v {
        ScVal::Map(Some(m)) if m.len() == 1 && m[0].key == sym("v") => Some(m[0].val.clone()),
        _ => None,
    }
}

/// soroban.scval-shapes: the caller's echo instance echoes deep vecs and maps, a mixed-key
/// map, colliding keys, integer extremes and a non-UTF-8 string; their refused twins fail.
#[test]
fn scval_shapes_reach_the_event_unchanged() {
    let fam = "soroban.scval-shapes";
    let caller = addr_of(fam, "caller");
    for n in NETWORKS {
        let echo = echo_instance(n, fam, "caller", fam);
        let values = [
            ("map-mixed-keys-sorted", mixed_map(caller.clone())),
            ("map-keys-collide-native", colliding_map()),
            ("integer-extremes", integer_extremes()),
            (
                "string-not-utf8",
                string_val(b"\x80shapekit\xff\xc3(\xed\xa0\x80"),
            ),
        ];
        for (slug, value) in values {
            let tx = entry_tx(fam, &format!("{fam}.p30-{slug}-meta"), n);
            assert!(tx.successful, "{n} {slug}");
            assert_eq!(events(&tx), vec![echo_event(&echo, &value)], "{n} {slug}");
            let d = contract_data(&tx);
            assert_eq!(d.len(), 1, "{n} {slug}");
            assert_eq!(
                (
                    d[0].contract.as_str(),
                    d[0].key_xdr.as_str(),
                    d[0].durability.as_str()
                ),
                (
                    contract_strkey(&echo).as_str(),
                    b64(&sym("last")).as_str(),
                    "persistent"
                )
            );
        }
        // the depth is what simulation found; the shape around a u32 leaf holding that
        // depth is the recipe's
        for (kind, unwrap) in [
            ("vec", unwrap_vec as fn(ScVal) -> Option<ScVal>),
            ("map", unwrap_map),
        ] {
            let tx = entry_tx(fam, &format!("{fam}.p30-{kind}-depth-max-meta"), n);
            assert!(tx.successful);
            let ev = events(&tx);
            assert_eq!(ev.len(), 1, "{n} {kind}");
            let v = ScVal::from_xdr_base64(&ev[0].3, Limits::none()).unwrap();
            let (levels, leaf) = nesting(v, unwrap);
            assert!(levels > 1, "{n} {kind}");
            assert_eq!(leaf, ScVal::U32(levels), "{n} {kind}");
        }
        for slug in [
            "vec-depth-over",
            "map-depth-over",
            "map-mixed-keys-unsorted",
            "symbol-not-utf8",
        ] {
            assert_fee_only(&only_tx(fam, &format!("{fam}.p30-{slug}"), n), slug);
        }
    }
}
