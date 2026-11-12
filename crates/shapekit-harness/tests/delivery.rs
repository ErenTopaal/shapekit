//! The delivery path a containerised target sees: a container on the harness's internal
//! network reaches the replay server at the network's gateway, sees the archive at the
//! bootstrap checkpoint and the head move, and reaches nothing off the host. Needs docker
//! and the pinned core image, which has python3.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use shapekit_core_driver::config::CoreImage;
use shapekit_harness::dockernet::Network;
use shapekit_serve::{DEFAULT_CACHE_BYTES, ServeConfig};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const FETCH: &str = r#"
import json, sys, urllib.request
out = {}
for name, url in json.loads(sys.argv[1]).items():
    try:
        with urllib.request.urlopen(url, timeout=5) as r:
            body = r.read()
            out[name] = {"status": r.status, "bytes": len(body)}
            if name == "has":
                out[name]["body"] = body.decode()
    except urllib.error.HTTPError as e:
        out[name] = {"status": e.code}
    except Exception as e:
        out[name] = {"error": type(e).__name__}
print(json.dumps(out))
"#;

fn fetch_in_container(net: &str, image: &str, urls: &Value) -> Value {
    let out = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--network",
            net,
            "--entrypoint",
            "python3",
            image,
            "-I",
            "-c",
            FETCH,
        ])
        .arg(urls.to_string())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn a_container_on_the_internal_network_reaches_only_the_replay_server() {
    let chain_dir = root().join("corpus/chains/cap40.signed-payload-signer.p28/testnet");
    let chain = shapekit_pack::chain::ChainFile::read(&chain_dir).unwrap();
    let bootstrap = *chain
        .checkpoints
        .first()
        .expect("the chain is padded through a checkpoint");
    let image = CoreImage::from_pins(&root().join("pins.toml"))
        .unwrap()
        .reference();

    let mut net =
        Network::create(&format!("shapekit-harness-test-{}", std::process::id())).unwrap();
    let server = shapekit_serve::start(ServeConfig {
        chain_dir,
        listen: SocketAddr::new(net.gateway, 0),
        head: Some(bootstrap),
        cache_bytes: DEFAULT_CACHE_BYTES,
        log_requests: false,
    })
    .unwrap();
    let base = server.url();
    let urls = serde_json::json!({
        "has": format!("{base}/archive/.well-known/stellar-history.json"),
        "next": format!("{base}/ledgers/FFFFFFFF--0-63999/{:08X}--{}.xdr.zst", u32::MAX - (bootstrap + 1), bootstrap + 1),
        "outside": "http://1.1.1.1/",
    });
    let got = fetch_in_container(&net.name, &image, &urls);
    let has: Value = serde_json::from_str(got["has"]["body"].as_str().unwrap()).unwrap();
    assert_eq!(has["currentLedger"], bootstrap);
    assert_eq!(
        got["next"]["status"], 404,
        "unreleased batch: {}",
        got["next"]
    );
    assert!(
        got["outside"].get("error").is_some(),
        "the internal network reached out: {}",
        got["outside"]
    );

    let resp = reqwest::blocking::Client::new()
        .post(format!("{base}/control/head"))
        .body(format!("{{\"ledger\": {}}}", bootstrap + 1))
        .send()
        .unwrap();
    assert!(resp.status().is_success());
    let got = fetch_in_container(&net.name, &image, &urls);
    assert_eq!(got["next"]["status"], 200, "{}", got["next"]);
    assert!(got["next"]["bytes"].as_u64().unwrap() > 0);

    server.stop().unwrap();
    net.remove().unwrap();
}
