//! An ordered close whose first transaction core refuses at submission, in learning mode.
//! The memo ids that fix the order are chosen for the whole set; a second transaction
//! queued with its id after the first was refused put `Memo:id` into a ledger the learned
//! recipe, which holds that transaction without a memo, does not repeat.

use std::path::Path;

use shapekit_contracts::RecipeIr;
use shapekit_contracts::recipe_ir::{NodeOptions, Step, TxExpect};
use shapekit_core_driver::config::CoreImage;
use shapekit_explore::arms::{arms_in, ledger_signature, reachable};
use shapekit_gen::recipe::ir_run::{IrRecipe, Session};
use shapekit_gen::recipe::{Network, start_chain};

fn payment(source: &str, to: &str, label: Option<&str>, fee: Option<u32>) -> Step {
    Step::Tx {
        label: label.map(str::to_owned),
        source: source.into(),
        source_mux_id: None,
        operations: vec![serde_json::json!({
            "source_account": null,
            "body": { "payment": {
                "destination": format!("@account:{to}"), "asset": "native", "amount": "1",
            }},
        })],
        signers: Vec::new(),
        memo: None,
        fee,
        cond: None,
        fee_bump: None,
        expect: TxExpect::default(),
    }
}

fn close(order: &[&str]) -> Step {
    Step::Close {
        label: None,
        order: order.iter().map(|s| (*s).to_owned()).collect(),
    }
}

#[test]
#[ignore = "needs docker and the pinned stellar-core image; run with --include-ignored"]
fn a_refused_first_transaction_leaves_the_rest_of_the_order_out() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let image = CoreImage::from_pins(&root.join("pins.toml")).unwrap();
    let ir = RecipeIr {
        schema_version: 1,
        id: "explore.test.ordered-learning".into(),
        family: "explore".into(),
        protocol: 30,
        node: NodeOptions {
            soroban_settings: false,
            soroban_limits: false,
            min_persistent_ttl: None,
            emit_classic_events: true,
            backfill_stellar_asset_events: true,
            emit_ledger_close_meta_ext_v1: false,
            emit_soroban_transaction_meta_ext_v1: false,
        },
        below_protocol_valid: false,
        steps: Vec::new(),
    };
    let work = tempfile::tempdir().unwrap();
    let node = work.path().join("node");
    std::fs::create_dir_all(&node).unwrap();
    let shell = IrRecipe::new(ir, &root);
    let mut ctx = start_chain(&shell, Network::Testnet, &image, &node).unwrap();
    let mut session = Session::new(&mut ctx, &root, false).unwrap();
    session.set_learning(true);
    let steps = vec![
        Step::Fund {
            label: None,
            roles: vec!["a".into(), "b".into(), "c".into()],
            balance: "100000000000".into(),
        },
        close(&[]),
        // a classic transaction in the ledger before, which the order prediction reads
        payment("c", "a", None, None),
        close(&[]),
        // a fee of 1 stroop is refused at submission (tx_insufficient_fee)
        payment("a", "c", Some("o1a"), Some(1)),
        payment("b", "c", Some("o1b"), None),
        close(&["o1a", "o1b"]),
    ];
    session.plan(&steps);
    for (i, s) in steps.iter().enumerate() {
        session.step(&mut ctx, i, s).unwrap();
    }
    let refused: Vec<usize> = session
        .take_observed()
        .into_iter()
        .filter(|o| o.outcome == shapekit_contracts::recipe_ir::Outcome::Rejected)
        .map(|o| o.step)
        .collect();
    let left_out = session.take_left_out();
    let sig = ledger_signature(ctx.driver().last_meta()).unwrap();
    let arms = arms_in(&sig, &reachable().unwrap());
    ctx.stop().unwrap();
    assert_eq!(refused, vec![4], "the 1 stroop fee is refused");
    assert!(
        !arms.contains("Memo:id"),
        "no transaction of the order went into the ledger with its ordering memo"
    );
    assert!(
        left_out.contains(&5),
        "the rest of the order is left out: {left_out:?}"
    );
}
