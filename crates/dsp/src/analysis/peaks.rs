//! A frame's spectral peaks (spec 009 Req 2): local maxima above a floor,
//! each placed between bins by a parabola through the log magnitudes.

/// A sinusoid in one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Peak {
    pub freq: f32,
    pub amp: f32,
    pub phase: f32,
    /// How much of `amp` is noise around the sinusoid, 0..=1 (bandwidth,
    /// spec 010 Req 2); 0 unless the analysis associates noise.
    pub noise: f32,
}

/// Peaks quieter than this (−100 dBFS) are never kept, so silence has none.
const SILENCE: f32 = 1e-5;

/// The peaks of one frame's amplitudes and phases into `out`, sorted by
/// frequency: at most `max`, the loudest, none more than `floor_db` under
/// the frame's largest.
pub fn peaks(
    mag: &[f32],
    phase: &[f32],
    bin_hz: f32,
    floor_db: f32,
    max: usize,
    out: &mut Vec<Peak>,
) {
    out.clear();
    let top = mag.iter().copied().fold(0.0, f32::max);
    let floor = (top * 10f32.powf(floor_db.min(0.0) / 20.0)).max(SILENCE);
    for (k, w) in mag.windows(3).enumerate() {
        let [a, b, c] = [w.first(), w.get(1), w.get(2)].map(|v| v.copied().unwrap_or(0.0));
        if b < floor || b <= a || b < c {
            continue;
        }
        let (la, lb, lc) = ((a + 1e-12).ln(), b.ln(), (c + 1e-12).ln());
        let curve = la - 2.0 * lb + lc;
        let p = if curve < 0.0 {
            (0.5 * (la - lc) / curve).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        let bin = k + 1;
        out.push(Peak {
            freq: (bin as f32 + p) * bin_hz,
            amp: (lb - 0.25 * (la - lc) * p).exp(),
            phase: phase.get(bin).copied().unwrap_or(0.0),
            noise: 0.0,
        });
    }
    if out.len() > max {
        out.sort_unstable_by(|x, y| y.amp.total_cmp(&x.amp));
        out.truncate(max);
        out.sort_unstable_by(|x, y| x.freq.total_cmp(&y.freq));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::stft::Stft;

    const RATE: f32 = 48_000.0;

    fn sine(hz: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (std::f32::consts::TAU * hz * i as f32 / RATE).sin())
            .collect()
    }

    #[test]
    fn interpolation_finds_the_frequency_between_bins() {
        let mut stft = Stft::new(2048).unwrap();
        let bin_hz = stft.bin_hz(RATE);
        let mut out = Vec::new();
        for hz in [100.0, 441.3, 1_000.0, 3_517.0, 12_345.6] {
            let x = sine(hz, 0.3, 8_192);
            let (mag, phase) = stft.frame(&x, 4_096);
            peaks(mag, phase, bin_hz, -60.0, 256, &mut out);
            let p = out.iter().max_by(|a, b| a.amp.total_cmp(&b.amp)).unwrap();
            assert!((p.freq - hz).abs() < 0.2, "{hz}: {}", p.freq);
            assert!(
                (20.0 * (p.amp / 0.3).log10()).abs() < 0.1,
                "{hz}: {}",
                p.amp
            );
        }
    }

    #[test]
    fn silence_and_the_floor_give_nothing() {
        let mut stft = Stft::new(1024).unwrap();
        let mut out = vec![Peak::default()];
        let bin_hz = stft.bin_hz(RATE);
        let (mag, phase) = stft.frame(&[0.0; 4_096], 2_048);
        peaks(mag, phase, bin_hz, -80.0, 256, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn the_loudest_are_kept_in_frequency_order() {
        let mut x = sine(500.0, 0.5, 8_192);
        for (i, s) in sine(2_000.0, 0.1, 8_192).iter().enumerate() {
            x[i] += s;
        }
        for (i, s) in sine(5_000.0, 0.3, 8_192).iter().enumerate() {
            x[i] += s;
        }
        let mut stft = Stft::new(2048).unwrap();
        let mut out = Vec::new();
        let bin_hz = stft.bin_hz(RATE);
        let (mag, phase) = stft.frame(&x, 4_096);
        peaks(mag, phase, bin_hz, -40.0, 2, &mut out);
        let hz: Vec<f32> = out.iter().map(|p| p.freq.round()).collect();
        assert_eq!(hz, [500.0, 5_000.0]);
    }
}
