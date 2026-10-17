//! A phaser (#745), Strudel's `phaser`: six first-order all-passes whose
//! corner an LFO sweeps, mixed half and half with the dry signal, so notches
//! move up and down the spectrum. Feedback around the chain deepens them.
//!
//! The corner moves with the LFO, so the all-passes' coefficient is worked
//! out every `EVERY` samples, not every sample; the LFO's sine is a turning
//! phasor (`ring::Quad`). The right side's LFO runs a set part of a cycle
//! behind the left, which spreads a stereo bus.

use super::ring::Quad;
use std::f32::consts::PI;

const STAGES: usize = 6;
/// Samples between two updates of the coefficient.
const EVERY: usize = 16;
/// How far the sweep reaches each way at full depth, in octaves.
const OCTAVES: f32 = 2.0;

pub struct Phaser {
    sample_rate: f32,
    lfo: Quad,
    centre: f32,
    depth: f32,
    feedback: f32,
    /// The right side's LFO as an angle behind the left's: its cos and sin.
    spread: (f32, f32),
    /// Each side's all-pass states, coefficient and last output.
    stages: [[f32; STAGES]; 2],
    coef: [f32; 2],
    last: [f32; 2],
    /// Samples until the next coefficient.
    count: usize,
}

impl Phaser {
    pub fn new(sample_rate: f32) -> Phaser {
        let mut p = Phaser {
            sample_rate,
            lfo: Quad::new(),
            centre: 1_000.0,
            depth: 0.5,
            feedback: 0.0,
            spread: (1.0, 0.0),
            stages: [[0.0; STAGES]; 2],
            coef: [0.0; 2],
            last: [0.0; 2],
            count: 0,
        };
        p.set([0.5, 0.5, 0.5, 0.5, 0.0]);
        p
    }

    /// The knobs: A rate (0.05 to 10 Hz), B depth (0 to ±2 octaves), C
    /// feedback (0 to 0.9), D centre (200 Hz to 4 kHz), E the right side's
    /// LFO behind the left's (0 to half a cycle).
    pub fn set(&mut self, [a, b, c, d, e]: [f32; 5]) {
        self.lfo
            .set(0.05 * 200.0_f32.powf(a.clamp(0.0, 1.0)), self.sample_rate);
        self.depth = b.clamp(0.0, 1.0);
        self.feedback = 0.9 * c.clamp(0.0, 1.0);
        self.centre = 200.0 * 20.0_f32.powf(d.clamp(0.0, 1.0));
        let (s, c) = (PI * e.clamp(0.0, 1.0)).sin_cos();
        self.spread = (c, s);
        self.count = 0;
    }

    pub fn reset(&mut self) {
        self.lfo.reset();
        self.stages = [[0.0; STAGES]; 2];
        self.last = [0.0; 2];
        self.count = 0;
    }

    /// The all-pass coefficient for an LFO value of `lfo`, −1 to 1.
    fn coef_at(&self, lfo: f32) -> f32 {
        let hz =
            (self.centre * 2.0_f32.powf(OCTAVES * self.depth * lfo)).min(0.45 * self.sample_rate);
        let t = (PI * hz / self.sample_rate).tan();
        (t - 1.0) / (t + 1.0)
    }

    /// Every `EVERY` samples, each side's coefficient from the LFO.
    fn step(&mut self) {
        if self.count == 0 {
            let (c, s) = self.spread;
            let right = self.lfo.sin * c - self.lfo.cos * s;
            self.coef = [self.coef_at(self.lfo.sin), self.coef_at(right)];
            self.count = EVERY;
        }
        self.count -= 1;
        self.lfo.tick();
    }

    fn sample(&mut self, ch: usize, x: f32) -> f32 {
        let (Some(stages), Some(&a), Some(last)) = (
            self.stages.get_mut(ch),
            self.coef.get(ch),
            self.last.get_mut(ch),
        ) else {
            return x;
        };
        let mut v = x + self.feedback * *last;
        for s in stages.iter_mut() {
            let y = a * v + *s;
            *s = v - a * y;
            v = y;
        }
        *last = v;
        0.5 * (x + v)
    }

    pub fn process_mono(&mut self, x: &mut [f32]) {
        for s in x.iter_mut() {
            self.step();
            *s = self.sample(0, *s);
        }
        self.lfo.renorm();
    }

    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.step();
            *l = self.sample(0, *l);
            *r = self.sample(1, *r);
        }
        self.lfo.renorm();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 48_000.0;

    #[test]
    fn a_tone_at_the_notch_comes_and_goes_with_the_lfo() {
        // A slow sweep round 1 kHz: a 1 kHz tone dips and comes back.
        let mut p = Phaser::new(SR);
        p.set([0.3, 1.0, 0.5, 0.5, 0.0]);
        let mut y: Vec<f32> = (0..96_000)
            .map(|i| 0.5 * (TAU * 1_000.0 * i as f32 / SR).sin())
            .collect();
        for c in y.chunks_mut(128) {
            p.process_mono(c);
        }
        let peaks: Vec<f32> = y
            .chunks(960)
            .map(|c| c.iter().fold(0.0_f32, |m, s| m.max(s.abs())))
            .collect();
        let (lo, hi) = peaks
            .iter()
            .skip(5)
            .fold((f32::MAX, 0.0_f32), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        assert!(hi > 0.4 && lo < 0.2, "the level moves: {lo} to {hi}");
    }

    #[test]
    fn silence_stays_silent_and_every_setting_is_bounded() {
        let mut p = Phaser::new(SR);
        let mut z = [0.0_f32; 512];
        p.process_mono(&mut z);
        assert!(z.iter().all(|s| *s == 0.0));
        for a in [0.0, 1.0] {
            for c in [0.0, 1.0] {
                for d in [0.0, 1.0] {
                    let mut p = Phaser::new(SR);
                    p.set([a, 1.0, c, d, 1.0]);
                    let (mut l, mut r): (Vec<f32>, Vec<f32>) = (0..9_600)
                        .map(|i| {
                            if i % 53 < 26 {
                                (0.9, -0.9)
                            } else {
                                (-0.9, 0.9)
                            }
                        })
                        .unzip();
                    for (l, r) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
                        p.process_stereo(l, r);
                    }
                    assert!(l.iter().chain(&r).all(|s| s.is_finite() && s.abs() < 10.0));
                }
            }
        }
    }
}
