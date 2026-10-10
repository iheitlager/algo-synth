//! The Spectral Lab's state on its analysis instance (ADR-0017, spec 009
//! Req 7): the WAV it was given, that WAV analysed, and the last
//! resynthesis as a WAV the lab's engine and the main window load like any
//! other sample. `ffi.rs` only forwards to it.

use super::additive::resynthesise;
use super::edit::{shift, top_n};
use super::frames::{split_attack, to_table};
use super::spectrogram::spectrogram;
use super::{Analysis, Settings, analyse};
use crate::sample;

/// Analysis errors cross the ABI past the WAV parser's own codes.
const ANALYSIS_ERRORS: i32 = -10;

#[derive(Default)]
pub struct Lab {
    input: Vec<u8>,
    analysis: Option<Analysis>,
    /// The original's unity note, which the resynthesis keeps.
    root: u8,
    tracks: Vec<f32>,
    out: Vec<u8>,
    /// The spectrograms of the original (0) and the last resynthesis (1).
    grams: [Vec<u8>; 2],
    /// The analysis window, which the spectrograms use too.
    window: usize,
    /// The original, mono at the analysis rate, for its attack.
    source: Vec<f32>,
    /// The last `table` or `attack`, for the view to copy out.
    values: Vec<f32>,
}

impl Lab {
    /// Size the WAV buffer for `len` bytes; `None` past `sample::MAX_WAV`.
    pub fn buffer(&mut self, len: usize) -> Option<&mut [u8]> {
        if len > sample::MAX_WAV {
            return None;
        }
        self.input.clear();
        self.input.resize(len, 0);
        Some(&mut self.input)
    }

    /// Parse the buffer at `rate`, mix it to mono and analyse it: the frame
    /// count, or a negative `sample::Error` code, or an `analysis::Error`
    /// code minus 10. A failure keeps nothing.
    pub fn analyse(&mut self, rate: f32, window: usize, hop: usize) -> i32 {
        self.analysis = None;
        self.tracks.clear();
        self.out.clear();
        self.grams = [Vec::new(), Vec::new()];
        self.window = window;
        self.source.clear();
        self.values.clear();
        let s = match sample::parse(&self.input, rate) {
            Ok(s) => s,
            Err(e) => return e.code(),
        };
        self.root = s.root;
        let ch = usize::from(s.channels.max(1));
        let mono: Vec<f32> = s
            .data
            .chunks(ch)
            .map(|f| f.iter().sum::<f32>() / ch as f32)
            .collect();
        let settings = Settings {
            window,
            hop,
            ..Settings::default()
        };
        match analyse(&mono, rate, &settings) {
            Ok(a) => {
                let frames = a.frames();
                self.grams[0] = spectrogram(&mono, rate, window, hop);
                self.source = mono;
                self.tracks = flatten(&a);
                self.analysis = Some(a);
                i32::try_from(frames).unwrap_or(i32::MAX)
            }
            Err(e) => ANALYSIS_ERRORS + e.code(),
        }
    }

    /// The tracks for the view to draw, one after the other: start frame,
    /// length n, then n frequencies and n amplitudes.
    pub fn tracks(&self) -> &[f32] {
        &self.tracks
    }

    /// Resynthesise the `n` loudest tracks (0 for all), shifted by `ratio`
    /// and stretched by `stretch`, as a 32-bit float WAV with the original's
    /// root: its length in
    /// bytes, 0 without an analysis. The phase is locked only when neither
    /// shift nor stretch moves it.
    pub fn render(&mut self, n: usize, ratio: f32, stretch: f32) -> usize {
        self.out.clear();
        let Some(a) = &self.analysis else {
            return 0;
        };
        let ratio = if ratio.is_finite() {
            ratio.clamp(0.25, 4.0)
        } else {
            1.0
        };
        let stretch = if stretch.is_finite() {
            stretch.clamp(0.25, 4.0)
        } else {
            1.0
        };
        let mut tracks = a.tracks.clone();
        if n > 0 {
            top_n(&mut tracks, n);
        }
        shift(&mut tracks, ratio);
        let lock = ratio == 1.0 && stretch == 1.0;
        let pcm = resynthesise(&tracks, a.frames(), a.hop as f32 * stretch, a.rate, lock);
        self.grams[1] = spectrogram(&pcm, a.rate, self.window, a.hop);
        self.out = sample::float_wav(&pcm, a.rate as u32, self.root);
        self.out.len()
    }

    /// The spectrogram of the original (`which` 0) or the last resynthesis (1):
    /// `spectrogram::BANDS` bytes a frame, at the analysis hop.
    pub fn spectrogram(&self, which: usize) -> &[u8] {
        self.grams.get(which).map_or(&[], |g| g.as_slice())
    }

    /// The analysed sound as a wavetable for the PPG: `WAVES * WAVE_LEN`
    /// values, or none without an analysis or a voiced frame (spec 010 Req 9).
    pub fn table(&mut self) -> &[f32] {
        self.values = self.analysis.as_ref().map(to_table).unwrap_or_default();
        &self.values
    }

    /// The original's attack as a PCM sample for the D-50 (spec 010 Req 10).
    pub fn attack(&mut self) -> &[f32] {
        let rate = self.analysis.as_ref().map_or(0.0, |a| a.rate);
        self.values = split_attack(&self.source, rate);
        &self.values
    }

    /// The last `table` or `attack`.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// The original's root note: where its table and attack play at their own pitch.
    pub fn root(&self) -> u8 {
        self.root
    }

    /// The last `render`'s WAV.
    pub fn rendered(&self) -> &[u8] {
        &self.out
    }
}

fn flatten(a: &Analysis) -> Vec<f32> {
    let points: usize = a.tracks.iter().map(|t| 2 + 2 * t.len()).sum();
    let mut out = Vec::with_capacity(points);
    for t in &a.tracks {
        out.extend([t.start as f32, t.len() as f32]);
        out.extend(&t.freq);
        out.extend(&t.amp);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    /// Half a second of A4, its root A4 too.
    fn sine_wav() -> Vec<u8> {
        let x: Vec<f32> = (0..24_000)
            .map(|i| 0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / RATE).sin())
            .collect();
        sample::test_wav(48_000, &x, Some((69, 0, 0)))
    }

    fn load(lab: &mut Lab, bytes: &[u8]) -> i32 {
        lab.buffer(bytes.len()).unwrap().copy_from_slice(bytes);
        lab.analyse(RATE, 2048, 256)
    }

    #[test]
    fn a_wav_is_analysed_drawn_and_rendered() {
        let mut lab = Lab::default();
        assert_eq!(lab.render(0, 1.0, 1.0), 0, "nothing before an analysis");
        assert_eq!(load(&mut lab, &sine_wav()), 24_000 / 256 + 1);
        // One long track at 440 Hz, drawn as start, n, n freqs, n amps.
        let t = lab.tracks();
        let n = t[1] as usize;
        assert!(n > 80);
        assert!((t[2 + n / 2] - 440.0).abs() < 1.0);
        let bytes = lab.render(0, 1.0, 1.0);
        let back = sample::parse(&lab.rendered()[..bytes], RATE).unwrap();
        assert!(back.frames() >= 24_000);
        assert_eq!(back.root, 69, "the resynthesis keeps the original's root");
        let peak = back.data.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.02, "{peak}");
        // Both spectrograms are drawn, a frame of bands each.
        let bands = crate::analysis::spectrogram::BANDS;
        assert_eq!(lab.spectrogram(0).len(), (24_000 / 256 + 1) * bands);
        assert!(lab.spectrogram(1).len() >= lab.spectrogram(0).len());
        // Stretched by 2 it is twice as long.
        lab.render(0, 1.0, 2.0);
        let long = sample::parse(lab.rendered(), RATE).unwrap();
        assert!(long.frames() >= 2 * 24_000);
    }

    #[test]
    fn errors_are_codes_and_keep_nothing() {
        let mut lab = Lab::default();
        load(&mut lab, &sine_wav());
        assert_eq!(load(&mut lab, b"not a wav"), sample::Error::NotWav.code());
        assert!(lab.tracks().is_empty());
        assert_eq!(lab.render(0, 1.0, 1.0), 0);
        lab.buffer(sine_wav().len())
            .unwrap()
            .copy_from_slice(&sine_wav());
        assert_eq!(lab.analyse(RATE, 1000, 256), -13);
        assert!(lab.buffer(sample::MAX_WAV + 1).is_none());
    }
}
