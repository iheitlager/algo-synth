//! Noise per partial (spec 010 Req 2), after Fitz, Haken & Christensen's
//! bandwidth association (ICMC 2000): peaks too quiet to be partials, and the
//! spectral energy no peak explains, go to the nearest partial, which becomes a
//! sinusoid with a bandwidth: how much of its amplitude is noise around it.

use super::peaks::Peak;

/// FFT bins either side of a peak that are its own main lobe: a 4-term
/// Blackman–Harris lobe is ±4 bins of its window, ±8 zero-padded twice.
pub const LOBE: usize = 8;
/// Peaks more than this under the frame's loudest are noise, not partials (dB).
pub const PARTIAL_DB: f32 = -40.0;
/// How far from a partial its noise may lie (Hz): as far as resynthesis
/// spreads it (`additive::NOISE_HZ`), so noise stays where it was.
pub const REACH_HZ: f32 = 500.0;
/// Bins quieter than this are not counted as noise (−100 dBFS).
const QUIET: f32 = 1e-5;

/// Turn a frame's peaks (sorted by frequency) into partials with bandwidth.
/// Peaks more than `PARTIAL_DB` under the loudest are removed and their energy
/// goes to the nearest partial within `REACH_HZ`, as does the energy of bins
/// outside every peak's lobe. `mag` is the frame's amplitude spectrum,
/// `bin_hz` its bin width, and `lobe_energy` what a sine of amplitude 1 puts
/// into its lobe (`Stft::lobe_energy`), which converts a bin's energy into a
/// sine's amplitude squared.
pub fn associate(mag: &[f32], peaks: &mut Vec<Peak>, bin_hz: f32, lobe_energy: f32) {
    if peaks.is_empty() || bin_hz <= 0.0 || lobe_energy <= 0.0 {
        return;
    }
    let loudest = peaks.iter().fold(0.0f32, |m, p| m.max(p.amp));
    let floor = loudest * 10f32.powf(PARTIAL_DB / 20.0);
    let bins = mag.len();
    let mut covered = vec![false; bins];
    for p in peaks.iter() {
        let centre = (p.freq / bin_hz).round() as usize;
        for c in covered
            .iter_mut()
            .take((centre + LOBE + 1).min(bins))
            .skip(centre.saturating_sub(LOBE))
        {
            *c = true;
        }
    }
    // Energy (as a sine's amplitude squared) at a frequency, from quiet peaks
    // and uncovered bins.
    let mut found: Vec<(f32, f32)> = peaks
        .iter()
        .filter(|p| p.amp < floor)
        .map(|p| (p.freq, p.amp * p.amp))
        .collect();
    for (k, (m, c)) in mag.iter().zip(&covered).enumerate().skip(1) {
        if !*c && *m >= QUIET {
            found.push((k as f32 * bin_hz, m * m / lobe_energy));
        }
    }
    peaks.retain(|p| p.amp >= floor);
    let mut noise = vec![0.0f32; peaks.len()];
    for (f, e) in found {
        let i = peaks.partition_point(|p| p.freq < f);
        let below = i.checked_sub(1);
        let nearest = match (below.and_then(|b| peaks.get(b)), peaks.get(i)) {
            (Some(lo), Some(hi)) if f - lo.freq <= hi.freq - f => below,
            (_, Some(_)) => Some(i),
            (Some(_), None) => below,
            (None, None) => None,
        };
        let near =
            nearest.filter(|&n| peaks.get(n).is_some_and(|p| (p.freq - f).abs() <= REACH_HZ));
        if let Some(n) = near.and_then(|n| noise.get_mut(n)) {
            *n += e;
        }
    }
    for (p, n) in peaks.iter_mut().zip(noise) {
        let total = p.amp * p.amp + n;
        if total > 0.0 {
            p.amp = total.sqrt();
            p.noise = n / total;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{Settings, analyse};
    use crate::mono::noise::{Noise, NoiseColour};
    use crate::mono::osc::{Blep, Osc, Waveform};
    use crate::voice::sine_table;

    const RATE: f32 = 48_000.0;

    fn sine(hz: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (std::f32::consts::TAU * hz * i as f32 / RATE).sin())
            .collect()
    }

    fn with_noise() -> Settings {
        Settings {
            noise: true,
            ..Settings::default()
        }
    }

    /// The track nearest `hz` that lasts at least half the sound.
    fn long_near(a: &crate::analysis::Analysis, hz: f32) -> &crate::analysis::track::Track {
        a.tracks
            .iter()
            .filter(|t| 2 * t.len() >= a.frames())
            .min_by(|x, y| {
                let d = |t: &crate::analysis::track::Track| (t.freq[t.len() / 2] - hz).abs();
                d(x).total_cmp(&d(y))
            })
            .expect("a long track")
    }

    #[test]
    fn noise_goes_to_the_nearest_partial() {
        // Two peaks and noise only in two bins, one near each.
        let mut mag = vec![0.0; 200];
        mag[30] = 0.1;
        mag[150] = 0.2;
        let mut peaks = vec![
            Peak {
                freq: 50.0,
                amp: 1.0,
                ..Peak::default()
            },
            Peak {
                freq: 170.0,
                amp: 1.0,
                ..Peak::default()
            },
            // Too quiet to be a partial: its energy goes to the one at 170.
            Peak {
                freq: 180.0,
                amp: 0.001,
                ..Peak::default()
            },
        ];
        associate(&mag, &mut peaks, 1.0, 1.0);
        assert!(
            (peaks[0].noise - 0.01 / 1.01).abs() < 1e-6,
            "{}",
            peaks[0].noise
        );
        let n = 0.04 + 0.001 * 0.001;
        assert_eq!(peaks.len(), 2);
        assert!(
            (peaks[1].noise - n / (1.0 + n)).abs() < 1e-6,
            "{}",
            peaks[1].noise
        );
        assert!((peaks[1].amp - (1.0 + n).sqrt()).abs() < 1e-6);
    }

    #[test]
    fn a_saw_has_almost_no_bandwidth() {
        let (blep, table) = (Blep::new(), sine_table());
        let mut osc = Osc::default();
        osc.wave = Waveform::Saw;
        osc.set_increment(220.0 / RATE);
        let x: Vec<f32> = (0..48_000)
            .map(|_| 0.5 * osc.step(&blep, &table, 0.5, None).0)
            .collect();
        let a = analyse(&x, RATE, &with_noise()).unwrap();
        // The audible harmonics: the weakest near Nyquist pick up the
        // oscillator's aliasing residue as noise.
        for t in a
            .tracks
            .iter()
            .filter(|t| 2 * t.len() >= a.frames() && t.freq[t.len() / 2] < 8_000.0)
        {
            let mid = t.noise_at(t.len() / 2);
            assert!(mid < 0.05, "{} Hz: {mid}", t.freq[t.len() / 2]);
        }
    }

    #[test]
    fn a_breathy_tone_has_bandwidth() {
        let mut noise = Noise::new(11);
        let x: Vec<f32> = sine(220.0, 0.5, 48_000)
            .iter()
            .map(|s| s + 0.05 * noise.sample(NoiseColour::White))
            .collect();
        let a = analyse(&x, RATE, &with_noise()).unwrap();
        let t = long_near(&a, 220.0);
        let mid = t.noise_at(t.len() / 2);
        assert!(mid > 0.0 && mid < 0.5, "{mid}");
        // Without association the same analysis has none.
        let plain = analyse(&x, RATE, &Settings::default()).unwrap();
        assert!(
            plain
                .tracks
                .iter()
                .all(|t| t.noise.iter().all(|n| *n == 0.0))
        );
    }
}
