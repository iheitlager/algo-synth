//! The Spectral Lab's state on its analysis instance (ADR-0017, spec 009
//! Req 7): the WAV it was given, that WAV analysed, and the last
//! resynthesis as a WAV the lab's engine and the main window load like any
//! other sample. `ffi.rs` only forwards to it.

use super::additive::resynthesise;
use super::edit::{
    decay_by_number, formant_scale, freeze, freq_shift, inharmonic, noise_amount, odd_even, shift,
    smear, spectral_filter, stretch, top_n,
};
use super::envelope::{Envelope, true_envelope};
use super::frames::{split_attack, to_table};
use super::harmonic::label;
use super::spectrogram::spectrogram;
use super::track::Track;
use super::{Analysis, Settings, analyse};
use crate::sample;

/// Analysis errors cross the ABI past the WAV parser's own codes.
const ANALYSIS_ERRORS: i32 = -10;

/// The transforms the lab applies (spec 010 Req 4), by id as `spectral_set`
/// takes them, each with the value that leaves the sound alone.
pub const STRETCH: usize = 0;
pub const INHARMONIC: usize = 1;
/// Hertz added to every partial.
pub const FREQ_SHIFT: usize = 2;
pub const FORMANT: usize = 3;
pub const SMEAR: usize = 4;
pub const ODD_EVEN: usize = 5;
/// Low-pass cutoff in Hz, 0 off; its resonance 0..=1.
pub const LOW_PASS: usize = 6;
pub const LOW_RES: usize = 7;
/// High-pass cutoff in Hz, 0 off.
pub const HIGH_PASS: usize = 8;
/// The position (0..=1) of the frame to hold, negative off.
pub const FREEZE: usize = 9;
pub const DECAY: usize = 10;
pub const NOISE: usize = 11;
/// 1 keeps the formants where they are when the pitch is shifted.
pub const KEEP_FORMANTS: usize = 12;
pub const EDITS: usize = 13;
pub const NO_EDITS: [f32; EDITS] = [
    1.0, 1.0, 0.0, 1.0, 0.0, 0.5, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0,
];

pub struct Lab {
    /// The transforms, by the ids above.
    edits: [f32; EDITS],
    /// The envelope of the analysis, made the first time a transform needs it.
    envelope: Option<Envelope>,
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

impl Default for Lab {
    fn default() -> Lab {
        Lab {
            edits: NO_EDITS,
            envelope: None,
            input: Vec::new(),
            analysis: None,
            root: 60,
            tracks: Vec::new(),
            out: Vec::new(),
            grams: [Vec::new(), Vec::new()],
            window: 0,
            source: Vec::new(),
            values: Vec::new(),
        }
    }
}

impl Lab {
    /// Set transform `id` (out of range is ignored); the next render, table
    /// or spectrogram uses it.
    pub fn set(&mut self, id: usize, v: f32) {
        if let Some(e) = self.edits.get_mut(id).filter(|_| v.is_finite()) {
            *e = v;
        }
    }

    /// The analysis's tracks with the transforms applied, and the pitch
    /// shifted by `ratio` (formants kept when asked).
    fn edited(&mut self, ratio: f32) -> Vec<Track> {
        let Some(a) = &self.analysis else {
            return Vec::new();
        };
        let e = self.edits;
        let at = |id: usize| e.get(id).copied().unwrap_or(0.0);
        let mut t = if at(FREEZE) >= 0.0 {
            freeze(
                &a.tracks,
                (at(FREEZE).min(1.0) * a.frames().saturating_sub(1) as f32) as usize,
            )
        } else {
            a.tracks.clone()
        };
        stretch(&mut t, at(STRETCH));
        inharmonic(&mut t, at(INHARMONIC));
        odd_even(&mut t, at(ODD_EVEN));
        decay_by_number(&mut t, at(DECAY), a.hop, a.rate);
        smear(&mut t, at(SMEAR));
        if at(LOW_PASS) > 0.0 {
            spectral_filter(&mut t, at(LOW_PASS), at(LOW_RES), false);
        }
        if at(HIGH_PASS) > 0.0 {
            spectral_filter(&mut t, at(HIGH_PASS), 0.0, true);
        }
        // Formants against the envelope of the partials as they are, before
        // the pitch moves; keeping them through a shift scales them by 1/ratio.
        let keep = if at(KEEP_FORMANTS) >= 0.5 {
            1.0 / ratio
        } else {
            1.0
        };
        let formant = at(FORMANT) * keep;
        if (formant - 1.0).abs() > 1e-6 {
            let env = self.envelope.get_or_insert_with(|| true_envelope(a));
            formant_scale(&mut t, env, formant);
        }
        noise_amount(&mut t, at(NOISE));
        freq_shift(&mut t, at(FREQ_SHIFT));
        shift(&mut t, ratio);
        t
    }

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
    pub fn analyse(&mut self, rate: f32, window: usize, hop: usize, noise: bool) -> i32 {
        self.analysis = None;
        self.envelope = None;
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
            noise,
            ..Settings::default()
        };
        match analyse(&mono, rate, &settings) {
            Ok(mut a) => {
                label(&mut a);
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
        let (frames, hop, rate) = (a.frames(), a.hop, a.rate);
        let untouched = self.edits == NO_EDITS;
        let mut tracks = self.edited(ratio);
        if n > 0 {
            top_n(&mut tracks, n);
        }
        // The measured phases only fit the sound as it was.
        let lock = ratio == 1.0 && stretch == 1.0 && untouched;
        let pcm = resynthesise(&tracks, frames, hop as f32 * stretch, rate, lock);
        let Some(a) = &self.analysis else {
            return 0;
        };
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
        // The transforms, but not the pitch: the PPG plays a table at the note's.
        let tracks = self.edited(1.0);
        self.values = self
            .analysis
            .as_ref()
            .map(|a| to_table(a, &tracks))
            .unwrap_or_default();
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
        lab.analyse(RATE, 2048, 256, false)
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
        assert_eq!(lab.analyse(RATE, 1000, 256, false), -13);
        assert!(lab.buffer(sample::MAX_WAV + 1).is_none());
    }

    #[test]
    fn the_view_mirrors_the_transforms() {
        // web/src/audio/spectral.ts names the ids and the values that leave a
        // sound alone; they must be these (as ADR-0004 keeps the parameters).
        let ts = include_str!("../../../../web/src/audio/spectral.ts");
        let ids = [
            ("Stretch", STRETCH),
            ("Inharmonic", INHARMONIC),
            ("FreqShift", FREQ_SHIFT),
            ("Formant", FORMANT),
            ("Smear", SMEAR),
            ("OddEven", ODD_EVEN),
            ("LowPass", LOW_PASS),
            ("LowRes", LOW_RES),
            ("HighPass", HIGH_PASS),
            ("Freeze", FREEZE),
            ("Decay", DECAY),
            ("Noise", NOISE),
            ("KeepFormants", KEEP_FORMANTS),
        ];
        assert_eq!(ids.len(), EDITS);
        for (name, id) in ids {
            assert!(ts.contains(&format!("{name}: {id},")), "{name}: {id}");
        }
        let defaults: Vec<String> = NO_EDITS.iter().map(|v| format!("{v}")).collect();
        let line = format!(
            "export const NO_EDITS: readonly number[] = [{}]",
            defaults.join(", ")
        );
        assert!(ts.contains(&line), "{line}");
    }

    #[test]
    fn transforms_reach_the_render_and_the_table() {
        let mut lab = Lab::default();
        // A saw rendered by the engine's oscillator, as a WAV with root A3.
        let (blep, sine) = (crate::mono::osc::Blep::new(), crate::voice::sine_table());
        let mut osc = crate::mono::osc::Osc::default();
        osc.wave = crate::mono::osc::Waveform::Saw;
        osc.set_increment(220.0 / RATE);
        let x: Vec<f32> = (0..24_000)
            .map(|_| 0.4 * osc.step(&blep, &sine, 0.5, None).0)
            .collect();
        let wav = sample::test_wav(48_000, &x, Some((57, 0, 0)));
        lab.buffer(wav.len()).unwrap().copy_from_slice(&wav);
        assert!(lab.analyse(RATE, 4096, 256, true) > 0);
        let plain_table = lab.table().to_vec();
        let plain = lab.render(0, 1.0, 1.0);
        let plain_wav = lab.rendered()[..plain].to_vec();
        // Odd harmonics only: a hollow saw, in the render and in the table.
        lab.set(ODD_EVEN, 0.0);
        let hollow = lab.render(0, 1.0, 1.0);
        assert_ne!(lab.rendered()[..hollow], plain_wav[..]);
        let table = lab.table().to_vec();
        assert_ne!(table, plain_table);
        let wave = &table[32 * crate::table::WAVE_LEN..33 * crate::table::WAVE_LEN];
        let h = |k: f32| {
            let (mut s, mut c) = (0.0f32, 0.0f32);
            for (i, y) in wave.iter().enumerate() {
                let p = std::f32::consts::TAU * k * i as f32 / crate::table::WAVE_LEN as f32;
                s += y * p.sin();
                c += y * p.cos();
            }
            s.hypot(c)
        };
        assert!(h(2.0) < 0.01 * h(1.0), "no second harmonic in the table");
        // Unknown ids are ignored.
        lab.set(99, 5.0);
    }
}
