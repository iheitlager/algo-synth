//! The flanger (spec 002 Req 2): a very short delay swept by a slow LFO with
//! feedback of either sign, which moves the notches of a comb filter. It
//! returns only the delayed signal; the dry signal is already in the mix.
//! Stereo in, stereo out, the right side a quarter cycle behind the left.

use super::{Delay, lfo, lowpass};

/// The longest delay, in seconds.
const MAX_DELAY: f32 = 0.02;
/// Feedback stays within this either way, so the loop always decays.
pub const MAX_FEEDBACK: f32 = 0.95;

pub struct Flanger {
    left: Delay,
    right: Delay,
    sample_rate: f32,
    phase: f32,
    inc: f32,
    /// The delay at the middle of the sweep, and the sweep either way, in samples.
    manual: f32,
    depth: f32,
    feedback: f32,
    tone: f32,
    lp: [f32; 2],
}

impl Flanger {
    pub fn new(sample_rate: f32) -> Flanger {
        let max = (MAX_DELAY * sample_rate) as usize + 2;
        let mut f = Flanger {
            left: Delay::new(max),
            right: Delay::new(max),
            sample_rate,
            phase: 0.0,
            inc: 0.0,
            manual: 0.0,
            depth: 0.0,
            feedback: 0.0,
            tone: 1.0,
            lp: [0.0; 2],
        };
        f.set_rate(0.3);
        f.set_manual(2.0);
        f.set_depth(0.7);
        f.set_tone(1.0);
        f
    }

    /// The LFO rate in Hz.
    pub fn set_rate(&mut self, hz: f32) {
        self.inc = hz / self.sample_rate;
    }

    /// The middle delay in ms, 0.5..=10.
    pub fn set_manual(&mut self, ms: f32) {
        self.manual = (ms * 0.001 * self.sample_rate).clamp(2.0, MAX_DELAY * self.sample_rate);
    }

    /// 0..=1 sweeps up to 90% of the middle delay either way.
    pub fn set_depth(&mut self, depth: f32) {
        self.depth = depth.clamp(0.0, 1.0);
    }

    /// −0.95..=0.95, clamped; a negative value moves the notches to the odd harmonics.
    pub fn set_feedback(&mut self, fb: f32) {
        self.feedback = fb.clamp(-MAX_FEEDBACK, MAX_FEEDBACK);
    }

    /// 0..=1 puts the low-pass in the loop from 500 Hz to 20 kHz.
    pub fn set_tone(&mut self, tone: f32) {
        self.tone = lowpass(500.0 * 40.0_f32.powf(tone), self.sample_rate);
    }

    /// Forget what the loop holds.
    pub fn clear(&mut self) {
        self.left.clear();
        self.right.clear();
        self.lp = [0.0; 2];
    }

    /// Feed `in_l` and `in_r` in and add the delayed signal into `out_l` and `out_r`.
    pub fn process(&mut self, in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        let sweep = self.depth * 0.9 * self.manual;
        let frames = in_l
            .iter()
            .zip(in_r)
            .zip(out_l.iter_mut().zip(out_r.iter_mut()));
        for ((xl, xr), (ol, or)) in frames {
            let dl = (self.manual + sweep * lfo(self.phase)).max(1.0);
            let dr = (self.manual + sweep * lfo(self.phase - 0.25)).max(1.0);
            let yl = self.left.read(dl);
            let yr = self.right.read(dr);
            let [fl, fr] = &mut self.lp;
            *fl += self.tone * (yl - *fl);
            *fr += self.tone * (yr - *fr);
            self.left.write(*xl + self.feedback * *fl);
            self.right.write(*xr + self.feedback * *fr);
            *ol += yl;
            *or += yr;
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

    fn run(f: &mut Flanger, x: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (vec![0.0; x.len()], vec![0.0; x.len()]);
        for ((a, o), p) in x.chunks(128).zip(l.chunks_mut(128)).zip(r.chunks_mut(128)) {
            f.process(a, a, o, p);
        }
        (l, r)
    }

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
    fn the_sweep_puts_sidebands_at_the_rate() {
        let x = tone(1_000.0, 96_000);
        let sidebands = |depth: f32| {
            let mut f = Flanger::new(SR);
            f.set_rate(4.0);
            f.set_depth(depth);
            let (l, _) = run(&mut f, &x);
            let tail = &l[48_000..];
            (goertzel(tail, 1_004.0) + goertzel(tail, 996.0)) / goertzel(tail, 1_000.0)
        };
        assert!(sidebands(1.0) > 0.05, "swept: {}", sidebands(1.0));
        assert!(sidebands(0.0) < 0.005, "still: {}", sidebands(0.0));
    }

    #[test]
    fn feedback_makes_the_comb_deeper_and_negative_moves_it() {
        // A fixed 1 ms delay: with the dry added, the comb has a null at 500 Hz.
        let level = |fb: f32, hz: f32| {
            let mut f = Flanger::new(SR);
            f.set_depth(0.0);
            f.set_manual(1.0);
            f.set_feedback(fb);
            let x = tone(hz, 48_000);
            let (l, _) = run(&mut f, &x);
            let sum: Vec<f32> = l.iter().zip(&x).map(|(w, d)| w + d).collect();
            goertzel(&sum[24_000..], hz)
        };
        // At 500 Hz the 1 ms tap is in antiphase: the dry and wet cancel.
        assert!(level(0.0, 500.0) < 0.05);
        // At 1 kHz they add.
        assert!(level(0.0, 1_000.0) > 0.9);
        // Positive feedback peaks the comb higher than none.
        assert!(level(0.8, 1_000.0) > level(0.0, 1_000.0) * 1.5);
        // Negative feedback moves the null: 1 kHz now cancels more than 500 Hz.
        assert!(level(-0.8, 1_000.0) < level(0.0, 1_000.0));
    }

    #[test]
    fn feedback_of_either_sign_stays_bounded() {
        let loud: Vec<f32> = (0..192_000)
            .map(|i| if (i / 7) % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        for fb in [5.0, -5.0, 0.95, -0.95] {
            let mut f = Flanger::new(SR);
            f.set_feedback(fb);
            f.set_depth(1.0);
            let (l, r) = run(&mut f, &loud);
            assert!(
                l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 40.0),
                "fb {fb}"
            );
            let (l, _) = run(&mut f, &vec![0.0; 960_000]);
            assert!(
                l[950_000..].iter().all(|x| x.abs() < 1.0e-3),
                "fb {fb} decays"
            );
        }
    }

    #[test]
    fn is_silent_for_silence_and_clears() {
        let mut f = Flanger::new(SR);
        f.set_feedback(0.9);
        run(&mut f, &tone(300.0, 9_600));
        f.clear();
        let (l, r) = run(&mut f, &vec![0.0; 9_600]);
        assert!(l.iter().chain(&r).all(|x| *x == 0.0));
    }
}
