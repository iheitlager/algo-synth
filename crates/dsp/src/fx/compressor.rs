//! The master compressor (spec 002 Req 2): feed-forward and stereo-linked.
//!
//! The level is the peak of each `SUB`-sample chunk, and the gain it asks for
//! is worked out once per chunk, so there is no transcendental function per
//! sample (ADR-0002). The gain itself glides per sample, with the attack time
//! when it falls and the release time when it recovers.

use std::f32::consts::LN_10;

/// Samples per level measurement: 0.7 ms at 48 kHz.
const SUB: usize = 32;

pub struct Compressor {
    sample_rate: f32,
    /// Threshold as a linear level.
    threshold: f32,
    ratio: f32,
    attack: f32,
    release: f32,
    makeup: f32,
    /// The gain applied now, 1 for no reduction.
    gain: f32,
}

/// One-pole coefficient for a time constant of `ms` milliseconds.
fn coef(ms: f32, sample_rate: f32) -> f32 {
    1.0 - (-1.0 / (ms * 0.001 * sample_rate)).exp()
}

fn db_to_linear(db: f32) -> f32 {
    (db * LN_10 / 20.0).exp()
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Compressor {
        let mut c = Compressor {
            sample_rate,
            threshold: 1.0,
            ratio: 1.0,
            attack: 0.0,
            release: 0.0,
            makeup: 1.0,
            gain: 1.0,
        };
        c.set_threshold(-12.0);
        c.set_attack(10.0);
        c.set_release(120.0);
        c
    }

    /// The threshold in dB below full scale.
    pub fn set_threshold(&mut self, db: f32) {
        self.threshold = db_to_linear(db);
    }

    /// 1 leaves the signal alone, 20 is nearly a limiter.
    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = ratio.max(1.0);
    }

    pub fn set_attack(&mut self, ms: f32) {
        self.attack = coef(ms.max(0.01), self.sample_rate);
    }

    pub fn set_release(&mut self, ms: f32) {
        self.release = coef(ms.max(1.0), self.sample_rate);
    }

    pub fn set_makeup(&mut self, db: f32) {
        self.makeup = db_to_linear(db);
    }

    /// How much the signal is being turned down now, in dB (0 or more).
    pub fn gain_reduction_db(&self) -> f32 {
        -20.0 * self.gain.max(1.0e-6).log10()
    }

    /// Compress `left` and `right` in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.ratio <= 1.0 && self.makeup == 1.0 {
            self.gain = 1.0;
            return;
        }
        let slope = 1.0 / self.ratio - 1.0;
        for (l, r) in left.chunks_mut(SUB).zip(right.chunks_mut(SUB)) {
            let peak = l
                .iter()
                .chain(r.iter())
                .fold(0.0_f32, |m, x| m.max(x.abs()));
            let target = if peak > self.threshold {
                (peak / self.threshold).powf(slope)
            } else {
                1.0
            };
            for (l, r) in l.iter_mut().zip(r.iter_mut()) {
                let c = if target < self.gain {
                    self.attack
                } else {
                    self.release
                };
                self.gain += (target - self.gain) * c;
                *l *= self.gain * self.makeup;
                *r *= self.gain * self.makeup;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 48_000.0;

    fn sine(amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (TAU * 440.0 * i as f32 / SR).sin())
            .collect()
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0_f32, |m, v| m.max(v.abs()))
    }

    fn run(c: &mut Compressor, x: &[f32]) -> Vec<f32> {
        let (mut l, mut r) = (x.to_vec(), x.to_vec());
        for (l, r) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
            c.process(l, r);
        }
        l
    }

    fn settled(ratio: f32, amp: f32) -> f32 {
        let mut c = Compressor::new(SR);
        c.set_threshold(-20.0);
        c.set_ratio(ratio);
        c.set_attack(1.0);
        c.set_release(50.0);
        let y = run(&mut c, &sine(amp, 48_000));
        peak(&y[40_000..])
    }

    #[test]
    fn ratio_one_is_transparent() {
        let x = sine(0.9, 4_800);
        let mut c = Compressor::new(SR);
        assert!(run(&mut c, &x) == x);
    }

    #[test]
    fn quiet_signals_pass_unchanged() {
        let amp = 0.05; // −26 dB, under the −20 dB threshold
        assert!((settled(8.0, amp) - amp).abs() < 1.0e-4);
    }

    #[test]
    fn above_the_threshold_the_rise_is_divided_by_the_ratio() {
        // Threshold 0.1; input peaks 0.8 (+18 dB), at 4:1 the output is
        // 0.1 · 8^(1/4) = 0.168 (+4.5 dB over the threshold).
        let out = settled(4.0, 0.8);
        let want = 0.1 * 8.0_f32.powf(0.25);
        assert!((out - want).abs() / want < 0.1, "{out} vs {want}");
    }

    #[test]
    fn attack_and_release_follow_their_times() {
        let mut c = Compressor::new(SR);
        c.set_threshold(-20.0);
        c.set_ratio(20.0);
        c.set_attack(5.0);
        c.set_release(200.0);
        // A loud burst, then quiet again.
        let loud = run(&mut c, &sine(0.9, 4_800));
        let reduced = c.gain_reduction_db();
        assert!(reduced > 10.0, "reduces under load: {reduced} dB");
        // The peak is not yet fully caught in the first millisecond.
        assert!(peak(&loud[..48]) > peak(&loud[4_000..]));
        run(&mut c, &sine(0.01, 2_400));
        assert!(c.gain_reduction_db() > 0.5 * reduced, "release is slow");
        run(&mut c, &sine(0.01, 96_000));
        assert!(c.gain_reduction_db() < 0.5, "and gets there");
    }

    #[test]
    fn makeup_raises_the_level() {
        let mut c = Compressor::new(SR);
        c.set_makeup(6.0);
        let y = run(&mut c, &sine(0.1, 4_800));
        assert!((peak(&y) / 0.1 - 1.995).abs() < 0.01);
    }
}
