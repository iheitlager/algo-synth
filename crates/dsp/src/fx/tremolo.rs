//! A tremolo and auto-pan (#745), Strudel's `tremolo`: the level moved by an
//! LFO, from a sine towards a square as knob C turns. On a stereo bus the
//! right side's LFO runs a set part of a cycle behind the left's, so at half
//! a cycle the sound moves from side to side.
//!
//! The LFO's sine is a turning phasor (`ring::Quad`): no sine is computed per
//! sample.

use super::ring::Quad;
use std::f32::consts::PI;

pub struct Tremolo {
    sample_rate: f32,
    lfo: Quad,
    depth: f32,
    /// How hard the sine is pushed towards a square: 1 a sine.
    drive: f32,
    /// The right side's LFO as an angle behind the left's: its cos and sin.
    spread: (f32, f32),
}

impl Tremolo {
    pub fn new(sample_rate: f32) -> Tremolo {
        let mut t = Tremolo {
            sample_rate,
            lfo: Quad::new(),
            depth: 0.5,
            drive: 1.0,
            spread: (1.0, 0.0),
        };
        t.set([0.5, 0.5, 0.5, 0.5, 0.0]);
        t
    }

    /// The knobs: A rate (0.1 to 20 Hz), B depth (0 to all the way down), C
    /// shape (a sine to a square), D the right side's LFO behind the left's
    /// (0 to half a cycle: auto-pan); E is unused.
    pub fn set(&mut self, [a, b, c, d, _]: [f32; 5]) {
        self.lfo
            .set(0.1 * 200.0_f32.powf(a.clamp(0.0, 1.0)), self.sample_rate);
        self.depth = b.clamp(0.0, 1.0);
        self.drive = 1.0 + 19.0 * c.clamp(0.0, 1.0);
        let (s, c) = (PI * d.clamp(0.0, 1.0)).sin_cos();
        self.spread = (c, s);
    }

    pub fn reset(&mut self) {
        self.lfo.reset();
    }

    /// The gain for an LFO value of `lfo`, −1 to 1: 1 at the top, 1 − depth
    /// at the bottom.
    fn gain(&self, lfo: f32) -> f32 {
        let shaped = (lfo * self.drive).clamp(-1.0, 1.0);
        1.0 - self.depth * (1.0 - shaped) * 0.5
    }

    pub fn process_mono(&mut self, x: &mut [f32]) {
        for s in x.iter_mut() {
            *s *= self.gain(self.lfo.sin);
            self.lfo.tick();
        }
        self.lfo.renorm();
    }

    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let (c, s) = self.spread;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let behind = self.lfo.sin * c - self.lfo.cos * s;
            *l *= self.gain(self.lfo.sin);
            *r *= self.gain(behind);
            self.lfo.tick();
        }
        self.lfo.renorm();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn the_level_follows_the_rate_and_depth() {
        // 4 Hz (A = log200(40)), full depth: the level falls to nothing and
        // comes back four times a second.
        let a = 40.0_f32.ln() / 200.0_f32.ln();
        let mut t = Tremolo::new(SR);
        t.set([a, 1.0, 0.0, 0.0, 0.0]);
        let mut y = vec![0.5_f32; 48_000];
        for c in y.chunks_mut(128) {
            t.process_mono(c);
        }
        let dips = y.windows(2).filter(|w| w[0] >= 0.01 && w[1] < 0.01).count();
        assert_eq!(dips, 4, "four dips a second");
        assert!(y.iter().all(|s| (0.0..=0.5 + 1e-4).contains(s)));
    }

    #[test]
    fn half_a_cycle_apart_the_sides_pan() {
        let mut t = Tremolo::new(SR);
        t.set([0.5, 1.0, 0.0, 1.0, 0.0]);
        let (mut l, mut r) = (vec![0.5_f32; 9_600], vec![0.5_f32; 9_600]);
        for (l, r) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
            t.process_stereo(l, r);
        }
        // The two gains add to a constant: one up while the other is down.
        assert!(l.iter().zip(&r).all(|(a, b)| (a + b - 0.5).abs() < 1e-3));
    }

    #[test]
    fn no_depth_leaves_the_signal_alone() {
        let mut t = Tremolo::new(SR);
        t.set([0.7, 0.0, 0.5, 0.5, 0.0]);
        let x: Vec<f32> = (0..1_024).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut y = x.clone();
        t.process_mono(&mut y);
        assert!(y == x);
    }
}
