//! The engine: a fixed voice pool rendering planar stereo blocks.
//!
//! Real-time rules (ADR-0002): `render` never allocates, never panics and
//! never calls `sin`/`exp`/`pow` per sample. Everything it touches is
//! allocated in `Engine::new`; coefficients are computed when a parameter or
//! note changes, not in the sample loop.

use crate::params::Param;
use crate::source::Source;

/// Frames per render call; the Web Audio render quantum.
pub const BLOCK: usize = 128;
/// Voices in the pool. Plenty for the test voice; sources get their own pools.
pub const VOICES: usize = 16;
/// Sine table size; one extra guard sample for interpolation.
const TABLE: usize = 2048;
/// Below this an envelope in release counts as silent and frees its voice.
const SILENT: f32 = 1.0e-4;

#[derive(Clone, Copy, Default)]
struct Voice {
    active: bool,
    gate: bool,
    source: Source,
    note: u8,
    velocity: f32,
    phase: f32,
    increment: f32,
    env: f32,
    /// Allocation order, to steal the oldest voice when the pool is full.
    started: u32,
}

/// The whole synth, one per wasm instance (one per AudioWorklet node).
pub struct Engine {
    sample_rate: f32,
    sine: Vec<f32>,
    voices: [Voice; VOICES],
    master_gain: f32,
    attack_step: f32,
    release_coef: f32,
    counter: u32,
    /// Planar output: `BLOCK` left samples, then `BLOCK` right samples.
    out: Box<[f32; 2 * BLOCK]>,
}

impl Engine {
    /// Allocate everything the engine will ever use.
    pub fn new(sample_rate: f32) -> Engine {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let sine = (0..=TABLE)
            .map(|i| (i as f32 / TABLE as f32 * std::f32::consts::TAU).sin())
            .collect();
        let mut engine = Engine {
            sample_rate,
            sine,
            voices: [Voice::default(); VOICES],
            master_gain: 0.5,
            attack_step: 0.0,
            release_coef: 0.0,
            counter: 0,
            out: Box::new([0.0; 2 * BLOCK]),
        };
        engine.set_param(Param::MasterGain, 0.5);
        engine.set_param(Param::Attack, 0.005);
        engine.set_param(Param::Release, 0.3);
        engine
    }

    /// Set a parameter; the value is clamped into its range.
    pub fn set_param(&mut self, param: Param, value: f32) {
        let v = param.clamp(value);
        match param {
            Param::MasterGain => self.master_gain = v,
            Param::Attack => self.attack_step = 1.0 / (v * self.sample_rate),
            // Release time is the time to fall to SILENT (−80 dB), not a time
            // constant: a 0.3 s release is silent after 0.3 s, not after 2.8 s.
            Param::Release => self.release_coef = (SILENT.ln() / (v * self.sample_rate)).exp(),
        }
    }

    /// Start a note on a free voice, stealing the oldest one if none is free.
    pub fn note_on(&mut self, source: Source, note: u8, velocity: f32) {
        let note = note.min(127);
        let increment = midi_to_hz(note) / self.sample_rate;
        self.counter = self.counter.wrapping_add(1);
        let started = self.counter;
        let slot = match self.voices.iter_mut().find(|v| !v.active) {
            Some(free) => Some(free),
            None => self.voices.iter_mut().min_by_key(|v| v.started),
        };
        if let Some(v) = slot {
            *v = Voice {
                active: true,
                gate: true,
                source,
                note,
                velocity: velocity.clamp(0.0, 1.0),
                phase: 0.0,
                increment,
                env: 0.0,
                started,
            };
        }
    }

    /// Release every gated voice playing this note on this source.
    pub fn note_off(&mut self, source: Source, note: u8) {
        for v in self
            .voices
            .iter_mut()
            .filter(|v| v.gate && v.source == source && v.note == note)
        {
            v.gate = false;
        }
    }

    /// Release every voice.
    pub fn all_off(&mut self) {
        for v in self.voices.iter_mut() {
            v.gate = false;
        }
    }

    /// Voices still sounding (gated or releasing).
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    /// Render `frames` (at most `BLOCK`) into the output buffer.
    pub fn render(&mut self, frames: usize) {
        let n = frames.min(BLOCK);
        let (left, right) = self.out.split_at_mut(BLOCK);
        left.fill(0.0);
        for v in self.voices.iter_mut().filter(|v| v.active) {
            for sample in left.iter_mut().take(n) {
                if v.gate {
                    v.env = (v.env + self.attack_step).min(1.0);
                } else {
                    v.env *= self.release_coef;
                    if v.env < SILENT {
                        v.active = false;
                        break;
                    }
                }
                *sample += lookup(&self.sine, v.phase) * v.env * v.velocity;
                v.phase += v.increment;
                if v.phase >= 1.0 {
                    v.phase -= 1.0;
                }
            }
        }
        for sample in left.iter_mut() {
            *sample *= self.master_gain;
        }
        right.copy_from_slice(left);
    }

    /// The planar output buffer: left then right, `BLOCK` samples each.
    pub fn output(&self) -> &[f32; 2 * BLOCK] {
        &self.out
    }
}

/// Equal temperament, A4 = 440 Hz. Called per note, not per sample.
fn midi_to_hz(note: u8) -> f32 {
    440.0 * ((f32::from(note) - 69.0) / 12.0).exp2()
}

/// Linear-interpolated table lookup for a phase in `0..1`.
fn lookup(table: &[f32], phase: f32) -> f32 {
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

    fn peak(e: &Engine) -> f32 {
        e.output().iter().fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn silent_without_notes() {
        let mut e = Engine::new(48_000.0);
        e.render(BLOCK);
        assert_eq!(peak(&e), 0.0);
    }

    #[test]
    fn note_sounds_then_releases_to_silence() {
        let mut e = Engine::new(48_000.0);
        e.set_param(Param::Release, 0.01);
        e.note_on(Source::Mono, 69, 1.0);
        for _ in 0..10 {
            e.render(BLOCK);
        }
        assert!(peak(&e) > 0.1);
        e.note_off(Source::Mono, 69);
        for _ in 0..100 {
            e.render(BLOCK);
        }
        assert_eq!(e.active_voices(), 0);
        assert_eq!(peak(&e), 0.0);
    }

    #[test]
    fn release_time_is_time_to_silence() {
        let mut e = Engine::new(48_000.0);
        e.set_param(Param::Release, 0.1);
        e.note_on(Source::Mono, 69, 1.0);
        for _ in 0..40 {
            e.render(BLOCK);
        }
        e.note_off(Source::Mono, 69);
        // 0.05 s = 18.75 blocks: still sounding.
        for _ in 0..18 {
            e.render(BLOCK);
        }
        assert_eq!(e.active_voices(), 1);
        // 0.1 s = 37.5 blocks, plus one block of float slack: silent.
        for _ in 0..21 {
            e.render(BLOCK);
        }
        assert_eq!(e.active_voices(), 0);
    }

    #[test]
    fn output_stays_finite_and_bounded_when_the_pool_is_full() {
        let mut e = Engine::new(44_100.0);
        e.set_param(Param::MasterGain, 1.0);
        for n in 0..40 {
            e.note_on(Source::Wave, n + 40, 1.0);
        }
        assert_eq!(e.active_voices(), VOICES);
        e.render(BLOCK);
        assert!(
            e.output()
                .iter()
                .all(|s| s.is_finite() && s.abs() <= VOICES as f32)
        );
    }

    #[test]
    fn note_off_only_touches_its_source() {
        let mut e = Engine::new(48_000.0);
        e.note_on(Source::Mono, 60, 1.0);
        e.note_on(Source::Drums, 60, 1.0);
        e.note_off(Source::Drums, 60);
        assert_eq!(e.voices.iter().filter(|v| v.gate).count(), 1);
    }

    #[test]
    fn a4_is_440() {
        assert!((midi_to_hz(69) - 440.0).abs() < 1.0e-3);
        assert!((midi_to_hz(81) - 880.0).abs() < 1.0e-2);
    }

    #[test]
    fn short_blocks_leave_the_tail_silent() {
        let mut e = Engine::new(48_000.0);
        e.note_on(Source::Mono, 69, 1.0);
        e.render(BLOCK);
        e.render(64);
        assert!(
            e.output()
                .iter()
                .skip(64)
                .take(BLOCK - 64)
                .all(|s| *s == 0.0)
        );
    }

    #[test]
    fn bad_sample_rate_falls_back() {
        let e = Engine::new(f32::NAN);
        assert_eq!(e.sample_rate, 48_000.0);
    }
}
