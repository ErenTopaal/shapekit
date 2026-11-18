//! The per-chain Docker network: `--internal`, so containers on it have no route off the
//! host, while the host side of its bridge (the gateway address) is where the replay server
//! listens.

use std::net::IpAddr;
use std::process::Command;

use anyhow::{Context, Result, bail};

fn docker(args: &[&str]) -> Result<String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .with_context(|| format!("running docker {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "docker {} failed ({}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

pub struct Network {
    pub name: String,
    pub gateway: IpAddr,
    removed: bool,
}

impl Network {
    pub fn create(name: &str) -> Result<Network> {
        // recorded before the create: a signal removes the network with what is attached,
        // and one that was never created is no error to remove
        shapekit_process::cleanup::network_created(name);
        docker(&[
            "network",
            "create",
            "--internal",
            "--label",
            "shapekit.harness=1",
            name,
        ])?;
        let mut net = Network {
            name: name.to_owned(),
            gateway: IpAddr::from([0, 0, 0, 0]),
            removed: false,
        };
        let gw = docker(&[
            "network",
            "inspect",
            name,
            "--format",
            "{{range .IPAM.Config}}{{.Gateway}} {{end}}",
        ])?;
        // ipv4 first: the replay server binds one address
        let gateway = gw
            .split_whitespace()
            .filter_map(|s| s.parse::<IpAddr>().ok())
            .min_by_key(|ip| !ip.is_ipv4());
        match gateway {
            Some(ip) => net.gateway = ip,
            None => {
                let msg = format!("network {name} has no gateway address ({gw:?})");
                net.remove()?;
                bail!(msg);
            }
        }
        Ok(net)
    }

    /// Removes every container still attached, then the network.
    pub fn remove(&mut self) -> Result<()> {
        if self.removed {
            return Ok(());
        }
        let filter = format!("network={}", self.name);
        let ids = docker(&["ps", "-aq", "--filter", &filter])?;
        let ids: Vec<&str> = ids.split_whitespace().collect();
        if !ids.is_empty() {
            let mut args = vec!["rm", "-f", "-v"];
            args.extend(ids);
            docker(&args)?;
        }
        docker(&["network", "rm", &self.name])?;
        shapekit_process::cleanup::network_removed(&self.name);
        self.removed = true;
        Ok(())
    }
}

impl Drop for Network {
    fn drop(&mut self) {
        if let Err(e) = self.remove() {
            eprintln!(
                "shapekit harness: removing network {}: {}",
                self.name,
                shapekit_process::error_text::chain(e.as_ref())
            );
        }
    }
}
