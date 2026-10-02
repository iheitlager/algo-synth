//! One voice of the pool, with a first timbre per source.
//!
//! These are previews, not the sources the plan builds: enough that Wave and
//! Drums sound different when a MIDI file is spread across the sources.
//! Wave is a table sine with a second harmonic (MVP 7: the PPG-style
//! wavetable); Drums picks a model from the General MIDI note (MVP 6: the
//! analog-style kit). Mono has its own voices (`mono::voice`).
//!
//! ADR-0002: every coefficient is computed in `start`; `render` only does
//! adds, multiplies and table reads.

use crate::mono::noise::Noise;
use crate::source::Source;

/// Sine table size; one extra guard sample for interpolation.
pub const TABLE: usize = 2048;
/// −80 dB: below this an envelope counts as silent and frees its voice.
pub const SILENT: f32 = 1.0e-4;

/// Who started a voice, so a note-off releases only its own notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Owner {
    /// The on-screen keys, pads or computer keyboard.
    #[default]
    Live,
    /// The MIDI player, on this channel (0..=15).
    Channel(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Drum {
    /// Sine with a falling pitch: kick (low start) and toms.
    #[default]
    Pitched,
    /// Noise plus a tone: snare, clap.
    Snare,
    /// Differentiated noise: hats and cymbals.
    Hat,
}

/// What `render` needs from the engine besides the voice itself.
pub struct Ctx<'a> {
    pub sine: &'a [f32],
    pub attack_step: f32,
    pub release_coef: f32,
}

#[derive(Clone, Copy, Default)]
pub struct Voice {
    pub active: bool,
    pub gate: bool,
    pub source: Source,
    pub owner: Owner,
    pub note: u8,
    /// Allocation order, to steal the oldest voice when the pool is full.
    pub started: u32,
    velocity: f32,
    phase: f32,
    increment: f32,
    env: f32,
    // Drums.
    drum: Drum,
    base_increment: f32,
    pitch_coef: f32,
    decay_coef: f32,
    /// The drums' noise.
    noise: Noise,
    hp_prev: f32,
}

impl Voice {
    /// A new sounding voice. Called per note, so `exp` is fine here.
    pub fn start(
        source: Source,
        owner: Owner,
        note: u8,
        velocity: f32,
        started: u32,
        sample_rate: f32,
    ) -> Voice {
        let hz = midi_to_hz(note);
        let mut v = Voice {
            active: true,
            gate: true,
            source,
            owner,
            note,
            started,
            velocity: velocity.clamp(0.0, 1.0),
            increment: hz / sample_rate,
            noise: Noise::new(started.wrapping_mul(2_654_435_761) | 1),
            ..Voice::default()
        };
        match source {
            Source::Mono | Source::Wave => {}
            Source::Drums => {
                v.env = 1.0;
                let (drum, start_hz, end_hz, decay) = drum_for(note);
                v.drum = drum;
                v.increment = start_hz / sample_rate;
                v.base_increment = end_hz / sample_rate;
                // Pitch falls with a 30 ms time constant.
                v.pitch_coef = (-1.0 / (0.03 * sample_rate)).exp();
                v.decay_coef = decay_coef(decay, sample_rate);
            }
        }
        v
    }

    /// Add this voice into `out`, advancing its state.
    pub fn render(&mut self, ctx: &Ctx, out: &mut [f32]) {
        match self.source {
            Source::Drums => self.render_drum(ctx, out),
            Source::Mono | Source::Wave => self.render_tonal(ctx, out),
        }
    }

    fn render_tonal(&mut self, ctx: &Ctx, out: &mut [f32]) {
        for sample in out.iter_mut() {
            if self.gate {
                self.env = (self.env + ctx.attack_step).min(1.0);
            } else {
                self.env *= ctx.release_coef;
                if self.env < SILENT {
                    self.active = false;
                    return;
                }
            }
            let octave = wrap(self.phase * 2.0);
            let x = lookup(ctx.sine, self.phase) * 0.45 + lookup(ctx.sine, octave) * 0.15;
            *sample += x * self.env * self.velocity;
            self.phase = wrap(self.phase + self.increment);
        }
    }

    fn render_drum(&mut self, ctx: &Ctx, out: &mut [f32]) {
        for sample in out.iter_mut() {
            self.env *= self.decay_coef;
            if self.env < SILENT {
                self.active = false;
                return;
            }
            let x = match self.drum {
                Drum::Pitched => {
                    self.increment = self.base_increment
                        + (self.increment - self.base_increment) * self.pitch_coef;
                    lookup(ctx.sine, self.phase) * 0.9
                }
                Drum::Snare => 0.45 * self.noise.white() + 0.3 * lookup(ctx.sine, self.phase),
                Drum::Hat => {
                    let n = self.noise.white();
                    let high = n - self.hp_prev;
                    self.hp_prev = n;
                    0.25 * high
                }
            };
            *sample += x * self.env * self.velocity;
            self.phase = wrap(self.phase + self.increment);
        }
    }
}

/// General MIDI drum note → (model, start Hz, end Hz, decay seconds).
fn drum_for(note: u8) -> (Drum, f32, f32, f32) {
    match note {
        35 | 36 => (Drum::Pitched, 160.0, 48.0, 0.45),
        37 | 38 | 40 => (Drum::Snare, 185.0, 185.0, 0.2),
        39 => (Drum::Snare, 900.0, 900.0, 0.25),
        42 | 44 => (Drum::Hat, 0.0, 0.0, 0.06),
        46 => (Drum::Hat, 0.0, 0.0, 0.35),
        49 | 51 | 52 | 55 | 57 | 59 => (Drum::Hat, 0.0, 0.0, 0.9),
        // Toms and everything else: pitch rises with the note.
        n => {
            let hz = 60.0 + (f32::from(n) - 35.0).max(0.0) * 7.0;
            (Drum::Pitched, hz * 1.6, hz, 0.3)
        }
    }
}

/// Per-sample multiplier that falls to SILENT (−80 dB) in `seconds`.
pub fn decay_coef(seconds: f32, sample_rate: f32) -> f32 {
    (SILENT.ln() / (seconds * sample_rate)).exp()
}

/// Equal temperament, A4 = 440 Hz. Called per note, not per sample.
pub fn midi_to_hz(note: u8) -> f32 {
    440.0 * ((f32::from(note) - 69.0) / 12.0).exp2()
}

/// One cycle of a sine in `TABLE + 1` samples (a guard sample for the
/// interpolation). Allocates: build it once, in `Engine::new`.
pub fn sine_table() -> Vec<f32> {
    (0..=TABLE)
        .map(|i| (i as f32 / TABLE as f32 * std::f32::consts::TAU).sin())
        .collect()
}

/// Wrap a phase that is at most one cycle past 1 back into `0..1`.
pub fn wrap(p: f32) -> f32 {
    if p >= 1.0 { p - 1.0 } else { p }
}

/// Linear-interpolated table lookup for a phase in `0..1`.
pub fn lookup(table: &[f32], phase: f32) -> f32 {
    let pos = phase * TABLE as f32;
    let i = pos as usize;
    let frac = pos - i as f32;
    let a = table.get(i).copied().unwrap_or(0.0);
    let b = table.get(i + 1).copied().unwrap_or(0.0);
    a + (b - a) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(source: Source, note: u8, blocks: usize) -> (Voice, Vec<f32>) {
        let table = sine_table();
        let ctx = Ctx {
            sine: &table,
            attack_step: 1.0 / 240.0,
            release_coef: decay_coef(0.1, 48_000.0),
        };
        let mut v = Voice::start(source, Owner::Live, note, 1.0, 1, 48_000.0);
        let mut out = vec![0.0; 128 * blocks];
        for chunk in out.chunks_mut(128) {
            v.render(&ctx, chunk);
        }
        (v, out)
    }

    #[test]
    fn a4_is_440() {
        assert!((midi_to_hz(69) - 440.0).abs() < 1.0e-3);
        assert!((midi_to_hz(81) - 880.0).abs() < 1.0e-2);
    }

    #[test]
    fn sources_sound_different() {
        let (_, drums) = render(Source::Drums, 57, 40);
        let (_, wave) = render(Source::Wave, 57, 40);
        let diff: f32 = drums.iter().zip(&wave).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff > 10.0, "Drums and Wave should differ, diff {diff}");
    }

    #[test]
    fn every_source_is_finite_and_bounded() {
        for source in [Source::Wave, Source::Drums] {
            for note in [0, 35, 36, 38, 42, 46, 49, 60, 127] {
                let (_, out) = render(source, note, 50);
                assert!(
                    out.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
                    "{source:?} {note}"
                );
            }
        }
    }

    #[test]
    fn drums_are_one_shots() {
        // A closed hat is gone within 0.06 s, gate or no gate.
        let (v, out) = render(Source::Drums, 42, 40);
        assert!(!v.active);
        assert!(out.iter().take(128).any(|s| *s != 0.0));
        // A kick's pitch falls toward its end frequency.
        let (v, _) = render(Source::Drums, 36, 60);
        assert!((v.increment - 48.0 / 48_000.0).abs() < 1.0e-4);
    }
}
