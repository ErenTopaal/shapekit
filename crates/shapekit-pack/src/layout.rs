//! SEP-54 object keys (R9), the same formulas as go-stellar-sdk v0.8.0
//! `support/datastore/schema.go` and `adapters/protocol/sep54.go`.

use crate::PackError;

/// Suffix of a batch object. SEP-54 only defines zstd.
pub const BATCH_EXTENSION: &str = ".xdr.zst";

/// One ledger per object, as quickstart's galexie writes.
pub const DEFAULT_LEDGERS_PER_BATCH: u32 = 1;

/// 64000 objects per partition, as quickstart's galexie writes.
pub const DEFAULT_BATCHES_PER_PARTITION: u32 = 64000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    ledgers_per_batch: u32,
    batches_per_partition: u32,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            ledgers_per_batch: DEFAULT_LEDGERS_PER_BATCH,
            batches_per_partition: DEFAULT_BATCHES_PER_PARTITION,
        }
    }
}

impl Layout {
    pub fn new(ledgers_per_batch: u32, batches_per_partition: u32) -> Result<Self, PackError> {
        if ledgers_per_batch == 0 {
            return Err(PackError::Layout(
                "ledgersPerBatch must be at least 1".into(),
            ));
        }
        // the sdk reads 0 as 1, but a config that says 0 is refused rather than guessed at
        if batches_per_partition == 0 {
            return Err(PackError::Layout(
                "batchesPerPartition must be at least 1".into(),
            ));
        }
        // the sdk multiplies these as uint32, so a larger partition would wrap there and the
        // keys would stop matching what it looks for
        if batches_per_partition > 1
            && u64::from(ledgers_per_batch) * u64::from(batches_per_partition) > u64::from(u32::MAX)
        {
            return Err(PackError::Layout(format!(
                "a partition of {batches_per_partition} batches of {ledgers_per_batch} ledgers exceeds {} ledgers",
                u32::MAX
            )));
        }
        Ok(Layout {
            ledgers_per_batch,
            batches_per_partition,
        })
    }

    pub fn ledgers_per_batch(&self) -> u32 {
        self.ledgers_per_batch
    }

    pub fn batches_per_partition(&self) -> u32 {
        self.batches_per_partition
    }

    pub fn partitioned(&self) -> bool {
        self.batches_per_partition > 1
    }

    /// First and last ledger of the batch holding `seq`.
    pub fn batch_range(&self, seq: u32) -> (u32, u32) {
        span(seq, self.ledgers_per_batch)
    }

    /// First and last ledger of the partition holding `seq`.
    pub fn partition_range(&self, seq: u32) -> (u32, u32) {
        // new() checked that the product fits
        span(seq, self.ledgers_per_batch * self.batches_per_partition)
    }

    /// The partition directory for `seq`, or `None` when the layout has no partitions.
    pub fn partition_name(&self, seq: u32) -> Option<String> {
        if !self.partitioned() {
            return None;
        }
        let (start, end) = self.partition_range(seq);
        Some(format!("{:08X}--{start}-{end}", u32::MAX - start))
    }

    /// The batch object name for `seq`. The short form is used whenever the batch starts
    /// and ends on the same ledger, which is the sdk's rule (it also covers a last batch
    /// cut short at `u32::MAX`).
    pub fn batch_file_name(&self, seq: u32) -> String {
        let (start, end) = self.batch_range(seq);
        if start == end {
            format!("{:08X}--{start}{BATCH_EXTENSION}", u32::MAX - start)
        } else {
            format!("{:08X}--{start}-{end}{BATCH_EXTENSION}", u32::MAX - start)
        }
    }

    /// Key of the batch holding `seq`, relative to the lake root.
    pub fn object_key(&self, seq: u32) -> String {
        match self.partition_name(seq) {
            Some(p) => format!("{p}/{}", self.batch_file_name(seq)),
            None => self.batch_file_name(seq),
        }
    }
}

fn span(seq: u32, size: u32) -> (u32, u32) {
    let start = seq - seq % size;
    (start, start.saturating_add(size - 1))
}

/// Parses a batch object name back to its first and last ledger, checking that the
/// inverted prefix agrees with the range.
pub fn parse_batch_name(name: &str) -> Option<(u32, u32)> {
    let stem = name.strip_suffix(BATCH_EXTENSION)?;
    let (inv, range) = stem.split_once("--")?;
    if inv.len() != 8
        || !inv
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_lowercase())
    {
        return None;
    }
    let inv = u32::from_str_radix(inv, 16).ok()?;
    let (start, end) = match range.split_once('-') {
        Some((s, e)) => (s.parse::<u32>().ok()?, e.parse::<u32>().ok()?),
        None => {
            let s = range.parse::<u32>().ok()?;
            (s, s)
        }
    };
    (u32::MAX - start == inv && start <= end).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    // expected keys are worked out by hand from the sep-54 formulas and match the table in
    // adapters/protocol/sep54_test.go, which is checked against go-stellar-sdk
    #[test]
    fn keys_match_sep54() {
        let cases: &[(u32, u32, u32, &str, &str)] = &[
            (1, 1, 2, "", "FFFFFFFD--2.xdr.zst"),
            (1, 1, 3, "", "FFFFFFFC--3.xdr.zst"),
            (1, 1, 63, "", "FFFFFFC0--63.xdr.zst"),
            (1, 1, 64, "", "FFFFFFBF--64.xdr.zst"),
            (1, 1, 64000, "", "FFFF05FF--64000.xdr.zst"),
            (1, 64000, 2, "FFFFFFFF--0-63999", "FFFFFFFD--2.xdr.zst"),
            (1, 64000, 3, "FFFFFFFF--0-63999", "FFFFFFFC--3.xdr.zst"),
            (1, 64000, 63, "FFFFFFFF--0-63999", "FFFFFFC0--63.xdr.zst"),
            (1, 64000, 64, "FFFFFFFF--0-63999", "FFFFFFBF--64.xdr.zst"),
            (
                1,
                64000,
                64000,
                "FFFF05FF--64000-127999",
                "FFFF05FF--64000.xdr.zst",
            ),
            (64, 1, 2, "", "FFFFFFFF--0-63.xdr.zst"),
            (64, 1, 3, "", "FFFFFFFF--0-63.xdr.zst"),
            (64, 1, 63, "", "FFFFFFFF--0-63.xdr.zst"),
            (64, 1, 64, "", "FFFFFFBF--64-127.xdr.zst"),
            (64, 1, 64000, "", "FFFF05FF--64000-64063.xdr.zst"),
            (64, 8, 2, "FFFFFFFF--0-511", "FFFFFFFF--0-63.xdr.zst"),
            (64, 8, 63, "FFFFFFFF--0-511", "FFFFFFFF--0-63.xdr.zst"),
            (64, 8, 64, "FFFFFFFF--0-511", "FFFFFFBF--64-127.xdr.zst"),
            (
                64,
                8,
                64000,
                "FFFF05FF--64000-64511",
                "FFFF05FF--64000-64063.xdr.zst",
            ),
            (2, 8, 2, "FFFFFFFF--0-15", "FFFFFFFD--2-3.xdr.zst"),
            (2, 8, 18, "FFFFFFEF--16-31", "FFFFFFED--18-19.xdr.zst"),
            (50, 2, 150, "FFFFFF9B--100-199", "FFFFFF69--150-199.xdr.zst"),
            (
                1,
                200,
                u32::MAX,
                "0000005F--4294967200-4294967295",
                "00000000--4294967295.xdr.zst",
            ),
            (
                200,
                1,
                u32::MAX,
                "",
                "0000005F--4294967200-4294967295.xdr.zst",
            ),
            (3, 1, u32::MAX, "", "00000000--4294967295.xdr.zst"),
        ];
        for &(lpb, bpp, seq, partition, batch) in cases {
            let l = Layout::new(lpb, bpp).unwrap();
            let ctx = format!("lpb={lpb} bpp={bpp} seq={seq}");
            assert_eq!(
                l.partition_name(seq).unwrap_or_default(),
                partition,
                "{ctx}"
            );
            assert_eq!(l.batch_file_name(seq), batch, "{ctx}");
            let key = if partition.is_empty() {
                batch.to_string()
            } else {
                format!("{partition}/{batch}")
            };
            assert_eq!(l.object_key(seq), key, "{ctx}");
            let (s, e) = parse_batch_name(batch).unwrap();
            assert_eq!((s, e), l.batch_range(seq), "{ctx}");
        }
    }

    #[test]
    fn rejects_bad_layouts() {
        for (lpb, bpp) in [(0, 1), (1, 0), (65536, 65536), (u32::MAX, 2)] {
            assert!(Layout::new(lpb, bpp).is_err(), "{lpb} {bpp}");
        }
        assert!(Layout::new(u32::MAX, 1).is_ok());
    }

    #[test]
    fn rejects_inconsistent_names() {
        assert_eq!(parse_batch_name("FFFFFFFD--2.xdr.zst"), Some((2, 2)));
        assert_eq!(parse_batch_name("FFFFFFFC--2.xdr.zst"), None);
        assert_eq!(parse_batch_name("fffffffd--2.xdr.zst"), None);
        assert_eq!(parse_batch_name("FFFFFFFD--2-1.xdr.zst"), None);
        assert_eq!(parse_batch_name("FFFFFFFD--2.xdr.gz"), None);
    }
}
