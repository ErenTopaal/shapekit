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

/// The pid in a network name the harness gives (`shapekit-harness-<pid>-<n>`).
fn owner_pid(name: &str) -> Option<i32> {
    let rest = name.strip_prefix("shapekit-harness-")?;
    let (pid, n) = rest.split_once('-')?;
    n.parse::<u64>().ok()?;
    pid.parse().ok()
}

#[cfg(unix)]
fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the process exists and may be signalled
    let r = unsafe { libc::kill(pid, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn alive(_pid: i32) -> bool {
    true
}

/// Removes the networks, with what is attached, of harness runs whose process is gone. A run
/// stopped by a signal removes its own (shapekit_process::cleanup); one killed with SIGKILL
/// cannot, and the next run on the host removes what it left. Returns the names removed.
pub fn sweep_stale() -> Result<Vec<String>> {
    let names = docker(&[
        "network",
        "ls",
        "--filter",
        "label=shapekit.harness=1",
        "--format",
        "{{.Name}}",
    ])?;
    let mut removed = Vec::new();
    for name in names.split_whitespace() {
        let Some(pid) = owner_pid(name) else { continue };
        if pid == std::process::id() as i32 || alive(pid) {
            continue;
        }
        let filter = format!("network={name}");
        let ids = docker(&["ps", "-aq", "--filter", &filter])?;
        let ids: Vec<&str> = ids.split_whitespace().collect();
        if !ids.is_empty() {
            let mut args = vec!["rm", "-f", "-v"];
            args.extend(ids);
            docker(&args)?;
        }
        docker(&["network", "rm", name])?;
        removed.push(name.to_owned());
    }
    Ok(removed)
}

impl Network {
    pub fn create(name: &str) -> Result<Network> {
        static SWEPT: std::sync::Once = std::sync::Once::new();
        let mut swept = Ok(());
        SWEPT.call_once(|| {
            swept = sweep_stale().map(|names| {
                for n in names {
                    eprintln!("shapekit harness: removed {n}, left by a run that was killed");
                }
            });
        });
        swept.context("removing the networks of killed runs")?;
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
        // a signal's cleanup removes this network; removing it here too would race it
        shapekit_process::cleanup::park_if_stopping();
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

#[cfg(test)]
mod tests {
    use super::owner_pid;

    #[test]
    fn the_owner_pid_is_read_only_from_harness_names() {
        assert_eq!(owner_pid("shapekit-harness-4242-0"), Some(4242));
        assert_eq!(owner_pid("shapekit-harness-4242-17"), Some(4242));
        assert_eq!(owner_pid("shapekit-harness-x-0"), None);
        assert_eq!(owner_pid("shapekit-harness-4242"), None);
        assert_eq!(owner_pid("other-4242-0"), None);
    }
}
