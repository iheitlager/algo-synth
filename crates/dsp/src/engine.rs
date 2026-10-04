//! The engine: up to `SYNTHS` synths, each with its own parameters and a voice
//! pool (`poly`: one Mono voice per owner, or a voice per note), the mixer, the
//! MIDI player, planar stereo blocks.
//!
//! Real-time rules (ADR-0002): `render` never allocates, never panics and
//! never calls `sin`/`exp`/`pow` per sample. The voices, tables and output are
//! allocated in `Engine::new`. Loading a MIDI file allocates, once, between
//! blocks (`load_midi`), never inside `render`.

use crate::arp::{ARP_DEFAULTS, Arp};
use crate::clock::{Clock, STEPS_PER_BEAT, TICKS_PER_STEP};
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
use crate::padsampler::PadField;
use crate::params::{GLOBAL_DEFAULTS, Param};
use crate::player::Sequence;
use crate::poly::{Pool, VOICE_BUDGET};

/// Song notes that may sound at once before one is dropped.
const NOTE_OFFS: usize = 256;

/// The events of a live fragment: the cycle playing and the next one, made
/// ahead of time into buffers reserved when the song loads (ADR-0002).
#[derive(Default)]
struct Live {
    cycle: Option<u64>,
    next: Option<u64>,
    cur: Vec<Event>,
    nxt: Vec<Event>,
}

impl Live {
    fn with_room(n: usize) -> Live {
        Live {
            cycle: None,
            next: None,
            cur: Vec::with_capacity(n),
            nxt: Vec::with_capacity(n),
        }
    }
}

/// The seed of a live fragment's cycle: the base seed mixed with the cycle
/// counted from the top of the song, so every run plays the same cycles.
fn cycle_seed(base: u32, cycle: u64) -> u32 {
    mix(
        base,
        u32::try_from(cycle % u64::from(u32::MAX)).unwrap_or(0),
    )
}
use crate::algo::mix;
use crate::notes::{Edit, Event, Seq, TICKS_PER_BAR};
use crate::sample::{self, Sample, SampleStore};
use crate::sampler::{ZoneField, ZoneMap};
use crate::smf;
use crate::song::{
    At, Kind, MAX_AUTOS, MAX_TEXT, MAX_TRACKS, STEPS_PER_BAR, Song, SongError, Step, Target,
};
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
    /// A WAV file's bytes, written by JavaScript before `load_sample`.
    wav: Vec<u8>,
    samples: SampleStore,
    /// The waveform peaks `sample_peaks` last computed, for the view to read.
    peaks: Vec<f32>,
    /// Each synth's zones, for when it is a sampler.
    zones: Vec<ZoneMap>,
    sequence: Sequence,
    /// The transport's tempo and sixteenth steps (spec 002 Req 5).
    clock: Clock,
    /// The synth each MIDI channel plays on; `None` mutes it.
    route: [Option<usize>; CHANNELS],
    /// The song (ADR-0012), its text as the view wrote it, its canonical
    /// print, the last load's error, and the synth each track plays on.
    song: Song,
    song_buf: Vec<u8>,
    song_text: String,
    song_error: Option<SongError>,
    song_route: [Option<usize>; MAX_TRACKS],
    /// Notes of the song waiting for their note-off, as (tick, track, note):
    /// a fixed table, so the clock can end a note without allocating.
    note_offs: [Option<(u64, u8, u8)>; NOTE_OFFS],
    /// One per fragment of the song; empty for all but the live ones.
    live: Vec<Live>,
    /// The value each automation lane last wrote, so it writes only changes
    /// and a hand on a knob holds until the next one (ADR-0015).
    auto_last: [f32; MAX_AUTOS],
    /// Strips (bit per strip, globals on bit 0) automation changed since the
    /// view last asked, so it can redraw their values.
    touched: u32,
    /// Each synth's live arpeggiator (spec 002 Req 7).
    arps: [Arp; SYNTHS],
    /// The arps' own grid while the song's clock is stopped: ticks fired, the
    /// sample the next one is on, and the samples run so far. Reset when the
    /// clock starts or stops.
    free_tick: u64,
    free_next: f64,
    free_pos: u64,
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
            wav: Vec::new(),
            samples: SampleStore::new(),
            peaks: Vec::new(),
            zones: (0..SYNTHS).map(|_| ZoneMap::new()).collect(),
            sequence: Sequence::default(),
            clock: Clock::new(sample_rate),
            route: [Some(0); CHANNELS],
            song: Song::default(),
            song_buf: Vec::new(),
            song_text: Song::default().print(),
            song_error: None,
            song_route: [None; MAX_TRACKS],
            note_offs: [None; NOTE_OFFS],
            live: Vec::new(),
            auto_last: [f32::NAN; MAX_AUTOS],
            touched: 0,
            arps: [Arp::default(); SYNTHS],
            free_tick: 0,
            free_next: 0.0,
            free_pos: 0,
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
        if param.is_arp() {
            self.set_arp(synth, param, v);
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
        // A kit's individual outs feed groups directly; the solos follow them.
        let feeds = mono.pad_groups();
        self.mixer.set_feeds(synth, feeds);
        if param == Param::ChorusMode {
            if let Some(c) = self.chorus.get_mut(synth) {
                c.set_mode(mono.chorus_mode);
            }
        }
    }

    /// An arp parameter of `synth`; turning the arp on or off hands the live
    /// voice over, so no key is left sounding.
    fn set_arp(&mut self, synth: usize, param: Param, v: f32) {
        let (Some(arp), Some(slot)) = (
            self.arps.get_mut(synth),
            self.values
                .get_mut(synth)
                .and_then(|r| r.get_mut(param as usize)),
        ) else {
            return;
        };
        *slot = v;
        if arp.set(param, v) || param == Param::ArpFree {
            let sounding = arp.take_sounding();
            if let (Some(n), Some(s)) = (sounding, live(synth)) {
                self.stop_note(Owner::Live(s), n);
            }
        }
        if param == Param::ArpOn {
            for pool in self.pools.iter_mut() {
                pool.release_owner(Owner::Live(u8::try_from(synth).unwrap_or(u8::MAX)));
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

    /// Put `synth`'s sound back to the defaults and leave its strip alone: the
    /// base a user preset is applied on (ADR-0014).
    pub fn synth_defaults(&mut self, synth: usize) {
        for (p, v) in DEFAULTS.iter() {
            self.set_param(synth, *p, *v);
        }
    }

    /// Put `synth` back to the defaults, for a newly added synth.
    pub fn reset(&mut self, synth: usize) {
        for (p, v) in DEFAULTS.iter().chain(STRIP_DEFAULTS.iter()) {
            self.set_param(synth, *p, *v);
        }
        for (p, v) in ARP_DEFAULTS {
            self.set_param(synth, p, v);
        }
    }

    /// Live input: press a key on `synth`'s live voice, or, with its arp on,
    /// add it to the arp's held notes.
    pub fn note_on(&mut self, synth: usize, note: u8, velocity: f32) {
        if let Some(s) = live(synth) {
            if let Some(arp) = self.arps.get_mut(synth).filter(|a| a.on) {
                arp.press(note);
                return;
            }
            self.start_voice(Owner::Live(s), note, velocity);
        }
    }

    /// Live input: release this key on `synth`.
    pub fn note_off(&mut self, synth: usize, note: u8) {
        if let Some(s) = live(synth) {
            if let Some(arp) = self.arps.get_mut(synth).filter(|a| a.on) {
                arp.release(note);
                return;
            }
            self.stop_note(Owner::Live(s), note);
        }
    }

    /// Release every voice, and let go of every arp's keys.
    pub fn all_off(&mut self) {
        for arp in self.arps.iter_mut() {
            arp.set(Param::ArpOn, if arp.on { 1.0 } else { 0.0 });
            arp.take_sounding();
        }
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
            Owner::Track(t) => self.song_routed(usize::from(t)),
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

    // --- Samples ---------------------------------------------------------

    /// Size the WAV buffer for `len` bytes and return it for writing.
    /// `None` if the file is larger than `sample::MAX_WAV`.
    pub fn sample_buffer(&mut self, len: usize) -> Option<&mut [u8]> {
        if len > sample::MAX_WAV {
            return None;
        }
        self.wav.clear();
        self.wav.resize(len, 0);
        Some(&mut self.wav)
    }

    /// Parse the buffer into `slot`, resampled to the engine's rate. Returns
    /// the frame count.
    pub fn load_sample(&mut self, slot: usize) -> Result<usize, sample::Error> {
        let rate = self.sample_rate;
        self.samples.load(slot, &self.wav, rate).map(Sample::frames)
    }

    pub fn samples(&self) -> &SampleStore {
        &self.samples
    }

    pub fn clear_sample(&mut self, slot: usize) {
        self.samples.clear(slot);
    }

    /// Set a field of one of `synth`'s zones (see `sampler::ZoneField`).
    pub fn set_zone(&mut self, synth: usize, zone: usize, field: ZoneField, value: f32) {
        if let Some(z) = self.zones.get_mut(synth) {
            z.set(zone, field, value);
        }
    }

    /// Empty every zone of `synth`.
    pub fn clear_zones(&mut self, synth: usize) {
        if let Some(z) = self.zones.get_mut(synth) {
            z.clear();
        }
    }

    /// A field of one of `synth`'s zones, as `set_zone` would take it back.
    pub fn zone_value(&self, synth: usize, zone: usize, field: ZoneField) -> f32 {
        self.zones
            .get(synth)
            .and_then(|z| z.get(zone))
            .map_or(0.0, |z| z.get(field))
    }

    /// Work out `bins` (min, max) pairs for the sample in `slot` into the peaks
    /// buffer and return how many values it holds (0 for an empty slot).
    /// Allocates: a control-thread call, never `render`.
    pub fn sample_peaks(&mut self, slot: usize, bins: usize) -> usize {
        self.peaks.clear();
        if let Some(s) = self.samples.get(slot) {
            s.peaks(bins.clamp(1, 4096), &mut self.peaks);
        }
        self.peaks.len()
    }

    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    /// Set a field of one of `synth`'s pads (see `padsampler::PadField`).
    pub fn set_pad(&mut self, synth: usize, pad: usize, field: PadField, value: f32) {
        if let Some(p) = self.synths.get_mut(synth) {
            p.pad_kit.set(pad, field, value);
        }
    }

    /// A field of one of `synth`'s pads, as `set_pad` would take it back.
    pub fn pad_value(&self, synth: usize, pad: usize, field: PadField) -> f32 {
        self.synths
            .get(synth)
            .and_then(|p| p.pad_kit.pad(pad))
            .map_or(0.0, |c| c.get(field))
    }

    /// Put every pad of `synth` back to its defaults.
    pub fn clear_pads(&mut self, synth: usize) {
        if let Some(p) = self.synths.get_mut(synth) {
            p.pad_kit.clear();
        }
    }

    pub fn zones(&self, synth: usize) -> Option<&ZoneMap> {
        self.zones.get(synth)
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

    /// The MIDI file's transport: play, stop and seek move only the file
    /// (spec 002 Req 9). The song has its own (`song_play`).
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

    /// The song's transport: the clock runs its lanes (spec 002 Req 5). Play
    /// continues from where it stopped; stop goes back to the top, as a
    /// drum machine's does. Hits ring out.
    pub fn song_play(&mut self) {
        if !self.clock.playing() {
            self.hand_arps_over();
        }
        self.clock.play();
    }

    /// The arps change grids with the transport: let their notes go and start
    /// the free grid again from the top.
    fn hand_arps_over(&mut self) {
        for synth in 0..SYNTHS {
            let sounding = self.arps.get_mut(synth).and_then(Arp::take_sounding);
            if let (Some(n), Some(s)) = (sounding, live(synth)) {
                self.stop_note(Owner::Live(s), n);
            }
        }
        self.free_tick = 0;
        self.free_next = 0.0;
        self.free_pos = 0;
    }

    pub fn song_stop(&mut self) {
        self.hand_arps_over();
        self.release_song_notes();
        self.clock.stop();
        self.clock.seek(0);
        // From the top every lane writes again.
        self.auto_last = [f32::NAN; MAX_AUTOS];
    }

    /// Move the song to the first step of `bar` (from 0); the clock fires it next.
    pub fn song_seek_bar(&mut self, bar: u64) {
        self.clock.seek_step(bar.saturating_mul(STEPS_PER_BAR));
    }

    /// Where the last fired step fell: the arrangement entry and the steps into
    /// it, or `None` without an arrangement or before the first step.
    pub fn song_place(&self) -> Option<(usize, u64)> {
        match self.song.at(self.clock.step()?) {
            At::In { entry, local, .. } => Some((entry, local)),
            _ => None,
        }
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
        while let Some(k) = self.clock.due() {
            self.play_step(k);
            self.play_tick(k * TICKS_PER_STEP);
        }
        while let Some(j) = self.clock.due_sub() {
            self.play_tick(j);
        }
        // With the clock stopped, the arps set to free run keep their own grid.
        if !self.clock.playing() && self.free_arps() {
            while self.free_next <= self.free_pos as f64 {
                let j = self.free_tick;
                self.free_tick += 1;
                self.free_next += self.free_tick_len();
                self.arp_tick(j, false);
            }
        }
    }

    fn free_arps(&self) -> bool {
        self.arps.iter().any(|a| a.on && a.free)
    }

    /// Samples in one tick at the clock's tempo.
    fn free_tick_len(&self) -> f64 {
        f64::from(self.sample_rate) * 60.0
            / f64::from(self.clock.tempo())
            / (STEPS_PER_BEAT * TICKS_PER_STEP) as f64
    }

    /// Frames until the free grid's next tick, when it runs.
    fn free_frames_until_next(&self, remaining: usize) -> usize {
        if self.clock.playing() || !self.free_arps() {
            return remaining;
        }
        let gap = (self.free_next - self.free_pos as f64).ceil().max(1.0);
        (gap as usize).clamp(1, remaining.max(1))
    }

    /// Tick `j` of an arp grid: the clock's, or with `clocked` false the free
    /// one, which only the free-run arps follow. A note ends before the next
    /// starts, so a full gate hands over cleanly.
    fn arp_tick(&mut self, j: u64, clocked: bool) {
        for synth in 0..SYNTHS {
            let Some(arp) = self
                .arps
                .get_mut(synth)
                .filter(|a| a.on && (clocked || a.free))
            else {
                continue;
            };
            let (off, on) = arp.tick(j);
            let Some(s) = live(synth) else {
                continue;
            };
            if let Some(n) = off {
                self.stop_note(Owner::Live(s), n);
            }
            if let Some(n) = on {
                self.start_voice(Owner::Live(s), n, 1.0);
            }
        }
    }

    /// Tick `j` of the clock: end the notes that are due, then start what
    /// every note fragment has on it (spec 002 Req 3, ADR-0016). Each fragment
    /// loops on its own length in bars; with an arrangement only the current
    /// section's fragments play, counted from its first tick (ADR-0015).
    /// Reads the song in place and uses the fixed note-off table: nothing
    /// allocates.
    fn play_tick(&mut self, j: u64) {
        self.arp_tick(j, true);
        for i in 0..self.note_offs.len() {
            let due = self
                .note_offs
                .get(i)
                .copied()
                .flatten()
                .filter(|(at, _, _)| *at <= j);
            if let Some((_, track, note)) = due {
                if let Some(slot) = self.note_offs.get_mut(i) {
                    *slot = None;
                }
                self.stop_note(Owner::Track(track), note);
            }
        }
        // Where the tick falls in the arrangement: the tick to count from and
        // the section that plays, if any. Note-offs above use the clock's own
        // ticks, so a note started in one section ends where it should.
        let step = j / TICKS_PER_STEP;
        let (from, section) = match self.song.at(step) {
            At::Free(_) => (j, None),
            At::In { section, local, .. } => {
                (local * TICKS_PER_STEP + j % TICKS_PER_STEP, Some(section))
            }
            At::End => return,
        };
        for f in 0..self.song.frags.len() {
            if let Some(s) = section {
                if !self
                    .song
                    .sections
                    .get(s)
                    .is_some_and(|sec| sec.frags.contains(&f))
                {
                    continue;
                }
            }
            let Some((track, span, live)) = self
                .song
                .frags
                .get(f)
                .and_then(|fr| Some((fr, fr.notes.as_ref()?)))
                .map(|(fr, n)| {
                    let track = u8::try_from(fr.track).unwrap_or(u8::MAX);
                    (track, u64::from(n.bars * TICKS_PER_BAR).max(1), fr.live)
                })
            else {
                continue;
            };
            let local = u32::try_from(from % span).unwrap_or(0);
            if live {
                self.step_live(f, from / span, local);
            }
            let first = self.first_event(f, local);
            for k in first.. {
                let Some(ev) = self.note_event(f, k).filter(|e| e.start == local) else {
                    break;
                };
                let Some(slot) = self.note_offs.iter_mut().find(|s| s.is_none()) else {
                    continue;
                };
                *slot = Some((j + u64::from(ev.len), track, ev.note));
                self.start_voice(Owner::Track(track), ev.note, ev.velocity());
            }
        }
    }

    /// Event `k` of fragment `f` for the cycle now playing.
    fn note_event(&self, f: usize, k: usize) -> Option<Event> {
        let fr = self.song.frags.get(f)?;
        if fr.live {
            self.live.get(f)?.cur.get(k).copied()
        } else {
            fr.notes.as_ref()?.events.get(k).copied()
        }
    }

    /// Index of the first event of fragment `f` at or after tick `local`.
    fn first_event(&self, f: usize, local: u32) -> usize {
        let Some(fr) = self.song.frags.get(f) else {
            return 0;
        };
        if fr.live {
            self.live
                .get(f)
                .map_or(0, |l| l.cur.partition_point(|e| e.start < local))
        } else {
            fr.notes
                .as_ref()
                .map_or(0, |n| n.events.partition_point(|e| e.start < local))
        }
    }

    /// Have live fragment `f`'s events for `cycle` ready, and, a tick into the
    /// cycle, those of the next, so the swap at the cycle line is a move and
    /// not a computation. A seek or a new song makes the cycle on the spot.
    /// The buffers were sized when the song loaded: nothing allocates.
    fn step_live(&mut self, f: usize, cycle: u64, local: u32) {
        let Some(Seq::Generated(call)) = self
            .song
            .frags
            .get(f)
            .and_then(|fr| fr.notes.as_ref())
            .map(|n| &n.seq)
        else {
            return;
        };
        let Some(live) = self.live.get_mut(f) else {
            return;
        };
        let scale = self.song.scale.as_ref();
        if live.cycle != Some(cycle) {
            if live.next == Some(cycle) {
                std::mem::swap(&mut live.cur, &mut live.nxt);
            } else {
                call.events_into(cycle_seed(call.seed(), cycle), scale, &mut live.cur);
            }
            live.cycle = Some(cycle);
            live.next = None;
        }
        if local > 0 && live.next != Some(cycle + 1) {
            call.events_into(cycle_seed(call.seed(), cycle + 1), scale, &mut live.nxt);
            live.next = Some(cycle + 1);
        }
    }

    /// One buffer pair per fragment, sized for its call; only live fragments
    /// get room.
    fn rebuild_live(&mut self) {
        self.live = self
            .song
            .frags
            .iter()
            .map(|fr| match fr.notes.as_ref().map(|n| &n.seq) {
                Some(Seq::Generated(call)) if fr.live => Live::with_room(call.max_events()),
                _ => Live::default(),
            })
            .collect();
    }

    /// The events fragment `frag` is playing: a live one's current cycle, else
    /// what it was written as (empty for a drum fragment).
    pub fn frag_events(&self, frag: usize) -> &[Event] {
        match (self.song.frags.get(frag), self.live.get(frag)) {
            (Some(fr), Some(l)) if fr.live && l.cycle.is_some() => &l.cur,
            (Some(fr), _) => fr.notes.as_ref().map_or(&[], |n| &n.events),
            _ => &[],
        }
    }

    /// Edit a note of fragment `frag` and print the song again; false when
    /// the song does not take it (`Song::edit_note`).
    pub fn edit_note(&mut self, frag: usize, op: Edit) -> bool {
        if !self.song.edit_note(frag, op) {
            return false;
        }
        self.song_text = self.song.print();
        true
    }

    /// Replace fragment `frag`'s generator call with the events it is playing
    /// now (a live one: this cycle's), as notes in the same notation, and
    /// print the song again. False when it is not a generated fragment or
    /// the events do not fit the notation.
    pub fn freeze(&mut self, frag: usize) -> bool {
        let playing = self
            .live
            .get(frag)
            .filter(|l| l.cycle.is_some())
            .map(|l| l.cur.clone());
        if !self.song.freeze(frag, playing.as_deref()) {
            return false;
        }
        self.rebuild_live();
        self.song_text = self.song.print();
        true
    }

    /// End every note of the song that is still sounding.
    fn release_song_notes(&mut self) {
        for i in 0..self.note_offs.len() {
            if let Some((_, track, note)) = self.note_offs.get_mut(i).and_then(Option::take) {
                self.stop_note(Owner::Track(track), note);
            }
        }
    }

    /// Hit what every lane of the song has on clock step `k`; each lane loops
    /// on its own length. With an arrangement only the current section's
    /// fragments play, counted from the section's first step, and the song
    /// stops after its last bar (ADR-0015). Reads the song in place: nothing
    /// allocates.
    fn play_step(&mut self, k: u64) {
        let (k, section) = match self.song.at(k) {
            At::Free(k) => (k, None),
            At::In { section, local, .. } => (local, Some(section)),
            At::End => {
                self.song_stop();
                return;
            }
        };
        if let (Some(s), 0) = (section, k) {
            self.apply_scenes(s);
        }
        for f in 0..self.song.frags.len() {
            let Some(frag) = self.song.frags.get(f) else {
                continue;
            };
            if let Some(s) = section {
                if !self
                    .song
                    .sections
                    .get(s)
                    .is_some_and(|sec| sec.frags.contains(&f))
                {
                    continue;
                }
            }
            let owner = Owner::Track(u8::try_from(frag.track).unwrap_or(u8::MAX));
            for l in 0..frag.lanes.len() {
                let hit = self
                    .song
                    .frags
                    .get(f)
                    .and_then(|fr| fr.lanes.get(l))
                    .and_then(|lane| {
                        let len = lane.steps.len() as u64;
                        let step = lane.steps.get(usize::try_from(k % len.max(1)).ok()?)?;
                        Some((lane.pad.note(), step.velocity()?))
                    });
                if let Some((note, velocity)) = hit {
                    self.start_voice(owner, note, velocity);
                }
            }
        }
    }

    /// Set the values of every scene section `s` lists (ADR-0015).
    fn apply_scenes(&mut self, s: usize) {
        let count = self.song.sections.get(s).map_or(0, |sec| sec.scenes.len());
        for i in 0..count {
            let Some(scene) = self
                .song
                .sections
                .get(s)
                .and_then(|sec| sec.scenes.get(i))
                .copied()
            else {
                continue;
            };
            let sets = self.song.scenes.get(scene).map_or(0, |sc| sc.sets.len());
            for j in 0..sets {
                if let Some((t, p, v)) = self
                    .song
                    .scenes
                    .get(scene)
                    .and_then(|sc| sc.sets.get(j))
                    .copied()
                {
                    self.automate(t, p, v);
                }
            }
        }
    }

    /// Write an automated value to its target's strip (a track goes where it
    /// is routed; an unrouted one is skipped) and mark the strip for the view.
    fn automate(&mut self, target: Target, param: Param, v: f32) {
        let strip = match target {
            Target::Master => 0,
            Target::Strip(s) => s,
            Target::Track(t) => match self.song_routed(t) {
                Some(s) => s,
                None => return,
            },
        };
        self.set_param(strip, param, v);
        self.touched |= 1u32.checked_shl(strip as u32).unwrap_or(0);
    }

    /// The automation lanes at the clock's position, once per block: each
    /// writes only when its value changed. Lanes of the current section play,
    /// counted from its first step; without an arrangement every lane loops.
    fn run_automation(&mut self) {
        if !self.clock.playing() || self.song.autos.is_empty() {
            return;
        }
        let pos = self.clock.step_position().max(0.0);
        let whole = pos.floor();
        let frac = pos - whole;
        let (local, section) = match self.song.at(whole as u64) {
            At::Free(k) => (k as f64 + frac, None),
            At::In { section, local, .. } => (local as f64 + frac, Some(section)),
            At::End => return,
        };
        for a in 0..self.song.autos.len() {
            if let Some(s) = section {
                if !self
                    .song
                    .sections
                    .get(s)
                    .is_some_and(|sec| sec.autos.contains(&a))
                {
                    continue;
                }
            }
            let Some((target, param, v)) = self
                .song
                .autos
                .get(a)
                .map(|auto| (auto.target, auto.param, auto.value_at(local)))
            else {
                continue;
            };
            let Some(last) = self.auto_last.get_mut(a) else {
                continue;
            };
            if *last == v {
                continue;
            }
            *last = v;
            self.automate(target, param, v);
        }
    }

    /// The strips automation changed since the last call (bit per strip), cleared.
    pub fn take_touched(&mut self) -> u32 {
        std::mem::take(&mut self.touched)
    }

    // --- The song (spec 002 Req 6, ADR-0012) -------------------------------

    /// Size the song text buffer and hand it out; `None` when too long.
    pub fn song_buffer(&mut self, len: usize) -> Option<&mut [u8]> {
        if len > MAX_TEXT {
            return None;
        }
        self.song_buf.clear();
        self.song_buf.resize(len, 0);
        Some(&mut self.song_buf)
    }

    /// Parse the buffer and play it from the next clock step, keeping each
    /// lane's place against the clock. A text that does not parse leaves the
    /// song playing and is reported (`song_error`). Tempo and swing go to the
    /// clock; a track keeps its synth, or goes to the first drum kit (the 808 or a pad sampler).
    pub fn load_song(&mut self) -> Result<(), SongError> {
        let parsed = match std::str::from_utf8(&self.song_buf) {
            Ok(text) => Song::parse(text),
            Err(e) => {
                let ok = self.song_buf.get(..e.valid_up_to()).unwrap_or(&[]);
                let ok = std::str::from_utf8(ok).unwrap_or("");
                Err(SongError {
                    line: ok.matches('\n').count() + 1,
                    col: ok.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1,
                    msg: "the text is not UTF-8",
                })
            }
        };
        match parsed {
            Ok(song) => {
                self.clock.set_tempo(song.tempo);
                self.clock.set_swing(song.swing);
                let kit = (0..SYNTHS).find(|s| {
                    self.synths
                        .get(*s)
                        .is_some_and(|p| p.model.uses_drums() || p.model.uses_pads())
                });
                let multi = (0..SYNTHS)
                    .find(|s| self.synths.get(*s).is_some_and(|p| p.model.uses_sampler()));
                let voiced = (0..SYNTHS).find(|s| {
                    self.synths
                        .get(*s)
                        .is_some_and(|p| !p.model.uses_drums() && !p.model.uses_pads())
                });
                for (t, route) in self.song_route.iter_mut().enumerate() {
                    if t >= song.tracks.len() {
                        *route = None;
                    } else if route.is_none() {
                        let notes = song.frags.iter().any(|f| f.track == t && f.notes.is_some());
                        *route = match song.tracks.get(t).map(|t| t.kind) {
                            Some(Kind::Synth) => voiced,
                            Some(Kind::Sampler) if notes => multi.or(voiced),
                            _ => kit,
                        };
                    }
                }
                self.song = song;
                self.rebuild_live();
                self.auto_last = [f32::NAN; MAX_AUTOS];
                self.song_text = self.song.print();
                self.song_error = None;
                Ok(())
            }
            Err(e) => {
                self.song_error = Some(e);
                Err(e)
            }
        }
    }

    /// Turn the loaded MIDI file into the song (#173): its text replaces the
    /// song's, and each track goes to the synth its channel plays on in the
    /// player. The number of tracks, or a negative code: the MIDI file's own
    /// (`smf::Error::code`), `ImportError::code`, or −9 when the text does
    /// not parse (a bug). Allocates; never called from `render`.
    pub fn import_midi(&mut self) -> Result<usize, i32> {
        let smf = smf::parse(&self.midi).map_err(smf::Error::code)?;
        let imported = crate::midi_import::import(&smf).map_err(|e| e.code())?;
        self.song_buf = imported.text.into_bytes();
        self.load_song().map_err(|_| -9)?;
        for (t, ch) in imported.channels.iter().enumerate() {
            let synth = self.routed(*ch);
            if let Some(slot) = self.song_route.get_mut(t) {
                *slot = synth;
            }
        }
        Ok(imported.channels.len())
    }

    pub fn song(&self) -> &Song {
        &self.song
    }

    /// The song as the engine prints it, after the last load or edit.
    pub fn song_text(&self) -> &str {
        &self.song_text
    }

    /// Why the last load failed, if it did.
    pub fn song_error(&self) -> Option<SongError> {
        self.song_error
    }

    /// Set one step of the song (level 0 off, 1 hit, 2 accent) and print it
    /// again; false when there is no such step or level.
    pub fn set_step(&mut self, frag: usize, lane: usize, step: usize, level: u32) -> bool {
        let Some(to) = Step::from_level(level) else {
            return false;
        };
        if !self.song.set_step(frag, lane, step, to) {
            return false;
        }
        self.song_text = self.song.print();
        true
    }

    /// An arranger edit (#171): 0 toggle (section, kind, item), 1 add a
    /// section (bars), 2 set a section's bars (section, bars), 3 insert an
    /// entry (place, section), 4 remove an entry (place), 5 move an entry
    /// (from, to), 6 set the loop (first, last; 0 0 clears). The song is
    /// printed again; false when refused.
    pub fn arrange_edit(&mut self, op: u32, a: u32, b: u32, c: u32) -> bool {
        let (a, b, cu) = (a as usize, b as usize, c as usize);
        let ok = match op {
            0 => self.song.toggle(a, b as u32, cu),
            1 => self.song.add_section(a as u32).is_some(),
            2 => self.song.set_bars(a, b as u32),
            3 => self.song.arrange_insert(a, b),
            4 => self.song.arrange_remove(a),
            5 => self.song.arrange_move(a, b),
            6 => self.song.set_loop(a as u32, b as u32),
            _ => false,
        };
        if ok {
            self.song_text = self.song.print();
        }
        ok
    }

    /// Set the song's tempo (BPM, clamped as the clock clamps it) and print
    /// it again; the clock follows from the next step.
    pub fn set_song_tempo(&mut self, bpm: f32) {
        self.clock.set_tempo(bpm);
        self.song.tempo = self.clock.tempo();
        self.song_text = self.song.print();
    }

    /// Set the song's swing (percent, 50 to 75) and print it again.
    pub fn set_song_swing(&mut self, pct: f32) {
        self.clock.set_swing(pct);
        self.song.swing = self.clock.swing();
        self.song_text = self.song.print();
    }

    /// Play song track `track` on `synth`, or mute it with `None`.
    pub fn song_route(&mut self, track: usize, synth: Option<usize>) {
        if let Some(slot) = self.song_route.get_mut(track) {
            *slot = synth.filter(|s| *s < SYNTHS);
        }
    }

    pub fn song_routed(&self, track: usize) -> Option<usize> {
        self.song_route.get(track).copied().flatten()
    }

    // --- Render ----------------------------------------------------------

    /// Render `frames` (at most `BLOCK`) into the output buffer. The block is
    /// split at player events and clock steps, so each lands on its exact
    /// sample.
    pub fn render(&mut self, frames: usize) {
        let n = frames.min(BLOCK);
        self.run_automation();
        self.out.fill(0.0);
        self.mixer.clear(n);
        let mut t = 0;
        while t < n {
            self.fire_due_events();
            let chunk = self
                .sequence
                .frames_until_next(n - t)
                .min(self.clock.frames_until_next(n - t))
                .min(self.free_frames_until_next(n - t));
            for (synth, (pool, params)) in self.pools.iter_mut().zip(self.synths.iter()).enumerate()
            {
                if pool.active() == 0 {
                    continue;
                }
                if params.model.uses_drums() {
                    // The kit's pads go to its strip or straight to a group (#162).
                    if let Some((bus, direct)) = self.mixer.kit_outs(synth, t..t + chunk) {
                        pool.render_kit(params, &self.sine, &self.blep, bus, direct, t);
                    }
                    continue;
                }
                if params.model.uses_pads() {
                    // Pads pan themselves: the synth gets a stereo bus.
                    if let Some((l, r)) = self.mixer.stereo_bus(synth, t..t + chunk) {
                        pool.render_pads(&self.samples, l, r);
                    }
                    continue;
                }
                let Some(zones) = self.zones.get(synth) else {
                    continue;
                };
                if let Some(buf) = self.mixer.bus(synth, t..t + chunk) {
                    let tools = Tools {
                        sine: &self.sine,
                        blep: &self.blep,
                        ladder: &self.ladder,
                        pitch: &self.pitch,
                        tables: self.tables,
                        samples: &self.samples,
                        zones,
                    };
                    pool.render(params, tools, buf);
                }
            }
            self.sequence.advance(chunk);
            self.clock.advance(chunk);
            if !self.clock.playing() {
                self.free_pos += chunk as u64;
            }
            t += chunk;
        }
        // A synth with its chorus on is stereo from here on.
        for (synth, chorus) in self.chorus.iter_mut().enumerate() {
            if chorus.on() && !self.mixer.is_wide(synth) {
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

    /// Every note a synth starts in `frames` frames, as (frame, synth, note),
    /// rendered `step` frames at a time (so a frame is known to within `step`).
    fn note_starts(e: &mut Engine, frames: u64, step: usize) -> Vec<(u64, usize, u8)> {
        let mut held: Vec<Vec<u8>> = vec![Vec::new(); SYNTHS];
        let mut out = Vec::new();
        for f in (0..frames).step_by(step) {
            e.render(step);
            for (s, was) in held.iter_mut().enumerate() {
                let now = e.pools.get(s).map(|p| p.held_notes()).unwrap_or_default();
                for n in &now {
                    if !was.contains(n) {
                        out.push((f, s, *n));
                    }
                }
                *was = now;
            }
        }
        out
    }

    /// #173: the demo Canon imported as a song starts the same notes on the
    /// same synths as the MIDI player plays them, at the same time (to the
    /// eight frames the test renders at once, far finer than a tick).
    #[test]
    fn the_demo_imported_plays_like_the_player() {
        let bytes = include_bytes!("../../../web/public/demo.mid");
        let fresh = || {
            let mut e = Engine::new(48_000.0);
            for s in 0..SYNTHS {
                e.set_param(s, Param::Polyphony, 8.0);
            }
            e
        };
        let mut player = fresh();
        assert!(load(&mut player, bytes).is_ok());
        player.play();
        let mut song = fresh();
        assert!(load(&mut song, bytes).is_ok());
        let tracks = song.import_midi().expect("the demo imports");
        assert_eq!(tracks, player.sequence().parts().len());
        assert!(song.song().arrange.len() > 1, "in sections");
        song.song_play();
        let frames = 48_000 * 24;
        let a = note_starts(&mut player, frames, 8);
        let b = note_starts(&mut song, frames, 8);
        assert!(a.len() > 40, "the demo plays: {}", a.len());
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!((x.1, x.2), (y.1, y.2), "{x:?} {y:?}");
            assert!(x.0.abs_diff(y.0) <= 8, "{x:?} {y:?}");
        }
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

    /// #144: a send taken before the fader ignores it; one switched off is
    /// silent and keeps its level; mute silences both kinds.
    #[test]
    fn sends_before_the_fader_and_switched_off() {
        let send = |setup: &dyn Fn(&mut Engine)| {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::Send1, 1.0);
            setup(&mut e);
            e.note_on(0, 57, 1.0);
            e.render(BLOCK);
            e.mixer.sends[0].iter().fold(0.0_f32, |m, x| m.max(x.abs()))
        };
        let post_full = send(&|_| {});
        let post_down = send(&|e| e.set_param(0, Param::Level, 0.0));
        let pre_down = send(&|e| {
            e.set_param(0, Param::Send1Pre, 1.0);
            e.set_param(0, Param::Level, 0.0);
        });
        assert!(
            post_full > 0.0 && post_down == 0.0,
            "post follows the fader"
        );
        assert_eq!(pre_down, post_full, "pre ignores it");
        let off = send(&|e| e.set_param(0, Param::Send1On, 0.0));
        assert_eq!(off, 0.0, "off is silent");
        let back = send(&|e| {
            e.set_param(0, Param::Send1On, 0.0);
            e.set_param(0, Param::Send1On, 1.0);
        });
        assert_eq!(back, post_full, "and keeps its level");
        let muted = send(&|e| {
            e.set_param(0, Param::Send1Pre, 1.0);
            e.set_param(0, Param::Mute, 1.0);
        });
        assert_eq!(muted, 0.0, "mute silences a pre send");
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
        // Every strip parameter has a default, and nothing else is one.
        assert_eq!(
            Param::ALL.iter().filter(|(p, _)| p.is_strip()).count(),
            STRIP_DEFAULTS.len()
        );
        assert!(STRIP_DEFAULTS.iter().all(|(p, _)| p.is_strip()));
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
        assert_eq!(
            e.param_value(0, Param::Out),
            9.0,
            "clamped to nowhere (#161), not wrapped"
        );
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
        e.song_play();
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
    fn the_song_and_the_file_have_their_own_transports() {
        let mut e = Engine::new(48_000.0);
        load(&mut e, &one_note(0)).expect("loads");
        e.set_tempo(60.0);
        // The song plays with the file stopped.
        e.song_play();
        for _ in 0..100 {
            e.render(BLOCK);
        }
        // 12_800 samples at 12_000 per step: steps 0 and 1 have fired.
        assert_eq!(e.clock().step(), Some(1));
        assert!(!e.sequence().playing());
        // The file plays and stops without touching the song.
        e.play();
        e.render(BLOCK);
        e.stop();
        assert!(e.clock().playing());
        assert_eq!(e.sequence().position(), BLOCK as u64);
        // Stopping the song goes back to the top and leaves the file alone.
        e.play();
        e.song_stop();
        e.render(BLOCK);
        assert!(e.sequence().playing());
        assert_eq!((e.clock().position(), e.clock().step()), (0, None));
        e.song_play();
        e.render(BLOCK);
        assert_eq!(e.clock().step(), Some(0), "from the top again");
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

    fn load_text(e: &mut Engine, text: &str) -> Result<(), SongError> {
        e.song_buffer(text.len())
            .expect("fits")
            .copy_from_slice(text.as_bytes());
        e.load_song()
    }

    const FOUR: &str = "tempo 120\ntrack kit drums\nfrag b = kit /16\n  bd x...x...x...x...\n";

    /// #100: `bd x...x...x...x...` at 120 BPM and 48 kHz hits on 0, 24000,
    /// 48000 and 72000, rendered a frame at a time.
    #[test]
    fn a_drum_lane_hits_on_its_exact_samples() {
        let mut e = kit(0);
        assert_eq!(load_text(&mut e, FOUR), Ok(()));
        assert_eq!(e.song_routed(0), Some(0), "the first kit plays the track");
        e.song_play();
        let mut hits = Vec::new();
        let mut count = e.note_count;
        for s in 0..96_000u64 {
            e.render(1);
            if e.note_count != count {
                count = e.note_count;
                hits.push(s);
            }
        }
        assert_eq!(hits, vec![0, 24_000, 48_000, 72_000]);
    }

    /// The clock steps (at 120 BPM and 48 kHz, 6000 samples each) that start
    /// a note within `steps` steps, rendered a frame at a time.
    fn hit_steps(e: &mut Engine, steps: u64) -> Vec<u64> {
        let mut hits = Vec::new();
        let mut count = e.note_count;
        for s in 0..steps * 6000 {
            e.render(1);
            if e.note_count != count {
                for _ in count..e.note_count {
                    hits.push(s / 6000);
                }
                count = e.note_count;
            }
        }
        hits
    }

    /// Spec 002 Req 4: each section plays its own fragments from its first
    /// bar, on the exact sample, and the song stops after the last bar.
    #[test]
    fn sections_play_in_order_and_the_song_ends() {
        let mut e = kit(0);
        let text = "tempo 120\ntrack kit drums\nfrag a = kit\n  bd x...\nfrag b = kit\n  sn x.\n\
                    section one 1: a\nsection two 1: b\narrange one two\n";
        assert_eq!(load_text(&mut e, text), Ok(()));
        e.song_play();
        let hits = hit_steps(&mut e, 40);
        assert_eq!(hits, vec![0, 4, 8, 12, 16, 18, 20, 22, 24, 26, 28, 30]);
        assert!(!e.clock().playing(), "the song stops after its last bar");
        assert_eq!(e.clock().position(), 0, "and goes back to the top");
    }

    /// A fragment starts again at each section's first bar: a long one is cut,
    /// a short one loops inside it.
    #[test]
    fn a_fragment_restarts_with_its_section() {
        let mut e = kit(0);
        let long = format!("x{}x{}", ".".repeat(19), ".".repeat(11));
        let text = format!(
            "track kit drums\nfrag l = kit\n  bd {long}\nfrag s = kit\n  sn x..\nsection a 1: l s\narrange a a\n"
        );
        assert_eq!(load_text(&mut e, &text), Ok(()));
        e.song_play();
        let hits = hit_steps(&mut e, 32);
        // l: step 0 of each bar (its 20th step is cut); s: 0, 3, 6, 9, 12, 15 of each bar.
        let mut want = Vec::new();
        for bar in [0, 16] {
            want.extend([bar, bar, bar + 3, bar + 6, bar + 9, bar + 12, bar + 15]);
        }
        assert_eq!(hits, want);
    }

    /// The loop region repeats its bars; seek lands on a bar.
    #[test]
    fn the_loop_region_repeats_and_seek_lands_on_a_bar() {
        let mut e = kit(0);
        let text = "track kit drums\nfrag a = kit\n  bd x...............\nfrag b = kit\n  sn x...............\n\
                    section one 1: a\nsection two 1: b\narrange one two one\nloop 2 2\n";
        assert_eq!(load_text(&mut e, text), Ok(()));
        e.song_play();
        hit_steps(&mut e, 16);
        assert_eq!(e.song_place(), Some((0, 15)));
        hit_steps(&mut e, 48);
        assert_eq!(e.song_place(), Some((1, 15)), "bar 2 three times over");
        assert!(e.clock().playing());
        e.song_seek_bar(2);
        let before = e.note_count;
        e.render(1);
        assert_eq!(e.note_count, before + 1, "bar 3 starts at once");
        assert_eq!(
            e.song_place(),
            Some((1, 0)),
            "inside the loop: bar 3 wraps to bar 2"
        );
    }

    /// ADR-0015: a scene sets its values on the first sample of its section.
    #[test]
    fn a_scene_lands_on_its_sections_first_sample() {
        let mut e = kit(0);
        let text = "track kit drums\nfrag b = kit\n  bd x...\nscene s: strip1.Send2 0.25, master.P2Return 0.6\n\
                    section one 1: b\nsection two 1: b [s]\narrange one two\n";
        assert_eq!(load_text(&mut e, text), Ok(()));
        e.song_play();
        let before = e.param_value(0, Param::Send2);
        let mut changed = None;
        for s in 0..120_000u64 {
            e.render(1);
            if changed.is_none() && e.param_value(0, Param::Send2) != before {
                changed = Some(s);
            }
        }
        assert_eq!(changed, Some(96_000), "bar 2 begins at 16 steps of 6000");
        assert_eq!(e.param_value(0, Param::Send2), 0.25);
        assert_eq!(
            e.param_value(5, Param::P2Return),
            0.6,
            "a global reaches every row"
        );
        assert_eq!(e.take_touched() & 1, 1, "the view is told");
        assert_eq!(e.take_touched(), 0, "once");
    }

    /// A ramp climbs over its bars and reaches its end value at its end, a
    /// block at a time.
    #[test]
    fn a_ramp_reaches_its_end_value() {
        let mut e = kit(0);
        let text = "track kit drums\nfrag b = kit\n  bd x\nauto r = strip1.Send1 ramp 0 1 /1\n";
        assert_eq!(load_text(&mut e, text), Ok(()));
        e.song_play();
        let mut last = -1.0;
        for _ in 0..(96_000 / BLOCK) {
            e.render(BLOCK);
            let v = e.param_value(0, Param::Send1);
            assert!(v >= last, "{v} after {last}");
            last = v;
        }
        assert!(last > 0.99, "{last}");
        e.render(BLOCK);
        assert!(e.param_value(0, Param::Send1) < 0.01, "and loops");
    }

    /// Automating a mixer level is the same as setting it by hand at that block.
    #[test]
    fn automation_is_bit_identical_to_a_hand_set_value() {
        let song = |auto: bool| {
            format!(
                "track kit drums\nfrag b = kit\n  bd x...\n{}",
                if auto {
                    "auto l = strip1.Level 0.3 /1\n"
                } else {
                    ""
                }
            )
        };
        let mut a = kit(0);
        let mut b = kit(0);
        assert_eq!(load_text(&mut a, &song(true)), Ok(()));
        assert_eq!(load_text(&mut b, &song(false)), Ok(()));
        b.set_param(0, Param::Level, 0.3);
        a.song_play();
        b.song_play();
        for _ in 0..200 {
            a.render(BLOCK);
            b.render(BLOCK);
            assert_eq!(a.output(), b.output());
        }
        assert!(a.output().iter().any(|s| *s != 0.0) || a.note_count > 0);
    }

    #[test]
    fn each_lane_loops_on_its_own_length() {
        let mut e = kit(0);
        let text = "track kit drums\nfrag p = kit\n  bd x..\n  sn x...\n";
        assert_eq!(load_text(&mut e, text), Ok(()));
        e.song_play();
        // Twelve steps at 6000 samples: the kick on 0, 3, 6, 9; the snare on 0, 4, 8.
        run(&mut e, 72_000 / BLOCK);
        assert_eq!(e.note_count, 7);
    }

    #[test]
    fn a_bad_text_is_reported_and_the_song_plays_on() {
        let mut e = kit(0);
        assert_eq!(load_text(&mut e, FOUR), Ok(()));
        let good = e.song().clone();
        let bad = "tempo 120\ntrack kit drums\nfrag b = kit\n  bd x..o\n";
        let err = load_text(&mut e, bad).expect_err("o is not a step");
        assert_eq!((err.line, err.col), (4, 9));
        assert_eq!(e.song_error(), Some(err));
        assert_eq!(e.song(), &good);
        e.song_play();
        assert!(run(&mut e, 40) > 0.05, "the old beat still plays");
        // Not UTF-8: reported where the bad byte is.
        e.song_buffer(4).expect("fits").copy_from_slice(b"\n\nab");
        e.song_buf[3] = 0xff;
        let err = e.load_song().expect_err("not UTF-8");
        assert_eq!((err.line, err.col), (3, 2));
        assert_eq!(load_text(&mut e, FOUR), Ok(()));
        assert_eq!(e.song_error(), None);
    }

    #[test]
    fn the_song_sets_the_clock_and_tracks_find_a_kit() {
        let mut e = Engine::new(48_000.0);
        assert_eq!(
            load_text(&mut e, "tempo 90\nswing 60\ntrack kit drums\n"),
            Ok(())
        );
        assert_eq!((e.clock().tempo(), e.clock().swing()), (90.0, 60.0));
        assert_eq!(e.song_routed(0), None, "no kit, so the track is muted");
        e.preset(3, Preset::Kit808);
        e.song_route(0, None);
        assert_eq!(load_text(&mut e, FOUR), Ok(()));
        assert_eq!(e.song_routed(0), Some(3));
        e.song_route(0, Some(5));
        assert_eq!(load_text(&mut e, FOUR), Ok(()));
        assert_eq!(e.song_routed(0), Some(5), "a reload keeps the route");
        e.song_route(0, Some(99));
        assert_eq!(e.song_routed(0), None, "an unknown synth mutes");
    }

    #[test]
    fn set_step_edits_the_playing_song_and_its_text() {
        let mut e = kit(0);
        assert_eq!(load_text(&mut e, FOUR), Ok(()));
        assert!(e.set_step(0, 0, 2, 2));
        assert!(e.song_text().contains("  bd x.X.x...x...x...\n"));
        assert!(!e.set_step(0, 0, 2, 3), "no level 3");
        assert!(!e.set_step(0, 1, 0, 1), "no second lane");
        e.song_play();
        run(&mut e, 12_001 / BLOCK + 1);
        assert_eq!(e.note_count, 2, "the new step at 12000 plays");
    }

    #[test]
    fn tempo_and_swing_change_the_song_and_the_clock() {
        let mut e = kit(0);
        assert_eq!(load_text(&mut e, FOUR), Ok(()));
        e.set_song_tempo(90.5);
        e.set_song_swing(400.0);
        assert_eq!((e.clock().tempo(), e.clock().swing()), (90.5, 75.0));
        assert!(e.song_text().starts_with("tempo 90.5\nswing 75\n"));
        e.set_song_tempo(f32::NAN);
        assert_eq!(e.song().tempo, 90.5, "NaN is ignored");
    }

    /// A poly synth on slot 0 with the song loaded, playing; the sample at
    /// which the held notes change, with what they are, over `frames`.
    fn held_changes(text: &str, frames: u64) -> Vec<(u64, Vec<u8>)> {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.set_param(0, Param::Polyphony, 8.0);
        assert_eq!(load_text(&mut e, text), Ok(()));
        assert_eq!(e.song_routed(0), Some(0), "a synth track finds a synth");
        e.song_play();
        let mut now: Vec<u8> = Vec::new();
        let mut out = Vec::new();
        for s in 0..frames {
            e.render(1);
            let held = e.pools[0].held_notes();
            if held != now {
                out.push((s, held.clone()));
                now = held;
            }
        }
        out
    }

    /// #163: `"c4 e4 g4 c5"` at 120 BPM and 48 kHz starts a note every beat
    /// (24000 samples), each ending as the next begins.
    #[test]
    fn note_fragments_sound_at_their_samples_and_pitches() {
        let got = held_changes(
            "tempo 120\ntrack lead synth\nfrag r = lead\n  \"c4 e4 g4 c5\"\n",
            48_000 * 4,
        );
        assert_eq!(
            got,
            vec![
                (0, vec![60]),
                (24_000, vec![64]),
                (48_000, vec![67]),
                (72_000, vec![72]),
                (96_000, vec![60]),
                (120_000, vec![64]),
                (144_000, vec![67]),
                (168_000, vec![72]),
            ]
        );
    }

    /// ADR-0015 with ADR-0016: a note fragment plays only in its sections,
    /// from each section's first bar. Two bars of `c4 e4` (one bar long) in
    /// `b`, after a silent bar `a`: notes from 96000, starting over at
    /// 192000 rather than running on from where the line would be.
    #[test]
    fn a_note_fragment_plays_in_its_section_from_its_start() {
        let got = held_changes(
            "tempo 120\ntrack lead synth\nfrag r = lead\n  c4:2 e4:4 g4:4\nsection a 1:\nsection b 1: r\narrange a b b\n",
            300_000,
        );
        let starts: Vec<(u64, Vec<u8>)> = got.into_iter().filter(|(_, n)| !n.is_empty()).collect();
        assert_eq!(
            starts,
            vec![
                (96_000, vec![60]),
                (144_000, vec![64]),
                (168_000, vec![67]),
                (192_000, vec![60]),
                (240_000, vec![64]),
                (264_000, vec![67]),
            ]
        );
    }

    #[test]
    fn a_note_lasts_its_written_length() {
        // a quarter note, then a rest: held 0 to 24000.
        let got = held_changes(
            "tempo 120\ntrack lead synth\nfrag r = lead\n  c4:4 r:4 r:2\n",
            60_000,
        );
        assert_eq!(got, vec![(0, vec![60]), (24_000, vec![])]);
    }

    #[test]
    fn a_triplet_lands_between_the_sixteenths() {
        // three notes in a bar: ticks 0, 16 and 32, at 2000 samples a tick.
        let got = held_changes(
            "tempo 120\ntrack lead synth\nfrag r = lead\n  \"c4 d4 e4\"\n",
            96_000,
        );
        let starts: Vec<u64> = got.iter().map(|(s, _)| *s).collect();
        assert_eq!(starts, vec![0, 32_000, 64_000]);
    }

    #[test]
    fn a_chord_uses_the_voice_pool() {
        let got = held_changes(
            "tempo 120\ntrack lead synth\nfrag r = lead\n  \"[c4,e4,g4] ~\"\n",
            60_000,
        );
        assert!(got.iter().any(|(s, n)| *s == 0 && n == &vec![60, 64, 67]));
        assert_eq!(
            got.last().map(|(s, n)| (*s, n.clone())),
            Some((48_000, vec![]))
        );
    }

    #[test]
    fn stopping_the_song_ends_its_notes() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Polyphony, 8.0);
        let text = "track lead synth\nfrag r = lead\n  c4:1\n";
        assert_eq!(load_text(&mut e, text), Ok(()));
        e.song_play();
        for _ in 0..4 {
            e.render(BLOCK);
        }
        assert_eq!(e.pools[0].held(), 1);
        e.song_stop();
        assert_eq!(e.pools[0].held(), 0);
    }

    #[test]
    fn a_song_load_with_a_bad_note_keeps_the_old_one_playing() {
        let mut e = Engine::new(48_000.0);
        let good = "track lead synth\nfrag r = lead\n  c4:4\n";
        assert_eq!(load_text(&mut e, good), Ok(()));
        let bad = "track lead synth\nfrag r = lead\n  c4:4 x4:4\n";
        let err = load_text(&mut e, bad).unwrap_err();
        assert_eq!((err.line, err.col), (3, 8));
        assert_eq!(e.song_text(), Song::parse(good).unwrap().print());
    }

    /// #124: a song's drum track finds a pad sampler as it finds the 808, and its lanes
    /// hit the pads their General MIDI notes name on the clock's steps (bd is note 36).
    #[test]
    fn a_song_track_plays_a_pad_sampler_on_the_clock() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(2, Preset::PadsLoud);
        let tone: Vec<f32> = (0..24_000)
            .map(|i| (i as f32 / 100.0 * std::f32::consts::TAU).sin() * 0.9)
            .collect();
        let wav = crate::sample::test_wav(48_000, &tone, None);
        e.sample_buffer(wav.len())
            .expect("fits")
            .copy_from_slice(&wav);
        e.load_sample(0).expect("loads");
        e.set_pad(2, 0, crate::padsampler::PadField::Sample, 0.0);
        assert_eq!(load_text(&mut e, FOUR), Ok(()));
        assert_eq!(e.song_routed(0), Some(2), "the pad sampler takes the track");
        e.song_play();
        let heard = run(&mut e, 48_000 / 2 / BLOCK);
        assert!(heard > 0.05, "the kick lane hits pad 1");
        assert_eq!(e.pools[0].active(), 0, "nothing on synth 0");
    }

    /// #164: a `sampler` track with lanes goes to the pad sampler and hits its pad.
    #[test]
    fn a_sampler_track_with_lanes_plays_the_pad_sampler() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(2, Preset::PadsLoud);
        let tone: Vec<f32> = (0..24_000)
            .map(|i| (i as f32 / 100.0 * std::f32::consts::TAU).sin() * 0.9)
            .collect();
        let wav = crate::sample::test_wav(48_000, &tone, None);
        e.sample_buffer(wav.len())
            .expect("fits")
            .copy_from_slice(&wav);
        e.load_sample(0).expect("loads");
        e.set_pad(2, 0, crate::padsampler::PadField::Sample, 0.0);
        let text = FOUR.replace("track kit drums", "track kit sampler");
        assert_eq!(load_text(&mut e, &text), Ok(()));
        assert_eq!(e.song_routed(0), Some(2));
        e.song_play();
        assert!(
            run(&mut e, 48_000 / 2 / BLOCK) > 0.05,
            "the kick lane hits pad 1"
        );
    }

    /// #164: a `sampler` track with notes goes to the multisampler, at pitch.
    #[test]
    fn a_sampler_track_with_notes_plays_the_multisampler_at_pitch() {
        let mut e = Engine::new(48_000.0);
        e.preset(3, Preset::SamplerKeys);
        let text = "track keys sampler\nfrag r = keys\n  c4:1\n";
        assert_eq!(load_text(&mut e, text), Ok(()));
        assert_eq!(e.song_routed(0), Some(3));
        e.song_play();
        e.render(1);
        assert_eq!(e.pools[3].held_notes(), vec![60]);
    }

    /// #165: `bd euclid(3,8)` hits on steps 0, 3 and 6 of each eight, so at 120
    /// BPM and 48 kHz on samples 0, 18000, 36000, then again from 48000.
    #[test]
    fn a_euclid_lane_plays_like_a_written_one() {
        let mut e = kit(0);
        let text = "tempo 120\ntrack kit drums\nfrag b = kit\n  bd euclid(3,8)\n";
        assert_eq!(load_text(&mut e, text), Ok(()));
        e.song_play();
        let mut hits = Vec::new();
        let mut count = e.note_count;
        for s in 0..60_000u64 {
            e.render(1);
            if e.note_count != count {
                count = e.note_count;
                hits.push(s);
            }
        }
        assert_eq!(hits, vec![0, 18_000, 36_000, 48_000]);
    }

    /// #165: a euclid note line plays the same events on every run.
    #[test]
    fn a_euclid_note_line_walks_the_scale_deterministically() {
        let text =
            "tempo 120\nscale c minor\ntrack lead synth\nfrag r = lead\n  euclid(4,8) scale c4\n";
        let a = held_changes(text, 96_000);
        let b = held_changes(text, 96_000);
        assert_eq!(a, b);
        let notes: Vec<Vec<u8>> = a
            .into_iter()
            .map(|(_, n)| n)
            .filter(|n| !n.is_empty())
            .collect();
        assert_eq!(notes, vec![vec![60], vec![62], vec![63], vec![65]]);
    }

    const LIVE: &str =
        "tempo 120\nscale c minor\ntrack lead synth\nfrag w = lead live\n  walk(c4,8,1)\n";

    /// The notes the song starts, in order, over `frames`: each new entry of
    /// the note-off table is one start (a repeated note ends at another tick).
    fn started(e: &mut Engine, frames: u64) -> Vec<u8> {
        let mut out = Vec::new();
        let mut seen: Vec<(u64, u8, u8)> = e.note_offs.iter().flatten().copied().collect();
        for _ in 0..frames {
            e.render(1);
            for entry in e.note_offs.iter().flatten() {
                if !seen.contains(entry) {
                    seen.push(*entry);
                    out.push(entry.2);
                }
            }
            seen.retain(|s| e.note_offs.iter().flatten().any(|x| x == s));
        }
        out
    }

    fn poly() -> Engine {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Polyphony, 8.0);
        e
    }

    /// #167: a live fragment plays a new walk each bar, the same ones every run.
    #[test]
    fn a_live_fragment_changes_each_cycle_and_repeats_each_run() {
        let mut e = poly();
        assert_eq!(load_text(&mut e, LIVE), Ok(()));
        e.song_play();
        let played = started(&mut e, 96_000 * 3);
        let Some(Seq::Generated(call)) = e.song().frags[0].notes.as_ref().map(|n| n.seq.clone())
        else {
            panic!("a generated frag");
        };
        let scale = e.song().scale;
        let mut want = Vec::new();
        for cycle in 0..3 {
            let evs = call.events(cycle_seed(call.seed(), cycle), scale.as_ref());
            want.extend(evs.iter().map(|ev| ev.note));
        }
        assert_eq!(played.len(), 24);
        assert_eq!(played, want);
        assert_ne!(played[..8], played[8..16], "the bars differ");
        let mut again = poly();
        assert_eq!(load_text(&mut again, LIVE), Ok(()));
        again.song_play();
        assert_eq!(started(&mut again, 96_000 * 3), played);
    }

    /// #167: nothing grows in `render`: the buffers keep the room reserved at load.
    #[test]
    fn a_live_fragment_does_not_grow_its_buffers() {
        let mut e = poly();
        assert_eq!(load_text(&mut e, LIVE), Ok(()));
        let room = (e.live[0].cur.capacity(), e.live[0].nxt.capacity());
        assert!(room.0 >= 8 && room.1 >= 8);
        e.song_play();
        for _ in 0..(96_000 * 6 / BLOCK) {
            e.render(BLOCK);
        }
        assert_eq!((e.live[0].cur.capacity(), e.live[0].nxt.capacity()), room);
        assert_eq!(e.live[0].cycle, Some(5));
    }

    /// #167: freezing prints the bar it was playing; parsing that plays the same.
    #[test]
    fn freezing_keeps_the_bar_that_was_playing() {
        let mut e = poly();
        assert_eq!(load_text(&mut e, LIVE), Ok(()));
        e.song_play();
        let first = started(&mut e, 96_000);
        let second = started(&mut e, 48_000); // half way into bar 2
        assert_eq!(first.len() + second.len(), 12);
        assert!(e.freeze(0));
        assert!(!e.song().frags[0].live);
        assert!(!e.song_text().contains("live") && !e.song_text().contains("walk("));
        let frozen: Vec<u8> = e.song().frags[0]
            .notes
            .as_ref()
            .unwrap()
            .events
            .iter()
            .map(|ev| ev.note)
            .collect();
        assert_eq!(&frozen[..4], &second[..], "the notes it had played so far");
        // Parsing the printed text plays the same bar, every bar.
        let text = e.song_text().to_string();
        let mut other = poly();
        assert_eq!(load_text(&mut other, &text), Ok(()));
        other.song_play();
        let again = started(&mut other, 96_000 * 2);
        assert_eq!([frozen.clone(), frozen].concat(), again);
        assert!(!e.freeze(0), "a frozen frag has no call to freeze");
    }

    /// #124: channel 10 plays on a drum/pad sampler slot too: pad 2 answers note 38.
    #[test]
    fn channel_ten_plays_on_a_pad_sampler_slot() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(2, Preset::PadsLoud);
        let tone: Vec<f32> = (0..24_000)
            .map(|i| (i as f32 / 100.0 * std::f32::consts::TAU).sin() * 0.9)
            .collect();
        let wav = crate::sample::test_wav(48_000, &tone, None);
        e.sample_buffer(wav.len())
            .expect("fits")
            .copy_from_slice(&wav);
        e.load_sample(0).expect("loads");
        e.set_pad(2, 2, crate::padsampler::PadField::Sample, 0.0);
        let mut file_bytes = one_note(9);
        // The note in the file is 60; make it the snare's 38.
        let at = file_bytes
            .windows(3)
            .position(|w| w == [0x99, 60, 100])
            .expect("note on");
        file_bytes[at + 1] = 38;
        let off = file_bytes
            .windows(3)
            .position(|w| w == [0x89, 60, 0])
            .expect("note off");
        file_bytes[off + 1] = 38;
        assert_eq!(load(&mut e, &file_bytes), Ok(1));
        e.route(9, Some(2));
        e.play();
        let heard = run(&mut e, 48_000 * 3 / 4 / BLOCK);
        assert!(heard > 0.05, "the snare pad at 0.5 s");
        assert_eq!(e.pools[0].active(), 0, "nothing on synth 0");
    }

    /// One clap on a kit at synth 0, the meters read after `blocks`.
    fn clap_meters(setup: &dyn Fn(&mut Engine)) -> (Vec<f32>, Vec<f32>) {
        let mut e = kit(0);
        setup(&mut e);
        e.clear_meters();
        e.note_on(0, 39, 0.8);
        let mut out = Vec::new();
        for _ in 0..20 {
            e.render(BLOCK);
            out.extend_from_slice(e.output());
        }
        (e.meters().to_vec(), out)
    }

    const GROUP_3: usize = SYNTHS + 2;

    /// #162: a pad on a group is heard only through that group, and its
    /// controls apply; a pad on Main sounds exactly as before.
    #[test]
    fn a_pad_on_a_group_goes_only_through_that_group() {
        let (main_meters, main_out) = clap_meters(&|_| {});
        assert!(main_meters[0] > 0.0 && main_meters[GROUP_3] == 0.0);
        let (meters, out) = clap_meters(&|e| e.set_param(0, Param::CpOut, 3.0));
        assert_eq!(meters[0], 0.0, "not on the kit's strip");
        assert!(meters[GROUP_3] > 0.0, "on group 3");
        assert!(
            out.iter().any(|x| *x != 0.0) && out.iter().all(|x| x.is_finite() && x.abs() <= 1.0)
        );
        // The group's fader and mute apply; the kit's own mute does not.
        let (_, down) = clap_meters(&|e| {
            e.set_param(0, Param::CpOut, 3.0);
            e.set_param(GROUP_3, Param::Level, 0.0);
        });
        assert!(down.iter().all(|x| *x == 0.0));
        let (_, kit_muted) = clap_meters(&|e| {
            e.set_param(0, Param::CpOut, 3.0);
            e.set_param(0, Param::Mute, 1.0);
        });
        assert_eq!(kit_muted, out, "an individual out bypasses the kit's strip");
        // Back on Main, bit for bit as a fresh kit.
        let (_, back) = clap_meters(&|e| {
            e.set_param(0, Param::CpOut, 3.0);
            e.set_param(0, Param::CpOut, 0.0);
        });
        assert_eq!(back, main_out);
    }

    #[test]
    fn a_pad_is_panned_into_its_group() {
        let (_, hard_left) = clap_meters(&|e| {
            e.set_param(0, Param::CpOut, 3.0);
            e.set_param(0, Param::CpPan, -1.0);
        });
        let right: Vec<f32> = hard_left
            .chunks(BLOCK)
            .skip(1)
            .step_by(2)
            .flatten()
            .copied()
            .collect();
        let left: Vec<f32> = hard_left
            .chunks(BLOCK)
            .step_by(2)
            .flatten()
            .copied()
            .collect();
        assert!(left.iter().any(|x| *x != 0.0));
        assert!(
            right.iter().all(|x| x.abs() < 1.0e-6),
            "nothing on the right"
        );
    }

    #[test]
    fn solos_follow_a_kits_individual_outs() {
        let heard = |setup: &dyn Fn(&mut Engine)| clap_meters(setup).1.iter().any(|x| *x != 0.0);
        assert!(
            heard(&|e| {
                e.set_param(0, Param::CpOut, 3.0);
                e.set_param(0, Param::Solo, 1.0);
            }),
            "soloing the kit keeps the group its clap goes to"
        );
        assert!(
            !heard(&|e| {
                e.set_param(0, Param::CpOut, 3.0);
                e.set_param(1, Param::Solo, 1.0);
            }),
            "soloing another synth silences it"
        );
        assert!(
            heard(&|e| {
                e.set_param(0, Param::CpOut, 3.0);
                e.set_param(GROUP_3, Param::Solo, 1.0);
            }),
            "soloing the group plays it"
        );
    }

    /// A carrier on synth 0 with a vocoder keyed to synth 1, both playing.
    fn vocoded(setup: &dyn Fn(&mut Engine)) -> Vec<f32> {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.set_param(0, Param::I1Type, 6.0);
        e.set_param(0, Param::Key, 2.0);
        setup(&mut e);
        e.note_on(0, 48, 1.0);
        e.note_on(1, 60, 1.0);
        let mut out = Vec::new();
        for _ in 0..40 {
            e.render(BLOCK);
            assert!(e.output().iter().all(|x| x.is_finite()));
            out.extend_from_slice(e.output());
        }
        out
    }

    fn loud(x: &[f32]) -> f32 {
        x.iter().fold(0.0_f32, |m, v| m.max(v.abs()))
    }

    /// #161: the vocoder follows its key's raw signal, which a muted or
    /// unrouted key strip still gives; without a key, or keyed to itself, it is silent.
    #[test]
    fn a_vocoder_follows_its_key_even_muted_or_routed_nowhere() {
        let both = vocoded(&|_| {});
        assert!(loud(&both) > 0.01);
        let muted_key = vocoded(&|e| e.set_param(1, Param::Mute, 1.0));
        assert!(loud(&muted_key) > 0.01, "the key is read before its mute");
        let hidden_key = vocoded(&|e| e.set_param(1, Param::Out, 9.0));
        assert!(loud(&hidden_key) > 0.01, "and before its Out");
        // With the key muted only the vocoded carrier is heard.
        let silent_key = vocoded(&|e| {
            e.set_param(1, Param::Mute, 1.0);
            e.set_param(0, Param::Key, 0.0);
        });
        assert!(silent_key.iter().all(|x| *x == 0.0), "no key, no sound");
        let own = vocoded(&|e| {
            e.set_param(1, Param::Mute, 1.0);
            e.set_param(0, Param::Key, 1.0);
        });
        assert!(own.iter().all(|x| *x == 0.0), "a strip cannot key itself");
    }

    /// #161: Out None takes a strip out of the mix, but not out of its sends.
    #[test]
    fn a_strip_routed_nowhere_leaves_no_trace_but_feeds_its_sends() {
        let render = |setup: &dyn Fn(&mut Engine)| {
            let mut e = Engine::new(48_000.0);
            setup(&mut e);
            e.note_on(0, 57, 1.0);
            let mut out = Vec::new();
            let mut sent = 0.0_f32;
            for _ in 0..20 {
                e.render(BLOCK);
                out.extend_from_slice(e.output());
                sent = sent.max(loud(&e.mixer.sends[0]));
            }
            (out, sent, e.meters()[0])
        };
        let (silent, _, _) = render(&|_| {});
        let (out, sent, meter) = render(&|e| {
            e.set_param(0, Param::Out, 9.0);
            e.set_param(0, Param::Send1, 1.0);
            e.set_param(0, Param::P1Type, 0.0);
        });
        assert!(loud(&silent) > 0.0);
        assert!(out.iter().all(|x| *x == 0.0), "nothing reaches the master");
        assert!(sent > 0.0, "but the send is fed");
        assert!(meter > 0.0, "and the strip's meter shows it");
    }

    /// #148: a beat written for the 808, pads the 909 lacks included, plays on a
    /// TR-909 slot; every hit is heard and nothing passes full scale.
    #[test]
    fn an_808_beat_plays_on_a_909() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(0, Preset::Kit909);
        let beat = "tempo 120\ntrack kit drums\nfrag b = kit /16\n  bd x...\n  sn .x..\n  cl ..x.\n  cb ...x\n  ma x...\n  lc .x..\n  cy ..x.\n  cr ...X\n";
        assert_eq!(load_text(&mut e, beat), Ok(()));
        assert_eq!(
            e.song_routed(0),
            Some(0),
            "the first kit, a 909, plays the track"
        );
        e.song_play();
        let heard = run(&mut e, 48_000 / BLOCK);
        assert!(heard > 0.05);
        // One second at 120 BPM is eight sixteenths: each four-step lane twice.
        assert_eq!(e.note_count, 8 * 2, "eight lanes, one hit each per pass");
    }

    /// Press C-E-G on synth 0 with its arp on, then render one frame at a
    /// time and return (sample, note) for every gate rising and (sample) for
    /// every fall.
    fn arp_run(e: &mut Engine, frames: u64) -> (Vec<(u64, u8)>, Vec<u64>) {
        let (mut ons, mut offs) = (Vec::new(), Vec::new());
        let mut gate = false;
        for s in 0..frames {
            e.render(1);
            let v = e.voice(Owner::Live(0));
            let g = v.is_some_and(|v| v.gated());
            if g && !gate {
                ons.push((s, v.map_or(0, |v| v.note())));
            }
            if !g && gate {
                offs.push(s);
            }
            gate = g;
        }
        (ons, offs)
    }

    fn arp_on(e: &mut Engine) {
        e.set_param(0, Param::ArpOn, 1.0);
        for n in [60, 64, 67] {
            e.note_on(0, n, 1.0);
        }
    }

    #[test]
    fn the_arp_steps_on_the_clock_from_the_next_step() {
        let mut e = Engine::new(48_000.0);
        e.song_play();
        e.render(1); // step 0 has fired; the next is at 6000
        arp_on(&mut e);
        let (ons, _) = arp_run(&mut e, 29_000);
        let at: Vec<u64> = ons.iter().map(|(s, _)| s + 1).collect();
        assert_eq!(at, [6000, 12_000, 18_000, 24_000]);
        let notes: Vec<u8> = ons.iter().map(|(_, n)| *n).collect();
        assert_eq!(notes, [60, 64, 67, 60]);
    }

    #[test]
    fn the_gate_is_a_fraction_of_the_step() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::ArpRate, 0.0); // 1/8: 12000 samples
        e.set_param(0, Param::ArpGate, 0.5);
        e.song_play();
        e.render(1);
        arp_on(&mut e);
        let (ons, offs) = arp_run(&mut e, 14_000);
        assert_eq!(ons.first().map(|(s, _)| s + 1), Some(12_000));
        assert_eq!(offs.len(), 0, "the first note is still on at 14000");
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::ArpRate, 0.0);
        e.set_param(0, Param::ArpGate, 0.5);
        e.song_play();
        e.render(1);
        arp_on(&mut e);
        let (_, offs) = arp_run(&mut e, 24_000);
        // On at 12000 for half a step: the gate ends 6000 samples later (the
        // voice's release shows as the gate dropping).
        assert_eq!(offs.first().map(|s| s + 1), Some(18_000));
    }

    #[test]
    fn latch_keeps_playing_after_the_keys_go() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::ArpLatch, 1.0);
        e.song_play();
        e.render(1);
        arp_on(&mut e);
        for n in [60, 64, 67] {
            e.note_off(0, n);
        }
        let (ons, _) = arp_run(&mut e, 20_000);
        assert_eq!(ons.len(), 3, "{ons:?}");
        // A new chord replaces it.
        e.note_on(0, 72, 1.0);
        let (ons, _) = arp_run(&mut e, 20_000);
        // The first entry is the old chord's note still gated when the run begins.
        assert!(ons.iter().skip(1).all(|(_, n)| *n == 72), "{ons:?}");
    }

    #[test]
    fn a_stopped_clock_silences_the_arp_unless_it_runs_free() {
        let mut e = Engine::new(48_000.0);
        arp_on(&mut e);
        let (ons, _) = arp_run(&mut e, 30_000);
        assert!(ons.is_empty());
        e.set_param(0, Param::ArpFree, 1.0);
        let (ons, _) = arp_run(&mut e, 30_000);
        let gaps: Vec<u64> = ons.windows(2).map(|w| w[1].0 - w[0].0).collect();
        assert!(ons.len() >= 4);
        assert!(gaps.iter().all(|g| *g == 6000), "{gaps:?}");
    }

    #[test]
    fn turning_the_arp_off_lets_go_and_live_input_plays_again() {
        let mut e = Engine::new(48_000.0);
        e.song_play();
        arp_on(&mut e);
        for _ in 0..200 {
            e.render(BLOCK);
        }
        e.set_param(0, Param::ArpOn, 0.0);
        e.render(BLOCK);
        assert!(!e.voice(Owner::Live(0)).is_some_and(|v| v.gated()));
        e.note_on(0, 60, 1.0);
        e.render(BLOCK);
        assert!(e.voice(Owner::Live(0)).is_some_and(|v| v.gated()));
    }

    #[test]
    fn the_arp_does_not_grow_its_buffers() {
        let mut e = Engine::new(48_000.0);
        e.song_play();
        arp_on(&mut e);
        let before = e.arps.len();
        for _ in 0..500 {
            e.render(BLOCK);
        }
        assert_eq!(e.arps.len(), before);
    }
}
