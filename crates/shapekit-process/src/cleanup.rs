//! The one signal path of the process. [`install`] blocks SIGINT, SIGTERM and SIGHUP before
//! any other thread exists, so every thread inherits the mask and the signals reach only the
//! thread it starts, which takes them with `sigwait`. That thread runs [`stop`] and exits
//! with `128 + signal`, the status the signal itself would have given.
//!
//! What [`stop`] removes is recorded here while it exists: hooks a subsystem adds for what
//! only it knows how to stop (the harness stops its adapters, which remove their own
//! containers), process groups started apart from this one, docker containers, docker
//! networks with whatever is still attached, and scratch directories, in that order:
//! containers go before the networks they are attached to, and every writer before the
//! directories it writes in.
//!
//! Child processes are not affected by the mask: the standard library resets it before
//! `exec`, so a child started with `std::process::Command` gets its signals as usual.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, Once};

type Hook = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
pub(crate) struct State {
    hooks: Vec<Hook>,
    groups: Vec<i32>,
    containers: Vec<String>,
    networks: Vec<String>,
    pub(crate) dirs: BTreeSet<PathBuf>,
    /// Set once [`stop`] has taken the record: nothing registered after it would be removed,
    /// so a scratch directory can no longer be created.
    pub(crate) stopping: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

pub(crate) fn state() -> MutexGuard<'static, Option<State>> {
    // every change under the lock is a single push, remove or take, so the record stays
    // consistent whatever panicked while holding it
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(state().get_or_insert_with(State::default))
}

/// Adds `hook`, run first when a signal stops the process, in the order hooks were added.
/// A hook must not wait on anything the thread it interrupts holds.
pub fn add_hook(hook: impl Fn() + Send + Sync + 'static) {
    with(|s| s.hooks.push(Arc::new(hook)));
}

/// Records a process group this process started (a child spawned as the leader of its own
/// group): a signal sent to this process does not reach it, so [`stop`] kills it.
pub fn group_started(pgid: u32) {
    if let Ok(pgid) = i32::try_from(pgid) {
        with(|s| s.groups.push(pgid));
    }
}

/// Drops a process group from the record once the process reaped its leader.
pub fn group_gone(pgid: u32) {
    if let Ok(pgid) = i32::try_from(pgid) {
        with(|s| s.groups.retain(|g| *g != pgid));
    }
}

/// Records a docker container this process created, by name or id.
pub fn container_started(name: &str) {
    with(|s| s.containers.push(name.to_owned()));
}

/// Drops a container from the record once the process removed it itself.
pub fn container_gone(name: &str) {
    with(|s| s.containers.retain(|c| c != name));
}

/// Records a docker network this process created.
pub fn network_created(name: &str) {
    with(|s| s.networks.push(name.to_owned()));
}

/// Drops a network from the record once the process removed it itself.
pub fn network_removed(name: &str) {
    with(|s| s.networks.retain(|n| n != name));
}

/// Blocks SIGINT, SIGTERM and SIGHUP and starts the thread that takes them. Call it from
/// `main` before anything starts a thread: a thread that does not block a signal is where
/// the kernel delivers it, and no cleanup would run.
#[cfg(unix)]
pub fn install() -> std::io::Result<()> {
    static ONCE: Once = Once::new();
    let mut result = Ok(());
    ONCE.call_once(|| result = install_once());
    result
}

#[cfg(unix)]
fn install_once() -> std::io::Result<()> {
    // SAFETY: the set is initialised by sigemptyset before use; pthread_sigmask only reads it
    let set = unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGHUP);
        let r = libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        if r != 0 {
            return Err(std::io::Error::from_raw_os_error(r));
        }
        set
    };
    std::thread::Builder::new()
        .name("signals".into())
        .spawn(move || {
            let mut sig: libc::c_int = 0;
            // SAFETY: set holds the three signals, blocked in every thread of the process
            if unsafe { libc::sigwait(&set, &mut sig) } != 0 {
                return;
            }
            for f in stop() {
                eprintln!("shapekit: {f}");
            }
            std::process::exit(128 + sig);
        })?;
    Ok(())
}

#[cfg(not(unix))]
pub fn install() -> std::io::Result<()> {
    Ok(())
}

/// Whether a signal is stopping the process: [`stop`] has taken the record.
pub fn stopping() -> bool {
    with(|s| s.stopping)
}

/// Parks the calling thread for good when a signal is stopping the process. The signal thread
/// removes what is recorded and exits with `128 + signal`; a thread that went on would race
/// it for the same containers and networks and could exit the process first, with another
/// status. Call it where a thread would clean up or exit after what the signal killed.
pub fn park_if_stopping() {
    if stopping() {
        loop {
            std::thread::park();
        }
    }
}

/// Runs the hooks, then removes every recorded container, network and scratch directory.
/// Returns what could not be removed, with why. Called by the signal thread; a process
/// about to exit without unwinding can call it too.
pub fn stop() -> Vec<String> {
    let taken = with(|s| {
        let taken = std::mem::take(s);
        s.stopping = true;
        taken
    });
    let mut failed = Vec::new();
    for hook in &taken.hooks {
        hook();
    }
    #[cfg(unix)]
    for g in &taken.groups {
        // SAFETY: killpg with a group this process started and has not reaped
        if unsafe { libc::killpg(*g, libc::SIGKILL) } != 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::ESRCH) {
                failed.push(format!("killing process group {g}: {e}"));
            }
        }
    }
    if !taken.containers.is_empty() {
        let mut args = vec!["rm", "-f", "-v"];
        args.extend(taken.containers.iter().map(String::as_str));
        if let Err(e) = docker(&args)
            && !e.contains("No such container")
        {
            failed.push(format!(
                "removing containers {}: {e}",
                taken.containers.join(", ")
            ));
        }
    }
    for n in &taken.networks {
        if let Err(e) = remove_network(n) {
            failed.push(format!("removing network {n}: {e}"));
        }
    }
    for d in &taken.dirs {
        if let Err(e) = std::fs::remove_dir_all(d)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            failed.push(format!("removing {}: {e}", d.display()));
        }
    }
    failed
}

/// Removes every container still attached to `network`, then the network.
fn remove_network(network: &str) -> Result<(), String> {
    let filter = format!("network={network}");
    let ids = docker(&["ps", "-aq", "--filter", &filter])?;
    let ids: Vec<&str> = ids.split_whitespace().collect();
    if !ids.is_empty() {
        let mut args = vec!["rm", "-f", "-v"];
        args.extend(ids);
        docker(&args)?;
    }
    match docker(&["network", "rm", network]) {
        Err(e) if e.contains("not found") => Ok(()),
        r => r.map(drop),
    }
}

fn docker(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("running docker: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}
