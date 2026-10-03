//! The echo send (spec 002 Req 2): a stereo delay of up to 2 s, in
//! milliseconds rather than beats, with a low-pass in the loop and an
//! optional ping-pong.

use std::f32::consts::TAU;

use super::Delay;

/// The longest delay, in seconds.
pub const MAX_SECONDS: f32 = 2.0;
/// Feedback stays below this, so the loop always decays.
pub const MAX_FEEDBACK: f32 = 0.95;
/// How fast the delay time follows a change, per sample, to avoid zipper noise.
const SLEW: f32 = 0.002;

pub struct Echo {
    left: Delay,
    right: Delay,
    sample_rate: f32,
    /// The delay in samples, and where it is heading.
    delay: f32,
    target: f32,
    feedback: f32,
    /// One-pole low-pass coefficient in the loop.
    tone: f32,
    ping_pong: bool,
    lp: [f32; 2],
}

impl Echo {
    pub fn new(sample_rate: f32) -> Echo {
        let max = (MAX_SECONDS * sample_rate) as usize + 1;
        let mut e = Echo {
            left: Delay::new(max),
            right: Delay::new(max),
            sample_rate,
            delay: 1.0,
            target: 1.0,
            feedback: 0.0,
            tone: 1.0,
            ping_pong: false,
            lp: [0.0; 2],
        };
        e.set_time(300.0);
        e.delay = e.target;
        e.set_tone(0.7);
        e
    }

    /// The delay in milliseconds.
    pub fn set_time(&mut self, ms: f32) {
        let max = MAX_SECONDS * self.sample_rate;
        self.target = (ms * 0.001 * self.sample_rate).clamp(1.0, max);
    }

    pub fn set_feedback(&mut self, fb: f32) {
        self.feedback = fb.clamp(0.0, MAX_FEEDBACK);
    }

    /// 0..=1 puts the loop's low-pass corner from 500 Hz to 20 kHz.
    pub fn set_tone(&mut self, tone: f32) {
        let hz = 500.0 * 40.0_f32.powf(tone);
        self.tone = 1.0 - (-TAU * hz / self.sample_rate).exp();
    }

    pub fn set_ping_pong(&mut self, on: bool) {
        self.ping_pong = on;
    }

    /// Forget what the loop holds.
    pub fn clear(&mut self) {
        self.left.clear();
        self.right.clear();
        self.lp = [0.0; 2];
    }

    /// Feed `send` in and add the echoes into `left` and `right`, at full level:
    /// the processor applies the return.
    pub fn process(&mut self, send: &[f32], left: &mut [f32], right: &mut [f32]) {
        let frames = send.iter().zip(left.iter_mut().zip(right.iter_mut()));
        for (x, (l, r)) in frames {
            self.delay += (self.target - self.delay) * SLEW;
            let yl = self.left.read(self.delay);
            let yr = self.right.read(self.delay);
            let [fl, fr] = &mut self.lp;
            *fl += self.tone * (yl - *fl);
            *fr += self.tone * (yr - *fr);
            if self.ping_pong {
                self.left.write(x + self.feedback * *fr);
                self.right.write(self.feedback * *fl);
            } else {
                self.left.write(x + self.feedback * *fl);
                self.right.write(x + self.feedback * *fr);
            }
            *l += yl;
            *r += yr;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn run(e: &mut Echo, input: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (vec![0.0; input.len()], vec![0.0; input.len()]);
        for ((x, l), r) in input
            .chunks(128)
            .zip(l.chunks_mut(128))
            .zip(r.chunks_mut(128))
        {
            e.process(x, l, r);
        }
        (l, r)
    }

    fn impulse(n: usize) -> Vec<f32> {
        let mut v = vec![0.0; n];
        v[0] = 1.0;
        v
    }

    /// The summed output in the `half` samples around `centre`.
    fn around(x: &[f32], centre: usize, half: usize) -> f32 {
        x[centre - half..centre + half].iter().sum()
    }

    #[test]
    fn echoes_land_at_the_set_time_and_decay_by_the_feedback() {
        let mut e = Echo::new(SR);
        e.set_time(100.0);
        e.set_feedback(0.5);
        let (l, _) = run(&mut e, &impulse(24_000));
        let t = 4_800;
        let (a, b, c) = (
            around(&l, t, 200),
            around(&l, 2 * t, 200),
            around(&l, 3 * t, 200),
        );
        assert!((a - 1.0).abs() < 0.05, "first echo {a}");
        assert!((b / a - 0.5).abs() < 0.05, "second {b}");
        assert!((c / b - 0.5).abs() < 0.05, "third {c}");
        assert!(
            l[..t - 200].iter().all(|x| x.abs() < 1.0e-6),
            "nothing before"
        );
    }

    #[test]
    fn feedback_is_clamped_and_the_loop_stays_bounded() {
        let mut e = Echo::new(SR);
        e.set_time(10.0);
        e.set_feedback(5.0);
        let noise: Vec<f32> = (0..96_000)
            .map(|i| if i % 3 == 0 { 1.0 } else { -1.0 })
            .collect();
        let (l, r) = run(&mut e, &noise);
        assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 100.0));
        let (l, _) = run(&mut e, &vec![0.0; 960_000]);
        assert!(l[950_000..].iter().all(|x| x.abs() < 1.0e-3), "decays");
    }

    #[test]
    fn ping_pong_alternates_sides() {
        let mut e = Echo::new(SR);
        e.set_time(50.0);
        e.set_feedback(0.6);
        e.set_ping_pong(true);
        let (l, r) = run(&mut e, &impulse(12_000));
        let t = 2_400;
        assert!(around(&l, t, 100) > 0.5 && around(&r, t, 100).abs() < 1.0e-3);
        assert!(around(&r, 2 * t, 100) > 0.2 && around(&l, 2 * t, 100).abs() < 1.0e-3);
    }
}
