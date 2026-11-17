//! A node left idle for longer than its virtual clock allows still takes the next close at
//! the driver's fixed close time. Needs docker, so it is ignored by default; run with
//! `--include-ignored`.

mod node_support;

use std::time::Duration;

use node_support::{TESTNET, config};
use shapekit_core_driver::config::{CLOSE_STEP, close_time};
use shapekit_core_driver::http::IDLE_PAUSE;
use shapekit_core_driver::node::CoreNode;

/// Longer than the idle virtual clock needs to pass one close step: it runs about a thousand
/// virtual seconds per wall second (`config::CLOSE_STEP`).
const IDLE: Duration = Duration::from_secs(CLOSE_STEP / 1000 * 2);

#[test]
#[ignore = "needs docker and the pinned stellar-core image; run with --include-ignored"]
fn a_node_idle_past_a_close_step_still_closes_on_time() {
    assert!(
        IDLE > IDLE_PAUSE * 4,
        "the idle period must outlast the pause delay"
    );
    let tmp = tempfile::tempdir().unwrap();
    let node = CoreNode::start(config(TESTNET, 30), tmp.path()).expect("core starts");
    node.upgrade_to(30, 2, close_time(2)).unwrap();
    node.close(3, close_time(3)).unwrap();
    std::thread::sleep(IDLE);
    node.close(4, close_time(4))
        .expect("the close after an idle period is accepted");
    assert!(node.info().unwrap().ledger.num >= 4);
    node.stop().unwrap();
}
