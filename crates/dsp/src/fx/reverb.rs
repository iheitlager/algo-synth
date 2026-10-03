//! The reverb send (spec 002 Req 2): an 8-line feedback delay network with a
//! Hadamard feedback matrix. `size` is the time the tail takes to fall 60 dB.

use std::f32::consts::TAU;

use super::Delay;

const LINES: usize = 8;
/// Line lengths in samples at 48 kHz: mutually prime, so echoes don't stack.
const LENGTHS: [usize; LINES] = [1051, 1181, 1327, 1453, 1601, 1733, 1879, 2011];
/// How the input is spread over the lines.
const SIGNS: [f32; LINES] = [1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0];
/// Longest pre-delay, in seconds.
pub const MAX_PRE_DELAY: f32 = 0.1;

pub struct Reverb {
    lines: Vec<Delay>,
    lens: [usize; LINES],
    /// Per-line gain for the current decay time.
    gain: [f32; LINES],
    /// One-pole low-pass state per line, and its coefficient (1 is open).
    lp: [f32; LINES],
    damp: f32,
    pre: Delay,
    pre_len: usize,
    sample_rate: f32,
    decay: f32,
    /// DC blocker per output side: previous input and output, and its pole.
    dc: [[f32; 2]; 2],
    dc_r: f32,
}

impl Reverb {
    pub fn new(sample_rate: f32) -> Reverb {
        let scale = sample_rate / 48_000.0;
        let lens: [usize; LINES] = std::array::from_fn(|i| {
            ((LENGTHS.get(i).copied().unwrap_or(1000) as f32 * scale) as usize).max(2)
        });
        let mut r = Reverb {
            lines: lens.iter().map(|n| Delay::new(*n)).collect(),
            lens,
            gain: [0.0; LINES],
            lp: [0.0; LINES],
            damp: 1.0,
            pre: Delay::new((MAX_PRE_DELAY * sample_rate) as usize + 1),
            pre_len: 1,
            sample_rate,
            decay: 2.0,
            dc: [[0.0; 2]; 2],
            dc_r: 1.0 - TAU * 10.0 / sample_rate,
        };
        r.set_size(2.0);
        r.set_damping(0.3);
        r
    }

    /// The time in seconds for the tail to fall 60 dB.
    pub fn set_size(&mut self, seconds: f32) {
        self.decay = seconds.max(0.05);
        for (g, len) in self.gain.iter_mut().zip(self.lens) {
            *g = 10.0_f32.powf(-3.0 * len as f32 / (self.sample_rate * self.decay));
        }
    }

    /// 0..=1 darkens the tail: 0 is open, 1 a low-pass at about 1 kHz.
    pub fn set_damping(&mut self, damping: f32) {
        let hz = 20_000.0 * 0.05_f32.powf(damping);
        self.damp = 1.0 - (-TAU * hz / self.sample_rate).exp();
    }

    pub fn set_pre_delay(&mut self, ms: f32) {
        let n = (ms * 0.001 * self.sample_rate) as usize;
        self.pre_len = n.clamp(1, (MAX_PRE_DELAY * self.sample_rate) as usize);
    }

    /// Forget the tail.
    pub fn clear(&mut self) {
        for line in self.lines.iter_mut() {
            line.clear();
        }
        self.pre.clear();
        self.lp = [0.0; LINES];
        self.dc = [[0.0; 2]; 2];
    }

    /// Feed `send` in and add the tail into `left` and `right`.
    pub fn process(&mut self, send: &[f32], left: &mut [f32], right: &mut [f32]) {
        let frames = send.iter().zip(left.iter_mut().zip(right.iter_mut()));
        for (x, (l, r)) in frames {
            self.pre.write(*x);
            let x = self.pre.at(self.pre_len);
            let mut v = [0.0_f32; LINES];
            for (i, v) in v.iter_mut().enumerate() {
                let len = self.lens.get(i).copied().unwrap_or(1);
                *v = self.lines.get(i).map_or(0.0, |line| line.at(len));
            }
            // Output: even lines to the left, odd to the right.
            let (mut out_l, mut out_r) = (0.0, 0.0);
            for (i, v) in v.iter().enumerate() {
                if i % 2 == 0 {
                    out_l += v;
                } else {
                    out_r += v;
                }
            }
            // Damp, decay, then mix with the Hadamard matrix.
            let mut w = [0.0_f32; LINES];
            for (i, w) in w.iter_mut().enumerate() {
                let (Some(lp), Some(v), Some(g)) = (self.lp.get_mut(i), v.get(i), self.gain.get(i))
                else {
                    continue;
                };
                *lp += self.damp * (v - *lp);
                *w = *lp * g;
            }
            hadamard(&mut w);
            for (i, line) in self.lines.iter_mut().enumerate() {
                let sign = SIGNS.get(i).copied().unwrap_or(1.0);
                line.write(w.get(i).copied().unwrap_or(0.0) + x * sign * 0.35);
            }
            *l += self.dc_block(0, out_l * 0.5);
            *r += self.dc_block(1, out_r * 0.5);
        }
    }

    fn dc_block(&mut self, side: usize, x: f32) -> f32 {
        let Some([px, py]) = self.dc.get_mut(side) else {
            return x;
        };
        let y = x - *px + self.dc_r * *py;
        *px = x;
        *py = y;
        y
    }
}

/// The 8×8 Walsh–Hadamard transform, scaled so it keeps energy.
fn hadamard(w: &mut [f32; LINES]) {
    let mut h = 1;
    while h < LINES {
        for start in (0..LINES).step_by(2 * h) {
            for i in start..start + h {
                let (a, b) = (
                    w.get(i).copied().unwrap_or(0.0),
                    w.get(i + h).copied().unwrap_or(0.0),
                );
                if let Some(x) = w.get_mut(i) {
                    *x = a + b;
                }
                if let Some(x) = w.get_mut(i + h) {
                    *x = a - b;
                }
            }
        }
        h *= 2;
    }
    let norm = 1.0 / (LINES as f32).sqrt();
    for x in w.iter_mut() {
        *x *= norm;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn run(r: &mut Reverb, input: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut rr) = (vec![0.0; input.len()], vec![0.0; input.len()]);
        for ((x, l), rr) in input
            .chunks(128)
            .zip(l.chunks_mut(128))
            .zip(rr.chunks_mut(128))
        {
            r.process(x, l, rr);
        }
        (l, rr)
    }

    fn impulse(n: usize) -> Vec<f32> {
        let mut v = vec![0.0; n];
        v[0] = 1.0;
        v
    }

    /// RMS dB of the samples in `from..to` seconds.
    fn db(x: &[f32], from: f32, to: f32) -> f32 {
        let w = &x[(from * SR) as usize..(to * SR) as usize];
        10.0 * (w.iter().map(|x| x * x).sum::<f32>() / w.len() as f32).log10()
    }

    #[test]
    fn the_tail_falls_60_db_in_about_the_size_setting() {
        let mut r = Reverb::new(SR);
        r.set_size(1.0);
        r.set_damping(0.0);
        let (l, _) = run(&mut r, &impulse(96_000));
        let peak = db(&l, 0.1, 0.2);
        let half = db(&l, 0.45, 0.55) - peak;
        let end = db(&l, 1.0, 1.1) - peak;
        assert!(half > -35.0 && half < -10.0, "half way: {half} dB");
        assert!(end < -40.0 && end > -65.0, "at the size: {end} dB");
    }

    #[test]
    fn a_longer_size_rings_longer() {
        let at_one_second = |size: f32| {
            let mut r = Reverb::new(SR);
            r.set_size(size);
            let (l, _) = run(&mut r, &impulse(96_000));
            db(&l, 1.0, 1.1) - db(&l, 0.1, 0.2)
        };
        assert!(at_one_second(4.0) > at_one_second(1.0) + 10.0);
    }

    #[test]
    fn damping_darkens_and_shortens_the_tail() {
        let late = |damping: f32| {
            let mut r = Reverb::new(SR);
            r.set_size(3.0);
            r.set_damping(damping);
            let (l, _) = run(&mut r, &impulse(144_000));
            db(&l, 2.0, 2.2)
        };
        assert!(late(1.0) < late(0.0) - 6.0);
    }

    #[test]
    fn no_dc_and_always_bounded() {
        let mut r = Reverb::new(SR);
        r.set_size(10.0);
        let loud: Vec<f32> = (0..240_000)
            .map(|i| 0.9 + 0.1 * (i as f32 * 0.37).sin())
            .collect();
        let (l, rr) = run(&mut r, &loud);
        assert!(l.iter().chain(&rr).all(|x| x.is_finite() && x.abs() < 20.0));
        let after = run(&mut r, &vec![0.0; 480_000]).0;
        let mean = after[240_000..].iter().sum::<f32>() / 240_000.0;
        assert!(mean.abs() < 1.0e-3, "dc {mean}");
    }

    #[test]
    fn pre_delay_holds_the_tail_back() {
        let mut r = Reverb::new(SR);
        r.set_pre_delay(50.0);
        let (l, rr) = run(&mut r, &impulse(9_600));
        let first = l
            .iter()
            .zip(&rr)
            .position(|(a, b)| a.abs() + b.abs() > 1.0e-6);
        assert!(first.is_some_and(|i| i >= 2_400), "starts at {first:?}");
    }

    #[test]
    fn hadamard_keeps_energy() {
        let mut w = [1.0, -2.0, 0.5, 3.0, 0.0, 1.0, -1.0, 2.0];
        let before: f32 = w.iter().map(|x| x * x).sum();
        hadamard(&mut w);
        let after: f32 = w.iter().map(|x| x * x).sum();
        assert!((before - after).abs() < 1.0e-4);
    }
}
