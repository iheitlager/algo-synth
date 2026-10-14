//! Additive resynthesis (spec 009 Req 4): a sine bank driven by the partial
//! tracks. Frequency and amplitude go in straight lines between frames, and
//! each track fades in over the hop before its first frame and out over the
//! hop after its last.
//!
//! The phase is the integral of the frequency. Locked, it is also pulled to
//! the measured phase at every frame by a constant frequency correction
//! across the hop (after McAulay and Quatieri 1986), so the partials keep
//! their phase relations and the waveform its shape: needed to subtract the
//! sines from the original. A shift, a stretch or a reduction to breakpoints
//! makes the measured phases meaningless, so those resynthesise unlocked.

use super::track::Track;

/// How far a partial's noise spreads around it (Hz): the noise modulator's
/// cutoff, as far as analysis gathers it (`bandwidth::REACH_HZ`).
pub const NOISE_HZ: f32 = 500.0;
/// Partials fade out over this last share of the band below Nyquist, so a
/// shifted or stretched partial leaves without a click.
const NYQUIST_FADE: f32 = 0.1;

/// Low-passed noise of unit variance, seeded so a render repeats exactly.
struct NoiseMod {
    state: u32,
    y: f32,
    a: f32,
    norm: f32,
}

impl NoiseMod {
    fn new(seed: u32, rate: f32) -> NoiseMod {
        let a = (-std::f32::consts::TAU * NOISE_HZ / rate).exp();
        // White noise uniform in −1..1 has variance 1/3; a one-pole low-pass
        // with pole `a` keeps (1 − a)/(1 + a) of it.
        let var = (1.0 / 3.0) * (1.0 - a) / (1.0 + a);
        NoiseMod {
            state: seed.wrapping_mul(2_654_435_761).max(1),
            y: 0.0,
            a,
            norm: 1.0 / var.sqrt().max(1e-9),
        }
    }

    fn next(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        let x = self.state as f32 / u32::MAX as f32 * 2.0 - 1.0;
        self.y = (1.0 - self.a) * x + self.a * self.y;
        self.y * self.norm
    }
}

/// The tracks as mono PCM at `rate`, frame `i` at sample `i * hop`,
/// `frames * hop` samples long. `hop` may differ from the analysis hop: that
/// stretches time and leaves pitch alone. `lock` locks the phase to the
/// tracks' phases. A partial with noise (bandwidth β) is a sinusoid
/// amplitude-modulated by `sqrt(1 − β) + sqrt(β)·ζ`, ζ low-passed noise of unit
/// variance, which keeps its energy and spreads its noise share around it
/// (spec 010 Req 2). A partial fades out over the last tenth below Nyquist.
pub fn resynthesise(tracks: &[Track], frames: usize, hop: f32, rate: f32, lock: bool) -> Vec<f32> {
    if frames == 0 || !(hop.is_finite() && hop > 0.0 && rate.is_finite() && rate > 0.0) {
        return Vec::new();
    }
    let len = (frames as f32 * hop).ceil() as usize;
    let mut out = vec![0.0f32; len];
    let nyquist = 0.5 * rate;
    let fade_from = nyquist * (1.0 - NYQUIST_FADE);
    let step = std::f64::consts::TAU / f64::from(rate);
    for (n, t) in tracks.iter().enumerate() {
        let (Some(&f0), Some(&p0)) = (t.freq.first(), t.phase.first()) else {
            continue;
        };
        // Breakpoints (frequency, amplitude, measured phase): silent one hop
        // either side, so a track never clicks.
        let noisy = t.noise.iter().any(|b| *b > 0.0);
        let mut modulator = NoiseMod::new(n as u32 + 1, rate);
        let points = std::iter::once((f0, 0.0, None, t.noise_at(0)))
            .chain(
                t.freq
                    .iter()
                    .zip(&t.amp)
                    .zip(&t.phase)
                    .enumerate()
                    .map(|(i, ((&f, &a), &p))| (f, a, Some(p), t.noise_at(i))),
            )
            .chain(
                t.freq
                    .last()
                    .map(|&f| (f, 0.0, None, t.noise_at(t.len().saturating_sub(1)))),
            );
        let first = t.start as f32 - 1.0;
        let start = (first * hop).ceil().max(0.0) as usize;
        // The phase at `start`, so the track meets its measured phase at its
        // first frame (the frequency held over the fade-in).
        let lead = (t.start as f32 * hop - start as f32).max(0.0);
        let mut phase = f64::from(p0) - step * f64::from(f0) * f64::from(lead);
        let mut prev: Option<(f32, f32, Option<f32>, f32)> = None;
        for (i, (f, a, measured, b)) in points.enumerate() {
            let Some((pf, pa, from_measured, pb)) = prev.replace((f, a, measured, b)) else {
                continue;
            };
            // From breakpoint i−1 to i: samples in [from, to).
            let at = first + i as f32;
            let from = (((at - 1.0) * hop).ceil().max(0.0) as usize).max(start);
            let to = ((at * hop).ceil().max(0.0) as usize).min(len);
            let ramp = |s: usize| (s as f32 / hop - (at - 1.0)).clamp(0.0, 1.0);
            // Between two measured frames, the correction that lands the
            // integrated phase on the measured one (the nearest turn of it).
            let mut correction = 0.0;
            if let (true, Some(_), Some(target)) = (lock, from_measured, measured)
                && to > from
            {
                let advance: f64 = (from..to)
                    .map(|s| step * f64::from(pf + (f - pf) * ramp(s)))
                    .sum();
                let end = phase + advance;
                let turns = ((end - f64::from(target)) / std::f64::consts::TAU).round();
                let goal = f64::from(target) + turns * std::f64::consts::TAU;
                correction = (goal - end) / (to - from) as f64;
            }
            for (s, y) in out.iter_mut().enumerate().take(to).skip(from) {
                let x = ramp(s);
                let freq = pf + (f - pf) * x;
                let edge = ((nyquist - freq) / (nyquist - fade_from)).clamp(0.0, 1.0);
                let mut amp = (pa + (a - pa) * x) * edge;
                if noisy {
                    let beta = (pb + (b - pb) * x).clamp(0.0, 1.0);
                    amp *= (1.0 - beta).sqrt() + beta.sqrt() * modulator.next();
                }
                *y += amp * phase.cos() as f32;
                phase += step * f64::from(freq) + correction;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::harmonic::harmonics;
    use crate::analysis::{Settings, analyse};
    use crate::mono::osc::{Blep, Osc, Waveform};
    use crate::voice::sine_table;

    const RATE: f32 = 48_000.0;

    fn saw(hz: f32, n: usize) -> Vec<f32> {
        let (blep, table) = (Blep::new(), sine_table());
        let mut osc = Osc::default();
        osc.wave = Waveform::Saw;
        osc.set_increment(hz / RATE);
        (0..n)
            .map(|_| 0.5 * osc.step(&blep, &table, 0.5, None).0)
            .collect()
    }

    fn steady(start: usize, frames: usize, hz: f32, amp: f32) -> Track {
        Track {
            start,
            freq: vec![hz; frames],
            amp: vec![amp; frames],
            phase: vec![0.0; frames],
            ..Track::default()
        }
    }

    #[test]
    fn round_trip_keeps_the_partials() {
        let s = Settings::default();
        let a = analyse(&saw(220.0, 48_000), RATE, &s).unwrap();
        let y = resynthesise(&a.tracks, a.frames(), a.hop as f32, RATE, true);
        let b = analyse(&y, RATE, &s).unwrap();
        let (ha, hb) = (harmonics(&a, 16), harmonics(&b, 16));
        let mid = ha.f0.len() / 2;
        assert!((hb.f0[mid] - 220.0).abs() < 1.0, "{}", hb.f0[mid]);
        for (k, (x, y)) in ha.frame(mid).iter().zip(hb.frame(mid)).enumerate() {
            let db = 20.0 * (y / x).log10();
            assert!(db.abs() < 1.0, "harmonic {}: {db} dB", k + 1);
        }
    }

    #[test]
    fn output_is_finite_and_bounded() {
        let x = saw(110.0, 48_000);
        let a = analyse(&x, RATE, &Settings::default()).unwrap();
        let y = resynthesise(&a.tracks, a.frames(), a.hop as f32, RATE, true);
        assert!(y.len() >= x.len());
        let peak = |v: &[f32]| v.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(y.iter().all(|s| s.is_finite()));
        assert!(peak(&y) < 1.5 * peak(&x), "{} vs {}", peak(&y), peak(&x));
    }

    #[test]
    fn locked_resynthesis_follows_the_waveform() {
        let x = saw(110.0, 48_000);
        let a = analyse(&x, RATE, &Settings::default()).unwrap();
        let y = resynthesise(&a.tracks, a.frames(), a.hop as f32, RATE, true);
        // Away from the ends, where the window hangs over the signal.
        let inner = 4_096..44_000;
        let rms = |v: &mut dyn Iterator<Item = f32>| {
            let (sum, n) = v.fold((0.0, 0), |(s, n), e| (s + e * e, n + 1));
            (sum / n as f32).sqrt()
        };
        let err = rms(&mut x[inner.clone()]
            .iter()
            .zip(&y[inner.clone()])
            .map(|(a, b)| a - b));
        let level = rms(&mut x[inner].iter().copied());
        assert!(err < 0.01 * level, "error {err} against {level}");
    }

    /// Energy of `x` between `lo` and `hi` Hz, by a Hann-windowed DFT at 10 Hz
    /// steps, so a strong sine nearby does not leak into the band.
    fn band_energy(x: &[f32], lo: f32, hi: f32) -> f32 {
        let n = x.len() as f32;
        let mut e = 0.0;
        let mut hz = lo;
        while hz < hi {
            let w = std::f32::consts::TAU * hz / RATE;
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (i, y) in x.iter().enumerate() {
                let hann = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n).cos();
                re += hann * y * (w * i as f32).cos();
                im += hann * y * (w * i as f32).sin();
            }
            e += re * re + im * im;
            hz += 10.0;
        }
        e
    }

    #[test]
    fn a_breathy_tone_keeps_its_breath() {
        let mut noise = crate::mono::noise::Noise::new(11);
        let x: Vec<f32> = (0..24_000)
            .map(|i| {
                0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / RATE).sin()
                    + 0.05 * noise.sample(crate::mono::noise::NoiseColour::White)
            })
            .collect();
        let s = Settings {
            noise: true,
            ..Settings::default()
        };
        let a = analyse(&x, RATE, &s).unwrap();
        let breathy = resynthesise(&a.tracks, a.frames(), a.hop as f32, RATE, false);
        let mut pure = a.tracks.clone();
        pure.iter_mut().for_each(|t| t.noise.clear());
        let clean = resynthesise(&pure, a.frames(), a.hop as f32, RATE, false);
        // Beside the partial, away from its own peak: the original's breath.
        let mid = 4_000..20_000;
        let near = |y: &[f32]| {
            band_energy(&y[mid.clone()], 600.0, 800.0) + band_energy(&y[mid.clone()], 100.0, 300.0)
        };
        let (orig, with, without) = (near(&x), near(&breathy), near(&clean));
        let db = |e: f32| 10.0 * (e / orig).log10();
        // Measured: −4.2 dB with noise, −59 dB without; the modulator's tails
        // spread some of the gathered noise beyond these bands.
        assert!(db(with).abs() < 5.0, "with noise {} dB", db(with));
        assert!(
            db(without) < db(with) - 40.0,
            "without noise {} dB",
            db(without)
        );
    }

    #[test]
    fn noise_resynthesis_repeats_exactly() {
        let mut t = steady(0, 20, 1_000.0, 0.5);
        t.noise = vec![0.3; 20];
        let a = resynthesise(&[t.clone()], 20, 256.0, RATE, false);
        let b = resynthesise(&[t], 20, 256.0, RATE, false);
        assert_eq!(a, b);
        assert!(a.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn tracks_fade_in_and_out() {
        let hop = 256.0;
        let y = resynthesise(&[steady(10, 10, 1_000.0, 0.5)], 40, hop, RATE, false);
        // Silent before frame 9 and after frame 20, and no step anywhere.
        assert!(y[..9 * 256].iter().all(|s| *s == 0.0));
        assert!(y[20 * 256 + 1..].iter().all(|s| *s == 0.0));
        let biggest_step = y.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        // A 1 kHz sine at 0.5 moves at most 2π·1000/48000·0.5 ≈ 0.065 a sample.
        assert!(biggest_step < 0.07, "{biggest_step}");
        let peak = y[12 * 256..18 * 256]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
    }

    #[test]
    fn above_nyquist_is_silent_and_nothing_is_nothing() {
        let y = resynthesise(&[steady(0, 5, 30_000.0, 0.5)], 5, 256.0, RATE, false);
        assert!(y.iter().all(|s| *s == 0.0));
        assert!(resynthesise(&[], 0, 256.0, RATE, false).is_empty());
        assert!(resynthesise(&[steady(0, 5, 100.0, 0.5)], 5, 0.0, RATE, false).is_empty());
    }
}
