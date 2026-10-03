//! The master equalizer (spec 002 Req 2): a low shelf, two parametric bands
//! and a high shelf, as biquads (RBJ cookbook). Coefficients are computed
//! when a parameter changes, never per sample (ADR-0002). A band at 0 dB is
//! skipped, so a flat equalizer passes the signal bit for bit.

use std::f32::consts::TAU;

#[derive(Clone, Copy)]
enum Shape {
    LowShelf,
    Peak,
    HighShelf,
}

/// One filter section for left and right, in transposed direct form II.
#[derive(Clone, Copy)]
struct Band {
    shape: Shape,
    freq: f32,
    gain_db: f32,
    q: f32,
    b: [f32; 3],
    a: [f32; 2],
    z: [[f32; 2]; 2],
}

impl Band {
    fn new(shape: Shape, freq: f32, q: f32) -> Band {
        Band {
            shape,
            freq,
            gain_db: 0.0,
            q,
            b: [1.0, 0.0, 0.0],
            a: [0.0; 2],
            z: [[0.0; 2]; 2],
        }
    }

    fn active(&self) -> bool {
        self.gain_db != 0.0
    }

    fn update(&mut self, sample_rate: f32) {
        let a = 10.0_f32.powf(self.gain_db / 40.0);
        let w0 = TAU * self.freq.clamp(10.0, 0.45 * sample_rate) / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let (b, a_c) = match self.shape {
            Shape::Peak => {
                let alpha = sin / (2.0 * self.q);
                (
                    [1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a],
                    [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a],
                )
            }
            // Shelves with a slope of 1: alpha = sin/2 · √2.
            Shape::LowShelf | Shape::HighShelf => {
                let alpha = sin / 2.0 * std::f32::consts::SQRT_2;
                let k = 2.0 * a.sqrt() * alpha;
                if matches!(self.shape, Shape::LowShelf) {
                    (
                        [
                            a * ((a + 1.0) - (a - 1.0) * cos + k),
                            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                            a * ((a + 1.0) - (a - 1.0) * cos - k),
                        ],
                        [
                            (a + 1.0) + (a - 1.0) * cos + k,
                            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                            (a + 1.0) + (a - 1.0) * cos - k,
                        ],
                    )
                } else {
                    (
                        [
                            a * ((a + 1.0) + (a - 1.0) * cos + k),
                            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                            a * ((a + 1.0) + (a - 1.0) * cos - k),
                        ],
                        [
                            (a + 1.0) - (a - 1.0) * cos + k,
                            2.0 * ((a - 1.0) - (a + 1.0) * cos),
                            (a + 1.0) - (a - 1.0) * cos - k,
                        ],
                    )
                }
            }
        };
        let inv = 1.0 / a_c[0];
        self.b = [b[0] * inv, b[1] * inv, b[2] * inv];
        self.a = [a_c[1] * inv, a_c[2] * inv];
    }

    fn run(&mut self, side: usize, x: &mut [f32]) {
        let Some(z) = self.z.get_mut(side) else {
            return;
        };
        for s in x.iter_mut() {
            let y = self.b[0] * *s + z[0];
            z[0] = self.b[1] * *s - self.a[0] * y + z[1];
            z[1] = self.b[2] * *s - self.a[1] * y;
            *s = y;
        }
    }
}

pub struct Equalizer {
    sample_rate: f32,
    bands: [Band; 4],
}

/// Which band a parameter belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqBand {
    Low = 0,
    Mid1 = 1,
    Mid2 = 2,
    High = 3,
}

impl Equalizer {
    pub fn new(sample_rate: f32) -> Equalizer {
        let mut e = Equalizer {
            sample_rate,
            bands: [
                Band::new(Shape::LowShelf, 100.0, 0.7),
                Band::new(Shape::Peak, 500.0, 1.0),
                Band::new(Shape::Peak, 3_000.0, 1.0),
                Band::new(Shape::HighShelf, 8_000.0, 0.7),
            ],
        };
        for band in e.bands.iter_mut() {
            band.update(sample_rate);
        }
        e
    }

    fn with(&mut self, band: EqBand, f: impl FnOnce(&mut Band)) {
        if let Some(b) = self.bands.get_mut(band as usize) {
            f(b);
            b.update(self.sample_rate);
        }
    }

    pub fn set_freq(&mut self, band: EqBand, hz: f32) {
        self.with(band, |b| b.freq = hz);
    }

    pub fn set_gain(&mut self, band: EqBand, db: f32) {
        self.with(band, |b| {
            // Going flat starts the filter from rest, so nothing rings on.
            if db == 0.0 {
                b.z = [[0.0; 2]; 2];
            }
            b.gain_db = db;
        });
    }

    pub fn set_q(&mut self, band: EqBand, q: f32) {
        self.with(band, |b| b.q = q);
    }

    /// Equalize `left` and `right` in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for band in self.bands.iter_mut().filter(|b| b.active()) {
            band.run(0, left);
            band.run(1, right);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.3 * (TAU * hz * i as f32 / SR).sin())
            .collect()
    }

    /// The gain in dB the equalizer gives a sine of `hz`.
    fn gain_at(e: &mut Equalizer, hz: f32) -> f32 {
        let x = sine(hz, 24_000);
        let (mut l, mut r) = (x.clone(), x.clone());
        e.process(&mut l, &mut r);
        let peak = |v: &[f32]| v[12_000..].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        20.0 * (peak(&l) / peak(&x)).log10()
    }

    #[test]
    fn flat_is_bit_exact() {
        let mut e = Equalizer::new(SR);
        let x = sine(440.0, 1000);
        let (mut l, mut r) = (x.clone(), x.clone());
        e.process(&mut l, &mut r);
        assert!(l == x && r == x);
        e.set_gain(EqBand::Mid1, 6.0);
        e.set_gain(EqBand::Mid1, 0.0);
        let (mut l, mut r) = (x.clone(), x.clone());
        e.process(&mut l, &mut r);
        assert!(l == x, "and again after a band is put back to 0 dB");
    }

    #[test]
    fn each_band_reaches_its_gain() {
        let mut e = Equalizer::new(SR);
        e.set_gain(EqBand::Low, 9.0);
        e.set_freq(EqBand::Low, 400.0);
        assert!(
            (gain_at(&mut e, 40.0) - 9.0).abs() < 0.5,
            "low shelf, far below"
        );
        assert!(gain_at(&mut e, 8_000.0).abs() < 0.5, "and flat above");
        let mut e = Equalizer::new(SR);
        e.set_gain(EqBand::Mid1, -8.0);
        e.set_freq(EqBand::Mid1, 700.0);
        assert!(
            (gain_at(&mut e, 700.0) + 8.0).abs() < 0.3,
            "cut at its centre"
        );
        assert!(gain_at(&mut e, 5_000.0).abs() < 0.7, "and little elsewhere");
        let mut e = Equalizer::new(SR);
        e.set_gain(EqBand::Mid2, 6.0);
        e.set_freq(EqBand::Mid2, 4_000.0);
        assert!((gain_at(&mut e, 4_000.0) - 6.0).abs() < 0.3);
        let mut e = Equalizer::new(SR);
        e.set_gain(EqBand::High, -10.0);
        e.set_freq(EqBand::High, 3_000.0);
        assert!(
            (gain_at(&mut e, 16_000.0) + 10.0).abs() < 0.7,
            "high shelf, far above"
        );
        assert!(gain_at(&mut e, 100.0).abs() < 0.5);
    }

    #[test]
    fn q_narrows_a_band() {
        let wide = {
            let mut e = Equalizer::new(SR);
            e.set_gain(EqBand::Mid1, 10.0);
            e.set_freq(EqBand::Mid1, 1_000.0);
            e.set_q(EqBand::Mid1, 0.5);
            gain_at(&mut e, 1_500.0)
        };
        let narrow = {
            let mut e = Equalizer::new(SR);
            e.set_gain(EqBand::Mid1, 10.0);
            e.set_freq(EqBand::Mid1, 1_000.0);
            e.set_q(EqBand::Mid1, 4.0);
            gain_at(&mut e, 1_500.0)
        };
        assert!(narrow < wide - 3.0, "{narrow} vs {wide}");
    }

    // Four bands at +15 dB on one frequency are +60 dB, so 0.5 may reach 500.
    #[test]
    fn stays_stable_and_bounded_under_any_setting() {
        let mut e = Equalizer::new(SR);
        let noise: Vec<f32> = (0..48_000)
            .map(|i| if (i / 7) % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        for step in 0..40 {
            let f = 20.0 * 1.25_f32.powi(step);
            for band in [EqBand::Low, EqBand::Mid1, EqBand::Mid2, EqBand::High] {
                e.set_freq(band, f);
                e.set_gain(band, if step % 2 == 0 { 15.0 } else { -15.0 });
                e.set_q(band, if step % 3 == 0 { 8.0 } else { 0.3 });
            }
            let (mut l, mut r) = (noise.clone(), noise.clone());
            e.process(&mut l, &mut r);
            assert!(
                l.iter()
                    .chain(&r)
                    .all(|x| x.is_finite() && x.abs() < 5_000.0),
                "step {step}"
            );
        }
    }
}
