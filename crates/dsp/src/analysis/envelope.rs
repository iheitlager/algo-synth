//! The spectral envelope (spec 010 Req 3): per frame, a smooth curve over the
//! partials' peaks, by Röbel & Rodet's true envelope (DAFx 2005), iterated
//! cepstral smoothing that rises until it lies over the peaks. It runs on a
//! spectrum drawn through the frame's peaks rather than the full STFT: the
//! envelope is only read at partial frequencies, and that keeps it cheap
//! enough to recompute as a lab control moves.

use super::Analysis;
use super::fft::Fft;
use super::harmonic::f0;

/// Points from DC to Nyquist the envelope holds per frame.
pub const POINTS: usize = 512;
/// Most smoothing steps a frame takes.
const STEPS: usize = 40;
/// Close enough: the envelope lies within this of every peak (1 dB, in ln).
const CLOSE: f32 = 0.115;
/// The level of no peak (ln of 1e-6, −120 dB).
const FLOOR: f32 = -13.8;
/// The cepstral order when a frame has no f0: about 25 harmonics' worth.
const DEFAULT_ORDER: usize = 25;

/// Envelopes of every frame of an analysis: `POINTS` natural-log amplitudes
/// each, from 0 Hz to Nyquist.
pub struct Envelope {
    nyquist: f32,
    values: Vec<f32>,
}

impl Envelope {
    /// The envelope's amplitude at `hz` in frame `frame`, interpolated; 0 past the frames.
    pub fn at(&self, frame: usize, hz: f32) -> f32 {
        let Some(row) = self.values.get(frame * POINTS..(frame + 1) * POINTS) else {
            return 0.0;
        };
        let x = (hz.max(0.0) / self.nyquist).min(1.0) * (POINTS - 1) as f32;
        let i = (x as usize).min(POINTS - 2);
        let frac = x - i as f32;
        let a = row.get(i).copied().unwrap_or(FLOOR);
        let b = row.get(i + 1).copied().unwrap_or(a);
        (a + (b - a) * frac).exp()
    }

    pub fn frames(&self) -> usize {
        self.values.len() / POINTS
    }
}

/// The true envelope of every frame of `a`.
pub fn true_envelope(a: &Analysis) -> Envelope {
    let nyquist = 0.5 * a.rate;
    let n = 2 * POINTS;
    let fft = Fft::new(n);
    let mut values = Vec::with_capacity(a.frames() * POINTS);
    let (mut re, mut im) = (vec![0.0f32; n], vec![0.0f32; n]);
    let mut spectrum = vec![FLOOR; POINTS];
    let mut env = vec![FLOOR; POINTS];
    for peaks in &a.peaks {
        // The log spectrum through the peaks: straight in ln amplitude between
        // neighbours, held flat past the first and the last.
        spectrum.fill(FLOOR);
        let at = |hz: f32| (hz / nyquist * (POINTS - 1) as f32).clamp(0.0, (POINTS - 1) as f32);
        let points: Vec<(f32, f32)> = peaks
            .iter()
            .filter(|p| p.amp > 0.0 && p.freq < nyquist)
            .map(|p| (at(p.freq), p.amp.ln()))
            .collect();
        if let (Some(&(x0, y0)), Some(&(xn, yn))) = (points.first(), points.last()) {
            for (i, s) in spectrum.iter_mut().enumerate() {
                let x = i as f32;
                *s = if x <= x0 {
                    y0
                } else if x >= xn {
                    yn
                } else {
                    let j = points.partition_point(|p| p.0 <= x).max(1);
                    let (xa, ya) = points.get(j - 1).copied().unwrap_or((x0, y0));
                    let (xb, yb) = points.get(j).copied().unwrap_or((xn, yn));
                    ya + (yb - ya) * (x - xa) / (xb - xa).max(1e-6)
                };
            }
        }
        // The order: what keeps single harmonics out of the envelope.
        let order = f0(peaks).map_or(DEFAULT_ORDER, |f| {
            ((0.5 * a.rate / f) as usize).clamp(4, POINTS / 2)
        });
        env.copy_from_slice(&spectrum);
        if let Some(fft) = &fft {
            for _ in 0..STEPS {
                for (e, s) in env.iter_mut().zip(&spectrum) {
                    *e = e.max(*s);
                }
                smooth(fft, &mut env, &mut re, &mut im, order);
                let gap = spectrum
                    .iter()
                    .zip(&env)
                    .map(|(s, e)| s - e)
                    .fold(f32::MIN, f32::max);
                if gap < CLOSE {
                    break;
                }
            }
        }
        values.extend_from_slice(&env);
    }
    Envelope { nyquist, values }
}

/// Cepstral smoothing: keep the first `order` cepstral coefficients of the
/// log spectrum `env` (mirrored into a real, even signal of `2 * POINTS`).
fn smooth(fft: &Fft, env: &mut [f32], re: &mut [f32], im: &mut [f32], order: usize) {
    let n = re.len();
    for (i, r) in re.iter_mut().enumerate() {
        let k = if i < POINTS { i } else { n - i };
        *r = env.get(k.min(POINTS - 1)).copied().unwrap_or(FLOOR);
    }
    im.fill(0.0);
    fft.inverse(re, im);
    for (q, (r, i)) in re.iter_mut().zip(im.iter_mut()).enumerate() {
        if q > order && q < n - order {
            *r = 0.0;
            *i = 0.0;
        }
    }
    fft.forward(re, im);
    for (e, r) in env.iter_mut().zip(re.iter()) {
        *e = *r;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{Settings, analyse};

    const RATE: f32 = 48_000.0;

    /// Harmonics of 110 Hz under a resonance at 1 kHz: a formant.
    fn formant() -> Vec<f32> {
        (0..24_000)
            .map(|i| {
                let t = i as f32 / RATE;
                (1..=60)
                    .map(|k| {
                        let f = 110.0 * k as f32;
                        let g = 1.0 / (1.0 + ((f - 1_000.0) / 250.0).powi(2));
                        0.3 * g * (std::f32::consts::TAU * f * t).sin()
                    })
                    .sum::<f32>()
            })
            .collect()
    }

    #[test]
    fn the_envelope_finds_a_formant() {
        let a = analyse(&formant(), RATE, &Settings::default()).unwrap();
        let env = true_envelope(&a);
        let mid = env.frames() / 2;
        let top = (100..4_000)
            .step_by(10)
            .max_by(|&x, &y| env.at(mid, x as f32).total_cmp(&env.at(mid, y as f32)))
            .unwrap() as f32;
        assert!((top / 1_000.0).log2().abs() < 1.0 / 3.0, "peak at {top} Hz");
    }

    #[test]
    fn the_envelope_lies_over_the_peaks() {
        let a = analyse(&formant(), RATE, &Settings::default()).unwrap();
        let env = true_envelope(&a);
        let mid = env.frames() / 2;
        for p in a.peaks[mid].iter().filter(|p| p.freq < 5_000.0) {
            let db = 20.0 * (env.at(mid, p.freq) / p.amp).log10();
            assert!(db > -1.5, "{} Hz: envelope {db} dB under the peak", p.freq);
        }
    }
}
