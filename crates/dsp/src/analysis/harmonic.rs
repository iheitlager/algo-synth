//! Harmonic mode (spec 009 Req 3): a fundamental per frame from its peaks,
//! then harmonic k's amplitude read at k·f0.

use super::Analysis;
use super::peaks::Peak;

/// The range a fundamental is looked for in.
pub const MIN_F0: f32 = 30.0;
pub const MAX_F0: f32 = 4_000.0;
/// The most harmonics a fundamental explains or a frame reads.
pub const MAX_HARMONICS: usize = 64;
/// How far from k·f0 a peak may be, as a fraction of f0.
const TOLERANCE: f32 = 0.03;
/// The candidates come from this many loudest peaks, each divided by 1..=this.
const SOURCES: usize = 8;
/// A frame is voiced when one fundamental explains this much of its energy.
const VOICED: f32 = 0.7;
/// Of the fundamentals within this share of the best, the highest wins, so
/// an octave below the true one never does.
const NEAR_BEST: f32 = 0.9;

/// The fundamental of a frame's peaks (sorted by frequency), or `None` for
/// an unvoiced frame.
pub fn f0(peaks: &[Peak]) -> Option<f32> {
    let total: f32 = peaks.iter().map(|p| p.amp * p.amp).sum();
    if total <= 0.0 {
        return None;
    }
    let mut loudest: Vec<&Peak> = peaks.iter().collect();
    loudest.sort_unstable_by(|a, b| b.amp.total_cmp(&a.amp));
    let candidates: Vec<(f32, f32)> = loudest
        .iter()
        .take(SOURCES)
        .flat_map(|p| (1..=SOURCES).map(move |d| p.freq / d as f32))
        .filter(|c| (MIN_F0..=MAX_F0).contains(c))
        .map(|c| fit(peaks, c))
        .collect();
    let best = candidates.iter().map(|c| c.1).fold(0.0, f32::max);
    if best < VOICED * total {
        return None;
    }
    candidates
        .iter()
        .filter(|c| c.1 >= NEAR_BEST * best)
        .map(|c| c.0)
        .max_by(f32::total_cmp)
}

/// A candidate refined by the peaks it explains, and the energy they hold.
fn fit(peaks: &[Peak], c: f32) -> (f32, f32) {
    let (mut energy, mut sum) = (0.0, 0.0);
    for p in peaks {
        let k = (p.freq / c).round();
        if (1.0..=MAX_HARMONICS as f32).contains(&k) && (p.freq / c - k).abs() < TOLERANCE {
            let e = p.amp * p.amp;
            energy += e;
            sum += e * p.freq / k;
        }
    }
    (if energy > 0.0 { sum / energy } else { c }, energy)
}

/// Harmonics per frame: the fundamental (0 when unvoiced) and `count`
/// amplitudes, harmonic 1 first, 0 where no peak is near k·f0.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Harmonics {
    pub count: usize,
    pub f0: Vec<f32>,
    pub amp: Vec<f32>,
}

impl Harmonics {
    /// Frame `frame`'s amplitudes.
    pub fn frame(&self, frame: usize) -> &[f32] {
        self.amp
            .get(frame * self.count..(frame + 1) * self.count)
            .unwrap_or(&[])
    }
}

/// The first `count` (at most `MAX_HARMONICS`) harmonics of every frame.
pub fn harmonics(a: &Analysis, count: usize) -> Harmonics {
    let count = count.min(MAX_HARMONICS);
    let mut h = Harmonics {
        count,
        f0: Vec::with_capacity(a.peaks.len()),
        amp: vec![0.0; a.peaks.len() * count],
    };
    for (frame, peaks) in a.peaks.iter().enumerate() {
        let f = f0(peaks).unwrap_or(0.0);
        h.f0.push(f);
        if f == 0.0 {
            continue;
        }
        let row = h
            .amp
            .get_mut(frame * count..(frame + 1) * count)
            .unwrap_or(&mut []);
        for (k, amp) in row.iter_mut().enumerate() {
            let target = (k + 1) as f32 * f;
            let i = peaks.partition_point(|p| p.freq < target);
            let near = [i.checked_sub(1), Some(i)]
                .into_iter()
                .flatten()
                .filter_map(|i| peaks.get(i))
                .filter(|p| (p.freq - target).abs() < TOLERANCE * f)
                .min_by(|a, b| (a.freq - target).abs().total_cmp(&(b.freq - target).abs()));
            *amp = near.map_or(0.0, |p| p.amp);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{Settings, analyse};
    use crate::mono::osc::{Blep, Osc, Waveform};
    use crate::voice::sine_table;

    const RATE: f32 = 48_000.0;

    /// A second of the engine's own band-limited sawtooth (its ±1 ramp has
    /// harmonic k at 2/(πk)).
    fn saw(hz: f32) -> Vec<f32> {
        let (blep, table) = (Blep::new(), sine_table());
        let mut osc = Osc::default();
        osc.wave = Waveform::Saw;
        osc.set_increment(hz / RATE);
        (0..48_000)
            .map(|_| 0.5 * osc.step(&blep, &table, 0.5, None).0)
            .collect()
    }

    #[test]
    fn f0_of_a_rendered_saw() {
        let a = analyse(&saw(220.0), RATE, &Settings::default()).unwrap();
        let h = harmonics(&a, 16);
        let inner = &h.f0[10..h.f0.len() - 10];
        assert!(inner.iter().all(|f| (f - 220.0).abs() < 1.0), "{inner:?}");
    }

    #[test]
    fn saw_harmonics_fall_as_one_over_k() {
        let a = analyse(&saw(220.0), RATE, &Settings::default()).unwrap();
        let h = harmonics(&a, 16);
        let row = h.frame(h.f0.len() / 2);
        for (k, amp) in row.iter().enumerate() {
            let db = 20.0 * (amp * (k + 1) as f32 / row[0]).log10();
            assert!(db.abs() < 1.0, "harmonic {}: {db} dB", k + 1);
        }
    }

    #[test]
    fn noise_is_unvoiced() {
        let mut noise = crate::mono::noise::Noise::new(7);
        let x: Vec<f32> = (0..48_000)
            .map(|_| 0.3 * noise.sample(crate::mono::noise::NoiseColour::White))
            .collect();
        let a = analyse(&x, RATE, &Settings::default()).unwrap();
        let voiced = a.peaks.iter().filter(|p| f0(p).is_some()).count();
        assert!(voiced * 20 < a.peaks.len(), "{voiced} of {}", a.peaks.len());
    }

    #[test]
    fn an_octave_below_never_wins() {
        let peaks: Vec<Peak> = (1..=10)
            .map(|k| Peak {
                freq: 300.0 * k as f32,
                amp: 1.0 / k as f32,
                phase: 0.0,
            })
            .collect();
        assert_eq!(f0(&peaks).map(f32::round), Some(300.0));
    }
}
