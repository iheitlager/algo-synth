//! A spectrogram for the Spectral Lab to paint (spec 009 Req 7): each frame's
//! level in `BANDS` bands spaced evenly in log frequency from `LOWEST_HZ` to
//! Nyquist, one byte each, 0 at `FLOOR_DB` or below and 255 at 0 dB.

use super::stft::Stft;

/// Bands per frame.
pub const BANDS: usize = 256;
/// The lowest band's lower edge; the view draws the same axis.
pub const LOWEST_HZ: f32 = 30.0;
/// The level a byte of 0 stands for.
pub const FLOOR_DB: f32 = -100.0;

/// The spectrogram of `x` at `rate`, frame `i` centred on sample `i * hop`:
/// `frames * BANDS` bytes, empty when the window or hop is out of range.
pub fn spectrogram(x: &[f32], rate: f32, window: usize, hop: usize) -> Vec<u8> {
    let Some(mut stft) = Stft::new(window) else {
        return Vec::new();
    };
    if hop == 0 || x.is_empty() || !(rate.is_finite() && rate > 0.0) {
        return Vec::new();
    }
    let bin_hz = stft.bin_hz(rate);
    let bins = stft.bins();
    // Each band's FFT bins, at least one, so a narrow low band reads its nearest bin.
    let ratio = (0.5 * rate / LOWEST_HZ).max(1.0);
    let bands: Vec<(usize, usize)> = (0..BANDS)
        .map(|b| {
            let lo = LOWEST_HZ * ratio.powf(b as f32 / BANDS as f32);
            let hi = LOWEST_HZ * ratio.powf((b + 1) as f32 / BANDS as f32);
            let first = ((lo / bin_hz).round() as usize).min(bins - 1);
            let last = ((hi / bin_hz).round() as usize).clamp(first + 1, bins);
            (first, last)
        })
        .collect();
    let frames = x.len() / hop + 1;
    let mut out = Vec::with_capacity(frames * BANDS);
    for frame in 0..frames {
        let (mag, _) = stft.frame(x, frame * hop);
        for &(first, last) in &bands {
            let peak = mag
                .get(first..last)
                .map_or(0.0, |m| m.iter().copied().fold(0.0, f32::max));
            let db = 20.0 * peak.max(1e-12).log10();
            let level = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
            out.push((level * 255.0).round() as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    /// The band a frequency falls in.
    fn band_of(hz: f32) -> usize {
        let ratio = 0.5 * RATE / LOWEST_HZ;
        ((hz / LOWEST_HZ).ln() / ratio.ln() * BANDS as f32) as usize
    }

    #[test]
    fn a_sine_lights_its_band() {
        let x: Vec<f32> = (0..24_000)
            .map(|i| 0.5 * (std::f32::consts::TAU * 1_000.0 * i as f32 / RATE).sin())
            .collect();
        let g = spectrogram(&x, RATE, 2048, 256);
        assert_eq!(g.len(), (24_000 / 256 + 1) * BANDS);
        let row = &g[40 * BANDS..41 * BANDS];
        let top = (0..BANDS).max_by_key(|&b| row[b]).unwrap();
        assert!(
            top.abs_diff(band_of(1_000.0)) <= 1,
            "{top} vs {}",
            band_of(1_000.0)
        );
        // 0.5 is −6 dB: 94% of the way from the floor.
        assert!(row[top] > 230, "{}", row[top]);
        // Far from it, the floor.
        assert!(row[band_of(8_000.0)] < 60, "{}", row[band_of(8_000.0)]);
    }

    #[test]
    fn silence_is_the_floor_and_bad_input_is_empty() {
        assert!(
            spectrogram(&[0.0; 4_096], RATE, 1024, 256)
                .iter()
                .all(|b| *b == 0)
        );
        assert!(spectrogram(&[], RATE, 1024, 256).is_empty());
        assert!(spectrogram(&[0.0; 100], RATE, 1000, 256).is_empty());
        assert!(spectrogram(&[0.0; 100], RATE, 1024, 0).is_empty());
    }
}
