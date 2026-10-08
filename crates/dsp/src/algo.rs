//! Seeded integer randomness for the song's generators and `?` (spec 002
//! Req 7, ADR-0016): the same text always gives the same notes, on every run
//! and every machine. No floating point decides anything here.
//!
//! The generators so far: `euclid(k, n, rot)` and the scales that note
//! generators walk.

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

/// Most steps a generated pattern may have, as a drum lane's.
pub const MAX_PULSES: u32 = 64;

/// `euclid(k, n, rot)`: `k` hits spread as evenly as they go over `n` steps,
/// the pattern rotated left by `rot`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Euclid {
    pub k: u32,
    pub n: u32,
    pub rot: u32,
}

impl Euclid {
    /// Parse a call such as `euclid(3,8)` or `euclid(3,8,2)`. `None` when the
    /// word is not a call of `euclid` at all.
    pub fn parse(word: &str) -> Option<Result<Euclid, &'static str>> {
        let rest = word.strip_prefix("euclid(")?;
        Some(Euclid::args(rest))
    }

    fn args(rest: &str) -> Result<Euclid, &'static str> {
        let usage = "euclid takes hits, steps and maybe a rotation: euclid(3,8) or euclid(3,8,2)";
        let inner = rest.strip_suffix(')').ok_or(usage)?;
        let mut nums = [0u32; 3];
        let mut count = 0;
        for part in inner.split(',') {
            let slot = nums.get_mut(count).ok_or(usage)?;
            *slot = part.parse().map_err(|_| usage)?;
            count += 1;
        }
        if count < 2 {
            return Err(usage);
        }
        let [k, n, rot] = nums;
        if n == 0 || n > MAX_PULSES {
            return Err("euclid has 1 to 64 steps");
        }
        if k > n {
            return Err("euclid cannot have more hits than steps");
        }
        Ok(Euclid { k, n, rot: rot % n })
    }

    /// The call as written canonically: the rotation only when it is not 0.
    pub fn print(&self) -> String {
        if self.rot == 0 {
            format!("euclid({},{})", self.k, self.n)
        } else {
            format!("euclid({},{},{})", self.k, self.n, self.rot)
        }
    }

    /// Which of the `n` steps are hits (Bjorklund's spreading: (3,8) is
    /// `x..x..x.`, (5,8) is `x.xx.xx.`).
    pub fn pattern(&self) -> Vec<bool> {
        let (k, n) = (self.k as usize, self.n as usize);
        if k == 0 {
            return vec![false; n];
        }
        let mut a: Vec<Vec<bool>> = vec![vec![true]; k];
        let mut b: Vec<Vec<bool>> = vec![vec![false]; n.saturating_sub(k)];
        while b.len() > 1 {
            let m = a.len().min(b.len());
            let rest: Vec<Vec<bool>> = if a.len() > m {
                a.get(m..).unwrap_or(&[]).to_vec()
            } else {
                b.get(m..).unwrap_or(&[]).to_vec()
            };
            a = a
                .iter()
                .zip(b.iter())
                .map(|(x, y)| x.iter().chain(y.iter()).copied().collect())
                .collect();
            b = rest;
        }
        let flat: Vec<bool> = a.into_iter().chain(b).flatten().collect();
        let len = flat.len().max(1);
        let rot = self.rot as usize % len;
        (0..flat.len())
            .map(|i| flat.get((i + rot) % len).copied().unwrap_or(false))
            .collect()
    }
}

/// The modes a `scale` line knows, as semitones above the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Major,
    Minor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    Pentatonic,
    Blues,
    PhrygianDominant,
    HarmonicMinor,
}

impl Mode {
    pub const ALL: [(Mode, &'static str, &'static [u8]); 11] = [
        (Mode::Major, "major", &[0, 2, 4, 5, 7, 9, 11]),
        (Mode::Minor, "minor", &[0, 2, 3, 5, 7, 8, 10]),
        (Mode::Dorian, "dorian", &[0, 2, 3, 5, 7, 9, 10]),
        (Mode::Phrygian, "phrygian", &[0, 1, 3, 5, 7, 8, 10]),
        (Mode::Lydian, "lydian", &[0, 2, 4, 6, 7, 9, 11]),
        (Mode::Mixolydian, "mixolydian", &[0, 2, 4, 5, 7, 9, 10]),
        (Mode::Locrian, "locrian", &[0, 1, 3, 5, 6, 8, 10]),
        (Mode::Pentatonic, "pentatonic", &[0, 2, 4, 7, 9]),
        (Mode::Blues, "blues", &[0, 3, 5, 6, 7, 10]),
        (
            Mode::PhrygianDominant,
            "phrygian-dominant",
            &[0, 1, 4, 5, 7, 8, 10],
        ),
        (
            Mode::HarmonicMinor,
            "harmonic-minor",
            &[0, 2, 3, 5, 7, 8, 11],
        ),
    ];

    pub fn from_name(name: &str) -> Option<Mode> {
        Mode::ALL.iter().find(|m| m.1 == name).map(|m| m.0)
    }

    pub fn name(self) -> &'static str {
        Mode::ALL.iter().find(|m| m.0 == self).map_or("", |m| m.1)
    }

    fn intervals(self) -> &'static [u8] {
        Mode::ALL
            .iter()
            .find(|m| m.0 == self)
            .map_or(&[0][..], |m| m.2)
    }
}

/// `c`, `c#` or `eb`: a pitch class, 0 for c.
pub fn pitch_class(s: &str) -> Option<u8> {
    let mut chars = s.chars();
    let base = match chars.next()? {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let semi = match (chars.next(), chars.next()) {
        (None, _) => base,
        (Some('#'), None) => base + 1,
        (Some('b'), None) => base + 11,
        _ => return None,
    };
    Some(semi % 12)
}

/// A key: a root pitch class (0 is c) and a mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scale {
    pub root: u8,
    pub mode: Mode,
}

impl Scale {
    /// Notes to the octave.
    pub fn degrees(&self) -> u32 {
        u32::try_from(self.mode.intervals().len()).unwrap_or(1)
    }

    /// The pitch class of degree `d` (0 is the root) of a seven-note scale,
    /// for roman numerals; `None` for a pentatonic or blues scale.
    pub fn degree(&self, d: usize) -> Option<u8> {
        let iv = self.mode.intervals();
        if iv.len() != 7 {
            return None;
        }
        iv.get(d).map(|i| (self.root + i) % 12)
    }

    /// The note of the scale nearest `note`, the lower one on a tie.
    pub fn snap(&self, note: u8) -> u8 {
        let iv = self.mode.intervals();
        let rel = (i32::from(note) - i32::from(self.root)).rem_euclid(12);
        let dist = |i: &u8| {
            let d = (i32::from(*i) - rel).rem_euclid(12);
            // Up by d, or down by 12 - d: the shorter way, down on a tie.
            if d * 2 < 12 { d } else { d - 12 }
        };
        let best = iv
            .iter()
            .map(dist)
            .min_by_key(|d| (d.abs(), *d > 0))
            .unwrap_or(0);
        u8::try_from((i32::from(note) + best).clamp(0, 127)).unwrap_or(127)
    }

    /// The note `steps` scale degrees above the scale note at or above
    /// `start`, climbing at most two octaves before it starts over, so a walk
    /// stays in range.
    pub fn walk(&self, start: u8, steps: u32) -> u8 {
        let iv = self.mode.intervals();
        let len = u32::try_from(iv.len()).unwrap_or(1).max(1);
        // The first degree at or above `start`, counted from the root's octave.
        let rel = (i32::from(start) - i32::from(self.root)).rem_euclid(12) as u8;
        let base = i32::from(start) - i32::from(rel);
        let first = iv.iter().position(|i| *i >= rel).unwrap_or(0);
        let top = if iv.iter().any(|i| *i >= rel) { 0 } else { len };
        let d = (u32::try_from(first).unwrap_or(0) + top + steps % (2 * len)) as usize;
        let (oct, deg) = (d / iv.len().max(1), d % iv.len().max(1));
        let n = base
            + 12 * i32::try_from(oct).unwrap_or(0)
            + i32::from(iv.get(deg).copied().unwrap_or(0));
        u8::try_from(n.clamp(0, 127)).unwrap_or(127)
    }
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

    fn hits(e: &Euclid) -> String {
        e.pattern()
            .iter()
            .map(|h| if *h { 'x' } else { '.' })
            .collect()
    }

    fn euclid(k: u32, n: u32, rot: u32) -> Euclid {
        Euclid { k, n, rot }
    }

    #[test]
    fn euclid_3_8() {
        let e = euclid(3, 8, 0);
        let at: Vec<usize> = (0..8).filter(|i| e.pattern()[*i]).collect();
        assert_eq!(at, [0, 3, 6]);
    }

    #[test]
    fn the_published_euclidean_rhythms() {
        assert_eq!(hits(&euclid(3, 8, 0)), "x..x..x.");
        assert_eq!(hits(&euclid(5, 8, 0)), "x.xx.xx.");
        assert_eq!(hits(&euclid(7, 16, 0)), "x..x.x.x..x.x.x.");
        assert_eq!(hits(&euclid(4, 16, 0)), "x...x...x...x...");
        assert_eq!(hits(&euclid(2, 5, 0)), "x.x..");
    }

    #[test]
    fn rotation_moves_the_pattern_left() {
        assert_eq!(hits(&euclid(3, 8, 1)), "..x..x.x");
        assert_eq!(hits(&euclid(3, 8, 8)), hits(&euclid(3, 8, 0)));
    }

    #[test]
    fn k_hits_in_n_steps_for_every_pair() {
        for n in 1..=MAX_PULSES {
            for k in 0..=n {
                let p = euclid(k, n, 0).pattern();
                assert_eq!(p.len(), n as usize);
                assert_eq!(p.iter().filter(|h| **h).count(), k as usize, "{k},{n}");
            }
        }
    }

    #[test]
    fn calls_parse_and_print() {
        assert_eq!(Euclid::parse("euclid(3,8)"), Some(Ok(euclid(3, 8, 0))));
        assert_eq!(Euclid::parse("euclid(3,8,10)"), Some(Ok(euclid(3, 8, 2))));
        assert_eq!(euclid(3, 8, 2).print(), "euclid(3,8,2)");
        assert_eq!(euclid(3, 8, 0).print(), "euclid(3,8)");
        assert_eq!(Euclid::parse("bd"), None);
        for bad in [
            "euclid(3)",
            "euclid(3,8",
            "euclid(9,8)",
            "euclid(1,0)",
            "euclid(1,65)",
            "euclid(a,8)",
            "euclid(1,2,3,4)",
            "euclid()",
        ] {
            assert!(matches!(Euclid::parse(bad), Some(Err(_))), "{bad}");
        }
    }

    #[test]
    fn a_scale_walk_climbs_the_scale() {
        let c_minor = Scale {
            root: 0,
            mode: Mode::Minor,
        };
        let walk: Vec<u8> = (0..8).map(|i| c_minor.walk(60, i)).collect();
        assert_eq!(walk, [60, 62, 63, 65, 67, 68, 70, 72]);
        // a start off the scale begins on the next scale note up
        assert_eq!(c_minor.walk(61, 0), 62);
        let a_pent = Scale {
            root: 9,
            mode: Mode::Pentatonic,
        };
        assert_eq!(a_pent.walk(60, 0), 61);
        let walk: Vec<u8> = (0..6).map(|i| a_pent.walk(69, i)).collect();
        assert_eq!(walk, [69, 71, 73, 76, 78, 81]);
        for i in 0..100 {
            assert!(c_minor.walk(120, i) <= 127);
        }
    }

    #[test]
    fn the_raised_modes_walk_their_raised_notes() {
        let e_phryg_dom = Scale {
            root: 4,
            mode: Mode::from_name("phrygian-dominant").unwrap(),
        };
        // e f g# a b c d e
        let walk: Vec<u8> = (0..8).map(|i| e_phryg_dom.walk(64, i)).collect();
        assert_eq!(walk, [64, 65, 68, 69, 71, 72, 74, 76]);
        let a_harm = Scale {
            root: 9,
            mode: Mode::from_name("harmonic-minor").unwrap(),
        };
        // a b c d e f g# a
        let walk: Vec<u8> = (0..8).map(|i| a_harm.walk(69, i)).collect();
        assert_eq!(walk, [69, 71, 72, 74, 76, 77, 80, 81]);
        assert_eq!(e_phryg_dom.mode.name(), "phrygian-dominant");
    }
}
