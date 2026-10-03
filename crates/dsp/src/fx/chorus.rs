//! The stereo chorus of the Juno-106 and the Solina ensemble family (spec 006
//! Req 7): a mono signal in, two modulated delays out, their triangle LFOs in
//! opposite phase, mixed with the dry signal.
//!
//! Modes I and II are a slow, deep sweep; I+II together a fast, shallow one that
//! is closer to a vibrato. The delays are read with linear interpolation from a
//! short ring, the LFO is arithmetic (no `sin` per sample, ADR-0002), and the
//! wet path passes a one-pole low-pass for the dull top of a bucket-brigade line.
//! Nothing allocates after `Chorus::new`.

/// Ring length in samples, a power of two: room for the deepest sweep at 96 kHz.
const RING: usize = 1024;
const MASK: usize = RING - 1;
/// The wet path's low-pass corner, a bucket-brigade line's dull top.
const WET_HZ: f32 = 9_000.0;

/// A chorus mode: its LFO rate in Hz, how far the delay sweeps and where it centres, in ms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mode {
    pub rate: f32,
    pub depth_ms: f32,
    pub centre_ms: f32,
}

/// Off, I, II, and I plus II (ids 0..=3, as `Param::ChorusMode`).
pub const MODES: [Option<Mode>; 4] = [
    None,
    Some(Mode {
        rate: 0.5,
        depth_ms: 1.2,
        centre_ms: 3.4,
    }),
    Some(Mode {
        rate: 0.83,
        depth_ms: 1.2,
        centre_ms: 3.4,
    }),
    Some(Mode {
        rate: 9.75,
        depth_ms: 0.18,
        centre_ms: 3.4,
    }),
];

pub struct Chorus {
    sample_rate: f32,
    left: Vec<f32>,
    right: Vec<f32>,
    write: usize,
    /// LFO phase 0..1, and its step per sample.
    phase: f32,
    inc: f32,
    /// Delay centre and sweep in samples.
    centre: f32,
    depth: f32,
    mode: usize,
    /// The wet paths' low-pass state and coefficient.
    wet: [f32; 2],
    coef: f32,
}

impl Chorus {
    pub fn new(sample_rate: f32) -> Chorus {
        Chorus {
            sample_rate,
            left: vec![0.0; RING],
            right: vec![0.0; RING],
            write: 0,
            phase: 0.0,
            inc: 0.0,
            centre: 0.0,
            depth: 0.0,
            mode: 0,
            wet: [0.0; 2],
            coef: 1.0 - (-std::f32::consts::TAU * WET_HZ / sample_rate).exp(),
        }
    }

    /// Choose a mode by its id; an unknown id is off.
    pub fn set_mode(&mut self, id: usize) {
        self.mode = if MODES.get(id).copied().flatten().is_some() {
            id
        } else {
            0
        };
        if let Some(Some(m)) = MODES.get(self.mode) {
            self.inc = m.rate / self.sample_rate;
            self.centre = m.centre_ms * 1.0e-3 * self.sample_rate;
            self.depth = m.depth_ms * 1.0e-3 * self.sample_rate;
        }
    }

    /// Whether it changes the signal.
    pub fn on(&self) -> bool {
        self.mode != 0
    }

    /// Read a delay of `d` samples behind the write head, interpolating.
    fn tap(line: &[f32], write: usize, d: f32) -> f32 {
        let d = d.clamp(1.0, (RING - 2) as f32);
        let whole = d as usize;
        let frac = d - whole as f32;
        let a = line
            .get(write.wrapping_sub(whole) & MASK)
            .copied()
            .unwrap_or(0.0);
        let b = line
            .get(write.wrapping_sub(whole + 1) & MASK)
            .copied()
            .unwrap_or(0.0);
        a + (b - a) * frac
    }

    /// Chorus `dry` into `left` and `right` (all the same length). Off copies it to both.
    pub fn process(&mut self, dry: &[f32], left: &mut [f32], right: &mut [f32]) {
        if !self.on() {
            for ((x, l), r) in dry.iter().zip(left.iter_mut()).zip(right.iter_mut()) {
                *l = *x;
                *r = *x;
            }
            return;
        }
        for ((x, l), r) in dry.iter().zip(left.iter_mut()).zip(right.iter_mut()) {
            if let Some(s) = self.left.get_mut(self.write) {
                *s = *x;
            }
            if let Some(s) = self.right.get_mut(self.write) {
                *s = *x;
            }
            // A triangle in −1..1; the two sides sweep in opposite directions.
            let tri = 4.0 * (self.phase - 0.5).abs() - 1.0;
            let wl = Chorus::tap(&self.left, self.write, self.centre + self.depth * tri);
            let wr = Chorus::tap(&self.right, self.write, self.centre - self.depth * tri);
            let [pl, pr] = &mut self.wet;
            *pl += self.coef * (wl - *pl);
            *pr += self.coef * (wr - *pr);
            // Equal power between the dry signal and the wet one.
            *l = (*x + *pl) * std::f32::consts::FRAC_1_SQRT_2;
            *r = (*x + *pr) * std::f32::consts::FRAC_1_SQRT_2;
            self.write = (self.write + 1) & MASK;
            self.phase += self.inc;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn tone(n: usize, hz: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (std::f32::consts::TAU * hz * i as f32 / SR).sin() * 0.5)
            .collect()
    }

    fn run(mode: usize, dry: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let mut c = Chorus::new(SR);
        c.set_mode(mode);
        let (mut l, mut r) = (vec![0.0; dry.len()], vec![0.0; dry.len()]);
        c.process(dry, &mut l, &mut r);
        (l, r)
    }

    /// Spec 006 Req 7: off is the same on both sides; the chorus makes them differ.
    #[test]
    fn off_is_identical_on_both_sides_and_the_chorus_widens() {
        let dry = tone(48_000, 220.0);
        let (l, r) = run(0, &dry);
        assert_eq!(l, dry);
        assert_eq!(r, dry);
        for mode in 1..=3 {
            let (l, r) = run(mode, &dry);
            let diff: f32 = l
                .iter()
                .zip(&r)
                .skip(4_800)
                .map(|(a, b)| (a - b).abs())
                .sum();
            assert!(diff > 50.0, "mode {mode} differs between the sides: {diff}");
        }
    }

    #[test]
    fn every_mode_stays_bounded_and_keeps_the_level() {
        let dry = tone(96_000, 330.0);
        for mode in 1..=3 {
            let (l, r) = run(mode, &dry);
            assert!(
                l.iter().chain(&r).all(|s| s.is_finite() && s.abs() <= 1.0),
                "mode {mode}"
            );
            let rms = |x: &[f32]| {
                (x.iter()
                    .skip(9_600)
                    .map(|s| f64::from(*s).powi(2))
                    .sum::<f64>()
                    / (x.len() - 9_600) as f64)
                    .sqrt()
            };
            let ratio = rms(&l) / rms(&dry);
            // A tone and its delayed copy add between silence and sqrt 2 times the dry level.
            assert!((0.3..=1.42).contains(&ratio), "mode {mode} level {ratio}");
        }
    }

    #[test]
    fn the_modes_sweep_at_their_own_rates() {
        // The left channel's instantaneous delay crosses its centre twice a cycle:
        // count sign changes of (left - dry) energy envelope is fragile, so check the LFO directly.
        let mut c = Chorus::new(SR);
        c.set_mode(2);
        let before = c.inc;
        c.set_mode(3);
        assert!(c.inc > 10.0 * before, "I+II is much faster");
        c.set_mode(1);
        assert!(c.inc < before, "I is slower than II");
        c.set_mode(9);
        assert!(!c.on(), "an unknown mode is off");
    }
}
