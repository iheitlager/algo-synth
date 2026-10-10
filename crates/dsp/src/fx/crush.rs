//! A bitcrusher (#470), the 8-bit sound: a sample-and-hold at a lower rate,
//! stair-stepping and aliasing the signal, then a quantizer to fewer bits.
//! Both channels share one clock, so a stereo bus stays in step.
//!
//! The quantizer rounds to the nearest step, so silence stays silent; at
//! 1 bit a signal within ±1 takes three levels, −1, 0 and +1. At 16 bits it
//! does not quantize and at the full rate it holds nothing, so the top of
//! both knobs passes the signal through.

/// The lowest rate the hold runs at, in Hz.
const LOW_RATE: f32 = 500.0;
/// The highest, in Hz: at or above the sample rate nothing is held.
const HIGH_RATE: f32 = 48_000.0;

pub struct Crush {
    sample_rate: f32,
    /// How far through one held sample the clock is; at 1 it takes the next.
    phase: f32,
    /// The clock's step per sample: the hold's rate over the sample rate.
    step: f32,
    /// Steps per unit, 2^(bits − 1); 0 does not quantize.
    q: f32,
    gain: f32,
    dry: f32,
    held: [f32; 2],
}

impl Crush {
    pub fn new(sample_rate: f32) -> Crush {
        let mut c = Crush {
            sample_rate,
            phase: 1.0,
            step: 1.0,
            q: 0.0,
            gain: 1.0,
            dry: 0.0,
            held: [0.0; 2],
        };
        c.set([0.5, 0.5, 0.5, 0.5, 0.0]);
        c
    }

    /// The knobs: A bits (1–16), B rate (500 Hz–48 kHz), C level (2·C, unity
    /// at 0.5), E dry; D is unused.
    pub fn set(&mut self, [a, b, c, _, e]: [f32; 5]) {
        let bits = 1.0 + 15.0 * a.clamp(0.0, 1.0);
        self.q = if bits >= 16.0 {
            0.0
        } else {
            2.0_f32.powf(bits - 1.0)
        };
        let rate = LOW_RATE * (HIGH_RATE / LOW_RATE).powf(b.clamp(0.0, 1.0));
        self.step = (rate / self.sample_rate).min(1.0);
        self.gain = 2.0 * c.clamp(0.0, 1.0);
        self.dry = e.clamp(0.0, 1.0);
    }

    pub fn reset(&mut self) {
        self.phase = 1.0;
        self.held = [0.0; 2];
    }

    /// Whether the knobs leave the signal as it is.
    fn transparent(&self) -> bool {
        self.q == 0.0 && self.step >= 1.0 && self.gain == 1.0
    }

    /// Advance the clock one sample; true when a new value is taken.
    fn tick(&mut self) -> bool {
        let take = self.phase >= 1.0;
        if take {
            self.phase -= 1.0;
        }
        self.phase += self.step;
        take
    }

    fn shape(&self, held: f32, x: f32) -> f32 {
        let y = if self.q > 0.0 {
            (held * self.q).round() / self.q
        } else {
            held
        };
        (y * (1.0 - self.dry) + x * self.dry) * self.gain
    }

    pub fn process_mono(&mut self, x: &mut [f32]) {
        if self.transparent() {
            return;
        }
        for s in x {
            if self.tick() {
                self.held[0] = *s;
            }
            *s = self.shape(self.held[0], *s);
        }
    }

    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.transparent() {
            return;
        }
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.tick() {
                self.held = [*l, *r];
            }
            *l = self.shape(self.held[0], *l);
            *r = self.shape(self.held[1], *r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 48_000.0;

    fn sine(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.8 * (TAU * 220.0 * i as f32 / SR).sin())
            .collect()
    }

    fn crushed(knobs: [f32; 5]) -> Vec<f32> {
        let mut c = Crush::new(SR);
        c.set(knobs);
        let mut y = sine(4_800);
        for chunk in y.chunks_mut(128) {
            c.process_mono(chunk);
        }
        y
    }

    #[test]
    fn full_bits_and_rate_pass_the_signal_bit_exact() {
        assert!(crushed([1.0, 1.0, 0.5, 0.0, 0.0]) == sine(4_800));
    }

    #[test]
    fn one_bit_leaves_three_levels_and_silence_silent() {
        let y = crushed([0.0, 1.0, 0.5, 0.0, 0.0]);
        assert!(y.iter().all(|v| [-1.0, 0.0, 1.0].contains(v)), "levels");
        assert!(y.contains(&1.0) && y.contains(&-1.0));
        let mut c = Crush::new(SR);
        c.set([0.0, 0.0, 0.5, 0.0, 0.0]);
        let mut quiet = [0.0; 256];
        c.process_mono(&mut quiet);
        assert!(quiet.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn a_low_rate_holds_each_value() {
        // B = 0 holds at 500 Hz: 96 samples a value at 48 kHz.
        let y = crushed([1.0, 0.0, 0.5, 0.0, 0.0]);
        let mut runs = vec![];
        let mut run = 1;
        for w in y.windows(2) {
            if w[0] == w[1] {
                run += 1;
            } else {
                runs.push(run);
                run = 1;
            }
        }
        assert!(runs.iter().all(|r| (95..=97).contains(r)), "{runs:?}");
    }

    #[test]
    fn every_knob_setting_stays_finite_and_bounded() {
        for a in [0.0, 0.3, 0.7, 1.0] {
            for b in [0.0, 0.5, 1.0] {
                for e in [0.0, 1.0] {
                    let y = crushed([a, b, 1.0, 0.0, e]);
                    assert!(y.iter().all(|v| v.is_finite() && v.abs() <= 2.0));
                }
            }
        }
    }

    #[test]
    fn the_sides_of_a_stereo_bus_match_the_mono_path() {
        let knobs = [0.3, 0.4, 0.6, 0.0, 0.2];
        let mono = crushed(knobs);
        let mut c = Crush::new(SR);
        c.set(knobs);
        let (mut l, mut r) = (sine(4_800), sine(4_800));
        for (l, r) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
            c.process_stereo(l, r);
        }
        assert!(l == mono && r == mono);
    }
}
