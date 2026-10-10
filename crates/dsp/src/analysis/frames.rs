//! An analysed sound for the live paths (ADR-0032, spec 010 Req 9-10): its
//! harmonics as the waves of a PPG wavetable, and its attack as a D-50 PCM
//! sample.

use super::Analysis;
use super::harmonic::f0;
use super::track::Track;
use crate::table::{MAX_USER_SAMPLE, WAVE_LEN, WAVES};

/// Harmonics in a wave: those the generated tables have, well under Nyquist of
/// a 256-sample cycle.
pub const HARMONICS: usize = 31;
/// An attack is at least this long (s), so a fast hammer or pluck keeps its body.
const MIN_ATTACK: f32 = 0.03;
/// The fade at the end of an attack (s).
const FADE: f32 = 0.01;
/// The envelope's step when finding the attack's peak (s): long enough to hold
/// a few cycles of a low note, so a cycle's phase never decides the peak.
const ENVELOPE: f32 = 0.02;

/// `WAVES` waves of `WAVE_LEN` samples from `tracks` (the analysis's own, or
/// transformed ones) over the voiced frames of `a`, spread evenly from its
/// first voiced frame to its last. A partial goes to the harmonic nearest its
/// frequency over the frame's f0, so an inharmonic one is rounded: a table
/// holds whole harmonics only. Each wave is in sine phase, so neighbouring
/// waves never cancel when crossfaded, and normalised to a peak of 1 (the
/// synth's envelope gives the loudness). An unvoiced frame takes the nearest
/// voiced one's harmonics. Empty when no frame is voiced.
pub fn to_table(a: &Analysis, tracks: &[Track]) -> Vec<f32> {
    let f0s: Vec<f32> = a.peaks.iter().map(|p| f0(p).unwrap_or(0.0)).collect();
    let voiced: Vec<usize> = (0..f0s.len())
        .filter(|&f| f0s.get(f).is_some_and(|f0| *f0 > 0.0))
        .collect();
    let (Some(&first), Some(&last)) = (voiced.first(), voiced.last()) else {
        return Vec::new();
    };
    let nearest = |f: usize| {
        let i = voiced.partition_point(|&v| v < f);
        let after = voiced.get(i).copied();
        let before = i.checked_sub(1).and_then(|j| voiced.get(j)).copied();
        match (before, after) {
            (Some(b), Some(a)) if f - b <= a - f => b,
            (_, Some(a)) => a,
            (Some(b), None) => b,
            (None, None) => first,
        }
    };
    let mut out = vec![0.0; WAVES * WAVE_LEN];
    for (k, wave) in out.chunks_exact_mut(WAVE_LEN).enumerate() {
        let frame = nearest(first + (last - first) * k / (WAVES - 1).max(1));
        let fund = f0s.get(frame).copied().unwrap_or(1.0);
        let mut amps = [0.0f32; HARMONICS];
        for t in tracks {
            let Some(i) = frame.checked_sub(t.start).filter(|i| *i < t.len()) else {
                continue;
            };
            let (f, a) = (
                t.freq.get(i).copied().unwrap_or(0.0),
                t.amp.get(i).copied().unwrap_or(0.0),
            );
            let n = (f / fund).round() as usize;
            if let Some(slot) = n.checked_sub(1).and_then(|n| amps.get_mut(n)) {
                // Partials rounded onto one harmonic add their energy.
                *slot = (*slot * *slot + a * a).sqrt();
            }
        }
        for (i, y) in wave.iter_mut().enumerate() {
            let x = std::f32::consts::TAU * i as f32 / WAVE_LEN as f32;
            *y = amps
                .iter()
                .enumerate()
                .map(|(n, a)| a * ((n + 1) as f32 * x).sin())
                .sum();
        }
        let peak = wave.iter().fold(0.0f32, |m, y| m.max(y.abs()));
        if peak > 1e-9 {
            wave.iter_mut().for_each(|y| *y /= peak);
        }
    }
    out
}

/// The attack of `x` at `rate`: up to its loudest moment, at least
/// `MIN_ATTACK` long, then faded out over `FADE`; at most `MAX_USER_SAMPLE`.
pub fn split_attack(x: &[f32], rate: f32) -> Vec<f32> {
    if x.is_empty() || !(rate.is_finite() && rate > 0.0) {
        return Vec::new();
    }
    let step = ((ENVELOPE * rate) as usize).max(1);
    // The loudest step in the first second or so: where the attack ends.
    let search = x.len().min((rate as usize).max(step));
    let peak = x
        .get(..search)
        .unwrap_or(x)
        .chunks(step)
        .enumerate()
        .map(|(i, c)| (i, c.iter().map(|v| v * v).sum::<f32>()))
        .fold(
            (0, -1.0),
            |best, (i, e)| if e > best.1 { (i, e) } else { best },
        )
        .0;
    let end = ((peak + 1) * step).max((MIN_ATTACK * rate) as usize);
    let fade = ((FADE * rate) as usize).max(1);
    let len = (end + fade).min(x.len()).min(MAX_USER_SAMPLE);
    let fade_from = len.saturating_sub(fade);
    x.iter()
        .take(len)
        .enumerate()
        .map(|(i, v)| {
            if i < fade_from {
                *v
            } else {
                v * (len - i) as f32 / fade as f32
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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

    /// Harmonic `k` of one cycle, by correlation.
    fn harmonic(wave: &[f32], k: usize) -> f32 {
        let n = wave.len() as f32;
        let (mut s, mut c) = (0.0, 0.0);
        for (i, y) in wave.iter().enumerate() {
            let x = std::f32::consts::TAU * k as f32 * i as f32 / n;
            s += y * x.sin();
            c += y * x.cos();
        }
        2.0 * s.hypot(c) / n
    }

    #[test]
    fn a_saw_becomes_a_saw_table() {
        let a = analyse(&saw(220.0, 48_000), RATE, &Settings::default()).unwrap();
        let t = to_table(&a, &a.tracks);
        assert_eq!(t.len(), WAVES * WAVE_LEN);
        let wave = &t[32 * WAVE_LEN..33 * WAVE_LEN];
        assert!((wave.iter().fold(0.0f32, |m, y| m.max(y.abs())) - 1.0).abs() < 1e-4);
        let h1 = harmonic(wave, 1);
        for k in 2..=16 {
            let db = 20.0 * (harmonic(wave, k) * k as f32 / h1).log10();
            assert!(db.abs() < 1.5, "harmonic {k}: {db} dB");
        }
    }

    #[test]
    fn noise_makes_no_table() {
        let mut noise = crate::mono::noise::Noise::new(3);
        let x: Vec<f32> = (0..24_000)
            .map(|_| 0.3 * noise.sample(crate::mono::noise::NoiseColour::White))
            .collect();
        let a = analyse(&x, RATE, &Settings::default()).unwrap();
        // A frame or two may look voiced in noise; a table needs at least one.
        let t = to_table(&a, &a.tracks);
        assert!(t.is_empty() || t.len() == WAVES * WAVE_LEN);
    }

    #[test]
    fn split_finds_the_attack() {
        // A 50 ms rise, then a quick decay: the loudest 20 ms are around 50 ms.
        let x: Vec<f32> = (0..48_000)
            .map(|i| {
                let t = i as f32 / RATE;
                let env = if t < 0.05 {
                    t / 0.05
                } else {
                    (-(t - 0.05) * 30.0).exp()
                };
                env * (std::f32::consts::TAU * 220.0 * t).sin()
            })
            .collect();
        let a = split_attack(&x, RATE);
        let ms = a.len() as f32 / RATE * 1000.0;
        assert!((55.0..=75.0).contains(&ms), "{ms} ms");
        assert!(a.last().unwrap().abs() < 0.01, "faded out");
        assert_eq!(&a[..100], &x[..100]);
        // A click is still 40 ms (30 + the fade).
        let mut click = vec![0.0; 48_000];
        click[10] = 1.0;
        assert_eq!(split_attack(&click, RATE).len(), (0.04 * RATE) as usize);
        assert!(split_attack(&[], RATE).is_empty());
    }
}
