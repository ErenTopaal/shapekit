//! A seeded generator and xdr-aware byte mutations. Everything is driven by [`Rng`] so a run
//! is reproducible from its seed, worker index and iteration.

/// splitmix64: tiny, fast and good enough to pick mutations; not used for anything secret.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`; `n` must be non-zero.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.next_u64() % 100 < percent
    }

    pub fn fill(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let r = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&r[..chunk.len()]);
        }
    }
}

/// The seed of worker `w` in a run seeded with `seed`, so workers never share a stream.
pub fn worker_seed(seed: u64, w: usize) -> u64 {
    let mut r = Rng::new(seed ^ (w as u64).wrapping_mul(0xa076_1d64_78bd_642f));
    r.next_u64()
}

// xdr lengths, discriminants and counts are big-endian u32 words, so the values that matter
// sit at word boundaries: lengths near limits, sign bits, small discriminants
const WORDS: &[u32] = &[
    0,
    1,
    2,
    3,
    4,
    5,
    0x7f,
    0x80,
    0xff,
    0x100,
    0xffff,
    0x7fff_ffff,
    0x8000_0000,
    0xffff_fffe,
    0xffff_ffff,
];

const BYTES: &[u8] = &[0, 1, 2, 0x7f, 0x80, 0xfe, 0xff];

/// Applies one to four mutations to `data`. `donor` is another input of the same type for
/// splicing.
pub fn mutate(rng: &mut Rng, data: &mut Vec<u8>, donor: Option<&[u8]>) {
    let rounds = 1 + rng.below(4);
    for _ in 0..rounds {
        mutate_once(rng, data, donor);
    }
}

fn word_offset(rng: &mut Rng, len: usize) -> Option<usize> {
    (len >= 4).then(|| rng.below(len / 4) * 4)
}

fn mutate_once(rng: &mut Rng, data: &mut Vec<u8>, donor: Option<&[u8]>) {
    let len = data.len();
    match rng.below(12) {
        0 if len > 0 => {
            let i = rng.below(len);
            data[i] ^= 1 << rng.below(8);
        }
        1 if len > 0 => {
            let i = rng.below(len);
            data[i] = BYTES[rng.below(BYTES.len())];
        }
        2 | 3 => {
            if let Some(o) = word_offset(rng, len) {
                let w = WORDS[rng.below(WORDS.len())];
                data[o..o + 4].copy_from_slice(&w.to_be_bytes());
            }
        }
        4 => {
            // a length word pointing just past, at or before what follows it
            if let Some(o) = word_offset(rng, len) {
                let rest = (len - o - 4) as i64;
                let w = (rest + rng.below(9) as i64 - 4).max(0) as u32;
                data[o..o + 4].copy_from_slice(&w.to_be_bytes());
            }
        }
        5 => {
            if let Some(o) = word_offset(rng, len) {
                let w = u32::from_be_bytes(data[o..o + 4].try_into().expect("four bytes"));
                let d = rng.below(5) as u32 + 1;
                let w = if rng.chance(50) {
                    w.wrapping_add(d)
                } else {
                    w.wrapping_sub(d)
                };
                data[o..o + 4].copy_from_slice(&w.to_be_bytes());
            }
        }
        6 => {
            // drop an aligned run of words
            if let Some(o) = word_offset(rng, len) {
                let max = (len - o) / 4;
                let n = 1 + rng.below(max.min(16));
                data.drain(o..o + n * 4);
            }
        }
        7 => {
            // repeat an aligned run of words, which grows arrays and unions in place
            if let Some(o) = word_offset(rng, len) {
                let max = (len - o) / 4;
                let n = 1 + rng.below(max.min(16));
                let run: Vec<u8> = data[o..o + n * 4].to_vec();
                let at = rng.below(len / 4 + 1) * 4;
                data.splice(at..at, run);
            }
        }
        8 => {
            let at = if len >= 4 {
                rng.below(len / 4 + 1) * 4
            } else {
                0
            };
            let n = 4 * (1 + rng.below(4));
            let mut ins = vec![0u8; n];
            if rng.chance(50) {
                rng.fill(&mut ins);
            }
            data.splice(at..at, ins);
        }
        9 if len > 0 => {
            let cut = rng.below(len);
            data.truncate(cut);
        }
        10 => {
            if let (Some(d), Some(o)) = (donor, word_offset(rng, len))
                && d.len() >= 4
            {
                let from = rng.below(d.len() / 4) * 4;
                let n = 1 + rng.below(((d.len() - from) / 4).min(32));
                let end = (o + n * 4).min(len);
                data.splice(o..end, d[from..from + n * 4].iter().copied());
            }
        }
        _ => {
            let n = 1 + rng.below(8);
            let mut tail = vec![0u8; n];
            rng.fill(&mut tail);
            data.extend_from_slice(&tail);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(worker_seed(7, 0), worker_seed(7, 1));
    }

    #[test]
    fn mutation_is_deterministic_and_terminates_on_empty_input() {
        let mut out = Vec::new();
        for _ in 0..2 {
            let mut rng = Rng::new(42);
            let mut data = Vec::new();
            for _ in 0..1000 {
                mutate(&mut rng, &mut data, Some(&[0, 0, 0, 1, 0, 0, 0, 2]));
                if data.len() > 4096 {
                    data.truncate(64);
                }
            }
            out.push(data);
        }
        assert_eq!(out[0], out[1]);
    }
}
