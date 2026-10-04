//! The engine: up to `SYNTHS` synths, each with its own parameters and a voice
//! pool (`poly`: one Mono voice per owner, or a voice per note), the mixer, the
//! MIDI player, planar stereo blocks.
//!
//! Real-time rules (ADR-0002): `render` never allocates, never panics and
//! never calls `sin`/`exp`/`pow` per sample. The voices, tables and output are
//! allocated in `Engine::new`. Loading a MIDI file allocates, once, between
//! blocks (`load_midi`), never inside `render`.

use crate::clock::Clock;
use crate::fm::sysex;
use crate::fx::compressor::Compressor;
use crate::fx::ensemble::Ensemble;
use crate::fx::eq::{EqBand, Equalizer};
use crate::fx::limiter::Limiter;
use crate::fx::processor::Processor;
use crate::mixer::{Mixer, SENDS, STRIP_DEFAULTS, STRIPS};
use crate::mono::MonoParams;
use crate::mono::ladder::LadderTables;
use crate::mono::osc::Blep;
use crate::mono::preset::{DEFAULTS, Preset};
use crate::mono::voice::{MonoVoice, PitchTable, Tools};
use crate::params::{GLOBAL_DEFAULTS, Param};
use crate::player::Sequence;
use crate::poly::{Pool, VOICE_BUDGET};
use crate::smf;
use crate::table::Tables;
use crate::voice::{Owner, sine_table};

/// Frames per render call; the Web Audio render quantum.
pub const BLOCK: usize = 128;
/// MIDI channels the player routes.
pub const CHANNELS: usize = 16;
/// Mono synths, each with its own parameters (plan.md MVP 5).
pub const SYNTHS: usize = 16;
/// Peak meters: one per strip (the synths, then the groups), then master left
/// and right, then one per processor return.
pub const METERS: usize = STRIPS + 2 + SENDS;
/// Largest MIDI file accepted: 16 MiB.
pub const MAX_MIDI: usize = 16 << 20;
/// The largest SysEx file taken: a bank is 4 104 bytes, so this leaves room for many.
pub const MAX_SYSEX: usize = 1 << 20;

/// The whole synth, one per wasm instance (one per AudioWorklet node).
pub struct Engine {
    sample_rate: f32,
    sine: Vec<f32>,
    blep: Blep,
    synths: [MonoParams; SYNTHS],
    /// The last value set per synth and parameter id, clamped, for the view.
    values: [[f32; Param::ALL.len()]; STRIPS],
    ladder: LadderTables,
    pitch: PitchTable,
    /// The wavetables and attack samples, generated once at start.
    tables: &'static Tables,
    /// Each synth's voices (spec 006).
    pools: Vec<Pool>,
    /// Counts the notes started, so a pool can tell which voice is oldest.
    note_count: u64,
    mixer: Mixer,
    /// Each synth's stereo chorus, run after its voices when it is on.
    chorus: Vec<Ensemble>,
    /// The effect processors P1–P4, fed by the mixer's sends.
    procs: [Processor; SENDS],
    /// Processor n+1 takes processor n's output (P2In…P4In).
    series: [bool; SENDS],
    eq: Equalizer,
    comp: Compressor,
    limiter: Limiter,
    master_gain: f32,
    /// Planar output: `BLOCK` left samples, then `BLOCK` right samples.
    out: Box<[f32; 2 * BLOCK]>,
    /// The highest level of each meter since `clear_meters`.
    meters: [f32; METERS],
    /// The MIDI file's bytes, written by JavaScript before `load_midi`.
    midi: Vec<u8>,
    /// A DX7 SysEx file's bytes, written by JavaScript before `load_sysex`, and the
    /// voices parsed from it.
    sysex: Vec<u8>,
    sysex_voices: Vec<sysex::Voice>,
    sequence: Sequence,
    /// The transport's tempo and sixteenth steps (spec 002 Req 5).
    clock: Clock,
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
            values: [[0.0; Param::ALL.len()]; STRIPS],
            ladder: LadderTables::new(sample_rate),
            pitch: PitchTable::new(sample_rate),
            tables: Tables::shared(sample_rate),
            pools: (0..SYNTHS).map(Pool::new).collect(),
            note_count: 0,
            mixer: Mixer::new(sample_rate),
            chorus: (0..SYNTHS).map(|_| Ensemble::new(sample_rate)).collect(),
            procs: std::array::from_fn(|_| Processor::new(sample_rate)),
            series: [false; SENDS],
            eq: Equalizer::new(sample_rate),
            comp: Compressor::new(sample_rate),
            limiter: Limiter::new(sample_rate),
            master_gain: 0.5,
            out: Box::new([0.0; 2 * BLOCK]),
            meters: [0.0; METERS],
            midi: Vec::new(),
            sysex: Vec::new(),
            sysex_voices: Vec::new(),
            sequence: Sequence::default(),
            clock: Clock::new(sample_rate),
            route: [Some(0); CHANNELS],
        };
        for (p, v) in GLOBAL_DEFAULTS {
            engine.set_param(0, p, v);
        }
        for strip in 0..STRIPS {
            engine.reset(strip);
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
        if param.is_strip() {
            // The mixer owns it, for a synth's strip or a group's; a route that
            // isn't allowed is refused and nothing changes.
            if !self.mixer.set(synth, param, v) {
                return;
            }
            if let Some(slot) = self
                .values
                .get_mut(synth)
                .and_then(|r| r.get_mut(param as usize))
            {
                *slot = v;
            }
            return;
        }
        // A synth parameter: groups have none.
        let (Some(values), Some(mono)) = (self.values.get_mut(synth), self.synths.get_mut(synth))
        else {
            return;
        };
        if let Some(slot) = values.get_mut(param as usize) {
            *slot = v;
        }
        mono.set(param, v);
        if param == Param::ChorusMode {
            if let Some(c) = self.chorus.get_mut(synth) {
                c.set_mode(mono.chorus_mode);
            }
        }
    }

    /// Processor `i` takes the one before it as input (`v` ≥ 0.5), or not.
    fn chain(&mut self, i: usize, v: f32) {
        let on = v >= 0.5;
        if let Some(s) = self.series.get_mut(i) {
            *s = on;
        }
        if let Some(prev) = i.checked_sub(1).and_then(|k| self.procs.get_mut(k)) {
            prev.set_feeds_next(on);
        }
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
            Param::P2In => self.chain(1, v),
            Param::P3In => self.chain(2, v),
            Param::P4In => self.chain(3, v),
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
        for pool in self.pools.iter_mut() {
            pool.release_all();
        }
    }

    /// Voices still sounding (gated or releasing), across every synth.
    pub fn active_voices(&self) -> usize {
        self.pools.iter().map(Pool::active).sum()
    }

    /// The Mono voice playing for `owner`, wherever it is (for tests and the view's debug).
    pub fn voice(&self, owner: Owner) -> Option<&MonoVoice> {
        self.pools.iter().find_map(|p| p.voice(owner))
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
        let Some(synth) = self.target(owner) else {
            return;
        };
        // An owner plays one synth at a time: leaving one starts clean on the next.
        for (i, pool) in self.pools.iter_mut().enumerate() {
            if i != synth {
                pool.release_owner(owner);
            }
        }
        self.note_count += 1;
        // At the voice budget a note takes the oldest voice in release anywhere,
        // else the oldest held note of its own synth; with nothing to take, it is dropped.
        let adds = match (self.pools.get(synth), self.synths.get(synth)) {
            (Some(pool), Some(params)) => pool.adds_a_voice(owner, note, params),
            _ => return,
        };
        if adds && self.active_voices() >= VOICE_BUDGET && !self.take_a_voice(synth) {
            return;
        }
        if let (Some(pool), Some(params)) = (self.pools.get_mut(synth), self.synths.get(synth)) {
            pool.note_on(owner, note.min(127), velocity, params, self.note_count);
        }
    }

    /// Free one voice for a note on `synth`; false when there is none to free.
    fn take_a_voice(&mut self, synth: usize) -> bool {
        let oldest = self
            .pools
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.oldest_release().map(|(slot, age)| (i, slot, age)))
            .min_by_key(|(_, _, age)| *age);
        if let Some((pool, slot, _)) = oldest {
            if let Some(p) = self.pools.get_mut(pool) {
                p.silence(slot);
            }
            return true;
        }
        self.pools
            .get_mut(synth)
            .is_some_and(Pool::silence_oldest_held)
    }

    /// Release `note` from `owner`.
    fn stop_note(&mut self, owner: Owner, note: u8) {
        for (pool, params) in self.pools.iter_mut().zip(self.synths.iter()) {
            pool.note_off(owner, note, params);
        }
    }

    fn release_player(&mut self) {
        for pool in self.pools.iter_mut() {
            pool.release_channels();
        }
    }

    // --- DX7 SysEx (spec 006 Req 14) -------------------------------------

    /// Size the SysEx buffer for `len` bytes and return it for writing.
    /// `None` if the file is larger than `MAX_SYSEX`.
    pub fn sysex_buffer(&mut self, len: usize) -> Option<&mut [u8]> {
        if len > MAX_SYSEX {
            return None;
        }
        self.sysex.clear();
        self.sysex.resize(len, 0);
        Some(&mut self.sysex)
    }

    /// Parse the buffer into voices, replacing the previous ones. Returns how many.
    pub fn load_sysex(&mut self) -> Result<usize, sysex::Error> {
        self.sysex_voices = sysex::parse(&self.sysex)?;
        Ok(self.sysex_voices.len())
    }

    #[cfg(test)]
    fn load_sysex_of(&mut self, bytes: &[u8]) -> Result<usize, sysex::Error> {
        if let Some(b) = self.sysex_buffer(bytes.len()) {
            b.copy_from_slice(bytes);
        }
        self.load_sysex()
    }

    /// The name of voice `i` from the last `load_sysex`.
    pub fn sysex_name(&self, i: usize) -> &str {
        self.sysex_voices.get(i).map_or("", |v| v.name.as_str())
    }

    /// Set every DX7 parameter of `synth` from voice `i`. The parameters go through
    /// `set_param`, so the view reads the same values back. False if there is no such voice.
    pub fn apply_sysex(&mut self, synth: usize, i: usize) -> bool {
        let Some(voice) = self.sysex_voices.get(i) else {
            return false;
        };
        let patch = voice.patch;
        for op in 0..6 {
            // `ops[0]` is operator 6, the last block of parameters.
            let base = Param::Op1R1 as u32 + (5 - op as u32) * 21;
            for k in 0..21 {
                if let Some(p) = Param::from_id(base + k as u32) {
                    self.set_param(synth, p, f32::from(patch.op_field(op, k)));
                }
            }
        }
        for k in 0..19 {
            if let Some(p) = Param::from_id(Param::PitchR1 as u32 + k as u32) {
                self.set_param(synth, p, f32::from(patch.global_field(k)));
            }
        }
        true
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

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// The clock's tempo in BPM; the song sets it (ADR-0012).
    pub fn set_tempo(&mut self, bpm: f32) {
        self.clock.set_tempo(bpm);
    }

    /// The clock's swing in percent, 50 (straight) to 75.
    pub fn set_swing(&mut self, pct: f32) {
        self.clock.set_swing(pct);
    }

    /// Start the transport: the clock, and the MIDI file if one is loaded.
    pub fn play(&mut self) {
        self.sequence.play();
        self.clock.play();
    }

    pub fn stop(&mut self) {
        self.sequence.stop();
        self.clock.stop();
        self.release_player();
    }

    pub fn seek(&mut self, sample: u64) {
        self.sequence.seek(sample);
        self.clock.seek(sample);
        self.release_player();
    }

    /// Play `channel` on `synth`, or mute it with `None`. An unknown synth
    /// mutes too.
    pub fn route(&mut self, channel: u8, synth: Option<usize>) {
        if let Some(slot) = self.route.get_mut(usize::from(channel)) {
            *slot = synth.filter(|s| *s < SYNTHS);
            for pool in self.pools.iter_mut() {
                pool.release_owner(Owner::Channel(channel));
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
        // Nothing plays on the steps yet: the song (#100) and the
        // arpeggiator (#110) will.
        while self.clock.due().is_some() {}
    }

    // --- Render ----------------------------------------------------------

    /// Render `frames` (at most `BLOCK`) into the output buffer. The block is
    /// split at player events and clock steps, so each lands on its exact
    /// sample.
    pub fn render(&mut self, frames: usize) {
        let n = frames.min(BLOCK);
        self.out.fill(0.0);
        self.mixer.clear(n);
        let mut t = 0;
        while t < n {
            self.fire_due_events();
            let chunk = self
                .sequence
                .frames_until_next(n - t)
                .min(self.clock.frames_until_next(n - t));
            for (synth, (pool, params)) in self.pools.iter_mut().zip(self.synths.iter()).enumerate()
            {
                if pool.active() == 0 {
                    continue;
                }
                if let Some(buf) = self.mixer.bus(synth, t..t + chunk) {
                    let tools = Tools {
                        sine: &self.sine,
                        blep: &self.blep,
                        ladder: &self.ladder,
                        pitch: &self.pitch,
                        tables: self.tables,
                    };
                    pool.render(params, tools, buf);
                }
            }
            self.sequence.advance(chunk);
            self.clock.advance(chunk);
            t += chunk;
        }
        // A synth with its chorus on is stereo from here on.
        for (synth, chorus) in self.chorus.iter_mut().enumerate() {
            if chorus.on() {
                self.mixer
                    .widen(synth, n, |dry, l, r| chorus.process(dry, l, r));
            }
        }
        let (left, right) = self.out.split_at_mut(BLOCK);
        self.mixer.mix(n, left, right);
        if let (Some(l), Some(r)) = (left.get_mut(..n), right.get_mut(..n)) {
            // P1 to P4 in order: a chained one hears the one before it.
            for (i, send) in self.mixer.sends.iter().enumerate() {
                let Some(send) = send.get(..n) else {
                    continue;
                };
                let (done, rest) = self.procs.split_at_mut(i);
                let Some(proc) = rest.first_mut() else {
                    continue;
                };
                let series = if self.series.get(i).copied().unwrap_or(false) {
                    i.checked_sub(1)
                        .and_then(|k| done.get(k))
                        .and_then(|p| p.wet(n))
                } else {
                    None
                };
                proc.process(send, series, l, r);
            }
            // The master chain: equalizer, compressor, gain, limiter.
            self.eq.process(l, r);
            self.comp.process(l, r);
            for s in l.iter_mut().chain(r.iter_mut()) {
                *s *= self.master_gain;
            }
            self.limiter.process(l, r);
            let peak = |x: &[f32]| x.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
            let (pl, pr) = (peak(l), peak(r));
            self.record(STRIPS, pl);
            self.record(STRIPS + 1, pr);
        }
        for i in 0..STRIPS {
            let level = self.mixer.peaks.get(i).copied().unwrap_or(0.0);
            self.record(i, level);
        }
        for i in 0..SENDS {
            let level = self.procs.get_mut(i).map_or(0.0, |p| p.take_peak());
            self.record(STRIPS + 2 + i, level);
        }
    }

    fn record(&mut self, meter: usize, level: f32) {
        if let Some(m) = self.meters.get_mut(meter) {
            *m = m.max(level);
        }
    }

    /// The meters since the last `clear_meters`: each synth strip after its
    /// fader, master left and right after the limiter, and each processor's
    /// return. For the view; levels are linear, 1 is full scale.
    pub fn meters(&self) -> &[f32; METERS] {
        &self.meters
    }

    /// Start the meters over, after the view has read them.
    pub fn clear_meters(&mut self) {
        self.meters.fill(0.0);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mono::model::Model;
    use crate::mono::osc::LATENCY;
    use crate::poly::VOICE_BUDGET;
    use crate::smf::tests::file;

    /// Whether `owner`'s voice has a key held.
    fn gated(e: &Engine, owner: Owner) -> bool {
        e.voice(owner).is_some_and(MonoVoice::gated)
    }

    fn peak(e: &Engine) -> f32 {
        e.output().iter().fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    /// Spec 006 Req 14: a SysEx voice reaches the DX7 parameters, operator 6 first.
    #[test]
    fn a_sysex_voice_sets_the_dx7_parameters() {
        let mut p = crate::fm::patch::FmPatch::default();
        p.set_op(0, 16, 77.0); // operator 6's output level
        p.set_op(5, 16, 55.0); // operator 1's
        p.set_global(8, 12.0); // algorithm
        let data = sysex::to_voice_bytes(&p, "TESTVOICE");
        let mut file = vec![0xf0, 0x43, 0x00, 0x00, 0x01, 0x1b];
        file.extend_from_slice(&data);
        file.extend_from_slice(&[sysex::checksum(&data), 0xf7]);
        let mut e = Engine::new(48_000.0);
        e.sysex_buffer(file.len())
            .expect("fits")
            .copy_from_slice(&file);
        assert_eq!(e.load_sysex(), Ok(1));
        assert_eq!(e.sysex_name(0), "TESTVOICE");
        assert!(e.apply_sysex(2, 0));
        assert!(!e.apply_sysex(2, 1));
        let get = |e: &Engine, id: u32| e.param_value(2, Param::from_id(id).expect("id"));
        assert_eq!(get(&e, Param::Op6R1 as u32 + 16), 77.0);
        assert_eq!(get(&e, Param::Op1R1 as u32 + 16), 55.0);
        assert_eq!(get(&e, Param::Algorithm as u32), 12.0);
        assert_eq!(e.load_sysex_of(b"junk"), Err(sysex::Error::Unsupported));
        assert!(e.sysex_buffer(MAX_SYSEX + 1).is_none());
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
        assert!(!gated(&e, Owner::Channel(2)));
        assert!(gated(&e, Owner::Channel(3)) && gated(&e, Owner::Live(0)));
        // Live Mono is monophonic: a second key moves the same voice.
        e.note_on(0, 69, 1.0);
        e.render(BLOCK);
        assert_eq!(e.voice(Owner::Live(0)).map(MonoVoice::note), Some(69));
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
        assert_eq!(
            e.param_value(1, Param::Model),
            (Model::ALL.len() - 1) as f32,
            "clamped into range"
        );
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
        assert!(gated(&e, Owner::Channel(4)));
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

    /// Spec 006 Req 1: a polyphonic synth plays a chord from live keys and a MIDI
    /// channel at once, each releasing only its own notes.
    #[test]
    fn a_poly_synth_plays_chords_from_live_keys_and_a_channel() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Polyphony, 8.0);
        // A chord of three on channel 0 at 0.5 s, held for 0.5 s.
        let chord = vec![
            0x83, 0x60, 0x90, 60, 100, 0x00, 0x90, 64, 100, 0x00, 0x90, 67, 100, 0x83, 0x60, 0x80,
            60, 0, 0x00, 0x80, 64, 0, 0x00, 0x80, 67, 0, 0x00, 0xFF, 0x2F, 0,
        ];
        load(&mut e, &file(0, 480, &[chord])).expect("loads");
        e.route(0, Some(0));
        e.note_on(0, 72, 1.0);
        e.play();
        let mut most = 0;
        for _ in 0..260 {
            e.render(BLOCK);
            most = most.max(e.active_voices());
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
        assert_eq!(most, 4, "three chord notes and the live key");
        for _ in 0..400 {
            e.render(BLOCK);
        }
        assert!(
            gated_notes(&e, 0) == 1,
            "the live key is still held after the chord ends"
        );
    }

    /// Spec 006 Req 6: the Prophet-5 has five voices; a sixth note steals the oldest.
    #[test]
    fn the_prophet_5_has_five_voices_and_the_sixth_steals_one() {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::P5Brass);
        // The model's own count limits a pool set for more.
        e.set_param(0, Param::Polyphony, 16.0);
        for n in [60, 64, 67, 71, 74] {
            e.note_on(0, n, 1.0);
        }
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 5);
        e.note_on(0, 77, 1.0);
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 5, "still five");
        assert_eq!(
            e.pools[0].held_notes(),
            vec![64, 67, 71, 74, 77],
            "the oldest, 60, was stolen"
        );
    }

    /// The Prophet bass is unison: one key, every voice.
    #[test]
    fn the_prophet_bass_plays_one_note_on_all_five_voices() {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::P5Bass);
        e.note_on(0, 40, 1.0);
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 5);
        assert_eq!(e.pools[0].held_notes(), vec![40; 5]);
    }

    /// Spec 006 Req 7: with the chorus off both sides of a Juno-106 are the same;
    /// with it on they differ, and the sends and meter still work on the stereo strip.
    #[test]
    fn the_juno_chorus_makes_the_two_sides_differ() {
        let sides = |mode: f32| {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::MasterGain, 1.0);
            e.preset(0, Preset::JunoPad);
            e.set_param(0, Param::ChorusMode, mode);
            e.set_param(0, Param::AdsrAttack, 0.01);
            for n in [57, 61, 64] {
                e.note_on(0, n, 1.0);
            }
            let (mut l, mut r) = (Vec::new(), Vec::new());
            for _ in 0..150 {
                e.render(BLOCK);
                l.extend_from_slice(&e.output()[..BLOCK]);
                r.extend_from_slice(&e.output()[BLOCK..]);
                assert!(e.output().iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            }
            (l, r)
        };
        let (l, r) = sides(0.0);
        assert!(
            l.iter().any(|x| x.abs() > 0.01) && l == r,
            "mono without the chorus"
        );
        for mode in [1.0, 2.0, 3.0] {
            let (l, r) = sides(mode);
            let diff: f32 = l
                .iter()
                .zip(&r)
                .skip(4_800)
                .map(|(a, b)| (a - b).abs())
                .sum();
            assert!(diff > 1.0, "mode {mode}: {diff}");
        }
    }

    /// The Juno-106 has six voices.
    #[test]
    fn the_juno_106_has_six_voices() {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::JunoPoly);
        for n in [48, 52, 55, 59, 62, 65, 69] {
            e.note_on(0, n, 1.0);
        }
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 6);
        assert_eq!(e.pools[0].held_notes(), vec![52, 55, 59, 62, 65, 69]);
    }

    /// The Jupiter-8 has eight voices.
    #[test]
    fn the_jupiter_8_has_eight_voices() {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::JupiterBrass);
        for n in [48, 52, 55, 59, 62, 65, 69, 72, 76] {
            e.note_on(0, n, 1.0);
        }
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 8);
        assert_eq!(
            e.pools[0].held_notes(),
            vec![52, 55, 59, 62, 65, 69, 72, 76]
        );
    }

    /// The Matrix-12 has twelve voices.
    #[test]
    fn the_matrix_12_has_twelve_voices() {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::MatrixPad);
        let chord: Vec<u8> = (0..13).map(|k| 40 + 3 * k).collect();
        for n in &chord {
            e.note_on(0, *n, 1.0);
        }
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 12);
        assert_eq!(e.pools[0].held_notes(), chord[1..].to_vec());
    }

    /// The PPG Wave has eight voices.
    #[test]
    fn the_ppg_wave_has_eight_voices() {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::PpgSweepPad);
        for n in [48, 52, 55, 59, 62, 65, 69, 72, 76] {
            e.note_on(0, n, 1.0);
        }
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 8);
        assert_eq!(
            e.pools[0].held_notes(),
            vec![52, 55, 59, 62, 65, 69, 72, 76]
        );
    }

    /// A D-50 voice set up with the given parameters, playing note `note`; its left
    /// channel for `blocks` blocks.
    fn d50_note(settings: &[(Param, f32)], note: u8, blocks: usize) -> Vec<f32> {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        for (p, v) in [
            (Param::Model, 12.0),
            (Param::Polyphony, 16.0),
            (Param::Cutoff, 20_000.0),
            (Param::P2Cutoff, 20_000.0),
        ]
        .iter()
        .chain(settings)
        {
            e.set_param(0, *p, *v);
        }
        e.note_on(0, note, 1.0);
        let mut out = Vec::new();
        for _ in 0..blocks {
            e.render(BLOCK);
            out.extend_from_slice(&e.output()[..BLOCK]);
        }
        out
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
    }

    /// Spec 006 Req 12: a PCM attack sounds in the first tens of milliseconds and the
    /// synthesised body carries on after it.
    #[test]
    fn the_d50_attack_sounds_first_and_the_body_carries_on() {
        let with_attack = d50_note(
            &[
                (Param::Pcm1Sample, 7.0),
                (Param::Vco1Level, 1.0),
                (Param::AdsrAttack, 0.001),
                (Param::AdsrDecay, 0.1),
                (Param::AdsrSustain, 0.0),
                (Param::Vco2Level, 0.15),
                (Param::P2AdsrAttack, 0.001),
                (Param::P2AdsrSustain, 1.0),
            ],
            48,
            300,
        );
        let (early, late) = (rms(&with_attack[..2_000]), rms(&with_attack[24_000..]));
        assert!(
            early > 2.5 * late,
            "the attack stands out over the body: {early} vs {late}"
        );
        assert!(late > 0.005, "and the body carries on: {late}");
        // Without the attack the body is steady from the first moments.
        let body = d50_note(
            &[
                (Param::Vco1Level, 0.0),
                (Param::Vco2Level, 0.15),
                (Param::P2AdsrAttack, 0.001),
                (Param::P2AdsrSustain, 1.0),
            ],
            48,
            300,
        );
        let ratio = rms(&body[2_000..6_000]) / rms(&body[24_000..]);
        assert!((0.7..1.4).contains(&ratio), "steady: {ratio}");
    }

    /// The pair adds, rings or syncs: ring needs both partials, sync makes partial 2
    /// repeat with partial 1.
    #[test]
    fn the_d50_partials_add_ring_and_sync() {
        let both = [
            (Param::Vco1Level, 0.8),
            (Param::Vco2Level, 0.8),
            (Param::Vco2Coarse, 7.0),
        ];
        let mut add = both.to_vec();
        add.push((Param::Structure, 0.0));
        let mut ring = both.to_vec();
        ring.push((Param::Structure, 2.0));
        let (a, r) = (d50_note(&add, 57, 60), d50_note(&ring, 57, 60));
        assert!(a != r, "ring is not add");
        // Ring with partial 2 silent is silence; add is not.
        let mut quiet = vec![
            (Param::Vco1Level, 0.8),
            (Param::Vco2Level, 0.0),
            (Param::Structure, 2.0),
        ];
        assert!(
            rms(&d50_note(&quiet, 57, 60)[4_800..]) < 1.0e-6,
            "ring of a silent partial"
        );
        quiet[2].1 = 0.0;
        assert!(
            rms(&d50_note(&quiet, 57, 60)[4_800..]) > 0.01,
            "while add keeps partial 1"
        );
        // Synced, the sound repeats with partial 1's period (220 Hz is 218.18
        // samples); added, a partial a fifth up does not.
        let repeats = |structure: f32| {
            let mut settings = both.to_vec();
            settings.push((Param::Structure, structure));
            let out = d50_note(&settings, 57, 80);
            let x = &out[6_000..];
            let norm = x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
            (216..=221)
                .map(|lag| {
                    x.iter()
                        .zip(&x[lag..])
                        .map(|(a, b)| f64::from(*a) * f64::from(*b))
                        .sum::<f64>()
                        / norm
                })
                .fold(f64::MIN, f64::max)
        };
        let (synced, added) = (repeats(1.0), repeats(0.0));
        assert!(synced > 0.9, "synced repeats with partial 1: {synced}");
        assert!(added < 0.8, "added does not: {added}");
    }

    /// The D-50 has sixteen voices.
    #[test]
    fn the_d50_has_sixteen_voices() {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::LaFantasia);
        let chord: Vec<u8> = (0..17).map(|k| 36 + 3 * k).collect();
        for n in &chord {
            e.note_on(0, *n, 1.0);
        }
        e.render(BLOCK);
        assert_eq!(e.active_voices(), 16);
        assert_eq!(e.pools[0].held_notes(), chord[1..].to_vec());
    }

    /// Changing a synth from a Mono-voice model to the D-50 plays the D-50 at the next note.
    #[test]
    fn a_model_change_replaces_the_voices() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.note_on(0, 57, 1.0);
        e.render(BLOCK);
        e.note_off(0, 57);
        e.preset(0, Preset::LaThumpBass);
        e.note_on(0, 45, 1.0);
        let mut heard = 0.0_f32;
        for _ in 0..40 {
            e.render(BLOCK);
            heard = heard.max(e.output().iter().fold(0.0, |m, s| m.max(s.abs())));
        }
        assert!(heard > 0.05);
        assert!(e.pools[0].held_notes().contains(&45));
    }

    /// The DX7 has sixteen voices, and its voices are FM voices.
    #[test]
    fn the_polymoog_has_sixteen_voices() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(0, Preset::PolyStrings);
        for k in 0..17 {
            e.note_on(0, 36 + 3 * k, 1.0);
        }
        let mut heard = 0.0_f32;
        for _ in 0..200 {
            e.render(BLOCK);
            heard = heard.max(e.output().iter().fold(0.0, |m, s| m.max(s.abs())));
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
        assert!(heard > 0.05);
        assert_eq!(e.active_voices(), 16);
    }

    /// Spec 006 Req 16: the Vox Humana's resonance peak falls with the filter envelope, so
    /// the strongest harmonic of a held note is higher early than late, and stands out
    /// from its neighbours like a formant.
    #[test]
    fn the_vox_humana_resonance_peak_follows_the_filter_envelope() {
        let f0 = 440.0 * 2.0_f64.powf((48.0 - 69.0) / 12.0);
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(0, Preset::VoxHumana);
        e.set_param(0, Param::Analog, 0.0);
        e.set_param(0, Param::ChorusMode, 0.0);
        e.set_param(0, Param::Vco2Level, 0.0);
        e.note_on(0, 48, 1.0);
        let mut left = Vec::new();
        for _ in 0..(48_000 * 2 / BLOCK) {
            e.render(BLOCK);
            left.extend_from_slice(&e.output()[..BLOCK]);
        }
        // The level of each harmonic of the note in a window: (the most prominent harmonic and how far it stands out, in dB).
        let peak = |from: usize, to: usize| {
            let out = &left[from..to];
            let levels: Vec<f64> = (1..=30)
                .map(|k| {
                    let w = std::f64::consts::TAU * f0 * f64::from(k) / 48_000.0;
                    let (mut re, mut im) = (0.0, 0.0);
                    for (i, y) in out.iter().enumerate() {
                        re += f64::from(*y) * (w * i as f64).cos();
                        im += f64::from(*y) * (w * i as f64).sin();
                    }
                    re * re + im * im
                })
                .collect();
            // The harmonic that stands highest above the ones around it, and by how much (dB).
            let db: Vec<f64> = levels.iter().map(|l| 10.0 * l.max(1e-9).log10()).collect();
            let prominence = |k: usize| {
                let near = |j: usize| db.get(j).copied().unwrap_or(f64::MIN);
                db.get(k).copied().unwrap_or(f64::MIN)
                    - 0.5 * (near(k.wrapping_sub(2)).max(-99.0) + near(k + 2).max(-99.0))
            };
            let k = (2..20).fold(2, |m, k| if prominence(k) > prominence(m) { k } else { m });
            (k + 1, prominence(k))
        };
        let (early, early_ratio) = peak(4_800, 14_400);
        let (late, _) = peak(57_600, 81_600);
        assert!(early > late, "the peak is at harmonic {early}, then {late}");
        assert!(
            early_ratio > 4.0,
            "the peak stands out by only {early_ratio}"
        );
    }

    #[test]
    fn the_dx7_has_sixteen_voices() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(0, Preset::FmElectricPiano);
        let chord: Vec<u8> = (0..17).map(|k| 40 + 3 * k).collect();
        for n in &chord {
            e.note_on(0, *n, 1.0);
        }
        let mut heard = 0.0_f32;
        for _ in 0..40 {
            e.render(BLOCK);
            heard = heard.max(e.output().iter().fold(0.0, |m, s| m.max(s.abs())));
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
        assert!(heard > 0.05);
        assert_eq!(e.active_voices(), 16);
        assert_eq!(e.pools[0].held_notes(), chord[1..].to_vec());
    }

    /// Notes held on a synth: its voices with a key down.
    fn gated_notes(e: &Engine, synth: usize) -> usize {
        e.pools[synth].held()
    }

    /// Spec 006 Req 5: at most the voice budget sounds at once, across synths.
    #[test]
    fn the_voice_budget_caps_the_voices_across_synths() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        for synth in 0..SYNTHS {
            e.set_param(synth, Param::Polyphony, 8.0);
            for k in 0..8 {
                e.note_on(synth, 36 + 3 * k + synth as u8, 1.0);
            }
        }
        for _ in 0..100 {
            e.render(BLOCK);
            assert!(
                e.active_voices() <= VOICE_BUDGET,
                "{} voices",
                e.active_voices()
            );
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
        assert_eq!(e.active_voices(), VOICE_BUDGET, "and the cap is used");
    }

    /// A note past the budget takes the oldest voice in release first.
    #[test]
    fn a_note_at_the_budget_takes_a_released_voice_first() {
        let mut e = Engine::new(48_000.0);
        for synth in 0..SYNTHS {
            e.set_param(synth, Param::Polyphony, 4.0);
            e.set_param(synth, Param::AdsrRelease, 5.0);
        }
        // Synth 9 has room for more notes than it plays.
        e.set_param(9, Param::Polyphony, 8.0);
        for synth in 0..SYNTHS {
            for k in 0..4 {
                e.note_on(synth, 40 + 4 * k, 1.0);
            }
        }
        e.render(BLOCK);
        assert_eq!(e.active_voices(), VOICE_BUDGET);
        // Synth 5 lets a note go: it is in release, the oldest of the releasing voices.
        e.note_off(5, 40);
        e.render(BLOCK);
        e.note_on(9, 90, 1.0);
        e.render(BLOCK);
        assert_eq!(e.active_voices(), VOICE_BUDGET);
        assert_eq!(e.pools[5].held(), 3, "synth 5 kept its three held notes");
        assert_eq!(
            e.pools[5].active(),
            3,
            "and its released tail was the voice taken"
        );
        assert_eq!(e.pools[9].held(), 5, "while the new note sounds on synth 9");
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
        assert!(Param::ALL.iter().filter(|(p, _)| p.is_strip()).count() == 27);
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
    fn inserts_are_per_strip_and_in_series() {
        let play = |setup: fn(&mut Engine)| {
            let mut e = Engine::new(48_000.0);
            setup(&mut e);
            e.note_on(0, 57, 1.0);
            e.render(BLOCK);
            e.output().to_vec()
        };
        let dry = play(|_| {});
        // The same slot types in two orders sound different.
        let drive_eq = play(|e| {
            e.set_param(0, Param::I1Type, 2.0);
            e.set_param(0, Param::I1A, 0.9);
            e.set_param(0, Param::I2Type, 4.0);
            e.set_param(0, Param::I2C, 1.0);
        });
        let eq_drive = play(|e| {
            e.set_param(0, Param::I1Type, 4.0);
            e.set_param(0, Param::I1C, 1.0);
            e.set_param(0, Param::I2Type, 2.0);
            e.set_param(0, Param::I2A, 0.9);
        });
        assert!(drive_eq != dry && eq_drive != dry && drive_eq != eq_drive);
        // A neutral EQ slot changes nothing; a slot on synth 1 doesn't touch synth 0.
        assert!(play(|e| e.set_param(0, Param::I3Type, 4.0)) == dry);
        assert!(
            play(|e| {
                e.set_param(1, Param::I1Type, 3.0);
                e.set_param(1, Param::I1A, 1.0);
            }) == dry
        );
    }

    #[test]
    fn insert_parameters_are_the_strips_own() {
        let mut e = Engine::new(48_000.0);
        e.set_param(2, Param::I2Type, 5.0);
        e.set_param(2, Param::I2B, 0.7);
        assert_eq!(e.param_value(2, Param::I2Type), 5.0);
        assert_eq!(e.param_value(0, Param::I2Type), 0.0);
        assert_eq!(e.param_value(2, Param::I2B), 0.7);
        e.reset(2);
        assert_eq!(
            e.param_value(2, Param::I2Type),
            0.0,
            "reset puts the slots back"
        );
        assert_eq!(e.param_value(2, Param::I2E), 0.0);
        assert_eq!(e.param_value(2, Param::I2A), 0.5);
    }

    #[test]
    fn drive_shapes_the_synth_bus_only() {
        let play = |mode: f32| {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::I1Type, mode);
            e.set_param(0, Param::I1A, 1.0);
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
            e.set_param(synth, Param::I1Type, 3.0);
            e.set_param(synth, Param::I1A, 1.0);
            e.set_param(synth, Param::I2Type, 4.0);
            e.set_param(synth, Param::I2A, 0.9);
            e.set_param(synth, Param::I3Type, 5.0);
            e.set_param(synth, Param::I3A, 0.5);
            e.set_param(synth, Param::I3B, 0.6);
            e.note_on(synth, 36 + 3 * synth as u8, 1.0);
        }
        for _ in 0..600 {
            e.render(BLOCK);
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
    }

    /// Spec 005 Req 9: 16 synths cycling through the models, each on
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
        // Every held note sounds; a drum kit's hit is a one-shot and has rung out.
        let held = (0..SYNTHS)
            .filter(|s| !Model::ALL[s % Model::ALL.len()].0.uses_drums())
            .count();
        assert_eq!(e.active_voices(), held);
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
    fn meters_read_the_post_fader_peaks_and_start_over() {
        let mut e = Engine::new(48_000.0);
        assert!(e.meters().iter().all(|m| *m == 0.0));
        e.note_on(0, 57, 1.0);
        e.set_param(0, Param::Send1, 1.0);
        e.set_param(0, Param::P1Return, 1.0);
        e.set_param(0, Param::P1A, 0.2);
        for _ in 0..80 {
            e.render(BLOCK);
        }
        let m = *e.meters();
        assert!(m[0] > 0.05 && m[0] <= 1.0, "strip 0: {}", m[0]);
        assert_eq!(m[1], 0.0, "an idle strip reads zero");
        assert!(
            m[STRIPS] > 0.0 && m[STRIPS + 1] > 0.0,
            "master left and right"
        );
        assert!(m[STRIPS + 2] > 0.0, "the echo return has something");
        assert_eq!(m[STRIPS + 3], 0.0, "and the reverb does not");
        // The fader scales the strip's reading.
        e.clear_meters();
        assert!(e.meters().iter().all(|m| *m == 0.0));
        e.set_param(0, Param::Level, 0.25);
        e.render(BLOCK);
        assert!(e.meters()[0] < 0.5 * m[0]);
    }

    #[test]
    fn muted_and_unsoloed_strips_read_zero() {
        let mut e = Engine::new(48_000.0);
        e.note_on(0, 57, 1.0);
        e.note_on(1, 64, 1.0);
        e.set_param(0, Param::Mute, 1.0);
        for _ in 0..40 {
            e.render(BLOCK);
        }
        assert_eq!(e.meters()[0], 0.0);
        assert!(e.meters()[1] > 0.0);
        e.set_param(0, Param::Mute, 0.0);
        e.set_param(0, Param::Solo, 1.0);
        e.clear_meters();
        for _ in 0..40 {
            e.render(BLOCK);
        }
        assert!(e.meters()[0] > 0.0);
        assert_eq!(e.meters()[1], 0.0);
    }

    /// Render `blocks` blocks and return the output.
    fn render_out(e: &mut Engine, blocks: usize) -> Vec<f32> {
        let mut all = Vec::new();
        for _ in 0..blocks {
            e.render(BLOCK);
            all.extend_from_slice(e.output());
        }
        all
    }

    const GROUPS_N: usize = crate::mixer::GROUPS;
    const G1: usize = SYNTHS;
    const G2: usize = SYNTHS + 1;

    #[test]
    fn a_strip_routed_to_a_group_is_heard_through_the_group() {
        let play = |setup: fn(&mut Engine)| {
            let mut e = Engine::new(48_000.0);
            setup(&mut e);
            e.note_on(0, 57, 1.0);
            render_out(&mut e, 40)
        };
        let direct = play(|_| {});
        let via = play(|e| e.set_param(0, Param::Out, 1.0));
        assert!(direct == via, "a group at unity changes nothing");
        let quiet = play(|e| {
            e.set_param(0, Param::Out, 1.0);
            e.set_param(G1, Param::Level, 0.25);
        });
        assert!(
            quiet.iter().fold(0.0_f32, |m, x| m.max(x.abs()))
                < 0.4 * direct.iter().fold(0.0_f32, |m, x| m.max(x.abs()))
        );
        let muted = play(|e| {
            e.set_param(0, Param::Out, 1.0);
            e.set_param(G1, Param::Mute, 1.0);
        });
        assert!(
            muted.iter().all(|x| *x == 0.0),
            "a muted group silences what it carries"
        );
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Out, 1.0);
        e.set_param(G1, Param::Pan, -1.0);
        e.note_on(0, 57, 1.0);
        let out = render_out(&mut e, 40);
        let side = |from: usize| {
            (0..40)
                .flat_map(|b| out[b * 2 * BLOCK + from..b * 2 * BLOCK + from + BLOCK].to_vec())
                .fold(0.0_f32, |m, x| m.max(x.abs()))
        };
        assert!(
            side(0) > 0.01 && side(BLOCK) == 0.0,
            "balance −1 keeps only the left"
        );
    }

    #[test]
    fn routes_cannot_loop() {
        let mut e = Engine::new(48_000.0);
        e.set_param(G1, Param::Out, 1.0);
        assert_eq!(
            e.param_value(G1, Param::Out),
            0.0,
            "a group can't feed itself"
        );
        e.set_param(G2, Param::Out, 1.0);
        assert_eq!(e.param_value(G2, Param::Out), 0.0, "nor a lower group");
        e.set_param(G1, Param::Out, 2.0);
        assert_eq!(e.param_value(G1, Param::Out), 2.0, "but a higher one");
        e.set_param(0, Param::Out, 8.0);
        assert_eq!(e.param_value(0, Param::Out), 8.0, "any group for a synth");
        e.set_param(0, Param::Out, 99.0);
        assert_eq!(e.param_value(0, Param::Out), 8.0, "clamped, not wrapped");
    }

    #[test]
    fn a_chain_of_groups_reaches_the_master() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Out, 1.0);
        e.set_param(G1, Param::Out, 2.0);
        e.set_param(G2, Param::Level, 0.0);
        e.note_on(0, 57, 1.0);
        assert!(
            render_out(&mut e, 40).iter().all(|x| *x == 0.0),
            "the second group's fader closes it"
        );
        e.set_param(G2, Param::Level, 1.0);
        assert!(render_out(&mut e, 40).iter().any(|x| *x != 0.0));
        let m = *e.meters();
        assert!(m[G1] > 0.0 && m[G2] > 0.0, "both groups meter");
    }

    #[test]
    fn solo_keeps_the_groups_in_a_soloed_path_heard() {
        let heard = |solo: usize| {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::Out, 1.0);
            e.note_on(0, 57, 1.0);
            e.note_on(1, 64, 1.0);
            e.set_param(solo, Param::Solo, 1.0);
            for _ in 0..40 {
                e.render(BLOCK);
            }
            let m = *e.meters();
            (m[0] > 0.0, m[1] > 0.0, m[G1] > 0.0)
        };
        assert_eq!(
            heard(0),
            (true, false, true),
            "a soloed strip is heard through its group"
        );
        assert_eq!(
            heard(1),
            (false, true, false),
            "and the group it doesn't pass is not"
        );
        assert_eq!(
            heard(G1),
            (true, false, true),
            "a soloed group is heard with what feeds it"
        );
    }

    #[test]
    fn a_group_has_inserts_for_the_mix_it_carries() {
        let play = |setup: fn(&mut Engine)| {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::Out, 1.0);
            e.set_param(1, Param::Out, 1.0);
            setup(&mut e);
            e.note_on(0, 57, 1.0);
            e.note_on(1, 64, 1.0);
            render_out(&mut e, 60)
        };
        let dry = play(|_| {});
        assert!(
            play(|e| e.set_param(G1, Param::I1Type, 4.0)) == dry,
            "a flat EQ on a group"
        );
        let boosted = play(|e| {
            e.set_param(G1, Param::I1Type, 4.0);
            e.set_param(G1, Param::I1C, 1.0);
            e.set_param(G1, Param::I1B, 0.4);
        });
        assert!(boosted != dry);
        let peak = |x: &[f32]| x.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        let squeezed = play(|e| {
            e.set_param(G1, Param::I1Type, 5.0);
            e.set_param(G1, Param::I1A, 0.1);
            e.set_param(G1, Param::I1B, 0.9);
        });
        assert!(
            peak(&squeezed) < 0.6 * peak(&dry),
            "{} vs {}",
            peak(&squeezed),
            peak(&dry)
        );
        // The inserts are the group's own: strips going straight to the master keep theirs.
        let strip_only = play(|e| {
            e.set_param(0, Param::I1Type, 3.0);
            e.set_param(0, Param::I1A, 1.0);
        });
        assert!(strip_only != dry);
    }

    #[test]
    fn groups_have_sends_of_their_own() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Out, 1.0);
        e.set_param(G1, Param::Send3, 1.0);
        e.note_on(0, 57, 1.0);
        e.render(BLOCK);
        let peak = |b: &[f32]| b.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        assert!(peak(&e.mixer.sends[2]) > 0.0);
        assert_eq!(peak(&e.mixer.sends[0]), 0.0);
    }

    #[test]
    fn sixteen_strips_through_eight_groups_stay_bounded() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        for synth in 0..SYNTHS {
            e.set_param(synth, Param::Out, 1.0 + (synth % GROUPS_N) as f32);
            e.set_param(synth, Param::Vco2Level, 1.0);
            e.note_on(synth, 36 + 3 * synth as u8, 1.0);
        }
        for g in 0..GROUPS_N {
            // Chain the groups: each into the next, the last into the master.
            if g + 1 < GROUPS_N {
                e.set_param(SYNTHS + g, Param::Out, (g + 2) as f32);
            }
        }
        for _ in 0..200 {
            e.render(BLOCK);
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
    }

    #[test]
    fn processors_can_be_chained_through_the_engine() {
        let play = |chained: bool| {
            let mut e = Engine::new(48_000.0);
            // P1 an echo with no return, P2 a reverb fed only by P1; the strip sends to P1 only.
            e.set_param(0, Param::Send1, 1.0);
            e.set_param(0, Param::P1Return, 0.0);
            e.set_param(0, Param::P2Return, 1.0);
            e.set_param(0, Param::P2In, if chained { 1.0 } else { 0.0 });
            // The echo's first repeat comes after 300 ms.
            e.note_on(0, 57, 1.0);
            render_out(&mut e, 400)
        };
        let alone = play(false);
        let chained = play(true);
        assert!(alone != chained, "the chain changes the mix");
        assert_eq!(Engine::new(48_000.0).param_value(0, Param::P2In), 0.0);
    }

    #[test]
    fn chain_parameters_are_global_and_p1_has_none() {
        let mut e = Engine::new(48_000.0);
        e.set_param(3, Param::P3In, 1.0);
        assert_eq!(e.param_value(0, Param::P3In), 1.0);
        e.reset(2);
        assert_eq!(e.param_value(0, Param::P3In), 1.0, "reset leaves it");
        e.set_param(0, Param::P4In, 7.0);
        assert_eq!(e.param_value(0, Param::P4In), 1.0, "clamped");
        assert!(!Param::ALL.iter().any(|(_, n)| *n == "P1In"));
    }

    #[test]
    fn four_chained_processors_stay_bounded() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        for (i, ty) in [(0, 4.0), (1, 3.0), (2, 1.0), (3, 2.0)] {
            let p = [Param::P1Type, Param::P2Type, Param::P3Type, Param::P4Type][i];
            let r = [
                Param::P1Return,
                Param::P2Return,
                Param::P3Return,
                Param::P4Return,
            ][i];
            e.set_param(0, p, ty);
            e.set_param(0, r, 1.0);
        }
        for p in [Param::P2In, Param::P3In, Param::P4In] {
            e.set_param(0, p, 1.0);
        }
        for synth in 0..SYNTHS {
            e.set_param(synth, Param::Send1, 1.0);
            e.note_on(synth, 36 + 3 * synth as u8, 1.0);
        }
        for _ in 0..400 {
            e.render(BLOCK);
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
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
    fn clock_steps_land_on_their_samples_through_render() {
        let mut e = Engine::new(48_000.0);
        e.play();
        let mut onsets = Vec::new();
        let mut last = None;
        // One frame at a time, so the step a frame fires is visible.
        for s in 0..48_000u64 {
            e.render(1);
            if e.clock().step() != last {
                last = e.clock().step();
                onsets.push(s);
            }
        }
        let want: Vec<u64> = (0..8).map(|k| k * 6000).collect();
        assert_eq!(onsets, want);
    }

    #[test]
    fn the_transport_drives_the_clock() {
        let mut e = Engine::new(48_000.0);
        e.set_tempo(60.0);
        e.play();
        for _ in 0..100 {
            e.render(BLOCK);
        }
        // 12_800 samples at 12_000 per step: steps 0 and 1 have fired.
        assert_eq!(e.clock().step(), Some(1));
        e.stop();
        e.render(BLOCK);
        assert_eq!(e.clock().position(), 12_800);
        e.seek(36_000);
        e.play();
        e.render(BLOCK);
        assert_eq!(e.clock().step(), Some(3));
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
        assert!(
            gated(&e, Owner::Live(0)),
            "the live Mono voice is still held"
        );
        assert!(
            !gated(&e, Owner::Channel(0)),
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

    /// A kit on synth `s`, master at full, and the peak of `blocks` rendered.
    fn kit(s: usize) -> Engine {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(s, Preset::Kit808);
        e
    }

    fn run(e: &mut Engine, blocks: usize) -> f32 {
        let mut heard = 0.0_f32;
        for _ in 0..blocks {
            e.render(BLOCK);
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            heard = heard.max(peak(e));
        }
        heard
    }

    /// #114: a synth slot holding the TR-808 plays its pads, one voice each,
    /// and they ring out.
    #[test]
    fn a_kit_slot_plays_its_pads() {
        let mut e = kit(0);
        for note in [36, 38, 42] {
            e.note_on(0, note, 0.8);
        }
        assert_eq!(e.active_voices(), 3);
        assert!(run(&mut e, 20) > 0.05);
        // One-shots: a note-off changes nothing.
        e.note_off(0, 36);
        assert_eq!(e.active_voices(), 3);
        run(&mut e, 48_000 * 3 / BLOCK);
        assert_eq!(e.active_voices(), 0, "every pad has rung out");
    }

    #[test]
    fn a_pad_hit_again_retriggers_its_own_voice() {
        let mut e = kit(0);
        for _ in 0..10 {
            e.note_on(0, 42, 0.8);
            run(&mut e, 2);
            assert_eq!(e.active_voices(), 1);
        }
        // Any other key with the closed hat's place plays it too.
        e.note_on(0, 44, 0.8);
        assert_eq!(e.active_voices(), 1);
    }

    #[test]
    fn the_closed_hat_chokes_the_open_hat() {
        let mut e = kit(0);
        e.note_on(0, 46, 0.8);
        run(&mut e, 10);
        e.note_on(0, 42, 0.8);
        run(&mut e, 1);
        assert_eq!(e.active_voices(), 1, "only the closed hat is left");
        run(&mut e, 48_000 / 5 / BLOCK);
        assert_eq!(e.active_voices(), 0);
        // Alone, the open hat is still ringing then.
        let mut open = kit(0);
        open.note_on(0, 46, 0.8);
        run(&mut open, 11 + 48_000 / 5 / BLOCK);
        assert_eq!(open.active_voices(), 1);
    }

    #[test]
    fn a_hard_hit_is_accented_and_the_knobs_reach_the_pads() {
        let hit = |velocity: f32, level: Option<f32>| {
            let mut e = kit(0);
            if let Some(l) = level {
                e.set_param(0, Param::BdLevel, l);
            }
            e.note_on(0, 36, velocity);
            run(&mut e, 40)
        };
        // Accent 0.5 at velocity 1, none at 0.8: 1.5 / 0.8 louder.
        let ratio = hit(1.0, None) / hit(0.8, None);
        assert!((ratio - 1.875).abs() < 1.0e-3, "{ratio}");
        assert_eq!(hit(1.0, Some(0.0)), 0.0, "a pad at level 0 is silent");
    }

    /// #114: a MIDI file's channel 10 plays on the slot it is routed to, when
    /// that slot holds the kit.
    #[test]
    fn channel_ten_plays_on_a_kit_slot() {
        let mut e = kit(2);
        assert_eq!(load(&mut e, &one_note(9)), Ok(1));
        e.route(9, Some(2));
        e.play();
        let heard = run(&mut e, 48_000 * 3 / 4 / BLOCK);
        assert!(heard > 0.05, "the kick at 0.5 s");
        assert_eq!(e.pools[0].active(), 0, "nothing on synth 0");
    }
}
