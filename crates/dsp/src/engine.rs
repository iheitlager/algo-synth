//! The engine: one Mono voice per owner, the MIDI player, planar stereo
//! blocks.
//!
//! Real-time rules (ADR-0002): `render` never allocates, never panics and
//! never calls `sin`/`exp`/`pow` per sample. The voices, tables and output are
//! allocated in `Engine::new`. Loading a MIDI file allocates, once, between
//! blocks (`load_midi`), never inside `render`.

use crate::mono::MonoParams;
use crate::mono::ladder::{LadderTables, saturate};
use crate::mono::osc::Blep;
use crate::mono::preset::{DEFAULTS, Preset};
use crate::mono::voice::{MonoCtx, MonoVoice, PitchTable};
use crate::params::Param;
use crate::player::Sequence;
use crate::smf;
use crate::voice::{Owner, sine_table};

/// Frames per render call; the Web Audio render quantum.
pub const BLOCK: usize = 128;
/// MIDI channels the player routes.
pub const CHANNELS: usize = 16;
/// Mono voices: one for live input, one per MIDI channel (spec 004 Req 6).
pub const MONO_VOICES: usize = 1 + CHANNELS;
/// Largest MIDI file accepted: 16 MiB.
pub const MAX_MIDI: usize = 16 << 20;
/// The master limiter passes everything below this level unchanged.
const KNEE: f32 = 0.5;

/// The whole synth, one per wasm instance (one per AudioWorklet node).
pub struct Engine {
    sample_rate: f32,
    sine: Vec<f32>,
    blep: Blep,
    mono: MonoParams,
    /// The last value set per parameter id, clamped, for the view to read.
    values: [f32; Param::ALL.len()],
    ladder: LadderTables,
    pitch: PitchTable,
    /// Index 0 plays live input, 1 + n plays MIDI channel n.
    monos: [MonoVoice; MONO_VOICES],
    master_gain: f32,
    /// Planar output: `BLOCK` left samples, then `BLOCK` right samples.
    out: Box<[f32; 2 * BLOCK]>,
    /// The MIDI file's bytes, written by JavaScript before `load_midi`.
    midi: Vec<u8>,
    sequence: Sequence,
    /// Whether each MIDI channel plays; `false` mutes it.
    route: [bool; CHANNELS],
}

impl Engine {
    /// Allocate everything `render` will ever use.
    pub fn new(sample_rate: f32) -> Engine {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let sine = sine_table();
        let mut engine = Engine {
            sample_rate,
            sine,
            blep: Blep::new(),
            mono: MonoParams::new(sample_rate),
            values: [0.0; Param::ALL.len()],
            ladder: LadderTables::new(sample_rate),
            pitch: PitchTable::new(sample_rate),
            monos: std::array::from_fn(|i| {
                MonoVoice::new((i as u32 + 1).wrapping_mul(2_654_435_761))
            }),
            master_gain: 0.5,
            out: Box::new([0.0; 2 * BLOCK]),
            midi: Vec::new(),
            sequence: Sequence::default(),
            route: [true; CHANNELS],
        };
        engine.set_param(Param::MasterGain, 0.5);
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

    /// Live input: press a key on the live Mono voice.
    pub fn note_on(&mut self, note: u8, velocity: f32) {
        self.start_voice(Owner::Live, note, velocity);
    }

    /// Live input: release this key.
    pub fn note_off(&mut self, note: u8) {
        self.stop_note(Owner::Live, note);
    }

    /// Release every voice.
    pub fn all_off(&mut self) {
        for m in self.monos.iter_mut() {
            m.release_all();
        }
    }

    /// Voices still sounding (gated or releasing).
    pub fn active_voices(&self) -> usize {
        self.monos.iter().filter(|m| m.active()).count()
    }

    fn start_voice(&mut self, owner: Owner, note: u8, velocity: f32) {
        if let Some(m) = self.monos.get_mut(mono_index(owner)) {
            m.press(note.min(127), velocity, &self.mono);
        }
    }

    /// Release `note` from `owner`.
    fn stop_note(&mut self, owner: Owner, note: u8) {
        if let Some(m) = self.monos.get_mut(mono_index(owner)) {
            m.release(note, &self.mono);
        }
    }

    fn release_player(&mut self) {
        for m in self.monos.iter_mut().skip(1) {
            m.release_all();
        }
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
        self.route = [true; CHANNELS];
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

    /// Play `channel` on Mono, or mute it with `false`.
    pub fn route(&mut self, channel: u8, on: bool) {
        if let Some(slot) = self.route.get_mut(usize::from(channel)) {
            *slot = on;
            if let Some(m) = self.monos.get_mut(mono_index(Owner::Channel(channel))) {
                m.release_all();
            }
        }
    }

    pub fn routed(&self, channel: u8) -> bool {
        self.route
            .get(usize::from(channel))
            .copied()
            .unwrap_or(false)
    }

    fn fire_due_events(&mut self) {
        while let Some(ev) = self.sequence.due() {
            let owner = Owner::Channel(ev.channel);
            if !ev.on {
                self.stop_note(owner, ev.note);
            } else if self.routed(ev.channel) {
                self.start_voice(owner, ev.note, ev.velocity);
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
            let mono = MonoCtx {
                params: &self.mono,
                sine: &self.sine,
                blep: &self.blep,
                ladder: &self.ladder,
                pitch: &self.pitch,
            };
            if let Some(buf) = self.out.get_mut(t..t + chunk) {
                for m in self.monos.iter_mut().filter(|m| m.active()) {
                    m.render(&mono, buf);
                }
            }
            self.sequence.advance(chunk);
            t += chunk;
        }
        let (left, right) = self.out.split_at_mut(BLOCK);
        for sample in left.iter_mut() {
            *sample = soft_clip(*sample * self.master_gain);
        }
        right.copy_from_slice(left);
    }

    /// The planar output buffer: left then right, `BLOCK` samples each.
    pub fn output(&self) -> &[f32; 2 * BLOCK] {
        &self.out
    }
}

/// Which of `Engine::monos` plays for `owner`.
fn mono_index(owner: Owner) -> usize {
    match owner {
        Owner::Live => 0,
        Owner::Channel(ch) => 1 + usize::from(ch),
    }
}

/// The last stage before the speakers (ADR-0002 rule 5): unchanged below
/// `KNEE`, then a smooth bend that never passes ±1; NaN and infinity become
/// silence instead of reaching the output.
fn soft_clip(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        (KNEE + (1.0 - KNEE) * saturate((a - KNEE) / (1.0 - KNEE))).copysign(x)
    }
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

    /// Mono's VCA is its ADSR: it sounds, then releases to silence.
    #[test]
    fn mono_follows_its_adsr() {
        let mut e = Engine::new(48_000.0);
        e.set_param(Param::AdsrRelease, 0.01);
        e.note_on(57, 1.0);
        for _ in 0..40 {
            e.render(BLOCK);
        }
        assert!(peak(&e) > 0.05);
        e.note_off(57);
        // 0.01 s is 3.75 blocks.
        for _ in 0..5 {
            e.render(BLOCK);
        }
        assert_eq!(e.active_voices(), 0);
        for _ in 0..2 {
            e.render(BLOCK);
        }
        assert_eq!(peak(&e), 0.0);
    }

    /// A note on and off before the next block still sounds, then ends.
    #[test]
    fn a_mono_tap_shorter_than_a_block_sounds() {
        let mut e = Engine::new(48_000.0);
        e.set_param(Param::AdsrAttack, 0.001);
        e.set_param(Param::AdsrRelease, 0.01);
        e.note_on(69, 1.0);
        e.note_off(69);
        let mut heard = 0.0_f32;
        for _ in 0..10 {
            e.render(BLOCK);
            heard = heard.max(peak(&e));
        }
        assert!(heard > 0.01, "peak {heard}");
        assert_eq!(e.active_voices(), 0);
    }

    /// Mono's sustain slider moves a held note.
    #[test]
    fn mono_sustain_moves_a_held_note() {
        let level = |sustain: f32| {
            let mut e = Engine::new(48_000.0);
            e.set_param(Param::AdsrDecay, 0.01);
            e.note_on(57, 1.0);
            for _ in 0..40 {
                e.render(BLOCK);
            }
            e.set_param(Param::AdsrSustain, sustain);
            let mut sum = 0.0;
            for _ in 0..40 {
                e.render(BLOCK);
                sum += e.output().iter().map(|s| s * s).sum::<f32>();
            }
            sum.sqrt()
        };
        let ratio = level(0.35) / level(0.7);
        assert!(
            (ratio - 0.5).abs() < 0.05,
            "half the sustain, half the level: {ratio}"
        );
    }

    /// Spec 004 Req 6: live input and each MIDI channel have their own Mono
    /// voice, and a note off on one leaves the others gated.
    #[test]
    fn mono_owners_are_independent() {
        let mut e = Engine::new(48_000.0);
        e.start_voice(Owner::Channel(2), 60, 1.0);
        e.start_voice(Owner::Channel(3), 64, 1.0);
        e.note_on(67, 1.0);
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 3);
        e.stop_note(Owner::Channel(2), 60);
        assert!(!e.monos[3].gated());
        assert!(e.monos[4].gated() && e.monos[0].gated());
        // Live Mono is monophonic: a second key moves the same voice.
        e.note_on(69, 1.0);
        e.render(BLOCK);
        assert_eq!(e.monos[0].note(), 69);
        assert_eq!(e.active_voices(), 3);
    }

    /// 16 Mono voices at full resonance, drive and level still stay in ±1.
    #[test]
    fn loud_patches_are_limited_to_full_scale() {
        let mut e = Engine::new(48_000.0);
        for (p, v) in [
            (Param::MasterGain, 1.0),
            (Param::Vco2Level, 1.0),
            (Param::Vco3Level, 1.0),
            (Param::NoiseLevel, 1.0),
            (Param::Resonance, 1.0),
            (Param::Drive, 1.0),
            (Param::AdsrSustain, 1.0),
        ] {
            e.set_param(p, v);
        }
        // 16 Mono voices: one per MIDI channel.
        for ch in 0..16 {
            e.start_voice(Owner::Channel(ch), 36 + 3 * ch, 1.0);
        }
        let mut peak_seen = 0.0_f32;
        for _ in 0..200 {
            e.render(BLOCK);
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            peak_seen = peak_seen.max(peak(&e));
        }
        assert!(peak_seen > 0.9, "the limiter is reached, peak {peak_seen}");
    }

    #[test]
    fn soft_clip_is_transparent_below_the_knee() {
        for x in [-0.5, -0.2, 0.0, 0.3, 0.5] {
            assert_eq!(soft_clip(x), x);
        }
        let mut prev = soft_clip(0.5);
        for i in 1..1000 {
            let y = soft_clip(0.5 + i as f32 * 0.01);
            assert!(y >= prev && y <= 1.0, "monotonic and bounded at {i}");
            prev = y;
        }
        assert_eq!(soft_clip(f32::NAN), 0.0);
        assert_eq!(soft_clip(f32::NEG_INFINITY), 0.0);
        assert_eq!(soft_clip(-50.0), -1.0);
    }

    #[test]
    fn short_blocks_leave_the_tail_silent() {
        let mut e = Engine::new(48_000.0);
        e.note_on(69, 1.0);
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
    fn every_channel_plays_and_mute_silences() {
        let mut e = Engine::new(48_000.0);
        load(&mut e, &one_note(9)).expect("loads");
        assert!((0..16).all(|ch| e.routed(ch)));
        e.route(9, false);
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
        e.note_on(72, 1.0);
        e.play();
        for _ in 0..200 {
            e.render(BLOCK);
        }
        e.stop();
        assert!(e.monos[0].gated(), "the live Mono voice is still held");
        assert!(!e.monos[1].gated(), "the player's Mono voice is released");
    }

    #[test]
    fn bad_files_are_rejected_and_keep_the_old_song() {
        let mut e = Engine::new(48_000.0);
        load(&mut e, &one_note(0)).expect("loads");
        assert_eq!(load(&mut e, b"not midi"), Err(smf::Error::NotMidi));
        assert_eq!(e.sequence().parts().len(), 1);
        assert!(e.midi_buffer(MAX_MIDI + 1).is_none());
    }

    /// The shipped demo (tools/make_demo_mid.py) parses into its four parts.
    #[test]
    fn demo_file_loads() {
        let mut e = Engine::new(48_000.0);
        let parts = load(&mut e, include_bytes!("../../../web/public/demo.mid")).expect("loads");
        assert_eq!(parts, 4);
        let channels: Vec<u8> = e.sequence().parts().iter().map(|p| p.channel).collect();
        assert_eq!(channels, vec![0, 1, 2, 3]);
    }
}
