//! Spectral analysis for the Spectral Lab (ADR-0017, spec 009): a WAV's
//! short-time spectrum, its peaks, and the partial tracks they make.
//!
//! These are offline calls, not `render`: they allocate what a sound needs,
//! within `MAX_SECONDS` and `MAX_PEAKS`, and never panic.

pub mod fft;
pub mod harmonic;
pub mod peaks;
pub mod stft;
pub mod track;

use peaks::Peak;
use stft::Stft;
use track::{Track, Tracker};

/// The longest signal analysed.
pub const MAX_SECONDS: f32 = 60.0;
/// The most peaks kept in a frame.
pub const MAX_PEAKS: usize = 256;

/// How a signal is analysed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Window length, a power of two from 64 to 8192.
    pub window: usize,
    /// Samples between frame centres, 1 to `window`.
    pub hop: usize,
    /// Peaks quieter than this under the frame's loudest are dropped (dB).
    pub floor_db: f32,
    /// The widest step of a track between frames, in cents.
    pub cents: f32,
    /// Frames a track may miss before it ends.
    pub gap: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            window: 4096,
            hop: 256,
            floor_db: -80.0,
            cents: 50.0,
            gap: 3,
        }
    }
}

/// Why a signal was not analysed. `code` is what crosses the C ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// No samples, or a rate that is not positive.
    Empty,
    /// Longer than `MAX_SECONDS`.
    TooLong,
    /// A window or hop out of range.
    BadSettings,
}

impl Error {
    pub fn code(self) -> i32 {
        match self {
            Error::Empty => -1,
            Error::TooLong => -2,
            Error::BadSettings => -3,
        }
    }
}

/// A signal taken apart: its peaks per frame and the tracks through them.
/// Frame `i` is centred on sample `i * hop`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    pub rate: f32,
    pub hop: usize,
    pub peaks: Vec<Vec<Peak>>,
    pub tracks: Vec<Track>,
}

impl Analysis {
    pub fn frames(&self) -> usize {
        self.peaks.len()
    }
}

/// Analyse a mono signal at `rate`.
pub fn analyse(x: &[f32], rate: f32, s: &Settings) -> Result<Analysis, Error> {
    if x.is_empty() || !rate.is_finite() || rate <= 0.0 {
        return Err(Error::Empty);
    }
    if x.len() as f32 > MAX_SECONDS * rate {
        return Err(Error::TooLong);
    }
    if s.hop == 0 || s.hop > s.window {
        return Err(Error::BadSettings);
    }
    let mut stft = Stft::new(s.window).ok_or(Error::BadSettings)?;
    let bin_hz = stft.bin_hz(rate);
    let frames = x.len() / s.hop + 1;
    let mut tracker = Tracker::new(s.cents, s.gap);
    let mut all = Vec::with_capacity(frames);
    let mut found = Vec::with_capacity(MAX_PEAKS);
    for frame in 0..frames {
        let (mag, phase) = stft.frame(x, frame * s.hop);
        peaks::peaks(mag, phase, bin_hz, s.floor_db, MAX_PEAKS, &mut found);
        tracker.push(frame, &found);
        all.push(found.clone());
    }
    Ok(Analysis {
        rate,
        hop: s.hop,
        peaks: all,
        tracks: tracker.finish(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn sines(parts: &[(f32, f32)], n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                parts
                    .iter()
                    .map(|(hz, a)| a * (std::f32::consts::TAU * hz * i as f32 / RATE).sin())
                    .sum()
            })
            .collect()
    }

    /// Tracks over at least half the frames.
    fn long(a: &Analysis) -> Vec<&Track> {
        a.tracks
            .iter()
            .filter(|t| 2 * t.len() >= a.frames())
            .collect()
    }

    #[test]
    fn three_sines_are_three_tracks() {
        let parts = [(220.0, 0.4), (330.0, 0.2), (1_210.0, 0.1)];
        let a = analyse(&sines(&parts, 48_000), RATE, &Settings::default()).unwrap();
        let long = long(&a);
        assert_eq!(long.len(), 3);
        for (t, (hz, amp)) in long.iter().zip(parts) {
            // The first and last frames hang over the ends of the signal.
            let inner = 8..t.len() - 8;
            for (f, x) in t.freq[inner.clone()].iter().zip(&t.amp[inner]) {
                assert!((f - hz).abs() < 1.0, "{hz}: {f}");
                assert!((20.0 * (x / amp).log10()).abs() < 0.5, "{hz}: {x}");
            }
        }
    }

    #[test]
    fn a_glide_is_one_track() {
        // 440 → 660 Hz over a second, the phase the integral of the frequency.
        let x: Vec<f32> = (0..48_000)
            .map(|i| {
                let t = i as f32 / RATE;
                0.5 * (std::f32::consts::TAU * (440.0 * t + 110.0 * t * t)).sin()
            })
            .collect();
        let a = analyse(&x, RATE, &Settings::default()).unwrap();
        let long = long(&a);
        assert_eq!(long.len(), 1);
        let t = long[0];
        assert!(t.len() + 2 >= a.frames(), "{} of {}", t.len(), a.frames());
        assert!((t.freq[8] - 440.0).abs() < 10.0 && (t.freq[t.len() - 9] - 660.0).abs() < 10.0);
    }

    #[test]
    fn silence_has_no_tracks() {
        let a = analyse(&[0.0; 48_000], RATE, &Settings::default()).unwrap();
        assert!(a.tracks.is_empty());
        assert_eq!(a.frames(), 48_000 / 256 + 1);
    }

    #[test]
    fn frames_follow_the_hop() {
        let s = Settings {
            hop: 100,
            ..Settings::default()
        };
        let a = analyse(&sines(&[(1_000.0, 0.5)], 1_000), RATE, &s).unwrap();
        assert_eq!(a.frames(), 11);
        assert_eq!(a.hop, 100);
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        let s = Settings::default();
        assert_eq!(analyse(&[], RATE, &s), Err(Error::Empty));
        assert_eq!(analyse(&[0.0; 10], 0.0, &s), Err(Error::Empty));
        let too_long = vec![0.0; (MAX_SECONDS * 1_000.0) as usize + 1];
        assert_eq!(analyse(&too_long, 1_000.0, &s), Err(Error::TooLong));
        for bad in [
            Settings { hop: 0, ..s },
            Settings { hop: 8_192, ..s },
            Settings { window: 1_000, ..s },
            Settings {
                window: 16_384,
                hop: 256,
                ..s
            },
        ] {
            assert_eq!(analyse(&[0.0; 10], RATE, &bad), Err(Error::BadSettings));
        }
    }
}
