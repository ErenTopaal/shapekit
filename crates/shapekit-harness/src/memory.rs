//! The harness's own resident memory, from the kernel.

use anyhow::{Context, Result};

/// `VmHWM` (peak resident set) and `VmRSS` of this process in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rss {
    pub peak: u64,
    pub current: u64,
}

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

/// Resets the peak to the current resident set, so a later [`rss`] reports the peak since
/// now (proc(5), `clear_refs` value 5).
pub fn reset_peak() -> Result<()> {
    std::fs::write("/proc/self/clear_refs", "5").context("writing 5 to /proc/self/clear_refs")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
