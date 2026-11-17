//! The harness's own resident memory, from the kernel.
//!
//! Linux reads `/proc/self/status` and can reset the peak between rounds. macOS has no
//! resettable peak: there the peak is `getrusage(RUSAGE_SELF).ru_maxrss`, the highest resident
//! set since the process started, so a round's peak is the highest of that round and every
//! round before it. The growth and budget checks of a repeated run still hold with it: a later
//! round that grows past the first raises the peak, and one that does not leaves it where the
//! first round put it.

use anyhow::Result;

/// Peak and current resident set of this process in bytes: on Linux `VmHWM` and `VmRSS`; on
/// macOS `ru_maxrss` and the task's resident size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rss {
    pub peak: u64,
    pub current: u64,
}

#[cfg(target_os = "linux")]
mod imp {
    use anyhow::{Context, Result};

    use super::Rss;

    fn kib_field(status: &str, name: &str) -> Result<u64> {
        let line = status
            .lines()
            .find(|l| l.starts_with(name))
            .with_context(|| format!("/proc/self/status has no {name}"))?;
        let kib: u64 = line
            .trim_start_matches(name)
            .trim()
            .trim_end_matches("kB")
            .trim()
            .parse()
            .with_context(|| format!("parsing {line:?}"))?;
        Ok(kib * 1024)
    }

    pub fn rss() -> Result<Rss> {
        let status =
            std::fs::read_to_string("/proc/self/status").context("reading /proc/self/status")?;
        Ok(Rss {
            peak: kib_field(&status, "VmHWM:")?,
            current: kib_field(&status, "VmRSS:")?,
        })
    }

    pub fn reset_peak() -> Result<()> {
        std::fs::write("/proc/self/clear_refs", "5").context("writing 5 to /proc/self/clear_refs")
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use anyhow::{Result, bail};

    use super::Rss;

    pub fn rss() -> Result<Rss> {
        // SAFETY: both calls only write into the zeroed structs passed to them, of the sizes
        // they are told
        let (usage, info) = unsafe {
            let mut usage: libc::rusage = std::mem::zeroed();
            if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
                bail!("getrusage: {}", std::io::Error::last_os_error());
            }
            let mut info: libc::proc_taskinfo = std::mem::zeroed();
            let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
            let n = libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDTASKINFO,
                0,
                (&mut info as *mut libc::proc_taskinfo).cast(),
                size,
            );
            if n != size {
                bail!(
                    "proc_pidinfo(PROC_PIDTASKINFO) returned {n} of {size} bytes: {}",
                    std::io::Error::last_os_error()
                );
            }
            (usage, info)
        };
        // ru_maxrss is in bytes on macOS (getrusage(2)); linux would report kilobytes
        Ok(Rss {
            peak: u64::try_from(usage.ru_maxrss)?,
            current: info.pti_resident_size,
        })
    }

    pub fn reset_peak() -> Result<()> {
        // macOS keeps no resettable peak; the module comment says what the peak means here
        Ok(())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    use anyhow::{Result, bail};

    use super::Rss;

    pub fn rss() -> Result<Rss> {
        bail!("reading this process's resident memory is implemented for linux and macos only")
    }

    pub fn reset_peak() -> Result<()> {
        bail!("reading this process's resident memory is implemented for linux and macos only")
    }
}

pub fn rss() -> Result<Rss> {
    imp::rss()
}

/// Resets the peak to the current resident set, so a later [`rss`] reports the peak since
/// now (proc(5), `clear_refs` value 5). On macOS it does nothing; see the module comment.
pub fn reset_peak() -> Result<()> {
    imp::reset_peak()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn peak_follows_an_allocation_and_resets() {
        reset_peak().unwrap();
        let before = rss().unwrap();
        // touch every page so it is resident
        let block = vec![1u8; 64 * 1024 * 1024];
        let during = rss().unwrap();
        assert!(
            during.peak >= before.peak + 60 * 1024 * 1024,
            "{before:?} {during:?}"
        );
        drop(std::hint::black_box(block));
        reset_peak().unwrap();
        let after = rss().unwrap();
        assert!(after.peak < during.peak, "{during:?} {after:?}");
    }

    // both values are in bytes, not kilobytes: a 64 MiB block shows as at least 60 MiB more.
    // the peak never falls, since macOS cannot reset it
    #[cfg(target_os = "macos")]
    #[test]
    fn peak_follows_an_allocation_and_never_falls() {
        let before = rss().unwrap();
        assert!(
            before.current > 0 && before.peak >= before.current,
            "{before:?}"
        );
        // touch every page so it is resident
        let block = vec![1u8; 64 * 1024 * 1024];
        let during = rss().unwrap();
        assert!(
            during.current >= before.current + 60 * 1024 * 1024,
            "{before:?} {during:?}"
        );
        assert!(during.peak >= during.current, "{during:?}");
        drop(std::hint::black_box(block));
        reset_peak().unwrap();
        let after = rss().unwrap();
        assert!(after.peak >= during.peak, "{during:?} {after:?}");
    }
}
