//! The drive insert (spec 004 Req 11): a waveshaper on a synth's bus, after
//! its voices and before the fader.
//!
//! Each shaper has a closed-form antiderivative, so first-order antiderivative
//! anti-aliasing (ADAA) replaces `y = f(x)` by `(F(x) − F(x₁)) / (x − x₁)`:
//! no oversampling and no per-sample `tanh` (ADR-0002), and far fewer aliases.

use std::f32::consts::TAU;

/// What the shaper does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DriveMode {
    /// Bit-exact pass-through.
    Off = 0,
    /// A soft, asymmetric cubic.
    Overdrive = 1,
    /// A hard clip.
    Distortion = 2,
    /// A hard clip that cuts one half much sooner, at a much higher gain.
    Fuzz = 3,
}

impl DriveMode {
    pub const ALL: [(DriveMode, &'static str); 4] = [
        (DriveMode::Off, "Off"),
        (DriveMode::Overdrive, "Overdrive"),
        (DriveMode::Distortion, "Distortion"),
        (DriveMode::Fuzz, "Fuzz"),
    ];

    pub fn from_id(id: u32) -> Option<DriveMode> {
        Self::ALL
            .iter()
            .find(|(m, _)| *m as u32 == id)
            .map(|(m, _)| *m)
    }
}

/// Input bound before the shaper, so no input makes the output non-finite.
const LIMIT: f32 = 1_000.0;
/// Overdrive's input offset, which makes the shaper asymmetric.
const BIAS: f32 = 0.2;
/// Fuzz's positive clip level; the negative one is −1.
const FUZZ_TOP: f32 = 0.35;
/// Below this step the quotient is replaced by the shaper at the midpoint.
const EPS: f32 = 1.0e-4;

/// `(f, F)` of the cubic soft clip, scaled to ±1: `1.5·(x − x³/3)` up to 1.
fn cubic(x: f32) -> (f32, f32) {
    let a = x.abs();
    if a <= 1.0 {
        (
            1.5 * (x - x * x * x / 3.0),
            1.5 * (x * x / 2.0 - x * x * x * x / 12.0),
        )
    } else {
        (1.0_f32.copysign(x), 1.5 * (2.0 / 3.0 * a - 0.25))
    }
}

/// `(f, F)` of a clip to `lo..=hi`.
fn clip(x: f32, lo: f32, hi: f32) -> (f32, f32) {
    if x > hi {
        (hi, hi * x - hi * hi / 2.0)
    } else if x < lo {
        (lo, lo * x - lo * lo / 2.0)
    } else {
        (x, x * x / 2.0)
    }
}

pub struct Drive {
    mode: DriveMode,
    gain: f32,
    level: f32,
    /// One-pole low-pass coefficient and the corner it comes from.
    tone: f32,
    sample_rate: f32,
    /// The shaper's previous input and its antiderivative there.
    x1: f32,
    f1: f32,
    lp: f32,
    /// DC blocker: previous input and output.
    dc_x: f32,
    dc_y: f32,
    dc_r: f32,
}

impl Drive {
    pub fn new(sample_rate: f32) -> Drive {
        let mut d = Drive {
            mode: DriveMode::Off,
            gain: 1.0,
            level: 1.0,
            tone: 1.0,
            sample_rate,
            x1: 0.0,
            f1: 0.0,
            lp: 0.0,
            dc_x: 0.0,
            dc_y: 0.0,
            dc_r: 1.0 - TAU * 10.0 / sample_rate,
        };
        d.set_amount(0.0);
        d.set_tone(1.0);
        d
    }

    /// Switching starts the filters from silence, so no click is carried over.
    pub fn set_mode(&mut self, mode: DriveMode) {
        if mode != self.mode {
            self.mode = mode;
            self.x1 = 0.0;
            self.f1 = 0.0;
            self.lp = 0.0;
            self.dc_x = 0.0;
            self.dc_y = 0.0;
        }
    }

    /// 0..=1 is 0 to +40 dB of gain into the shaper.
    pub fn set_amount(&mut self, amount: f32) {
        self.gain = 10.0_f32.powf(2.0 * amount);
    }

    /// 0..=1 puts the low-pass corner from 200 Hz to 20 kHz.
    pub fn set_tone(&mut self, tone: f32) {
        let hz = 200.0 * 100.0_f32.powf(tone);
        self.tone = 1.0 - (-TAU * hz / self.sample_rate).exp();
    }

    pub fn set_level(&mut self, level: f32) {
        self.level = level;
    }

    /// Shape `buf` in place; `Off` leaves it untouched.
    pub fn process(&mut self, buf: &mut [f32]) {
        let (gain, bias, lo, hi) = match self.mode {
            DriveMode::Off => return,
            DriveMode::Overdrive => (self.gain, BIAS, 0.0, 0.0),
            DriveMode::Distortion => (self.gain, 0.0, -1.0, 1.0),
            DriveMode::Fuzz => (self.gain * 4.0, 0.0, -1.0, FUZZ_TOP),
        };
        for s in buf.iter_mut() {
            let x = if s.is_finite() { *s } else { 0.0 };
            let u = (x * gain + bias).clamp(-LIMIT, LIMIT);
            let shape = |v: f32| {
                if self.mode == DriveMode::Overdrive {
                    cubic(v)
                } else {
                    clip(v, lo, hi)
                }
            };
            let big_f = shape(u).1;
            let step = u - self.x1;
            let y = if step.abs() > EPS {
                (big_f - self.f1) / step
            } else {
                shape(0.5 * (u + self.x1)).0
            };
            self.x1 = u;
            self.f1 = big_f;
            self.lp += self.tone * (y - self.lp);
            // Remove the DC the asymmetric shapers add.
            let out = self.lp - self.dc_x + self.dc_r * self.dc_y;
            self.dc_x = self.lp;
            self.dc_y = out;
            *s = out * self.level;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(hz: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (TAU * hz * i as f32 / SR).sin())
            .collect()
    }

    fn driven(mode: DriveMode, amount: f32, input: &[f32]) -> Vec<f32> {
        let mut d = Drive::new(SR);
        d.set_mode(mode);
        d.set_amount(amount);
        let mut out = input.to_vec();
        d.process(&mut out);
        out
    }

    /// Magnitude of the `hz` component of `x`.
    fn goertzel(x: &[f32], hz: f32) -> f32 {
        let w = TAU * hz / SR;
        let (mut re, mut im) = (0.0_f64, 0.0_f64);
        for (i, v) in x.iter().enumerate() {
            re += f64::from(*v) * (w * i as f32).cos() as f64;
            im += f64::from(*v) * (w * i as f32).sin() as f64;
        }
        (re.hypot(im) * 2.0 / x.len() as f64) as f32
    }

    #[test]
    fn off_is_bit_exact() {
        let input = sine(440.0, 0.8, 1000);
        let mut d = Drive::new(SR);
        d.set_amount(1.0);
        d.set_level(0.3);
        let mut out = input.clone();
        d.process(&mut out);
        assert!(out == input);
    }

    #[test]
    fn every_mode_is_finite_and_bounded_for_any_input() {
        let nasty: Vec<f32> = [
            f32::MAX,
            -f32::MAX,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            1.0e30,
            -3.0,
            0.0,
            1.0e-30,
            0.7,
        ]
        .iter()
        .cycle()
        .take(4000)
        .copied()
        .chain(sine(60.0, 100.0, 4000))
        .collect();
        for (mode, name) in DriveMode::ALL.iter().skip(1) {
            for amount in [0.0, 0.5, 1.0] {
                for s in driven(*mode, amount, &nasty) {
                    assert!(s.is_finite() && s.abs() <= 3.0, "{name} {amount}: {s}");
                }
            }
        }
    }

    #[test]
    fn more_drive_means_more_harmonics() {
        let input = sine(220.0, 0.05, 9600);
        for mode in [DriveMode::Overdrive, DriveMode::Distortion, DriveMode::Fuzz] {
            let share = |amount: f32| {
                let y = driven(mode, amount, &input);
                let tail = &y[4800..];
                let harmonics: f32 = (2..9)
                    .map(|k| goertzel(tail, 220.0 * k as f32).powi(2))
                    .sum();
                harmonics.sqrt() / goertzel(tail, 220.0)
            };
            let (low, high) = (share(0.1), share(0.9));
            assert!(high > 2.0 * low, "{mode:?}: {low} then {high}");
        }
    }

    #[test]
    fn a_high_sine_at_full_drive_keeps_its_aliases_low() {
        // 4.7 kHz: its 6th harmonic, 28.2 kHz, folds to 19.8 kHz, which is
        // no harmonic of the fundamental.
        let input = sine(4_700.0, 0.8, 9600);
        for mode in [DriveMode::Overdrive, DriveMode::Distortion, DriveMode::Fuzz] {
            let y = driven(mode, 1.0, &input);
            let tail = &y[4800..];
            let alias = goertzel(tail, 19_800.0) / goertzel(tail, 4_700.0);
            assert!(alias < 0.01, "{mode:?}: alias at {alias}");
        }
    }

    #[test]
    fn asymmetric_shapers_leave_no_dc() {
        let y = driven(DriveMode::Fuzz, 0.7, &sine(110.0, 0.3, 48_000));
        let mean = y[24_000..].iter().sum::<f32>() / 24_000.0;
        assert!(mean.abs() < 0.01, "dc {mean}");
    }
}
