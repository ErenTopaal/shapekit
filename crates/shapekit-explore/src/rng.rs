//! A small seeded generator. The search does not need cryptographic randomness, only a
//! stream that a seed reproduces, so a run can be repeated from its printed seed.

/// SplitMix64 (Steele, Lea and Flood, "Fast splittable pseudorandom number generators",
/// 2014), the generator the xoshiro authors recommend for seeding.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`; `n` must be positive.
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0)");
        // the modulo bias is at most n / 2^64, irrelevant for choosing among a few values
        (self.next_u64() % n as u64) as usize
    }

    /// True with probability `num / den`.
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        self.below(den as usize) < num as usize
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// An index drawn with probability proportional to its weight; weights must not all be
    /// zero.
    pub fn weighted(&mut self, weights: &[u64]) -> usize {
        let total: u64 = weights.iter().sum();
        assert!(total > 0, "all weights are zero");
        let mut x = self.next_u64() % total;
        for (i, w) in weights.iter().enumerate() {
            if x < *w {
                return i;
            }
            x -= w;
        }
        unreachable!("x is below the total")
    }

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n + 8);
        while out.len() < n {
            out.extend_from_slice(&self.next_u64().to_le_bytes());
        }
        out.truncate(n);
        out
    }

    /// A child generator, so one seed gives independent streams to parallel workers.
    pub fn split(&mut self) -> Self {
        Self(self.next_u64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_repeats_its_stream() {
        let a: Vec<u64> = (0..5)
            .map({
                let mut r = Rng::new(7);
                move |_| r.next_u64()
            })
            .collect();
        let mut r = Rng::new(7);
        let b: Vec<u64> = (0..5).map(|_| r.next_u64()).collect();
        assert_eq!(a, b);
        // splitmix64 reference value for seed 0, first output
        assert_eq!(Rng::new(0).next_u64(), 0xe220_a839_7b1d_cdaf);
    }

    #[test]
    fn weighted_never_picks_a_zero_weight() {
        let mut r = Rng::new(1);
        for _ in 0..1000 {
            assert_ne!(r.weighted(&[3, 0, 1]), 1);
        }
    }
}
