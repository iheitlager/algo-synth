//! A ring modulator and a frequency shifter (#745), Strudel's `ring` and
//! `fshift`.
//!
//! Both multiply by a sine they make by turning a phasor, so no sine is
//! computed per sample: a rotation a sample, the phasor set back to unit
//! length every block. The frequency shifter splits the signal into two
//! parts 90° apart with a pair of all-pass chains (Olli Niemitalo's
//! coefficients, flat within a degree from 20 Hz to 20 kHz at 48 kHz) and
//! mixes them with the phasor's cosine and sine, which moves every partial
//! by the same number of hertz.

use std::f32::consts::TAU;

/// A sine and a cosine of a frequency, by rotation.
#[derive(Clone, Copy)]
pub(crate) struct Quad {
    pub cos: f32,
    pub sin: f32,
    step_cos: f32,
    step_sin: f32,
}

impl Quad {
    pub fn new() -> Quad {
        Quad {
            cos: 1.0,
            sin: 0.0,
            step_cos: 1.0,
            step_sin: 0.0,
        }
    }

    /// Turn at `hz`, below 0 the other way, at `sample_rate`.
    pub fn set(&mut self, hz: f32, sample_rate: f32) {
        let w = TAU * hz / sample_rate.max(1.0);
        (self.step_sin, self.step_cos) = w.sin_cos();
    }

    /// One sample on.
    pub fn tick(&mut self) {
        let (c, s) = (self.cos, self.sin);
        self.cos = c * self.step_cos - s * self.step_sin;
        self.sin = c * self.step_sin + s * self.step_cos;
    }

    /// Back to unit length, once a block, so rounding never grows it.
    pub fn renorm(&mut self) {
        let n = (self.cos * self.cos + self.sin * self.sin).sqrt();
        if n > 0.0 {
            self.cos /= n;
            self.sin /= n;
        } else {
            (self.cos, self.sin) = (1.0, 0.0);
        }
    }

    pub fn reset(&mut self) {
        (self.cos, self.sin) = (1.0, 0.0);
    }
}

/// The carrier at knob position `a`: 20 Hz to 5 kHz.
fn ring_hz(a: f32) -> f32 {
    20.0 * 250.0_f32.powf(a.clamp(0.0, 1.0))
}

pub struct Ring {
    sample_rate: f32,
    osc: Quad,
    depth: f32,
    gain: f32,
}

impl Ring {
    pub fn new(sample_rate: f32) -> Ring {
        let mut r = Ring {
            sample_rate,
            osc: Quad::new(),
            depth: 1.0,
            gain: 1.0,
        };
        r.set([0.5, 0.5, 0.5, 0.5, 0.0]);
        r
    }

    /// The knobs: A the carrier (20 Hz to 5 kHz), B depth (0 dry, 1 only
    /// the ring), C level (2·C); D and E are unused.
    pub fn set(&mut self, [a, b, c, _, _]: [f32; 5]) {
        self.osc.set(ring_hz(a), self.sample_rate);
        self.depth = b.clamp(0.0, 1.0);
        self.gain = 2.0 * c.clamp(0.0, 1.0);
    }

    pub fn reset(&mut self) {
        self.osc.reset();
    }

    fn ring(&self, x: f32) -> f32 {
        (x * (1.0 - self.depth) + x * self.osc.sin * self.depth) * self.gain
    }

    pub fn process_mono(&mut self, x: &mut [f32]) {
        for s in x.iter_mut() {
            *s = self.ring(*s);
            self.osc.tick();
        }
        self.osc.renorm();
    }

    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l = self.ring(*l);
            *r = self.ring(*r);
            self.osc.tick();
        }
        self.osc.renorm();
    }
}

/// The all-pass coefficients of the two paths of the Hilbert pair.
const PATH_A: [f32; 4] = [0.692_387_8, 0.936_065_4, 0.988_229_5, 0.998_748_8];
const PATH_B: [f32; 4] = [0.402_192_1, 0.856_171_1, 0.972_290_95, 0.995_288_5];

/// One path of the pair: four second-order all-passes in z⁻², each
/// `y = a²·(x + y[n−2]) − x[n−2]`.
#[derive(Clone, Copy, Default)]
struct Path {
    /// Each stage's last two inputs and outputs.
    x: [[f32; 2]; 4],
    y: [[f32; 2]; 4],
}

impl Path {
    fn run(&mut self, coefs: &[f32; 4], mut v: f32) -> f32 {
        for ((a, x), y) in coefs.iter().zip(self.x.iter_mut()).zip(self.y.iter_mut()) {
            let out = a * a * (v + y[1]) - x[1];
            *x = [v, x[0]];
            *y = [out, y[0]];
            v = out;
        }
        v
    }
}

/// The shift at knob position `a`: 0 Hz at 0.5, out to about ±4 kHz at
/// either end, finer near the middle.
pub fn shift_hz(a: f32) -> f32 {
    let d = 2.0 * a.clamp(0.0, 1.0) - 1.0;
    d.signum() * (2.0_f32.powf(12.0 * d.abs()) - 1.0)
}

/// The two paths of one channel, and path A's sample before.
#[derive(Clone, Copy, Default)]
struct Hilbert {
    a: Path,
    b: Path,
    a_last: f32,
}

impl Hilbert {
    /// The parts of `x` 90° apart: in phase and in quadrature.
    fn split(&mut self, x: f32) -> (f32, f32) {
        let i = self.a_last;
        self.a_last = self.a.run(&PATH_A, x);
        (i, self.b.run(&PATH_B, x))
    }
}

pub struct FreqShift {
    sample_rate: f32,
    osc: Quad,
    pair: [Hilbert; 2],
    mix: f32,
    gain: f32,
}

impl FreqShift {
    pub fn new(sample_rate: f32) -> FreqShift {
        let mut f = FreqShift {
            sample_rate,
            osc: Quad::new(),
            pair: [Hilbert::default(); 2],
            mix: 1.0,
            gain: 1.0,
        };
        f.set([0.5, 0.5, 0.5, 0.5, 0.0]);
        f
    }

    /// The knobs: A the shift (0 Hz at 0.5, down to the left, up to the
    /// right), B mix (0 dry, 1 only the shifted), C level (2·C); D and E
    /// are unused.
    pub fn set(&mut self, [a, b, c, _, _]: [f32; 5]) {
        self.osc.set(shift_hz(a), self.sample_rate);
        self.mix = b.clamp(0.0, 1.0);
        self.gain = 2.0 * c.clamp(0.0, 1.0);
    }

    pub fn reset(&mut self) {
        self.osc.reset();
        self.pair = [Hilbert::default(); 2];
    }

    fn shift(&mut self, ch: usize, x: f32) -> f32 {
        let (i, q) = self.pair.get_mut(ch).map_or((x, 0.0), |h| h.split(x));
        let shifted = i * self.osc.cos + q * self.osc.sin;
        (i * (1.0 - self.mix) + shifted * self.mix) * self.gain
    }

    pub fn process_mono(&mut self, x: &mut [f32]) {
        for s in x.iter_mut() {
            *s = self.shift(0, *s);
            self.osc.tick();
        }
        self.osc.renorm();
    }

    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l = self.shift(0, *l);
            *r = self.shift(1, *r);
            self.osc.tick();
        }
        self.osc.renorm();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.5 * (TAU * hz * i as f32 / SR).sin())
            .collect()
    }

    /// The strongest frequency in `x`, to 1 Hz, by trying each.
    fn peak_hz(x: &[f32], lo: f32, hi: f32) -> f32 {
        let power = |hz: f32| {
            let (mut re, mut im) = (0.0_f64, 0.0_f64);
            for (n, s) in x.iter().enumerate() {
                let w = f64::from(TAU * hz) * n as f64 / f64::from(SR);
                re += f64::from(*s) * w.cos();
                im += f64::from(*s) * w.sin();
            }
            re * re + im * im
        };
        let mut best = (lo, 0.0);
        let mut hz = lo;
        while hz <= hi {
            let p = power(hz);
            if p > best.1 {
                best = (hz, p);
            }
            hz += 1.0;
        }
        best.0
    }

    #[test]
    fn the_phasor_stays_a_sine_of_its_frequency() {
        let mut q = Quad::new();
        q.set(1_000.0, SR);
        let mut y = Vec::new();
        for _ in 0..375 {
            for _ in 0..128 {
                y.push(q.sin);
                q.tick();
            }
            q.renorm();
        }
        let tail = &y[y.len() - 4_800..];
        assert!((peak_hz(tail, 950.0, 1_050.0) - 1_000.0).abs() <= 1.0);
        assert!(tail.iter().all(|s| s.abs() <= 1.0 + 1e-4));
    }

    #[test]
    fn a_ring_makes_the_sum_and_difference() {
        // A 300 Hz carrier on 1 kHz: 700 and 1300 Hz, no 1 kHz left.
        let mut r = Ring::new(SR);
        let a = (300.0_f32 / 20.0).ln() / 250.0_f32.ln();
        r.set([a, 1.0, 0.5, 0.0, 0.0]);
        let mut y = sine(1_000.0, 9_600);
        for c in y.chunks_mut(128) {
            r.process_mono(c);
        }
        let p = peak_hz(&y[4_800..], 500.0, 1_500.0);
        assert!((p - 700.0).abs() < 3.0 || (p - 1_300.0).abs() < 3.0, "{p}");
    }

    #[test]
    fn a_shift_moves_a_tone_by_its_hertz() {
        for (a, want) in [(0.75, 440.0 + shift_hz(0.75)), (0.3, 440.0 + shift_hz(0.3))] {
            let mut f = FreqShift::new(SR);
            f.set([a, 1.0, 0.5, 0.0, 0.0]);
            let mut y = sine(440.0, 19_200);
            for c in y.chunks_mut(128) {
                f.process_mono(c);
            }
            let got = peak_hz(&y[9_600..], (want - 100.0).max(1.0), want + 100.0);
            assert!((got - want).abs() < 3.0, "a {a}: {got} not {want}");
        }
        assert_eq!(shift_hz(0.5), 0.0);
    }

    #[test]
    fn silence_stays_silent_and_both_stay_bounded() {
        let mut r = Ring::new(SR);
        let mut f = FreqShift::new(SR);
        let mut z = [0.0_f32; 512];
        r.process_mono(&mut z);
        f.process_mono(&mut z);
        assert!(z.iter().all(|s| *s == 0.0));
        for a in [0.0, 0.5, 1.0] {
            let mut f = FreqShift::new(SR);
            f.set([a, 1.0, 1.0, 0.0, 0.0]);
            let mut x: Vec<f32> = (0..9_600)
                .map(|i| if i % 61 < 30 { 0.9 } else { -0.9 })
                .collect();
            f.process_mono(&mut x);
            assert!(x.iter().all(|s| s.is_finite() && s.abs() < 6.0));
        }
    }
}
