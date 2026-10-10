//! A live spectrum of one synth's own output, for its faceplate (#519): the
//! last `SIZE` samples of its bus, before its strip, windowed and transformed
//! when the view asks, as levels in `BANDS` bands spaced evenly in log
//! frequency from `LOWEST_HZ` to `HIGHEST_HZ`.
//!
//! Everything is allocated in `new` (ADR-0002): `push` copies a block into a
//! ring in `render`, and `compute` runs one FFT where the worklet reads the
//! meters, not per sample.

use crate::analysis::fft::Fft;

/// Samples transformed: 43 ms at 48 kHz, a 23 Hz bin.
pub const SIZE: usize = 2048;
/// Bands the view draws.
pub const BANDS: usize = 128;
pub const LOWEST_HZ: f32 = 20.0;
pub const HIGHEST_HZ: f32 = 20_000.0;
/// The level of silence, so the view's scale has a bottom.
pub const FLOOR_DB: f32 = -120.0;

pub struct Spectrum {
    synth: Option<usize>,
    ring: Vec<f32>,
    pos: usize,
    fft: Option<Fft>,
    window: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    /// Each band's FFT bins, `first..last`.
    bands: Vec<(usize, usize)>,
    /// Scales a bin's magnitude to the amplitude of the sine that made it.
    gain: f32,
    out: [f32; BANDS],
}

impl Spectrum {
    pub fn new(sample_rate: f32) -> Spectrum {
        let window: Vec<f32> = (0..SIZE)
            .map(|i| (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / SIZE as f64).cos()) as f32)
            .collect();
        let gain = 2.0 / window.iter().sum::<f32>();
        let bin_hz = sample_rate.max(1.0) / SIZE as f32;
        let bins = SIZE / 2 + 1;
        let top = HIGHEST_HZ.min(0.5 * sample_rate).max(LOWEST_HZ * 2.0);
        let ratio = top / LOWEST_HZ;
        let bands = (0..BANDS)
            .map(|b| {
                let lo = LOWEST_HZ * ratio.powf(b as f32 / BANDS as f32);
                let hi = LOWEST_HZ * ratio.powf((b + 1) as f32 / BANDS as f32);
                let first = ((lo / bin_hz).round() as usize).min(bins - 1);
                let last = ((hi / bin_hz).round() as usize).clamp(first + 1, bins);
                (first, last)
            })
            .collect();
        Spectrum {
            synth: None,
            ring: vec![0.0; SIZE],
            pos: 0,
            fft: Fft::new(SIZE),
            window,
            re: vec![0.0; SIZE],
            im: vec![0.0; SIZE],
            bands,
            gain,
            out: [FLOOR_DB; BANDS],
        }
    }

    /// The synth whose output is watched, or none; a new one starts from silence.
    pub fn watch(&mut self, synth: Option<usize>) {
        if synth != self.synth {
            self.ring.fill(0.0);
            self.out = [FLOOR_DB; BANDS];
        }
        self.synth = synth;
    }

    pub fn watched(&self) -> Option<usize> {
        self.synth
    }

    /// The bands of the last `compute`.
    pub fn bands(&self) -> &[f32; BANDS] {
        &self.out
    }

    /// Append a block of the watched synth's output, both sides averaged when it is stereo.
    pub fn push(&mut self, left: &[f32], right: Option<&[f32]>) {
        for (i, l) in left.iter().enumerate() {
            let v = match right.and_then(|r| r.get(i)) {
                Some(r) => 0.5 * (l + r),
                None => *l,
            };
            if let Some(slot) = self.ring.get_mut(self.pos) {
                *slot = v;
            }
            self.pos = (self.pos + 1) % SIZE;
        }
    }

    /// The bands' levels in dB (`FLOOR_DB` at silence), from the last `SIZE` samples.
    pub fn compute(&mut self) -> &[f32; BANDS] {
        let Some(fft) = &self.fft else {
            return &self.out;
        };
        // Oldest sample first, so the window lies over the ring in order, less
        // its mean: an offset would otherwise leak into the lowest bands.
        let mean = self.ring.iter().sum::<f32>() / SIZE as f32;
        let (newer, older) = self.ring.split_at(self.pos);
        for ((re, x), w) in self
            .re
            .iter_mut()
            .zip(older.iter().chain(newer))
            .zip(&self.window)
        {
            *re = (x - mean) * w;
        }
        self.im.fill(0.0);
        fft.forward(&mut self.re, &mut self.im);
        for (out, &(first, last)) in self.out.iter_mut().zip(&self.bands) {
            let re = self.re.get(first..last).unwrap_or(&[]);
            let im = self.im.get(first..last).unwrap_or(&[]);
            let peak = re
                .iter()
                .zip(im)
                .fold(0.0f32, |m, (r, i)| m.max(r.hypot(*i)));
            *out = (20.0 * (peak * self.gain).max(1e-12).log10()).max(FLOOR_DB);
        }
        &self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn band_of(hz: f32) -> usize {
        ((hz / LOWEST_HZ).ln() / (HIGHEST_HZ / LOWEST_HZ).ln() * BANDS as f32) as usize
    }

    #[test]
    fn a_sine_shows_in_its_band_at_its_level() {
        let mut s = Spectrum::new(RATE);
        s.watch(Some(3));
        let x: Vec<f32> = (0..4_096)
            .map(|i| 0.5 * (std::f32::consts::TAU * 1_000.0 * i as f32 / RATE).sin())
            .collect();
        for block in x.chunks(128) {
            s.push(block, None);
        }
        let out = *s.compute();
        let top = (0..BANDS)
            .max_by(|&a, &b| out[a].total_cmp(&out[b]))
            .unwrap();
        assert!(
            top.abs_diff(band_of(1_000.0)) <= 1,
            "{top} vs {}",
            band_of(1_000.0)
        );
        // 0.5 is −6 dB; a Hann window loses up to 1.4 dB between bins.
        assert!((out[top] + 6.0).abs() < 2.0, "{}", out[top]);
        assert!(out[band_of(10_000.0)] < -60.0, "{}", out[band_of(10_000.0)]);
    }

    #[test]
    fn an_offset_does_not_light_the_bass() {
        let mut s = Spectrum::new(RATE);
        s.watch(Some(0));
        let x: Vec<f32> = (0..4_096)
            .map(|i| 0.3 + 0.2 * (std::f32::consts::TAU * 440.0 * i as f32 / RATE).sin())
            .collect();
        for block in x.chunks(128) {
            s.push(block, None);
        }
        let out = *s.compute();
        // An offset of 0.3 would read about −10 dB here; what is left is the sine's leakage.
        assert!(out[0] < -45.0 && out[3] < -45.0, "{} {}", out[0], out[3]);
        assert!(out[band_of(440.0)] > -20.0, "{}", out[band_of(440.0)]);
    }

    #[test]
    fn silence_and_a_new_synth_are_the_floor() {
        let mut s = Spectrum::new(RATE);
        s.watch(Some(0));
        s.push(&[0.9; 128], Some(&[0.9; 128]));
        s.watch(Some(1));
        assert!(s.compute().iter().all(|v| *v == FLOOR_DB));
        assert_eq!(s.watched(), Some(1));
    }
}
