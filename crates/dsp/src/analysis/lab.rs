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
use super::frames::{
    by_label_table, harmonic_wave, label_amps, split_attack, steady_frame, to_table,
};
use super::harmonic::label;
use super::morph::{Positions, from_analysis, morph};
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

/// The morph (spec 010 Req 7, 13), by id as `spectral_morph` takes them:
/// where between A (0) and B (1) pitch, amplitude, noise and formant lie, and
/// whether a table runs from A to B across its waves instead of over time.
pub const MPITCH: usize = 0;
pub const MAMP: usize = 1;
pub const MNOISE: usize = 2;
pub const MFORMANT: usize = 3;
pub const SWEEP: usize = 4;
pub const MORPHS: usize = 5;
pub const NO_MORPH: [f32; MORPHS] = [0.0; MORPHS];
/// A rendered preset or macros sound this long before its release (s).
const GENERATED: f32 = 1.2;

pub struct Lab {
    /// The transforms, by the ids above.
    edits: [f32; EDITS],
    /// The morph, by the ids above.
    morph: [f32; MORPHS],
    /// The second sound, B, to morph with, and its envelope once a formant morph needs it.
    other: Option<Analysis>,
    other_envelope: Option<Envelope>,
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
    /// Whether the analysis gave each partial its noise.
    noise: bool,
    /// A text from the view: the source a recipe names.
    text: Vec<u8>,
    /// The last recipe line, for the view to copy.
    line: Vec<u8>,
    /// The original, mono at the analysis rate, for its attack.
    source: Vec<f32>,
    /// The last `table` or `attack`, for the view to copy out.
    values: Vec<f32>,
}

impl Default for Lab {
    fn default() -> Lab {
        Lab {
            edits: NO_EDITS,
            morph: NO_MORPH,
            other: None,
            other_envelope: None,
            envelope: None,
            input: Vec::new(),
            analysis: None,
            root: 60,
            tracks: Vec::new(),
            out: Vec::new(),
            grams: [Vec::new(), Vec::new()],
            window: 0,
            noise: false,
            text: Vec::new(),
            line: Vec::new(),
            source: Vec::new(),
            values: Vec::new(),
        }
    }
}

impl Lab {
    /// Size the text buffer for `len` bytes (at most 256), for the source a recipe names.
    pub fn text_buffer(&mut self, len: usize) -> Option<&mut [u8]> {
        if len > 256 {
            return None;
        }
        self.text.clear();
        self.text.resize(len, 0);
        Some(&mut self.text)
    }

    /// The recipe of `kind` into `slot` from the source in the text buffer.
    pub fn recipe_from_text(&mut self, kind: crate::song::recipe::Kind, slot: usize) -> String {
        let source = String::from_utf8_lossy(&self.text).into_owned();
        self.recipe(kind, slot, &source)
    }

    /// Set transform `id` (out of range is ignored); the next render, table
    /// or spectrogram uses it.
    pub fn set(&mut self, id: usize, v: f32) {
        if let Some(e) = self.edits.get_mut(id).filter(|_| v.is_finite()) {
            *e = v;
        }
    }

    /// Set morph position `id` (out of range is ignored), 0 to 1.
    pub fn set_morph(&mut self, id: usize, v: f32) {
        if let Some(m) = self.morph.get_mut(id).filter(|_| v.is_finite()) {
            *m = v.clamp(0.0, 1.0);
        }
    }

    /// Whether the sound is a morph: a B, and a position off A or a sweep.
    fn morphing(&self) -> bool {
        self.other.is_some() && self.morph.iter().any(|m| *m > 0.0)
    }

    /// The tracks a render starts from, and their frames: A's, or A and B
    /// morphed at `pos`.
    fn base(&mut self, pos: Positions) -> (Vec<Track>, usize) {
        let (Some(a), Some(b)) = (&self.analysis, &self.other) else {
            return self
                .analysis
                .as_ref()
                .map_or((Vec::new(), 0), |a| (a.tracks.clone(), a.frames()));
        };
        let (da, db) = (from_analysis(a, &a.tracks), from_analysis(b, &b.tracks));
        // The formant needs both envelopes only when it differs from the amplitude.
        if (pos.formant - pos.amp).abs() > 1e-6 {
            let ea = self.envelope.get_or_insert_with(|| true_envelope(a));
            let eb = self.other_envelope.get_or_insert_with(|| true_envelope(b));
            morph(&da, &db, pos, Some((ea, eb)))
        } else {
            morph(&da, &db, pos, None)
        }
    }

    /// The morph's positions as set.
    fn positions(&self) -> Positions {
        let at = |id: usize| self.morph.get(id).copied().unwrap_or(0.0);
        Positions {
            pitch: at(MPITCH),
            amp: at(MAMP),
            noise: at(MNOISE),
            formant: at(MFORMANT),
        }
    }

    /// `t` (of `frames` frames) with the transforms applied, and the pitch
    /// shifted by `ratio` (formants kept when asked).
    fn apply_edits(&mut self, t: Vec<Track>, frames: usize, ratio: f32) -> Vec<Track> {
        let Some(a) = &self.analysis else {
            return t;
        };
        let (hop, rate) = (a.hop, a.rate);
        let e = self.edits;
        let at = |id: usize| e.get(id).copied().unwrap_or(0.0);
        let mut t = if at(FREEZE) >= 0.0 {
            freeze(
                &t,
                (at(FREEZE).min(1.0) * frames.saturating_sub(1) as f32) as usize,
            )
        } else {
            t
        };
        stretch(&mut t, at(STRETCH));
        inharmonic(&mut t, at(INHARMONIC));
        odd_even(&mut t, at(ODD_EVEN));
        decay_by_number(&mut t, at(DECAY), hop, rate);
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

    /// The sound's tracks (A, or the morph) with the transforms, and their frames.
    fn edited(&mut self, ratio: f32) -> (Vec<Track>, usize) {
        let (t, frames) = if self.morphing() {
            self.base(self.positions())
        } else {
            self.base(Positions::default())
        };
        (self.apply_edits(t, frames, ratio), frames)
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

    /// Parse the buffer at `rate` and mix it to mono: the samples and the
    /// file's root note, or a negative `sample::Error` code.
    fn input_mono(&self, rate: f32) -> Result<(Vec<f32>, u8), i32> {
        let s = sample::parse(&self.input, rate).map_err(|e| e.code())?;
        let ch = usize::from(s.channels.max(1));
        let mono = s
            .data
            .chunks(ch)
            .map(|f| f.iter().sum::<f32>() / ch as f32)
            .collect();
        Ok((mono, s.root))
    }

    /// Parse the buffer at `rate`, mix it to mono and analyse it as A: the
    /// frame count, or a negative `sample::Error` code, or an
    /// `analysis::Error` code minus 10. A failure keeps nothing.
    pub fn analyse(&mut self, rate: f32, window: usize, hop: usize, noise: bool) -> i32 {
        self.clear_a(window, noise);
        match self.input_mono(rate) {
            Ok((mono, root)) => self.take_a(mono, root, rate, window, hop, noise),
            Err(code) => code,
        }
    }

    /// The buffer as B, the sound to morph with (spec 010 Req 7): the frame
    /// count or a negative code, as `analyse`. A failure clears B.
    pub fn analyse_other(&mut self, rate: f32, window: usize, hop: usize, noise: bool) -> i32 {
        self.other = None;
        self.other_envelope = None;
        match self.input_mono(rate) {
            Ok((mono, _)) => self.take_b(&mono, rate, window, hop, noise),
            Err(code) => code,
        }
    }

    /// A sound the engine makes from the source in the text buffer (a preset
    /// at a note, or Plaits macros; spec 010 Req 8), as A (`which` 0) or B (1):
    /// the frame count, or −1 for a source that is not one of those.
    pub fn generate(
        &mut self,
        which: usize,
        rate: f32,
        window: usize,
        hop: usize,
        noise: bool,
    ) -> i32 {
        let text = String::from_utf8_lossy(&self.text).into_owned();
        let Some((mono, root)) = generated(&text, rate) else {
            return -1;
        };
        if which == 0 {
            self.clear_a(window, noise);
            self.take_a(mono, root, rate, window, hop, noise)
        } else {
            self.other = None;
            self.other_envelope = None;
            self.take_b(&mono, rate, window, hop, noise)
        }
    }

    /// Forget B.
    pub fn clear_other(&mut self) {
        self.other = None;
        self.other_envelope = None;
    }

    fn clear_a(&mut self, window: usize, noise: bool) {
        self.analysis = None;
        self.envelope = None;
        self.tracks.clear();
        self.out.clear();
        self.grams = [Vec::new(), Vec::new()];
        self.window = window;
        self.noise = noise;
        self.source.clear();
        self.values.clear();
    }

    fn take_a(
        &mut self,
        mono: Vec<f32>,
        root: u8,
        rate: f32,
        window: usize,
        hop: usize,
        noise: bool,
    ) -> i32 {
        self.root = root;
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

    fn take_b(&mut self, mono: &[f32], rate: f32, window: usize, hop: usize, noise: bool) -> i32 {
        let settings = Settings {
            window,
            hop,
            noise,
            ..Settings::default()
        };
        match analyse(mono, rate, &settings) {
            Ok(mut b) => {
                label(&mut b);
                let frames = b.frames();
                self.other = Some(b);
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
        self.render_root(n, ratio, stretch, self.root)
    }

    /// Resynthesise as `render` does, shifted to sound at `root` (within two
    /// octaves of the original's), its WAV marked with that root: one key of a
    /// multisample (#520). Its length is the original's, stretched.
    pub fn render_at(&mut self, n: usize, root: u8, stretch: f32) -> usize {
        let root = root.clamp(self.root.saturating_sub(24), self.root.saturating_add(24));
        let ratio = 2f32.powf((f32::from(root) - f32::from(self.root)) / 12.0);
        self.render_root(n, ratio, stretch, root)
    }

    fn render_root(&mut self, n: usize, ratio: f32, stretch: f32, root: u8) -> usize {
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
        let (hop, rate) = (a.hop, a.rate);
        let untouched = self.edits == NO_EDITS && !self.morphing();
        let (mut tracks, frames) = self.edited(ratio);
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
        self.out = sample::float_wav(&pcm, a.rate as u32, root);
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
        if self.other.is_some() && self.morph.get(SWEEP).is_some_and(|s| *s >= 0.5) {
            self.values = self.sweep_table();
            return &self.values;
        }
        // The transforms, but not the pitch: the PPG plays a table at the note's.
        let (tracks, frames) = self.edited(1.0);
        self.values = if self.morphing() {
            // A morph's partials carry their harmonic numbers.
            by_label_table(&tracks, frames)
        } else {
            self.analysis
                .as_ref()
                .map(|a| to_table(a, &tracks))
                .unwrap_or_default()
        };
        &self.values
    }

    /// A table whose waves run from A to B (spec 010 Req 13): wave k is the
    /// morph at `k / (WAVES − 1)` in every dimension, at a steady moment of
    /// the sound (a third of the way from its peak to its release).
    fn sweep_table(&mut self) -> Vec<f32> {
        use crate::table::{WAVE_LEN, WAVES};
        let mut out = Vec::with_capacity(WAVES * WAVE_LEN);
        for k in 0..WAVES {
            let x = k as f32 / (WAVES - 1) as f32;
            let (t, frames) = self.base(Positions::all(x));
            let t = self.apply_edits(t, frames, 1.0);
            let at = steady_frame(&t, frames);
            out.extend_from_slice(&harmonic_wave(&label_amps(&t, at)));
        }
        out
    }

    /// The original's attack as a PCM sample for the D-50 (spec 010 Req 10).
    pub fn attack(&mut self) -> &[f32] {
        let rate = self.analysis.as_ref().map_or(0.0, |a| a.rate);
        self.values = split_attack(&self.source, rate);
        &self.values
    }

    /// The recipe (ADR-0033) of what `table` or `attack` gives, from `source`
    /// (`A`, or `A ~ B` when the lab morphs) into user slot `slot` (1-8): the
    /// line the song keeps. Empty for a source that is not one.
    pub fn recipe(&self, kind: crate::song::recipe::Kind, slot: usize, source: &str) -> String {
        use crate::song::recipe::{Recipe, is_source};
        let (a, b) = match source.split_once(" ~ ") {
            Some((a, b)) => (a.trim(), Some(b.trim())),
            None => (source.trim(), None),
        };
        if !is_source(a) || b.is_some_and(|b| !is_source(b)) || !(1..=kind.slots()).contains(&slot)
        {
            return String::new();
        }
        let mut r = Recipe::new(kind, slot, a);
        if let Some(b) = b.filter(|_| self.other.is_some()) {
            r.other = Some(b.to_string());
            r.morph = self.morph;
        }
        r.edits = self.edits;
        // A recipe always analyses with noise per partial: without it, none.
        if !self.noise {
            if let Some(n) = r.edits.get_mut(NOISE) {
                *n = 0.0;
            }
        }
        r.window = self.window;
        r.hop = self
            .analysis
            .as_ref()
            .map_or(crate::song::recipe::HOP, |a| a.hop);
        r.print()
    }

    /// Keep a recipe line for the view to copy.
    pub fn set_recipe_line(&mut self, line: String) -> &[u8] {
        self.line = line.into_bytes();
        &self.line
    }

    pub fn recipe_line(&self) -> &[u8] {
        &self.line
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

/// The sound a source of the engine's own makes, mono at `rate`, and its
/// root note: a factory preset rendered at a note, or Plaits-style macros
/// played at C4 (spec 010 Req 8). `None` for any other source.
fn generated(source: &str, rate: f32) -> Option<(Vec<f32>, u8)> {
    use crate::song::recipe::{Source, parse_source};
    let held = (GENERATED * rate) as usize;
    let tail = held / 3;
    let mut out = match parse_source(source)? {
        Source::Synth(preset, note) => {
            let mut e = crate::engine::Engine::new(rate);
            e.preset(0, preset);
            e.note_on(0, note, 1.0);
            let block = crate::engine::BLOCK;
            let mut x = Vec::with_capacity(held + tail + block);
            while x.len() < held {
                e.render(block);
                x.extend_from_slice(e.output().get(..block).unwrap_or(&[]));
            }
            e.note_off(0, note);
            while x.len() < held + tail {
                e.render(block);
                x.extend_from_slice(e.output().get(..block).unwrap_or(&[]));
            }
            (x, note)
        }
        Source::Plaits(h, t, m) => {
            let amps = plaits(h, t, m);
            let f0 = crate::voice::midi_to_hz(60);
            let fade = (0.01 * rate) as usize;
            let x = (0..held + tail)
                .map(|i| {
                    let g = (i.min(held + tail - i) as f32 / fade as f32).min(1.0);
                    let ph = std::f32::consts::TAU * f0 * i as f32 / rate;
                    g * amps
                        .iter()
                        .enumerate()
                        .map(|(k, a)| a * ((k + 1) as f32 * ph).sin())
                        .sum::<f32>()
                })
                .collect();
            (x, 60)
        }
        Source::Pack(..) | Source::Hash(_) => return None,
    };
    // A level to analyse at: the peak at 0.5.
    let peak = out.0.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if peak > 1e-6 {
        out.0.iter_mut().for_each(|v| *v *= 0.5 / peak);
    }
    Some(out)
}

/// Harmonic amplitudes from three macros, after Plaits' additive model (MIT,
/// Mutable Instruments), as its manual describes it: `harmonics` the number of
/// spectral bumps, `timbre` the most prominent harmonic, `morph` the bumps'
/// shape from wide to peaked; normalised to sum to 1.
pub fn plaits(harmonics: f32, timbre: f32, morph: f32) -> [f32; 24] {
    let (h, t, m) = (
        harmonics.clamp(0.0, 1.0),
        timbre.clamp(0.0, 1.0),
        morph.clamp(0.0, 1.0),
    );
    let centre = t * 23.0;
    let bumps = 16.0 * h * h;
    let raw = (1.0 - 0.6 * h) * m;
    let slope = 0.01 + 1.99 * raw * raw * raw;
    let mut amps = [0.0f32; 24];
    for (i, a) in amps.iter_mut().enumerate() {
        let order = (i as f32 - centre).abs() * slope;
        let gain = (1.0 - order).max(0.0);
        let shaped =
            gain * gain * (1.0 + (std::f32::consts::TAU * (0.25 + order * bumps)).sin()) * 0.5;
        *a = shaped * shaped;
    }
    let sum: f32 = amps.iter().sum();
    if sum > 1e-9 {
        amps.iter_mut().for_each(|a| *a /= sum);
    }
    amps
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
    fn a_render_at_a_root_keeps_the_length() {
        let mut lab = Lab::default();
        load(&mut lab, &sine_wav());
        let n = lab.render(0, 1.0, 1.0);
        let frames = sample::parse(&lab.rendered()[..n], RATE).unwrap().frames();
        // A fifth up: E5, the same length, the pitch 440 * 2^(7/12).
        let n = lab.render_at(0, 76, 1.0);
        let up = sample::parse(&lab.rendered()[..n], RATE).unwrap();
        assert_eq!(up.root, 76);
        assert_eq!(up.frames(), frames);
        let zero = |d: &[f32]| d.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let mid = &up.data[4_000..20_000];
        let hz = zero(mid) as f32 * RATE / mid.len() as f32;
        assert!((hz - 440.0 * 2f32.powf(7.0 / 12.0)).abs() < 3.0, "{hz}");
        // Beyond two octaves it stops at two.
        let n = lab.render_at(0, 120, 1.0);
        assert_eq!(
            sample::parse(&lab.rendered()[..n], RATE).unwrap().root,
            69 + 24
        );
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

    #[test]
    fn the_lab_writes_its_recipe() {
        use crate::song::recipe::Kind;
        let mut lab = Lab::default();
        lab.buffer(sine_wav().len())
            .unwrap()
            .copy_from_slice(&sine_wav());
        assert!(lab.analyse(RATE, 2048, 256, true) > 0);
        lab.set(INHARMONIC, 1.6);
        assert_eq!(
            lab.recipe(Kind::Table, 2, "upright-piano-kw/C4vH"),
            "table 2 = upright-piano-kw/C4vH : inharmonic 1.6, window 2048"
        );
        assert_eq!(lab.recipe(Kind::Attack, 9, "a/b"), "", "no slot 9");
        assert_eq!(lab.recipe(Kind::Attack, 1, "nope"), "", "not a source");
        // Without noise in the analysis, the recipe clears it.
        assert!(lab.analyse(RATE, 4096, 256, false) > 0);
        assert!(lab.recipe(Kind::Table, 1, "a/b").ends_with("noise 0"));
    }

    /// A saw (A) and a square (B) as WAVs at 220 Hz.
    fn wave_wav(odd_only: bool) -> Vec<u8> {
        let x: Vec<f32> = (0..24_000)
            .map(|i| {
                let ph = std::f32::consts::TAU * 220.0 * i as f32 / RATE;
                (1..=20)
                    .filter(|k| !odd_only || k % 2 == 1)
                    .map(|k| 0.3 * (k as f32 * ph).sin() / k as f32)
                    .sum::<f32>()
            })
            .collect();
        sample::test_wav(48_000, &x, Some((57, 0, 0)))
    }

    #[test]
    fn two_sources_morph_and_sweep_into_a_table() {
        let mut lab = Lab::default();
        let (saw, square) = (wave_wav(false), wave_wav(true));
        lab.buffer(saw.len()).unwrap().copy_from_slice(&saw);
        assert!(lab.analyse(RATE, 4096, 256, true) > 0);
        lab.buffer(square.len()).unwrap().copy_from_slice(&square);
        assert!(lab.analyse_other(RATE, 4096, 256, true) > 0);
        let plain = lab.render(0, 1.0, 1.0);
        let saw_render = lab.rendered()[..plain].to_vec();
        lab.set_morph(MAMP, 1.0);
        let n = lab.render(0, 1.0, 1.0);
        assert_ne!(lab.rendered()[..n], saw_render[..], "the morph sounds");
        // A sweep table: wave 0 a saw, the last a square.
        lab.set_morph(SWEEP, 1.0);
        let t = lab.table().to_vec();
        assert_eq!(t.len(), crate::table::WAVES * crate::table::WAVE_LEN);
        let h = |wave: &[f32], k: f32| {
            let (mut s, mut c) = (0.0f32, 0.0f32);
            for (i, y) in wave.iter().enumerate() {
                let p = std::f32::consts::TAU * k * i as f32 / crate::table::WAVE_LEN as f32;
                s += y * p.sin();
                c += y * p.cos();
            }
            s.hypot(c)
        };
        let (first, last) = (
            &t[..crate::table::WAVE_LEN],
            &t[t.len() - crate::table::WAVE_LEN..],
        );
        assert!(
            h(first, 2.0) > 0.3 * h(first, 1.0),
            "wave 0 has the saw's second harmonic"
        );
        assert!(
            h(last, 2.0) < 0.01 * h(last, 1.0),
            "the last wave is a square"
        );
        assert_eq!(
            lab.recipe(crate::song::recipe::Kind::Table, 3, "a/saw ~ a/square"),
            "table 3 = a/saw ~ a/square : mamp 1, sweep 1"
        );
    }

    #[test]
    fn a_preset_and_macros_are_sources() {
        let mut lab = Lab::default();
        let src = b"synth:LaChoir:c4";
        lab.text_buffer(src.len()).unwrap().copy_from_slice(src);
        assert!(lab.generate(0, RATE, 4096, 256, true) > 0);
        assert_eq!(lab.root(), 60);
        assert!(
            !lab.table().is_empty(),
            "a choir has a pitch to make a table from"
        );
        let mac = b"plaits:0.2:0.4:0.6";
        lab.text_buffer(mac.len()).unwrap().copy_from_slice(mac);
        assert!(lab.generate(1, RATE, 4096, 256, true) > 0);
        assert!(lab.other.is_some());
        let none = b"upright-piano-kw/C4vH";
        lab.text_buffer(none.len()).unwrap().copy_from_slice(none);
        assert_eq!(
            lab.generate(1, RATE, 4096, 256, true),
            -1,
            "a pack sample comes as bytes"
        );
        let amps = plaits(0.3, 0.5, 0.8);
        assert!((amps.iter().sum::<f32>() - 1.0).abs() < 1e-4);
    }
}
