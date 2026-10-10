//! The short-time Fourier transform (spec 009 Req 1): a 4-term
//! Blackman–Harris window, whose sidelobes (−92 dB) stay under the peak
//! floor so they never make tracks of their own, zero-padded to twice its
//! length and rotated so its centre is time zero, so each bin's phase is the
//! phase at the frame's centre.

use super::fft::{self, Fft};

pub struct Stft {
    fft: Fft,
    window: Vec<f32>,
    /// Scales a bin's magnitude to the amplitude of the sine that made it.
    gain: f32,
    /// What a sine of amplitude 1 puts into its lobe, in squared scaled magnitudes.
    lobe_energy: f32,
    re: Vec<f32>,
    im: Vec<f32>,
    mag: Vec<f32>,
    phase: Vec<f32>,
}

impl Stft {
    /// A transform with a window of `size` points, or `None` unless `size`
    /// is a power of two that fits twice into the largest FFT.
    pub fn new(size: usize) -> Option<Stft> {
        if size < fft::MIN {
            return None;
        }
        let fft = Fft::new(2 * size)?;
        let window: Vec<f32> = (0..size)
            .map(|i| {
                let x = std::f64::consts::TAU * i as f64 / size as f64;
                (0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos()
                    - 0.01168 * (3.0 * x).cos()) as f32
            })
            .collect();
        let gain = 2.0 / window.iter().sum::<f32>();
        let bins = size + 1;
        // The window's own spectrum, peak 1: a sine's lobe has its shape.
        let lobe_energy = {
            let n = 2 * size;
            let (mut re, mut im) = (vec![0.0; n], vec![0.0; n]);
            for (r, w) in re.iter_mut().zip(&window) {
                *r = *w;
            }
            fft.forward(&mut re, &mut im);
            let peak = re.first().map_or(1.0, |r| r.hypot(0.0)).max(1e-12);
            re.iter()
                .zip(&im)
                .map(|(r, i)| (r * r + i * i) / (peak * peak))
                .sum::<f32>()
        };
        Some(Stft {
            fft,
            window,
            gain,
            lobe_energy,
            re: vec![0.0; 2 * size],
            im: vec![0.0; 2 * size],
            mag: vec![0.0; bins],
            phase: vec![0.0; bins],
        })
    }

    /// What a sine of amplitude 1 puts into the bins of its lobe, in squared
    /// amplitudes as `frame` gives them: the sum of the window's spectrum
    /// squared over its peak squared.
    pub fn lobe_energy(&self) -> f32 {
        self.lobe_energy
    }

    /// Bins from DC to Nyquist.
    pub fn bins(&self) -> usize {
        self.mag.len()
    }

    /// The width of a bin in hertz at `rate`.
    pub fn bin_hz(&self, rate: f32) -> f32 {
        rate / self.fft.len() as f32
    }

    /// The frame centred on sample `centre` of `x`, zero outside it: each
    /// bin's amplitude and phase.
    pub fn frame(&mut self, x: &[f32], centre: usize) -> (&[f32], &[f32]) {
        let n = self.re.len();
        let size = self.window.len();
        self.re.fill(0.0);
        self.im.fill(0.0);
        for (i, w) in self.window.iter().enumerate() {
            // Sample i of the window is at time i − size/2 from the centre,
            // which sits at index 0 of the buffer.
            let s = (centre + i)
                .checked_sub(size / 2)
                .and_then(|t| x.get(t))
                .copied()
                .unwrap_or(0.0);
            let at = (i + n - size / 2) % n;
            if let Some(v) = self.re.get_mut(at) {
                *v = s * w;
            }
        }
        self.fft.forward(&mut self.re, &mut self.im);
        for (((m, p), re), im) in self
            .mag
            .iter_mut()
            .zip(&mut self.phase)
            .zip(&self.re)
            .zip(&self.im)
        {
            *m = re.hypot(*im) * self.gain;
            *p = im.atan2(*re);
        }
        (&self.mag, &self.phase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_checked() {
        assert!(Stft::new(2048).is_some());
        assert!(Stft::new(32).is_none());
        assert!(Stft::new(1000).is_none());
        assert!(Stft::new(fft::MAX).is_none());
    }

    #[test]
    fn a_sine_gives_its_amplitude_and_centre_phase() {
        let (rate, hz) = (48_000.0, 1_000.0);
        let x: Vec<f32> = (0..48_000)
            .map(|i| 0.5 * (std::f32::consts::TAU * hz * i as f32 / rate).cos())
            .collect();
        let mut stft = Stft::new(2048).unwrap();
        let bin_hz = stft.bin_hz(rate);
        // 1 kHz is a whole number of cycles in 48 samples: at a centre on a
        // multiple of 48 the cosine's phase is 0.
        let (mag, phase) = stft.frame(&x, 48 * 200);
        let k = (hz / bin_hz).round() as usize;
        let top = (0..mag.len())
            .max_by(|&a, &b| mag[a].total_cmp(&mag[b]))
            .unwrap();
        assert_eq!(top, k);
        assert!((mag[k] - 0.5).abs() < 0.01, "{}", mag[k]);
        assert!(phase[k].abs() < 0.05, "{}", phase[k]);
    }
}
