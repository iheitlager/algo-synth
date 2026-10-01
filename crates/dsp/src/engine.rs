//! The engine: a fixed voice pool, the MIDI player, planar stereo blocks.
//!
//! Real-time rules (ADR-0002): `render` never allocates, never panics and
//! never calls `sin`/`exp`/`pow` per sample. The pool, tables and output are
//! allocated in `Engine::new`. Loading a MIDI file allocates, once, between
//! blocks (`load_midi`), never inside `render`.

use crate::mono::MonoParams;
use crate::mono::ladder::LadderTables;
use crate::mono::osc::Blep;
use crate::mono::preset::{DEFAULTS, Preset};
use crate::params::Param;
use crate::player::Sequence;
use crate::smf;
use crate::source::Source;
use crate::voice::{Ctx, Owner, TABLE, Voice, decay_coef};

/// Frames per render call; the Web Audio render quantum.
pub const BLOCK: usize = 128;
/// Voices in the pool, shared by live input and the player.
pub const VOICES: usize = 32;
/// MIDI channels the player routes.
pub const CHANNELS: usize = 16;
/// Largest MIDI file accepted: 16 MiB.
pub const MAX_MIDI: usize = 16 << 20;

/// The whole synth, one per wasm instance (one per AudioWorklet node).
pub struct Engine {
    sample_rate: f32,
    sine: Vec<f32>,
    blep: Blep,
    mono: MonoParams,
    /// The last value set per parameter id, clamped, for the view to read.
    values: [f32; Param::ALL.len()],
    ladder: LadderTables,
    voices: [Voice; VOICES],
    master_gain: f32,
    attack_step: f32,
    release_coef: f32,
    counter: u32,
    /// Planar output: `BLOCK` left samples, then `BLOCK` right samples.
    out: Box<[f32; 2 * BLOCK]>,
    /// The MIDI file's bytes, written by JavaScript before `load_midi`.
    midi: Vec<u8>,
    sequence: Sequence,
    /// Which source plays each MIDI channel; `None` mutes it.
    route: [Option<Source>; CHANNELS],
}

impl Engine {
    /// Allocate everything `render` will ever use.
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
            blep: Blep::new(),
            mono: MonoParams::new(sample_rate),
            values: [0.0; Param::ALL.len()],
            ladder: LadderTables::new(sample_rate),
            voices: [Voice::default(); VOICES],
            master_gain: 0.5,
            attack_step: 0.0,
            release_coef: 0.0,
            counter: 0,
            out: Box::new([0.0; 2 * BLOCK]),
            midi: Vec::new(),
            sequence: Sequence::default(),
            route: default_route(),
        };
        engine.set_param(Param::MasterGain, 0.5);
        engine.set_param(Param::Attack, 0.005);
        engine.set_param(Param::Release, 0.3);
        for (p, v) in DEFAULTS {
            engine.set_param(p, v);
        }
        engine
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Set a parameter; the value is clamped into its range.
    pub fn set_param(&mut self, param: Param, value: f32) {
        let v = param.clamp(value);
        if let Some(slot) = self.values.get_mut(param as usize) {
            *slot = v;
        }
        match param {
            Param::MasterGain => self.master_gain = v,
            Param::Attack => self.attack_step = 1.0 / (v * self.sample_rate),
            // Release time is the time to fall to −80 dB, not a time constant.
            Param::Release => self.release_coef = decay_coef(v, self.sample_rate),
            _ => self.mono.set(param, v),
        }
    }

    /// The value `param` was last set to, after clamping.
    pub fn param_value(&self, param: Param) -> f32 {
        self.values.get(param as usize).copied().unwrap_or(0.0)
    }

    /// Set every Mono parameter: the defaults, then the preset's changes.
    pub fn preset(&mut self, preset: Preset) {
        for (p, v) in DEFAULTS.iter().chain(preset.changes()) {
            self.set_param(*p, *v);
        }
    }

    /// Live input: start a note on a free voice, stealing the oldest.
    pub fn note_on(&mut self, source: Source, note: u8, velocity: f32) {
        self.start_voice(source, Owner::Live, note, velocity);
    }

    /// Live input: release this note on this source.
    pub fn note_off(&mut self, source: Source, note: u8) {
        self.release(|v| v.owner == Owner::Live && v.source == source && v.note == note);
    }

    /// Release every voice.
    pub fn all_off(&mut self) {
        self.release(|_| true);
    }

    /// Voices still sounding (gated or releasing).
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    fn start_voice(&mut self, source: Source, owner: Owner, note: u8, velocity: f32) {
        self.counter = self.counter.wrapping_add(1);
        let voice = Voice::start(
            source,
            owner,
            note.min(127),
            velocity,
            self.counter,
            self.sample_rate,
        );
        let slot = match self.voices.iter_mut().find(|v| !v.active) {
            Some(free) => Some(free),
            None => self.voices.iter_mut().min_by_key(|v| v.started),
        };
        if let Some(v) = slot {
            *v = voice;
        }
    }

    fn release(&mut self, matches: impl Fn(&Voice) -> bool) {
        for v in self.voices.iter_mut().filter(|v| v.gate && matches(v)) {
            v.gate = false;
        }
    }

    fn release_player(&mut self) {
        self.release(|v| matches!(v.owner, Owner::Channel(_)));
    }

    // --- MIDI player -----------------------------------------------------

    /// Size the MIDI buffer for `len` bytes and return it for writing.
    /// `None` if the file is larger than `MAX_MIDI`.
    pub fn midi_buffer(&mut self, len: usize) -> Option<&mut [u8]> {
        if len > MAX_MIDI {
            return None;
        }
        self.midi.clear();
        self.midi.resize(len, 0);
        Some(&mut self.midi)
    }

    /// Parse the buffer and make it the current sequence, stopped at the
    /// top with the default routing. Returns the number of parts.
    pub fn load_midi(&mut self) -> Result<usize, smf::Error> {
        let parsed = smf::parse(&self.midi)?;
        self.release_player();
        self.sequence = Sequence::compile(&parsed, self.sample_rate);
        self.route = default_route();
        Ok(self.sequence.parts().len())
    }

    pub fn sequence(&self) -> &Sequence {
        &self.sequence
    }

    pub fn play(&mut self) {
        self.sequence.play();
    }

    pub fn stop(&mut self) {
        self.sequence.stop();
        self.release_player();
    }

    pub fn seek(&mut self, sample: u64) {
        self.sequence.seek(sample);
        self.release_player();
    }

    /// Route `channel` to `source`, or mute it with `None`.
    pub fn route(&mut self, channel: u8, source: Option<Source>) {
        if let Some(slot) = self.route.get_mut(usize::from(channel)) {
            *slot = source;
            self.release(|v| v.owner == Owner::Channel(channel));
        }
    }

    pub fn routed(&self, channel: u8) -> Option<Source> {
        self.route.get(usize::from(channel)).copied().flatten()
    }

    fn fire_due_events(&mut self) {
        while let Some(ev) = self.sequence.due() {
            let owner = Owner::Channel(ev.channel);
            if ev.on {
                if let Some(source) = self.routed(ev.channel) {
                    self.start_voice(source, owner, ev.note, ev.velocity);
                }
            } else {
                self.release(|v| v.owner == owner && v.note == ev.note);
            }
        }
        if self.sequence.finished() {
            self.stop();
        }
    }

    // --- Render ----------------------------------------------------------

    /// Render `frames` (at most `BLOCK`) into the output buffer. The block is
    /// split at player events, so every note starts on its exact sample.
    pub fn render(&mut self, frames: usize) {
        let n = frames.min(BLOCK);
        self.out.fill(0.0);
        let mut t = 0;
        while t < n {
            self.fire_due_events();
            let chunk = self.sequence.frames_until_next(n - t);
            let ctx = Ctx {
                sine: &self.sine,
                blep: &self.blep,
                mono: &self.mono,
                ladder: &self.ladder,
                attack_step: self.attack_step,
                release_coef: self.release_coef,
            };
            if let Some(buf) = self.out.get_mut(t..t + chunk) {
                for v in self.voices.iter_mut().filter(|v| v.active) {
                    v.render(&ctx, buf);
                }
            }
            self.sequence.advance(chunk);
            t += chunk;
        }
        let (left, right) = self.out.split_at_mut(BLOCK);
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

/// Channel 10 (index 9) is drums in General MIDI; everything else starts on
/// Mono: the ensemble of 2600s.
fn default_route() -> [Option<Source>; CHANNELS] {
    let mut route = [Some(Source::Mono); CHANNELS];
    if let Some(drums) = route.get_mut(9) {
        *drums = Some(Source::Drums);
    }
    route
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mono::osc::LATENCY;
    use crate::smf::tests::file;

    fn peak(e: &Engine) -> f32 {
        e.output().iter().fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    fn load(e: &mut Engine, bytes: &[u8]) -> Result<usize, smf::Error> {
        e.midi_buffer(bytes.len())
            .expect("fits")
            .copy_from_slice(bytes);
        e.load_midi()
    }

    /// One note on channel `ch` at tick 480 (0.5 s at 120 BPM), 480 ticks long.
    fn one_note(ch: u8) -> Vec<u8> {
        let t = vec![
            0x83,
            0x60,
            0x90 | ch,
            60,
            100,
            0x83,
            0x60,
            0x80 | ch,
            60,
            0,
            0x00,
            0xFF,
            0x2F,
            0,
        ];
        file(0, 480, &[t])
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
        e.note_on(Source::Wave, 69, 1.0);
        for _ in 0..10 {
            e.render(BLOCK);
        }
        assert!(peak(&e) > 0.1);
        e.note_off(Source::Wave, 69);
        for _ in 0..100 {
            e.render(BLOCK);
        }
        assert_eq!(e.active_voices(), 0);
        assert_eq!(peak(&e), 0.0);
    }

    /// Mono's VCA is its own ADSR, not the test voice's envelope.
    #[test]
    fn mono_follows_its_adsr() {
        let mut e = Engine::new(48_000.0);
        e.set_param(Param::Release, 10.0);
        e.set_param(Param::AdsrRelease, 0.01);
        e.note_on(Source::Mono, 57, 1.0);
        for _ in 0..40 {
            e.render(BLOCK);
        }
        assert!(peak(&e) > 0.05);
        e.note_off(Source::Mono, 57);
        // 0.01 s is 3.75 blocks.
        for _ in 0..5 {
            e.render(BLOCK);
        }
        assert_eq!(e.active_voices(), 0);
    }

    #[test]
    fn release_time_is_time_to_silence() {
        let mut e = Engine::new(48_000.0);
        e.set_param(Param::Release, 0.1);
        e.note_on(Source::Wave, 69, 1.0);
        for _ in 0..40 {
            e.render(BLOCK);
        }
        e.note_off(Source::Wave, 69);
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
        for n in 0..80 {
            e.note_on(Source::Mono, n + 20, 1.0);
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
        e.note_on(Source::Wave, 60, 1.0);
        e.note_off(Source::Wave, 60);
        assert_eq!(e.voices.iter().filter(|v| v.gate).count(), 1);
    }

    #[test]
    fn short_blocks_leave_the_tail_silent() {
        let mut e = Engine::new(48_000.0);
        e.note_on(Source::Wave, 69, 1.0);
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

    #[test]
    fn player_note_starts_on_its_exact_sample() {
        let mut e = Engine::new(48_000.0);
        assert_eq!(load(&mut e, &one_note(0)), Ok(1));
        e.play();
        // 24_000 = 187 blocks + 64 frames; Mono's VCOs lag by LATENCY.
        for _ in 0..187 {
            e.render(BLOCK);
            assert_eq!(peak(&e), 0.0);
        }
        e.render(BLOCK);
        let left = &e.output()[..BLOCK];
        assert!(left[..64].iter().all(|s| *s == 0.0));
        assert!(
            left[64..64 + LATENCY + 2].iter().any(|s| *s != 0.0),
            "the note should start at frame 64"
        );
    }

    #[test]
    fn player_stops_at_the_end_and_releases() {
        let mut e = Engine::new(48_000.0);
        load(&mut e, &one_note(0)).expect("loads");
        e.play();
        for _ in 0..(48_000 * 2 / BLOCK) {
            e.render(BLOCK);
        }
        assert!(!e.sequence().playing());
        assert_eq!(e.active_voices(), 0);
    }

    #[test]
    fn channel_ten_is_drums_and_mute_silences() {
        let mut e = Engine::new(48_000.0);
        load(&mut e, &one_note(9)).expect("loads");
        assert_eq!(e.routed(9), Some(Source::Drums));
        assert_eq!(e.routed(0), Some(Source::Mono));
        e.route(9, None);
        e.play();
        for _ in 0..300 {
            e.render(BLOCK);
            assert_eq!(peak(&e), 0.0);
        }
    }

    #[test]
    fn stop_releases_player_voices_but_not_live_ones() {
        let mut e = Engine::new(48_000.0);
        load(&mut e, &one_note(0)).expect("loads");
        e.note_on(Source::Wave, 72, 1.0);
        e.play();
        for _ in 0..200 {
            e.render(BLOCK);
        }
        e.stop();
        let gated: Vec<Owner> = e
            .voices
            .iter()
            .filter(|v| v.gate)
            .map(|v| v.owner)
            .collect();
        assert_eq!(gated, vec![Owner::Live]);
    }

    #[test]
    fn bad_files_are_rejected_and_keep_the_old_song() {
        let mut e = Engine::new(48_000.0);
        load(&mut e, &one_note(0)).expect("loads");
        assert_eq!(load(&mut e, b"not midi"), Err(smf::Error::NotMidi));
        assert_eq!(e.sequence().parts().len(), 1);
        assert!(e.midi_buffer(MAX_MIDI + 1).is_none());
    }

    /// The shipped demo (tools/make_demo_mid.py) parses into its five parts.
    #[test]
    fn demo_file_loads() {
        let mut e = Engine::new(48_000.0);
        let parts = load(&mut e, include_bytes!("../../../web/public/demo.mid")).expect("loads");
        assert_eq!(parts, 5);
        let channels: Vec<u8> = e.sequence().parts().iter().map(|p| p.channel).collect();
        assert_eq!(channels, vec![0, 1, 2, 3, 9]);
    }
}
