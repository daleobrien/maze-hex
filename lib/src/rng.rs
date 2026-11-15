//! A small seeded random-number generator (xoshiro256**, seeded by
//! SplitMix64).
//!
//! maze-hex has no dependencies yet, so it carries its own generator. It is
//! also a deliberate choice: the same seed must give the same maze on every
//! platform and in wasm, forever, so the generator is pinned here rather than
//! left to a crate whose output could change between versions.

/// A deterministic random-number generator.
#[derive(Clone, Debug)]
pub struct Rng {
    state: [u64; 4],
}

impl Rng {
    /// A generator whose whole sequence is fixed by `seed`.
    pub fn new(seed: u64) -> Self {
        let mut s = seed;
        let mut next = || {
            // SplitMix64, which spreads even tiny seeds over all 256 bits.
            s = s.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = s;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        };
        Rng {
            state: [next(), next(), next(), next()],
        }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let [s0, s1, s2, s3] = &mut self.state;
        let result = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = *s1 << 17;
        *s2 ^= *s0;
        *s3 ^= *s1;
        *s1 ^= *s2;
        *s0 ^= *s3;
        *s2 ^= t;
        *s3 = s3.rotate_left(45);
        result
    }

    /// A number in `0..n`, without bias. `n` must not be zero.
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "Rng::below(0)");
        let n = n as u64;
        // Reject the top sliver of values that would make some results likelier.
        let zone = u64::MAX - u64::MAX % n;
        loop {
            let x = self.next_u64();
            if x < zone {
                return (x % n) as usize;
            }
        }
    }

    /// A float in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Whether an event with probability `p` happens.
    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Shuffles `items` in place (Fisher–Yates).
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_numbers() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(7).next_u64(), Rng::new(8).next_u64());
    }

    #[test]
    fn below_stays_in_range_and_hits_every_value() {
        let mut rng = Rng::new(1);
        let mut seen = [false; 6];
        for _ in 0..1000 {
            let x = rng.below(6);
            seen[x] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn unit_is_in_zero_to_one() {
        let mut rng = Rng::new(3);
        for _ in 0..1000 {
            let x = rng.unit();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn shuffle_keeps_every_item() {
        let mut rng = Rng::new(5);
        let mut items: Vec<u32> = (0..50).collect();
        rng.shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(items, sorted);
    }
}
