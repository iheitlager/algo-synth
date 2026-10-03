//! The engine: up to `SYNTHS` Mono synths, each with its own parameters,
//! one Mono voice per owner, the MIDI player, planar stereo blocks.
//!
//! Real-time rules (ADR-0002): `render` never allocates, never panics and
//! never calls `sin`/`exp`/`pow` per sample. The voices, tables and output are
//! allocated in `Engine::new`. Loading a MIDI file allocates, once, between
//! blocks (`load_midi`), never inside `render`.

use crate::fx::compressor::Compressor;
use crate::fx::eq::{EqBand, Equalizer};
use crate::fx::limiter::Limiter;
use crate::fx::processor::Processor;
use crate::mixer::{Mixer, SENDS, STRIP_DEFAULTS};
use crate::mono::MonoParams;
use crate::mono::ladder::LadderTables;
use crate::mono::osc::Blep;
use crate::mono::preset::{DEFAULTS, Preset};
use crate::mono::voice::{MonoCtx, MonoVoice, PitchTable};
use crate::params::{GLOBAL_DEFAULTS, Param};
use crate::player::Sequence;
use crate::smf;
use crate::voice::{Owner, sine_table};

/// Frames per render call; the Web Audio render quantum.
pub const BLOCK: usize = 128;
/// MIDI channels the player routes.
pub const CHANNELS: usize = 16;
/// Mono synths, each with its own parameters (plan.md MVP 5).
pub const SYNTHS: usize = 16;
/// Mono voices: one live voice per synth, one per MIDI channel (spec 004 Req 6).
pub const MONO_VOICES: usize = SYNTHS + CHANNELS;
/// Largest MIDI file accepted: 16 MiB.
pub const MAX_MIDI: usize = 16 << 20;

/// The whole synth, one per wasm instance (one per AudioWorklet node).
pub struct Engine {
    sample_rate: f32,
    sine: Vec<f32>,
    blep: Blep,
    synths: [MonoParams; SYNTHS],
    /// The last value set per synth and parameter id, clamped, for the view.
    values: [[f32; Param::ALL.len()]; SYNTHS],
    ladder: LadderTables,
    pitch: PitchTable,
    /// Index s plays synth s's live input, `SYNTHS + n` plays MIDI channel n.
    monos: [MonoVoice; MONO_VOICES],
    /// The synth each voice plays, fixed when its note starts.
    synth_of: [usize; MONO_VOICES],
    mixer: Mixer,
    /// The effect processors P1–P4, fed by the mixer's sends.
    procs: [Processor; SENDS],
    eq: Equalizer,
    comp: Compressor,
    limiter: Limiter,
    master_gain: f32,
    /// Planar output: `BLOCK` left samples, then `BLOCK` right samples.
    out: Box<[f32; 2 * BLOCK]>,
    /// The MIDI file's bytes, written by JavaScript before `load_midi`.
    midi: Vec<u8>,
    sequence: Sequence,
    /// The synth each MIDI channel plays on; `None` mutes it.
    route: [Option<usize>; CHANNELS],
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
            synths: [MonoParams::new(sample_rate); SYNTHS],
            values: [[0.0; Param::ALL.len()]; SYNTHS],
            ladder: LadderTables::new(sample_rate),
            pitch: PitchTable::new(sample_rate),
            monos: std::array::from_fn(|i| {
                MonoVoice::new((i as u32 + 1).wrapping_mul(2_654_435_761))
            }),
            synth_of: std::array::from_fn(|i| if i < SYNTHS { i } else { 0 }),
            mixer: Mixer::new(sample_rate),
            procs: std::array::from_fn(|_| Processor::new(sample_rate)),
            eq: Equalizer::new(sample_rate),
            comp: Compressor::new(sample_rate),
            limiter: Limiter::new(sample_rate),
            master_gain: 0.5,
            out: Box::new([0.0; 2 * BLOCK]),
            midi: Vec::new(),
            sequence: Sequence::default(),
            route: [Some(0); CHANNELS],
        };
        for (p, v) in GLOBAL_DEFAULTS {
            engine.set_param(0, p, v);
        }
        for synth in 0..SYNTHS {
            engine.reset(synth);
        }
        engine
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Set a parameter of `synth`; the value is clamped into its range.
    /// `MasterGain` is global, whichever synth it is sent to. Unknown synths
    /// are ignored.
    pub fn set_param(&mut self, synth: usize, param: Param, value: f32) {
        let v = param.clamp(value);
        if param.is_global() {
            self.set_global(param, v);
            for values in self.values.iter_mut() {
                if let Some(slot) = values.get_mut(param as usize) {
                    *slot = v;
                }
            }
            return;
        }
        let (Some(values), Some(mono)) = (self.values.get_mut(synth), self.synths.get_mut(synth))
        else {
            return;
        };
        if let Some(slot) = values.get_mut(param as usize) {
            *slot = v;
        }
        if !param.is_strip() {
            mono.set(param, v);
        }
        self.mixer.set(synth, param, v);
    }

    fn set_global(&mut self, param: Param, v: f32) {
        match param {
            Param::MasterGain => self.master_gain = v,
            Param::CompThreshold => self.comp.set_threshold(v),
            Param::CompRatio => self.comp.set_ratio(v),
            Param::CompAttack => self.comp.set_attack(v),
            Param::CompRelease => self.comp.set_release(v),
            Param::CompMakeup => self.comp.set_makeup(v),
            Param::EqLowFreq => self.eq.set_freq(EqBand::Low, v),
            Param::EqLowGain => self.eq.set_gain(EqBand::Low, v),
            Param::EqMid1Freq => self.eq.set_freq(EqBand::Mid1, v),
            Param::EqMid1Gain => self.eq.set_gain(EqBand::Mid1, v),
            Param::EqMid1Q => self.eq.set_q(EqBand::Mid1, v),
            Param::EqMid2Freq => self.eq.set_freq(EqBand::Mid2, v),
            Param::EqMid2Gain => self.eq.set_gain(EqBand::Mid2, v),
            Param::EqMid2Q => self.eq.set_q(EqBand::Mid2, v),
            Param::EqHighFreq => self.eq.set_freq(EqBand::High, v),
            Param::EqHighGain => self.eq.set_gain(EqBand::High, v),
            _ => {}
        }
        if let Some((slot, field)) = param.processor() {
            if let Some(p) = self.procs.get_mut(slot) {
                p.set(field, v);
            }
        }
    }

    /// The value `param` of `synth` was last set to, after clamping.
    pub fn param_value(&self, synth: usize, param: Param) -> f32 {
        self.values
            .get(synth)
            .and_then(|v| v.get(param as usize))
            .copied()
            .unwrap_or(0.0)
    }

    /// Set every Mono parameter of `synth`: the defaults, then the preset's
    /// changes.
    pub fn preset(&mut self, synth: usize, preset: Preset) {
        for (p, v) in DEFAULTS.iter().chain(preset.changes()) {
            self.set_param(synth, *p, *v);
        }
    }

    /// Put `synth` back to the defaults, for a newly added synth.
    pub fn reset(&mut self, synth: usize) {
        for (p, v) in DEFAULTS.iter().chain(STRIP_DEFAULTS.iter()) {
            self.set_param(synth, *p, *v);
        }
    }

    /// Live input: press a key on `synth`'s live voice.
    pub fn note_on(&mut self, synth: usize, note: u8, velocity: f32) {
        if let Some(s) = live(synth) {
            self.start_voice(Owner::Live(s), note, velocity);
        }
    }

    /// Live input: release this key on `synth`.
    pub fn note_off(&mut self, synth: usize, note: u8) {
        if let Some(s) = live(synth) {
            self.stop_note(Owner::Live(s), note);
        }
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

    /// The synth `owner` plays now: its own for live input, the route for a
    /// channel.
    fn target(&self, owner: Owner) -> Option<usize> {
        match owner {
            Owner::Live(s) => Some(usize::from(s)),
            Owner::Channel(ch) => self.routed(ch),
        }
    }

    fn start_voice(&mut self, owner: Owner, note: u8, velocity: f32) {
        let i = mono_index(owner);
        let Some(synth) = self.target(owner) else {
            return;
        };
        let (Some(m), Some(slot), Some(params)) = (
            self.monos.get_mut(i),
            self.synth_of.get_mut(i),
            self.synths.get(synth),
        ) else {
            return;
        };
        // A voice changing synth starts clean, not legato from the old one.
        if *slot != synth {
            m.release_all();
            *slot = synth;
        }
        m.press(note.min(127), velocity, params);
    }

    /// Release `note` from `owner`.
    fn stop_note(&mut self, owner: Owner, note: u8) {
        let i = mono_index(owner);
        let synth = self.synth_of.get(i).copied().unwrap_or(0);
        if let (Some(m), Some(params)) = (self.monos.get_mut(i), self.synths.get(synth)) {
            m.release(note, params);
        }
    }

    fn release_player(&mut self) {
        for m in self.monos.iter_mut().skip(SYNTHS) {
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
    /// top, its parts on synths 0, 1, 2… in order. Returns the number of
    /// parts.
    pub fn load_midi(&mut self) -> Result<usize, smf::Error> {
        let parsed = smf::parse(&self.midi)?;
        self.release_player();
        self.sequence = Sequence::compile(&parsed, self.sample_rate);
        self.route = [Some(0); CHANNELS];
        for (synth, part) in self.sequence.parts().iter().enumerate().take(SYNTHS) {
            if let Some(slot) = self.route.get_mut(usize::from(part.channel)) {
                *slot = Some(synth);
            }
        }
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

    /// Play `channel` on `synth`, or mute it with `None`. An unknown synth
    /// mutes too.
    pub fn route(&mut self, channel: u8, synth: Option<usize>) {
        if let Some(slot) = self.route.get_mut(usize::from(channel)) {
            *slot = synth.filter(|s| *s < SYNTHS);
            if let Some(m) = self.monos.get_mut(mono_index(Owner::Channel(channel))) {
                m.release_all();
            }
        }
    }

    pub fn routed(&self, channel: u8) -> Option<usize> {
        self.route.get(usize::from(channel)).copied().flatten()
    }

    fn fire_due_events(&mut self) {
        while let Some(ev) = self.sequence.due() {
            let owner = Owner::Channel(ev.channel);
            if !ev.on {
                self.stop_note(owner, ev.note);
            } else {
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
        self.mixer.clear(n);
        let mut t = 0;
        while t < n {
            self.fire_due_events();
            let chunk = self.sequence.frames_until_next(n - t);
            for (m, synth) in self.monos.iter_mut().zip(self.synth_of.iter()) {
                let Some(params) = self.synths.get(*synth) else {
                    continue;
                };
                if m.active() {
                    if let Some(buf) = self.mixer.bus(*synth, t..t + chunk) {
                        let mono = MonoCtx {
                            params,
                            sine: &self.sine,
                            blep: &self.blep,
                            ladder: &self.ladder,
                            pitch: &self.pitch,
                        };
                        m.render(&mono, buf);
                    }
                }
            }
            self.sequence.advance(chunk);
            t += chunk;
        }
        let (left, right) = self.out.split_at_mut(BLOCK);
        self.mixer.mix(n, left, right);
        if let (Some(l), Some(r)) = (left.get_mut(..n), right.get_mut(..n)) {
            for (proc, send) in self.procs.iter_mut().zip(self.mixer.sends.iter()) {
                if let Some(send) = send.get(..n) {
                    proc.process(send, l, r);
                }
            }
            // The master chain: equalizer, compressor, gain, limiter.
            self.eq.process(l, r);
            self.comp.process(l, r);
            for s in l.iter_mut().chain(r.iter_mut()) {
                *s *= self.master_gain;
            }
            self.limiter.process(l, r);
        }
    }

    /// How far the master compressor turns the signal down now, in dB.
    pub fn gain_reduction_db(&self) -> f32 {
        self.comp.gain_reduction_db()
    }

    /// The planar output buffer: left then right, `BLOCK` samples each.
    pub fn output(&self) -> &[f32; 2 * BLOCK] {
        &self.out
    }
}

/// The live owner index for `synth`, or `None` past `SYNTHS`, so a live note
/// never lands on a channel's voice.
fn live(synth: usize) -> Option<u8> {
    u8::try_from(synth)
        .ok()
        .filter(|s| usize::from(*s) < SYNTHS)
}

/// Which of `Engine::monos` plays for `owner`.
fn mono_index(owner: Owner) -> usize {
    match owner {
        Owner::Live(s) => usize::from(s),
        Owner::Channel(ch) => SYNTHS + usize::from(ch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mono::model::Model;
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
        e.set_param(0, Param::AdsrRelease, 0.01);
        e.note_on(0, 57, 1.0);
        for _ in 0..40 {
            e.render(BLOCK);
        }
        assert!(peak(&e) > 0.05);
        e.note_off(0, 57);
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
        e.set_param(0, Param::AdsrAttack, 0.001);
        e.set_param(0, Param::AdsrRelease, 0.01);
        e.note_on(0, 69, 1.0);
        e.note_off(0, 69);
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
            e.set_param(0, Param::AdsrDecay, 0.01);
            e.note_on(0, 57, 1.0);
            for _ in 0..40 {
                e.render(BLOCK);
            }
            e.set_param(0, Param::AdsrSustain, sustain);
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
        e.note_on(0, 67, 1.0);
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 3);
        e.stop_note(Owner::Channel(2), 60);
        assert!(!e.monos[SYNTHS + 2].gated());
        assert!(e.monos[SYNTHS + 3].gated() && e.monos[0].gated());
        // Live Mono is monophonic: a second key moves the same voice.
        e.note_on(0, 69, 1.0);
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
            e.set_param(0, p, v);
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

    /// Mute `synth` at its mixer: every VCO and the noise at level 0.
    fn silence(e: &mut Engine, synth: usize) {
        for p in [
            Param::Vco1Level,
            Param::Vco2Level,
            Param::Vco3Level,
            Param::NoiseLevel,
        ] {
            e.set_param(synth, p, 0.0);
        }
    }

    /// Peak over `blocks` blocks.
    fn heard(e: &mut Engine, blocks: usize) -> f32 {
        (0..blocks).fold(0.0_f32, |m, _| {
            e.render(BLOCK);
            m.max(peak(e))
        })
    }

    /// plan.md MVP 5: each synth has its own patch.
    #[test]
    fn synths_have_their_own_parameters() {
        let mut e = Engine::new(48_000.0);
        silence(&mut e, 1);
        assert_eq!(e.param_value(1, Param::Vco1Level), 0.0);
        assert!(e.param_value(0, Param::Vco1Level) > 0.0);
        e.note_on(1, 57, 1.0);
        // Below −80 dB: the ladder leaves a residue of about −107 dB.
        assert!(heard(&mut e, 20) < 1.0e-4, "synth 1 is silenced");
        e.note_on(0, 57, 1.0);
        assert!(heard(&mut e, 20) > 0.05, "synth 0 still sounds");
        assert_eq!(e.active_voices(), 2);
    }

    /// Spec 005 Req 1: the model is a parameter of its own synth.
    #[test]
    fn models_are_per_synth() {
        let mut e = Engine::new(48_000.0);
        e.set_param(1, Param::Model, 1.0);
        e.set_param(1, Param::Model, 99.0);
        assert_eq!(e.param_value(1, Param::Model), 5.0, "clamped into range");
        e.set_param(1, Param::Model, 1.0);
        assert_eq!(e.param_value(1, Param::Model), 1.0);
        assert_eq!(e.param_value(0, Param::Model), 0.0);
        assert_eq!(e.param_value(2, Param::Model), 0.0);
        for (p, v) in DEFAULTS.iter().filter(|(p, _)| *p != Param::Model) {
            assert_eq!(e.param_value(0, *p), *v, "{p:?} on synth 0");
        }
    }

    /// Spec 005 Req 1: two synths with the same settings and different
    /// models sound different, and one's model leaves the other alone.
    #[test]
    fn models_sound_different() {
        let render = |model: f32| {
            let mut e = Engine::new(48_000.0);
            for (p, v) in [
                (Param::Model, model),
                (Param::Cutoff, 300.0),
                (Param::EnvCutoff, 0.6),
                (Param::FenvAttack, 0.5),
                (Param::AdsrSustain, 1.0),
            ] {
                e.set_param(1, p, v);
            }
            e.note_on(1, 45, 1.0);
            let mut out = Vec::new();
            for _ in 0..200 {
                e.render(BLOCK);
                out.extend_from_slice(e.output().get(..BLOCK).unwrap_or(&[]));
            }
            out
        };
        let (arp, mini) = (render(0.0), render(1.0));
        let diff: f32 = arp.iter().zip(&mini).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff > 1.0, "the models differ: {diff}");
        assert_eq!(arp, render(0.0), "and each is repeatable");
    }

    #[test]
    fn a_preset_on_one_synth_leaves_the_others() {
        let mut e = Engine::new(48_000.0);
        e.preset(2, Preset::Bass);
        for (p, v) in DEFAULTS {
            assert_eq!(e.param_value(0, p), v, "{p:?} on synth 0");
            assert_eq!(e.param_value(3, p), v, "{p:?} on synth 3");
        }
        e.reset(2);
        for (p, v) in DEFAULTS {
            assert_eq!(e.param_value(2, p), v, "{p:?} after reset");
        }
    }

    #[test]
    fn master_gain_is_global() {
        let mut e = Engine::new(48_000.0);
        e.set_param(5, Param::MasterGain, 0.2);
        assert_eq!(e.param_value(0, Param::MasterGain), 0.2);
        assert_eq!(e.master_gain, 0.2);
    }

    /// Out-of-range synths are ignored, and a live note never reaches a
    /// channel's voice.
    #[test]
    fn unknown_synths_are_ignored() {
        let mut e = Engine::new(48_000.0);
        e.start_voice(Owner::Channel(4), 60, 1.0);
        e.note_on(SYNTHS, 62, 1.0);
        e.note_off(SYNTHS + 4, 60);
        e.set_param(SYNTHS, Param::Cutoff, 300.0);
        e.preset(usize::MAX, Preset::Bass);
        e.reset(SYNTHS);
        assert!(e.monos[SYNTHS + 4].gated());
        assert_eq!(e.param_value(SYNTHS, Param::Cutoff), 0.0);
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 1);
    }

    #[test]
    fn a_channel_plays_on_its_routed_synth() {
        let render = |synth: Option<usize>| {
            let mut e = Engine::new(48_000.0);
            silence(&mut e, 1);
            load(&mut e, &one_note(0)).expect("loads");
            e.route(0, synth);
            e.play();
            heard(&mut e, 300)
        };
        assert!(render(Some(0)) > 0.05);
        assert!(render(Some(1)) < 1.0e-4, "synth 1 is silenced");
        assert_eq!(render(None), 0.0, "muted");
        assert_eq!(render(Some(SYNTHS)), 0.0, "an unknown synth mutes");
    }

    /// Changing a synth's patch reaches the channel voice playing on it.
    #[test]
    fn a_channel_voice_follows_its_synths_parameters() {
        let mut e = Engine::new(48_000.0);
        load(&mut e, &one_note(0)).expect("loads");
        e.route(0, Some(4));
        e.play();
        // The note starts at 0.5 s (187.5 blocks).
        assert!(heard(&mut e, 200) > 0.05);
        silence(&mut e, 4);
        heard(&mut e, 2);
        assert!(
            heard(&mut e, 10) < 1.0e-4,
            "silencing synth 4 silences the channel"
        );
    }

    #[test]
    fn sixteen_differently_patched_synths_play_together() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        for synth in 0..SYNTHS {
            let (preset, _) = Preset::ALL[synth % Preset::ALL.len()];
            e.preset(synth, preset);
            e.note_on(synth, 36 + 3 * synth as u8, 1.0);
        }
        for _ in 0..200 {
            e.render(BLOCK);
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
        assert_eq!(e.active_voices(), SYNTHS);
    }

    /// Left and right peaks over `blocks` blocks of one held note.
    fn side_peaks(e: &mut Engine, blocks: usize) -> (f32, f32) {
        (0..blocks).fold((0.0_f32, 0.0_f32), |(l, r), _| {
            e.render(BLOCK);
            let out = e.output();
            let side = |s: &[f32]| s.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
            (l.max(side(&out[..BLOCK])), r.max(side(&out[BLOCK..])))
        })
    }

    #[test]
    fn pan_is_equal_power() {
        // A fresh engine per pan, so each hears the same note.
        let at = |pan: f32| {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::Pan, pan);
            e.note_on(0, 57, 1.0);
            side_peaks(&mut e, 40)
        };
        let (cl, cr) = at(0.0);
        assert!(cl > 0.01 && (cl - cr).abs() < 1.0e-6, "centre {cl} {cr}");
        let (l, r) = at(-1.0);
        assert!(l > 0.01 && r == 0.0, "left only: {l} {r}");
        assert!((cl / l - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.02);
        let (l, r) = at(1.0);
        assert!(l == 0.0 && r > 0.01, "right only: {l} {r}");
    }

    #[test]
    fn fader_mute_and_solo() {
        let mut e = Engine::new(48_000.0);
        e.note_on(0, 57, 1.0);
        e.note_on(1, 64, 1.0);
        assert!(heard(&mut e, 40) > 0.05);
        e.set_param(0, Param::Level, 0.0);
        e.set_param(1, Param::Mute, 1.0);
        assert_eq!(heard(&mut e, 10), 0.0, "fader 0 and a mute are silent");
        e.set_param(1, Param::Mute, 0.0);
        assert!(heard(&mut e, 10) > 0.05, "synth 1 unmuted");
        e.set_param(0, Param::Level, 1.0);
        e.set_param(0, Param::Solo, 1.0);
        e.set_param(1, Param::Level, 0.0);
        assert!(
            heard(&mut e, 10) > 0.05,
            "solo silences the others, not itself"
        );
        e.set_param(0, Param::Solo, 0.0);
        e.set_param(1, Param::Level, 1.0);
        e.set_param(1, Param::Solo, 1.0);
        e.set_param(0, Param::Level, 0.0);
        assert!(heard(&mut e, 10) > 0.05, "only the soloed synth sounds");
    }

    #[test]
    fn sends_follow_the_fader_and_leave_the_mix_alone() {
        let mut e = Engine::new(48_000.0);
        e.note_on(0, 57, 1.0);
        e.render(BLOCK);
        let dry: Vec<f32> = e.output().to_vec();
        assert!(
            e.mixer.sends[0]
                .iter()
                .chain(&e.mixer.sends[1])
                .all(|x| *x == 0.0)
        );
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Send1, 0.5);
        e.set_param(0, Param::Send2, 1.0);
        e.set_param(0, Param::Level, 0.5);
        e.note_on(0, 57, 1.0);
        e.render(BLOCK);
        let peak = |b: &[f32]| b.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        let (echo, reverb) = (peak(&e.mixer.sends[0]), peak(&e.mixer.sends[1]));
        assert!(
            echo > 0.0 && (reverb / echo - 2.0).abs() < 1.0e-4,
            "{echo} {reverb}"
        );
        // Sends are taps: the dry mix only changes with the fader.
        assert!(peak(e.output()) < peak(&dry));
    }

    #[test]
    fn each_send_feeds_its_own_processor_bus() {
        for (i, send) in [Param::Send1, Param::Send2, Param::Send3, Param::Send4]
            .into_iter()
            .enumerate()
        {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, send, 1.0);
            e.note_on(0, 57, 1.0);
            e.render(BLOCK);
            for (n, bus) in e.mixer.sends.iter().enumerate() {
                let peak = bus.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
                assert_eq!(peak > 0.0, n == i, "send {} on bus {n}", i + 1);
            }
        }
    }

    #[test]
    fn strip_parameters_never_reach_the_synth() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Level, 0.25);
        assert_eq!(e.param_value(0, Param::Level), 0.25);
        assert!(Param::ALL.iter().filter(|(p, _)| p.is_strip()).count() == 8);
    }

    #[test]
    fn sixteen_full_synths_stay_bounded_in_stereo() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        for synth in 0..SYNTHS {
            e.set_param(synth, Param::Pan, synth as f32 / 7.5 - 1.0);
            e.set_param(synth, Param::Vco2Level, 1.0);
            e.set_param(synth, Param::Vco3Level, 1.0);
            e.note_on(synth, 36 + 3 * synth as u8, 1.0);
        }
        for _ in 0..200 {
            e.render(BLOCK);
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
    }

    #[test]
    fn drive_shapes_the_synth_bus_only() {
        let play = |mode: f32| {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::DriveMode, mode);
            e.set_param(0, Param::DriveAmount, 1.0);
            e.set_param(1, Param::Level, 0.0);
            e.note_on(0, 57, 1.0);
            e.note_on(1, 57, 1.0);
            e.render(BLOCK);
            e.output().to_vec()
        };
        assert!(play(0.0) != play(2.0), "drive changes the sound");
        assert_eq!(play(0.0), play(0.0));
    }

    /// The first block of a held note on synth 0, after `setup`.
    fn first_block(setup: impl FnOnce(&mut Engine)) -> Vec<f32> {
        let mut e = Engine::new(48_000.0);
        setup(&mut e);
        e.note_on(0, 57, 1.0);
        let mut all = Vec::new();
        for _ in 0..80 {
            e.render(BLOCK);
            all.extend_from_slice(e.output());
        }
        all
    }

    #[test]
    fn the_effects_only_sound_through_a_send_and_a_return() {
        let dry = first_block(|_| {});
        // Sends up, returns at 0: nothing changes.
        let muted = first_block(|e| {
            e.set_param(0, Param::Send1, 1.0);
            e.set_param(0, Param::Send2, 1.0);
        });
        assert!(dry == muted, "a send alone is silent");
        // A return up, sends at 0: nothing changes either.
        let no_send = first_block(|e| {
            e.set_param(0, Param::P1Return, 1.0);
            e.set_param(0, Param::P2Return, 1.0);
        });
        assert!(dry == no_send, "a return alone is silent");
        let wet = first_block(|e| {
            e.set_param(0, Param::Send1, 1.0);
            e.set_param(0, Param::P1Return, 1.0);
            e.set_param(0, Param::P1A, 0.39);
            e.set_param(0, Param::Send2, 1.0);
            e.set_param(0, Param::P2Return, 1.0);
        });
        assert!(dry != wet, "both together sound");
    }

    #[test]
    fn effect_parameters_are_global() {
        let mut e = Engine::new(48_000.0);
        e.set_param(3, Param::P1Return, 0.4);
        e.set_param(5, Param::P2A, 0.9);
        for synth in [0, 3, 15] {
            assert_eq!(e.param_value(synth, Param::P1Return), 0.4);
            assert_eq!(e.param_value(synth, Param::P2A), 0.9);
        }
        e.reset(2);
        assert_eq!(e.param_value(0, Param::P1Return), 0.4, "reset leaves them");
    }

    #[test]
    fn sixteen_synths_through_both_effects_stay_bounded() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.set_param(0, Param::P1Return, 1.0);
        e.set_param(0, Param::P1B, 1.0);
        e.set_param(0, Param::P1D, 1.0);
        e.set_param(0, Param::P2Return, 1.0);
        e.set_param(0, Param::P2A, 1.0);
        for synth in 0..SYNTHS {
            e.set_param(synth, Param::Send1, 1.0);
            e.set_param(synth, Param::Send2, 1.0);
            e.set_param(synth, Param::DriveMode, 3.0);
            e.set_param(synth, Param::DriveAmount, 1.0);
            e.note_on(synth, 36 + 3 * synth as u8, 1.0);
        }
        for _ in 0..600 {
            e.render(BLOCK);
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
    }

    /// Spec 005 Req 9: 16 synths cycling through the six models, each on
    /// that model's first preset, play together within ±1.
    #[test]
    fn sixteen_synths_of_every_model_play_together() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        for synth in 0..SYNTHS {
            let (model, _) = Model::ALL[synth % Model::ALL.len()];
            let (preset, _) = Preset::ALL
                .iter()
                .find(|(p, _)| p.model() == model)
                .copied()
                .expect("every model has a preset");
            e.preset(synth, preset);
            assert_eq!(e.param_value(synth, Param::Model), model as u32 as f32);
            e.note_on(synth, 36 + 3 * synth as u8, 1.0);
        }
        for _ in 0..400 {
            e.render(BLOCK);
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
        assert_eq!(e.active_voices(), SYNTHS);
    }

    /// Peak of a held loud note on synth 0 after `setup`.
    fn loud_peak(setup: impl FnOnce(&mut Engine)) -> f32 {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.set_param(0, Param::Vco2Level, 1.0);
        e.set_param(0, Param::AdsrSustain, 1.0);
        setup(&mut e);
        e.note_on(0, 57, 1.0);
        let mut peak = 0.0_f32;
        for i in 0..400 {
            e.render(BLOCK);
            if i > 300 {
                peak = peak.max(side_peaks(&mut e, 1).0);
            }
        }
        peak
    }

    #[test]
    fn the_master_compressor_turns_a_loud_mix_down() {
        let plain = loud_peak(|_| {});
        let squeezed = loud_peak(|e| {
            e.set_param(0, Param::CompThreshold, -30.0);
            e.set_param(0, Param::CompRatio, 8.0);
        });
        assert!(squeezed < 0.7 * plain, "{squeezed} vs {plain}");
        let mut e = Engine::new(48_000.0);
        assert_eq!(e.gain_reduction_db(), 0.0);
        e.set_param(0, Param::CompThreshold, -40.0);
        e.set_param(0, Param::CompRatio, 20.0);
        e.note_on(0, 57, 1.0);
        for _ in 0..100 {
            e.render(BLOCK);
        }
        assert!(e.gain_reduction_db() > 3.0);
    }

    #[test]
    fn the_master_eq_shapes_the_mix_and_flat_leaves_it() {
        let flat = first_block(|_| {});
        let same = first_block(|e| e.set_param(0, Param::EqMid1Freq, 800.0));
        assert!(flat == same, "a band at 0 dB changes nothing");
        let boosted = first_block(|e| {
            e.set_param(0, Param::EqMid1Freq, 220.0);
            e.set_param(0, Param::EqMid1Gain, 12.0);
        });
        assert!(flat != boosted);
        let mut e = Engine::new(48_000.0);
        e.set_param(3, Param::EqHighGain, -6.0);
        assert_eq!(e.param_value(0, Param::EqHighGain), -6.0);
    }

    #[test]
    fn compressor_parameters_are_global() {
        let mut e = Engine::new(48_000.0);
        e.set_param(4, Param::CompRatio, 6.0);
        assert_eq!(e.param_value(0, Param::CompRatio), 6.0);
        e.reset(1);
        assert_eq!(e.param_value(0, Param::CompRatio), 6.0);
    }

    #[test]
    fn short_blocks_leave_the_tail_silent() {
        let mut e = Engine::new(48_000.0);
        e.note_on(0, 69, 1.0);
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
        assert!((0..16).all(|ch| e.routed(ch).is_some()));
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
        e.note_on(0, 72, 1.0);
        e.play();
        for _ in 0..200 {
            e.render(BLOCK);
        }
        e.stop();
        assert!(e.monos[0].gated(), "the live Mono voice is still held");
        assert!(
            !e.monos[SYNTHS].gated(),
            "the player's Mono voice is released"
        );
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
        let synths: Vec<Option<usize>> = (0..4).map(|ch| e.routed(ch)).collect();
        assert_eq!(
            synths,
            vec![Some(0), Some(1), Some(2), Some(3)],
            "one synth per part"
        );
    }
}
