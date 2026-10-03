//! The chorus (spec 002 Req 2): two delay taps per side, a few milliseconds
//! long, swept in opposite directions by a slow LFO. It returns only the
//! delayed signal; the dry signal is already in the mix. Stereo in, stereo out.

use super::{Delay, lfo, lowpass};

/// The longest base delay, in seconds.
const MAX_DELAY: f32 = 0.04;

pub struct Chorus {
    left: Delay,
    right: Delay,
    sample_rate: f32,
    phase: f32,
    /// LFO increment per sample, in cycles.
    inc: f32,
    /// The base delay and the sweep around it, in samples.
    base: f32,
    depth: f32,
    /// How far the right side's LFO runs ahead of the left's, in cycles.
    spread: f32,
    tone: f32,
    lp: [f32; 2],
}

impl Chorus {
    pub fn new(sample_rate: f32) -> Chorus {
        let max = (MAX_DELAY * sample_rate) as usize + 2;
        let mut c = Chorus {
            left: Delay::new(max),
            right: Delay::new(max),
            sample_rate,
            phase: 0.0,
            inc: 0.0,
            base: 0.0,
            depth: 0.0,
            spread: 0.0,
            tone: 1.0,
            lp: [0.0; 2],
        };
        c.set_rate(1.0);
        c.set_delay(15.0);
        c.set_depth(0.5);
        c.set_tone(1.0);
        c
    }

    /// The LFO rate in Hz.
    pub fn set_rate(&mut self, hz: f32) {
        self.inc = hz / self.sample_rate;
    }

    /// The base delay in ms, 1..=40.
    pub fn set_delay(&mut self, ms: f32) {
        self.base = (ms * 0.001 * self.sample_rate).clamp(2.0, MAX_DELAY * self.sample_rate);
    }

    /// 0..=1 sweeps up to half the base delay either way.
    pub fn set_depth(&mut self, depth: f32) {
        self.depth = depth.clamp(0.0, 1.0);
    }

    /// 0..=1 is 0 to half a cycle between the sides.
    pub fn set_spread(&mut self, spread: f32) {
        self.spread = spread.clamp(0.0, 1.0) * 0.5;
    }

    /// 0..=1 puts the low-pass on the output from 500 Hz to 20 kHz.
    pub fn set_tone(&mut self, tone: f32) {
        self.tone = lowpass(500.0 * 40.0_f32.powf(tone), self.sample_rate);
    }

    /// Forget what the delays hold.
    pub fn clear(&mut self) {
        self.left.clear();
        self.right.clear();
        self.lp = [0.0; 2];
    }

    /// Feed `in_l` and `in_r` in and add the delayed signal into `out_l` and `out_r`.
    pub fn process(&mut self, in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        let sweep = self.depth * 0.5 * self.base;
        let frames = in_l
            .iter()
            .zip(in_r)
            .zip(out_l.iter_mut().zip(out_r.iter_mut()));
        for ((xl, xr), (ol, or)) in frames {
            self.left.write(*xl);
            self.right.write(*xr);
            let (a, b) = (lfo(self.phase), lfo(self.phase + self.spread));
            let yl = 0.5
                * (self.left.read(self.base + sweep * a) + self.left.read(self.base - sweep * a));
            let yr = 0.5
                * (self.right.read(self.base + sweep * b) + self.right.read(self.base - sweep * b));
            let [fl, fr] = &mut self.lp;
            *fl += self.tone * (yl - *fl);
            *fr += self.tone * (yr - *fr);
            *ol += *fl;
            *or += *fr;
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
    use std::f32::consts::TAU;

    const SR: f32 = 48_000.0;

    fn tone(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.5 * (TAU * hz * i as f32 / SR).sin())
            .collect()
    }

    fn run(c: &mut Chorus, l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let (mut ol, mut or) = (vec![0.0; l.len()], vec![0.0; l.len()]);
        for (((a, b), x), y) in l
            .chunks(128)
            .zip(r.chunks(128))
            .zip(ol.chunks_mut(128))
            .zip(or.chunks_mut(128))
        {
            c.process(a, b, x, y);
        }
        (ol, or)
    }

    /// Magnitude of the `hz` component of `x`.
    fn goertzel(x: &[f32], hz: f32) -> f32 {
        let w = TAU * hz / SR;
        let (mut re, mut im) = (0.0_f64, 0.0_f64);
        for (i, v) in x.iter().enumerate() {
            re += f64::from(*v) * f64::from((w * i as f32).cos());
            im += f64::from(*v) * f64::from((w * i as f32).sin());
        }
        (re.hypot(im) * 2.0 / x.len() as f64) as f32
    }

    #[test]
    fn sweeping_the_delay_puts_sidebands_at_twice_the_rate() {
        let x = tone(1_000.0, 96_000);
        let sidebands = |depth: f32| {
            let mut c = Chorus::new(SR);
            c.set_rate(4.0);
            c.set_depth(depth);
            let (l, _) = run(&mut c, &x, &x);
            let tail = &l[48_000..];
            // The two taps sweep in opposite directions, so the first sidebands cancel
            // and what is left is at twice the rate.
            (goertzel(tail, 1_008.0) + goertzel(tail, 992.0)) / goertzel(tail, 1_000.0)
        };
        assert!(sidebands(1.0) > 0.05, "swept: {}", sidebands(1.0));
        assert!(sidebands(0.0) < 0.005, "still: {}", sidebands(0.0));
    }

    #[test]
    fn spread_decorrelates_the_sides() {
        let x = tone(500.0, 48_000);
        let mut c = Chorus::new(SR);
        c.set_rate(2.0);
        c.set_depth(1.0);
        c.set_spread(0.0);
        let (l, r) = run(&mut c, &x, &x);
        assert!(l == r, "no spread, the same on both sides");
        let mut c = Chorus::new(SR);
        c.set_rate(2.0);
        c.set_depth(1.0);
        c.set_spread(1.0);
        let (l, r) = run(&mut c, &x, &x);
        assert!(l != r);
    }

    #[test]
    fn is_finite_bounded_and_silent_for_silence() {
        let mut c = Chorus::new(SR);
        c.set_depth(1.0);
        let loud: Vec<f32> = (0..96_000)
            .map(|i| if (i / 5) % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let (l, r) = run(&mut c, &loud, &loud);
        assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 1.01));
        let mut c = Chorus::new(SR);
        let z = vec![0.0; 9_600];
        let (l, r) = run(&mut c, &z, &z);
        assert!(l.iter().chain(&r).all(|x| *x == 0.0));
    }

    #[test]
    fn has_no_dc_from_an_ac_input_and_clears() {
        let mut c = Chorus::new(SR);
        let (l, _) = run(&mut c, &tone(300.0, 96_000), &tone(300.0, 96_000));
        let mean = l[48_000..].iter().sum::<f32>() / 48_000.0;
        assert!(mean.abs() < 0.01, "dc {mean}");
        c.clear();
        let z = vec![0.0; 4_800];
        let (l, _) = run(&mut c, &z, &z);
        assert!(l.iter().all(|x| *x == 0.0), "nothing left after a clear");
    }
}
