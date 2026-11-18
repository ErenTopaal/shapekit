//! What `cleanup::stop` does with a record holding hooks, a process group and a scratch
//! directory. One test per file: `stop` closes the record for the rest of the process.

use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::{Arc, Mutex};

use shapekit_process::{cleanup, scratch};

#[test]
fn stop_runs_hooks_in_order_kills_groups_and_removes_scratch() {
    let ran = Arc::new(Mutex::new(Vec::new()));
    for n in [1, 2] {
        let ran = Arc::clone(&ran);
        cleanup::add_hook(move || ran.lock().unwrap().push(n));
    }
    let mut child = Command::new("sleep")
        .arg("600")
        .process_group(0)
        .spawn()
        .unwrap();
    cleanup::group_started(child.id());
    let dir = scratch::scratch_dir("shapekit-stop-test-").unwrap();
    let path = dir.path().to_path_buf();
    std::mem::forget(dir);

    assert_eq!(cleanup::stop(), Vec::<String>::new());
    assert_eq!(*ran.lock().unwrap(), vec![1, 2]);
    let status = child.wait().unwrap();
    assert!(!status.success(), "the group was killed: {status:?}");
    assert!(!path.exists());
    assert!(scratch::scratch_dir("shapekit-stop-test-").is_err());
}
