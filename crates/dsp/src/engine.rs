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
    At, Kind, MAX_AUTOS, MAX_MODS, MAX_TEXT, MAX_TRACKS, Mix, MixLine, STEPS_PER_BAR, Song,
    SongError, Step, Target, signal,
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

/// A parsed song waiting for the bar line (`Engine::load_song`).
struct Pending {
    song: Song,
    live: Vec<Live>,
    route: [Option<usize>; MAX_TRACKS],
}

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
    /// While a MIDI file is imported its parts keep the synths they play on.
    keep_synths: bool,
    /// Notes of the song waiting for their note-off, as (tick, track, note):
    /// a fixed table, so the clock can end a note without allocating.
    note_offs: [Option<(u64, u8, u8)>; NOTE_OFFS],
    /// One per fragment of the song; empty for all but the live ones.
    live: Vec<Live>,
    /// A song loaded while the song plays, ready to take over at the next
    /// bar with its live buffers and track routes (#208).
    pending: Option<Pending>,
    /// The song and live buffers it replaced, kept so `render` never frees
    /// them; the next load drops them.
    spent: Option<(Song, Vec<Live>)>,
    /// A pending song took over on a bar line since the view last asked.
    taken: bool,
    /// The value each automation lane last wrote, so it writes only changes
    /// and a hand on a knob holds until the next one (ADR-0015).
    auto_last: [f32; MAX_AUTOS],
    /// The same for each modulation (ADR-0019).
    mod_last: [f32; MAX_MODS],
    /// The value each modulation found when it began to write, to put back
    /// when it stops; `None` while it is not writing (ADR-0019).
    mod_base: [Option<f32>; MAX_MODS],
    /// The state of the song's `lag` nodes, NaN until each first runs.
    mod_state: [f32; signal::MAX_NODES],
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
            keep_synths: false,
            note_offs: [None; NOTE_OFFS],
            live: Vec::new(),
            pending: None,
            spent: None,
            taken: false,
            auto_last: [f32::NAN; MAX_AUTOS],
            mod_last: [f32::NAN; MAX_MODS],
            mod_base: [None; MAX_MODS],
            mod_state: [f32::NAN; signal::MAX_NODES],
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
            // The pads' outs feed groups directly; the solos follow them (#220).
            if field == PadField::Out {
                self.mixer.set_feeds(synth, p.pad_groups());
            }
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
            self.mixer.set_feeds(synth, p.pad_groups());
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
        self.commit_song();
        self.hand_arps_over();
        self.release_song_notes();
        self.clock.stop();
        self.clock.seek(0);
        // Each modulation puts back the value it found.
        for m in 0..MAX_MODS {
            let target = self.song.mods.get(m).map(|md| (md.target, md.param));
            let base = self.mod_base.get_mut(m).and_then(Option::take);
            if let (Some((t, p)), Some(base)) = (target, base) {
                self.automate(t, p, base);
            }
        }
        // From the top every lane and modulation writes again.
        self.auto_last = [f32::NAN; MAX_AUTOS];
        self.mod_last = [f32::NAN; MAX_MODS];
        self.mod_state = [f32::NAN; signal::MAX_NODES];
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
            if k % STEPS_PER_BAR == 0 {
                self.commit_song();
            }
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
        self.live = Self::live_for(&self.song);
    }

    /// The live buffers of `song`'s fragments, sized for their generators.
    fn live_for(song: &Song) -> Vec<Live> {
        song.frags
            .iter()
            .map(|fr| match fr.notes.as_ref().map(|n| &n.seq) {
                Some(Seq::Generated(call)) if fr.live => Live::with_room(call.max_events()),
                _ => Live::default(),
            })
            .collect()
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
        self.commit_song();
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
        self.commit_song();
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
                    self.mods_again(t, p);
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
            self.mods_again(target, param);
        }
    }

    /// A modulation of a parameter something else just wrote writes again at
    /// the next block, even unchanged: it goes last (ADR-0019).
    fn mods_again(&mut self, target: Target, param: Param) {
        for (m, last) in self.song.mods.iter().zip(self.mod_last.iter_mut()) {
            if m.target == target && m.param == param {
                *last = f32::NAN;
            }
        }
    }

    /// The modulations at the clock's position, once per block, after the
    /// lanes: each signal is evaluated at the song's position in bars and
    /// writes only when its value changed (ADR-0019). A fragment's methods
    /// write while it plays (#204); a modulation that begins to write keeps
    /// the value it found and puts it back when it stops.
    fn run_mods(&mut self, frames: usize) {
        if !self.clock.playing() || self.song.mods.is_empty() {
            return;
        }
        let pos = self.clock.step_position().max(0.0);
        let section = match self.song.at(pos.floor() as u64) {
            At::Free(_) => None,
            At::In { section, .. } => Some(section),
            At::End => return,
        };
        let t = pos / STEPS_PER_BAR as f64;
        let cps = f64::from(self.clock.tempo()) / 240.0;
        let dt = frames as f32 / self.sample_rate;
        for m in 0..self.song.mods.len().min(MAX_MODS) {
            let Some(md) = self.song.mods.get(m) else {
                continue;
            };
            let (target, param) = (md.target, md.param);
            let playing = md.frag.is_none_or(|f| {
                section.is_none_or(|s| {
                    self.song
                        .sections
                        .get(s)
                        .is_some_and(|sec| sec.frags.contains(&f))
                })
            });
            if !playing {
                if let Some(base) = self.mod_base.get_mut(m).and_then(Option::take) {
                    if let Some(last) = self.mod_last.get_mut(m) {
                        *last = f32::NAN;
                    }
                    self.automate(target, param, base);
                }
                continue;
            }
            let found = self.target_value(target, param);
            if let Some(base @ None) = self.mod_base.get_mut(m) {
                *base = Some(found);
            }
            let mut ctx = signal::Ctx {
                cps,
                dt,
                state: &mut self.mod_state,
            };
            let v = md.signal.eval(t, &mut ctx);
            match self.mod_last.get_mut(m) {
                Some(last) if *last != v => *last = v,
                _ => continue,
            }
            self.automate(target, param, v);
        }
    }

    /// The value a target's parameter has now; 0 for an unrouted track.
    fn target_value(&self, target: Target, param: Param) -> f32 {
        let strip = match target {
            Target::Master => Some(0),
            Target::Strip(s) => Some(s),
            Target::Track(t) => self.song_routed(t),
        };
        strip.map_or(0.0, |s| self.param_value(s, param))
    }

    /// The strips automation changed since the last call (bit per strip), cleared.
    pub fn take_touched(&mut self) -> u32 {
        std::mem::take(&mut self.touched)
    }

    // --- The song (spec 002 Req 6, ADR-0012) -------------------------------

    /// Take over the pending song, if any: only moves and copies, so `render`
    /// may call it on a bar line (ADR-0002). The old song waits in `spent`.
    fn commit_song(&mut self) {
        let Some(p) = self.pending.take() else {
            return;
        };
        let song = std::mem::replace(&mut self.song, p.song);
        let live = std::mem::replace(&mut self.live, p.live);
        self.spent = Some((song, live));
        self.taken = true;
        self.song_route = p.route;
        self.clock.set_tempo(self.song.tempo);
        self.clock.set_swing(self.song.swing);
        self.auto_last = [f32::NAN; MAX_AUTOS];
        self.mod_last = [f32::NAN; MAX_MODS];
        self.mod_state = [f32::NAN; signal::MAX_NODES];
        // A modulation the new song keeps keeps the value it found; one it
        // drops puts that value back.
        let old = std::mem::replace(&mut self.mod_base, [None; MAX_MODS]);
        let mut restore = [None; MAX_MODS];
        if let Some((spent, _)) = &self.spent {
            for (o, md) in spent.mods.iter().enumerate() {
                let Some(base) = old.get(o).copied().flatten() else {
                    continue;
                };
                let kept = self.song.mods.iter().position(|n| {
                    n.target == md.target
                        && n.param == md.param
                        && n.frag.is_some() == md.frag.is_some()
                });
                match kept.and_then(|n| self.mod_base.get_mut(n)) {
                    Some(slot) if slot.is_none() => *slot = Some(base),
                    _ => {
                        if let Some(r) = restore.get_mut(o) {
                            *r = Some((md.target, md.param, base));
                        }
                    }
                }
            }
        }
        for (t, p, v) in restore.into_iter().flatten() {
            self.automate(t, p, v);
        }
    }

    /// Whether a song loaded while playing took over since the last call, so
    /// the view can draw it; cleared.
    pub fn take_taken(&mut self) -> bool {
        std::mem::take(&mut self.taken)
    }

    /// Size the song text buffer and hand it out; `None` when too long.
    pub fn song_buffer(&mut self, len: usize) -> Option<&mut [u8]> {
        if len > MAX_TEXT {
            return None;
        }
        self.song_buf.clear();
        self.song_buf.resize(len, 0);
        Some(&mut self.song_buf)
    }

    /// Parse the buffer and play it: at once when the song is stopped, else
    /// from the next bar (#208), keeping each lane's place against the clock.
    /// A text that does not parse leaves the song playing and is reported
    /// (`song_error`). Tempo and swing go to the clock with the song; a track
    /// keeps its synth, or goes to the first drum kit (the 808 or a pad
    /// sampler). Patches and mixer lines are set at once: they go to synths
    /// the old song does not play, or are a hand on a knob.
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
            Ok(mut song) => {
                // Where each track played before, to know which strips are new to a track.
                let before = self.song_route;
                // The routes the new song takes over with.
                let mut route = self.song_route;
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
                for t in song.tracks.len()..MAX_TRACKS {
                    if let Some(r) = route.get_mut(t) {
                        *r = None;
                    }
                }
                for t in 0..song.tracks.len() {
                    let routed = route.get(t).copied().flatten();
                    let free = |s: &usize| !route.contains(&Some(*s));
                    let model_of = |s: usize| self.synths.get(s).map(|p| p.model);
                    // A drum track whose kit was picked plays a kit already in
                    // the rack (a 909, a pad sampler) before an 808 is made of
                    // another synth, and the text then names that kit.
                    let picked_kit = song
                        .tracks
                        .get(t)
                        .is_some_and(|tr| tr.picked && tr.kind == Kind::Drums);
                    let same = song.patch(t).and_then(|(preset, _)| {
                        (0..SYNTHS)
                            .filter(free)
                            .find(|s| model_of(*s) == Some(preset.model()))
                    });
                    if routed.is_none() && same.is_none() && picked_kit {
                        let rack = (0..SYNTHS).filter(free).find(|s| {
                            self.synths
                                .get(*s)
                                .is_some_and(|p| p.model.uses_drums() || p.model.uses_pads())
                        });
                        if let Some(m) = rack.and_then(model_of) {
                            song.pick_on(t, m);
                        }
                    }
                    let Some(track) = song.tracks.get(t) else {
                        continue;
                    };
                    let patch = song.patch(t);
                    // A track with a patch gets a synth of its own and the patch
                    // on it (#210): one already on the patch's model (it keeps its
                    // samples), else the first free one. The patch is set again
                    // only when the text changes it.
                    if let Some((preset, sets)) = patch {
                        let synth = routed.or_else(|| {
                            (0..SYNTHS)
                                .filter(free)
                                .find(|s| model_of(*s) == Some(preset.model()))
                                .or_else(|| (0..SYNTHS).find(free))
                        });
                        let changed = routed.is_none() || self.song.patch(t) != patch;
                        if let Some(s) = synth.filter(|_| changed && !self.keep_synths) {
                            self.preset(s, preset);
                            for (p, v) in sets {
                                self.set_param(s, *p, *v);
                            }
                        }
                        if let Some(r) = route.get_mut(t) {
                            *r = synth;
                        }
                    } else if routed.is_none() {
                        let notes = song.frags.iter().any(|f| f.track == t && f.notes.is_some());
                        let synth = match track.kind {
                            Kind::Synth => voiced,
                            Kind::Sampler if notes => multi.or(voiced),
                            _ => kit,
                        };
                        if let Some(r) = route.get_mut(t) {
                            *r = synth;
                        }
                    }
                }
                self.apply_mix(&song, &before, &route);
                self.song_text = song.print();
                self.song_error = None;
                self.spent = None;
                self.pending = Some(Pending {
                    live: Self::live_for(&song),
                    song,
                    route,
                });
                if !self.clock.playing() {
                    self.commit_song();
                    // The view hears of this load from its answer.
                    self.taken = false;
                }
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
        self.keep_synths = true;
        let loaded = self.load_song();
        self.keep_synths = false;
        loaded.map_err(|_| -9)?;
        self.commit_song();
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
        self.commit_song();
        let Some(to) = Step::from_level(level) else {
            return false;
        };
        if !self.song.set_step(frag, lane, step, to) {
            return false;
        }
        self.song_text = self.song.print();
        true
    }

    /// Set the mixer lines of `song` (ADR-0018) that are new: a value whose
    /// text changed since the song playing, or on a strip its track has just
    /// moved to. A value the text keeps is left as it is, so a fader moved by
    /// hand holds; a line taken out changes nothing.
    fn apply_mix(
        &mut self,
        song: &Song,
        before: &[Option<usize>; MAX_TRACKS],
        route: &[Option<usize>; MAX_TRACKS],
    ) {
        for line in &song.mix {
            let (strip, moved) = match line.at {
                Mix::Track(t) => {
                    let now = route.get(t).copied().flatten();
                    (now, before.get(t).copied().flatten() != now)
                }
                Mix::Strip(i) => (Some(i), false),
                Mix::Group(g) => (Some(SYNTHS + g), false),
                Mix::Master => (Some(0), false),
            };
            let Some(strip) = strip else {
                continue;
            };
            for (p, v) in &line.sets {
                let was = match line.at {
                    // A track's old line, if it is the same track.
                    Mix::Track(t) => self
                        .song
                        .tracks
                        .get(t)
                        .filter(|old| song.tracks.get(t).is_some_and(|new| new.name == old.name))
                        .and_then(|_| self.song.mix_value(line.at, *p)),
                    at => self.song.mix_value(at, *p),
                };
                if moved || was != Some(*v) {
                    self.set_param(strip, *p, *v);
                }
            }
        }
    }

    /// Print the mixer as it is into the song (ADR-0018, Write mixer to
    /// song): a line for each track's strip, each other strip and group, and
    /// the master, holding what differs from the defaults. The song's old
    /// mixer lines are replaced; a group keeps its name.
    pub fn write_mixer(&mut self) {
        self.commit_song();
        let differs = |e: &Engine, strip: usize, defaults: &[(Param, f32)]| -> Vec<(Param, f32)> {
            defaults
                .iter()
                .filter_map(|(p, d)| {
                    let v = e.param_value(strip, *p);
                    ((v - p.clamp(*d)).abs() > 1e-6).then_some((*p, v))
                })
                .collect()
        };
        let mut mix = Vec::new();
        let mut claimed = [false; SYNTHS];
        for t in 0..self.song.tracks.len() {
            if let Some(s) = self.song_routed(t).filter(|s| *s < SYNTHS) {
                if let Some(c) = claimed.get_mut(s) {
                    *c = true;
                }
                let sets = differs(self, s, &STRIP_DEFAULTS);
                if !sets.is_empty() {
                    mix.push(MixLine {
                        at: Mix::Track(t),
                        name: None,
                        sets,
                    });
                }
            }
        }
        for (s, taken) in claimed.iter().enumerate() {
            let sets = differs(self, s, &STRIP_DEFAULTS);
            if !taken && !sets.is_empty() {
                mix.push(MixLine {
                    at: Mix::Strip(s),
                    name: None,
                    sets,
                });
            }
        }
        for g in 0..crate::mixer::GROUPS {
            let name = self
                .song
                .mix
                .iter()
                .find(|m| m.at == Mix::Group(g))
                .and_then(|m| m.name.clone());
            let sets = differs(self, SYNTHS + g, &STRIP_DEFAULTS);
            if !sets.is_empty() || name.is_some() {
                mix.push(MixLine {
                    at: Mix::Group(g),
                    name,
                    sets,
                });
            }
        }
        let sets = differs(self, 0, &GLOBAL_DEFAULTS);
        if !sets.is_empty() {
            mix.push(MixLine {
                at: Mix::Master,
                name: None,
                sets,
            });
        }
        self.song.mix = mix;
        self.song_text = self.song.print();
    }

    /// A track edit from the composer (#213): 0 plays factory preset `a`, 1
    /// plays the song's setting `a`, 2 saves the synth's sound as a new
    /// setting that the track plays. The track's synth takes the patch at once
    /// (a reload sets a patch only when its text changes) and the song is
    /// printed again; false when refused.
    pub fn track_edit(&mut self, op: u32, t: u32, a: u32) -> bool {
        self.commit_song();
        let t = t as usize;
        let ok = match op {
            0 => Preset::from_id(a).is_some_and(|p| self.song.set_track_preset(t, p)),
            1 => self.song.set_track_setting(t, a as usize),
            2 => {
                let sets = self.changed_params(t);
                sets.is_some_and(|sets| self.song.add_setting(t, sets).is_some())
            }
            _ => false,
        };
        if !ok {
            return false;
        }
        if op != 2 {
            if let (Some(s), Some((preset, sets))) = (self.song_routed(t), self.song.patch(t)) {
                let sets = sets.to_vec();
                self.preset(s, preset);
                for (p, v) in sets {
                    self.set_param(s, p, v);
                }
            }
        }
        self.song_text = self.song.print();
        true
    }

    /// The parameters of track `t`'s synth that differ from its preset, as a
    /// setting holds them: the sound only, no model, strip, global or arp
    /// parameters. `None` without a synth or a preset, or past a setting's room.
    fn changed_params(&self, t: usize) -> Option<Vec<(Param, f32)>> {
        let s = self.song_routed(t)?;
        let preset = self.song.tracks.get(t)?.preset?;
        let mut sets = Vec::new();
        for (p, d) in DEFAULTS.iter() {
            if *p == Param::Model || p.is_strip() || p.is_global() || p.is_arp() {
                continue;
            }
            let base = preset
                .changes()
                .iter()
                .rev()
                .find(|(q, _)| q == p)
                .map_or(*d, |(_, v)| *v);
            let now = self.param_value(s, *p);
            if (now - p.clamp(base)).abs() > 1e-6 {
                sets.push((*p, now));
            }
        }
        (sets.len() <= crate::song::MAX_SETS).then_some(sets)
    }

    /// An arranger edit (#171): 0 toggle (section, kind, item), 1 add a
    /// section (bars), 2 set a section's bars (section, bars), 3 insert an
    /// entry (place, section), 4 remove an entry (place), 5 move an entry
    /// (from, to), 6 set the loop (first, last; 0 0 clears). The song is
    /// printed again; false when refused.
    pub fn arrange_edit(&mut self, op: u32, a: u32, b: u32, c: u32) -> bool {
        self.commit_song();
        let (a, b, cu) = (a as usize, b as usize, c as usize);
        let ok = match op {
            0 => self.song.toggle(a, b as u32, cu),
            1 => {
                // The first section turns the arrangement on: the clock keeps its
                // place in the bar instead of landing past the new end and stopping.
                let first = self.song.arrange.is_empty();
                let ok = self.song.add_section(a as u32).is_some();
                if let Some(k) = self.clock.step().filter(|_| ok && first) {
                    let len = u64::from(a as u32).max(1) * STEPS_PER_BAR;
                    self.clock.seek_step((k + 1) % len);
                }
                ok
            }
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
        self.commit_song();
        self.clock.set_tempo(bpm);
        self.song.tempo = self.clock.tempo();
        self.song_text = self.song.print();
    }

    /// Set the song's swing (percent, 50 to 75) and print it again.
    pub fn set_song_swing(&mut self, pct: f32) {
        self.commit_song();
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
        self.run_mods(n);
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
                    // Pads pan themselves: the synth gets a stereo bus, and a pad may go
                    // straight to a group (#220).
                    if let Some((l, r, direct)) = self.mixer.pad_outs(synth, t..t + chunk) {
                        pool.render_pads(&self.samples, l, r, direct, t);
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
mod tests;
