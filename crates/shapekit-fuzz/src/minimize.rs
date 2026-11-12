//! Shrinks a disagreeing input while it stays in the same class. XDR is a sequence of 4-byte
//! words, so removal works on aligned word runs first (delta debugging, halving the run each
//! pass), then single bytes at the tail, then each remaining word is pushed towards zero.

use anyhow::Result;

/// Shrinks `input` while `same_class` holds, spending at most `budget` predicate calls.
/// Returns the smallest input found and the number of calls made.
pub fn minimize(
    input: &[u8],
    budget: usize,
    mut same_class: impl FnMut(&[u8]) -> Result<bool>,
) -> Result<(Vec<u8>, usize)> {
    let mut best = input.to_vec();
    let mut calls = 0usize;
    let mut try_it = |cand: &[u8], calls: &mut usize| -> Result<bool> {
        if *calls >= budget {
            return Ok(false);
        }
        *calls += 1;
        same_class(cand)
    };

    // aligned removal, largest runs first
    let mut chunk = (best.len() / 2).max(4) / 4 * 4;
    while chunk >= 4 && calls < budget {
        let mut i = 0;
        let mut removed_any = false;
        while i + chunk <= best.len() && calls < budget {
            let mut cand = best.clone();
            cand.drain(i..i + chunk);
            if try_it(&cand, &mut calls)? {
                best = cand;
                removed_any = true;
            } else {
                i += chunk;
            }
        }
        if !removed_any {
            chunk = chunk / 2 / 4 * 4;
        }
    }

    // unaligned tail bytes, for inputs whose class is about a short read
    while !best.is_empty() && calls < budget {
        let cand = best[..best.len() - 1].to_vec();
        if try_it(&cand, &mut calls)? {
            best = cand;
        } else {
            break;
        }
    }

    // simpler values: zero a word, else make it one, so what is left is the part that matters
    let mut o = 0;
    while o + 4 <= best.len() && calls < budget {
        if best[o..o + 4] != [0, 0, 0, 0] {
            for w in [0u32, 1] {
                let mut cand = best.clone();
                cand[o..o + 4].copy_from_slice(&w.to_be_bytes());
                if cand != best && try_it(&cand, &mut calls)? {
                    best = cand;
                    break;
                }
            }
        }
        o += 4;
    }
    Ok((best, calls))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_word_that_matters() {
        let mut input = vec![0u8; 64];
        input[20..24].copy_from_slice(&0xdead_beefu32.to_be_bytes());
        input[40..44].copy_from_slice(&5u32.to_be_bytes());
        let (out, calls) = minimize(&input, 10_000, |b| {
            Ok(b.chunks(4).any(|w| w == 0xdead_beefu32.to_be_bytes()))
        })
        .unwrap();
        assert_eq!(out, 0xdead_beefu32.to_be_bytes());
        assert!(calls < 10_000);
    }

    #[test]
    fn respects_the_budget() {
        let input = vec![1u8; 4096];
        let (_, calls) = minimize(&input, 7, |_| Ok(false)).unwrap();
        assert_eq!(calls, 7);
    }
}
