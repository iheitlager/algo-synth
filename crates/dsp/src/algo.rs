//! Seeded integer randomness for the song's generators and `?` (spec 002
//! Req 7, ADR-0016): the same text always gives the same notes, on every run
//! and every machine. No floating point decides anything here.

/// A xorshift32 stream; seed 0 is replaced, as the noise source does.
#[derive(Clone, Copy, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(if seed == 0 { 0x9E37_79B9 } else { seed })
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
}

/// Mix two numbers into one seed (a base seed and a cycle, say).
pub fn mix(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(0x85EB_CA6B) ^ b.wrapping_add(0x9E37_79B9).wrapping_mul(0xC2B2_AE35);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^ (x >> 13)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_stream() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
        assert_ne!(Rng::new(7).next_u32(), Rng::new(8).next_u32());
    }

    #[test]
    fn seed_zero_does_not_stick() {
        assert_ne!(Rng::new(0).next_u32(), 0);
    }

    #[test]
    fn mix_separates_neighbours() {
        assert_ne!(mix(1, 2), mix(2, 1));
        assert_ne!(mix(5, 0), mix(5, 1));
    }
}
