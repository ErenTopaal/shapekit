//! `cleanup::stop` removes a recorded network with the container attached to it, and a
//! recorded container. Needs docker and the alpine:3 image, so it is ignored by default; run
//! with `--include-ignored`.

use std::process::Command;

use shapekit_process::cleanup;

fn docker(args: &[&str]) -> String {
    let out = Command::new("docker").args(args).output().unwrap();
    assert!(
        out.status.success(),
        "docker {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn exists(kind: &str, name: &str) -> bool {
    Command::new("docker")
        .args([kind, "inspect", name])
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
#[ignore = "needs docker and the alpine:3 image; run with --include-ignored"]
fn stop_removes_recorded_networks_and_containers() {
    let id = std::process::id();
    let net = format!("shapekit-stop-test-net-{id}");
    let attached = format!("shapekit-stop-test-attached-{id}");
    let alone = format!("shapekit-stop-test-alone-{id}");
    cleanup::network_created(&net);
    docker(&["network", "create", "--internal", &net]);
    // attached to the network but not recorded itself: the network's removal takes it
    docker(&[
        "run",
        "-d",
        "--name",
        &attached,
        "--network",
        &net,
        "alpine:3",
        "sleep",
        "600",
    ]);
    cleanup::container_started(&alone);
    docker(&[
        "run",
        "-d",
        "--name",
        &alone,
        "--network",
        "none",
        "alpine:3",
        "sleep",
        "600",
    ]);
    // recorded and never created: no error
    cleanup::container_started(&format!("shapekit-stop-test-never-{id}"));
    cleanup::network_created(&format!("shapekit-stop-test-never-{id}"));

    assert_eq!(cleanup::stop(), Vec::<String>::new());
    assert!(!exists("network", &net));
    assert!(!exists("container", &attached));
    assert!(!exists("container", &alone));
}
