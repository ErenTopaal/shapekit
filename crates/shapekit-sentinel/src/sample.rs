//! The ledgers a random audit draws: a 32-bit linear congruential generator (the Numerical
//! Recipes constants), so a seed names the same ledgers in any language that reimplements it.

/// Lowest ledger a draw returns. Ledger 1 is the genesis ledger no transaction ever closed in.
pub const FIRST_DRAWN: u32 = 2;

pub struct Draw {
    state: u32,
}

impl Draw {
    pub fn new(seed: u32) -> Draw {
        Draw { state: seed }
    }

    /// The next value in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        self.state = self
            .state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        f64::from(self.state) / 4_294_967_296.0
    }

    /// The next ledger in `[FIRST_DRAWN, below)`.
    pub fn ledger(&mut self, below: u32) -> u32 {
        let span = below - FIRST_DRAWN;
        // the product rounds as a double does, so the draw matches the same formula written
        // with doubles elsewhere
        FIRST_DRAWN + (self.unit() * f64::from(span)).floor() as u32
    }
}

/// `n` distinct ledgers in `[FIRST_DRAWN, below)` drawn with `seed`, in draw order. A ledger
/// drawn twice is drawn again, so the sample is always `n` ledgers.
pub fn ledgers(n: u32, seed: u32, below: u32) -> Vec<u32> {
    assert!(below > FIRST_DRAWN, "the range below {below} is empty");
    assert!(
        u64::from(n) <= u64::from(below - FIRST_DRAWN),
        "{n} distinct ledgers do not fit below {below}"
    );
    let mut draw = Draw::new(seed);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(n as usize);
    while out.len() < n as usize {
        let l = draw.ledger(below);
        if seen.insert(l) {
            out.push(l);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_draws() {
        // the same formula in javascript doubles: s = (imul(s, 1664525) + 1013904223) >>> 0,
        // 2 + floor(s / 2**32 * (65500000 - 2))
        let l = ledgers(3000, 20_261_116, 65_500_000);
        assert_eq!(
            l[..5],
            [31_489_132, 51_494_477, 36_127_806, 36_121_077, 35_553_118]
        );
        assert_eq!(l[2999], 6_726_607);
        assert!(ledgers(600, 424_242, 65_500_000).contains(&57_369_500));
    }

    #[test]
    fn distinct_and_in_range() {
        let l = ledgers(50, 7, 60);
        assert_eq!(l.len(), 50);
        let set: std::collections::BTreeSet<_> = l.iter().collect();
        assert_eq!(set.len(), 50);
        assert!(l.iter().all(|x| (2..60).contains(x)));
    }
}
