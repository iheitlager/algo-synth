//! The engine: up to `SYNTHS` synths, each with its own parameters and a voice
//! pool (`poly`: one Mono voice per owner, or a voice per note), the mixer, the
//! song and its one clock (ADR-0022), planar stereo blocks.
//!
//! Real-time rules (ADR-0002): `render` never allocates, never panics and
//! never calls `sin`/`exp`/`pow` per sample. The voices, tables and output are
//! allocated in `Engine::new`. Importing a MIDI file allocates, once, between
//! blocks (`import_midi`), never inside `render`.

use crate::arp::{ARP_DEFAULTS, Arp};
use crate::clock::{Clock, STEPS_PER_BEAT, TICKS_PER_STEP};
use crate::deck::DeckMixer;
use crate::fm::sysex;
use crate::fx::compressor::Compressor;
use crate::fx::ensemble::Ensemble;
use crate::fx::eq::{EqBand, Equalizer};
use crate::fx::limiter::Limiter;
use crate::fx::processor::Processor;
use crate::midi::{self, MidiIn};
use crate::mixer::{Mixer, SENDS, STRIP_DEFAULTS, STRIPS};
use crate::mono::MonoParams;
use crate::mono::ladder::LadderTables;
use crate::mono::osc::Blep;
use crate::mono::preset::{DEFAULTS, Preset};
use crate::mono::voice::{MonoVoice, PitchTable, Tools};
use crate::padsampler::PadField;
use crate::params::{GLOBAL_DEFAULTS, Param};
use crate::poly::{MAX_VOICES, Pool, VOICE_BUDGET};
use crate::synth::RevisionDef;

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
use crate::modular::Program;
use crate::modular::sc::{CodeError, Patch, compile};
use crate::notes::{Edit, Event, Seq, TICKS_PER_BAR};
use crate::sample::{self, Sample, SampleStore};
use crate::sampler::{ZoneField, ZoneMap};
use crate::smf;
use crate::song::{
    At, GRACE_VELOCITY, Kind, MAX_AUTOS, MAX_MODS, MAX_TEXT, MAX_TRACKS, Mix, MixLine,
    STEPS_PER_BAR, Song, SongError, Step, Target, signal,
};
use crate::table::Tables;
use crate::voice::{Owner, sine_table};

/// Frames per render call; the Web Audio render quantum.
pub const BLOCK: usize = 128;
/// Frames a synced deck may be off its master's bar and stay put: the grid's
/// own rounding (ADR-0029).
const SYNC_TOLERANCE: i64 = 2;
/// Synth slots, each any model, with its own parameters.
pub const SYNTHS: usize = 16;
/// The bit of `Engine::fold` that marks the master's global parameters.
const FOLD_GLOBAL: u32 = 1 << 31;
/// What `Engine::live_only` reports the song cannot hold yet (#361).
pub const LIVE_ARP: u32 = 1;
pub const LIVE_ZONES: u32 = 2;
pub const LIVE_PADS: u32 = 4;
pub const LIVE_FULL: u32 = 8;
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

/// A drum hit between the clock's steps (#353), played when the clock
/// reaches sample `at`.
#[derive(Clone, Copy, Debug)]
struct Hit {
    at: u64,
    owner: Owner,
    note: u8,
    velocity: f32,
}

/// How many hits may wait between two steps: a lane of 48 a bar puts two
/// between every pair of 16ths, so this is many lanes' worth.
const HITS: usize = 256;

const NO_HIT: Hit = Hit {
    at: 0,
    owner: Owner::Live(0),
    note: 0,
    velocity: 0.0,
};

/// The switches a revision sets, in the order of `RevisionDef`'s parts (#343).
const REVISED: [Param; 3] = [Param::VcoRev, Param::FilterRev, Param::EnvRev];

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
    /// Decks B–D summed with this engine's output, after its master (ADR-0029).
    deck: DeckMixer,
    /// Frames until a cued start (`song_play_in`), if one is pending.
    start_in: Option<usize>,
    /// Frames until the master's next bar line, for sync lock (`sync_bar_in`).
    sync_in: Option<usize>,
    /// How far the last sync found the song off the master's bar, in frames.
    sync_error: i64,
    master_gain: f32,
    /// Planar output: `BLOCK` left samples, then `BLOCK` right samples.
    out: Box<[f32; 2 * BLOCK]>,
    /// The highest level of each meter since `clear_meters`.
    meters: [f32; METERS],
    /// The MIDI file's bytes, written by JavaScript before `import_midi`.
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
    /// The transport's tempo and sixteenth steps (spec 002 Req 5).
    clock: Clock,
    /// The song (ADR-0012), its text as the view wrote it, its canonical
    /// print, the last load's error, and the synth each track plays on.
    song: Song,
    song_buf: Vec<u8>,
    song_text: String,
    song_error: Option<SongError>,
    /// Each Modular synth's SynthDef: its code, knobs and modules (ADR-0024).
    codes: Vec<Option<Patch>>,
    /// The last code that did not build, and a code's text handed out.
    code_error: Option<CodeError>,
    code_text: String,
    /// The knob list handed out last (`knob_list`).
    knob_text: String,
    song_route: [Option<usize>; MAX_TRACKS],
    /// While a MIDI file is imported every part's patch is set on its synth,
    /// so the synths are what the imported text says (#327).
    import_patches: bool,
    /// Notes of the song waiting for their note-off, as (tick, track, note):
    /// a fixed table, so the clock can end a note without allocating.
    note_offs: [Option<(u64, u8, u8)>; NOTE_OFFS],
    /// One per fragment of the song; empty for all but the live ones.
    live: Vec<Live>,
    /// A song loaded while the song plays, ready to take over at the next
    /// bar with its live buffers and track routes (#208).
    pending: Option<Pending>,
    /// The fragment playing alone, if one is cued (#375), and its name, to
    /// find it again in a song that takes over.
    cue: Option<usize>,
    cue_name: String,
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
    /// Strips (bit per strip, globals on bit 0) automation or a Revision
    /// switch changed since the view last asked, so it can redraw their values.
    touched: u32,
    /// The strips a hand changed since the last fold (bit per strip), and
    /// the master in `FOLD_GLOBAL` (ADR-0027).
    fold: u32,
    /// Drum hits between steps, waiting for their sample (#353), and how many.
    hits: [Hit; HITS],
    hit_count: usize,
    /// Each synth's live arpeggiator (spec 002 Req 7).
    arps: [Arp; SYNTHS],
    /// MIDI input's target and held notes (#10).
    midi_in: MidiIn,
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
            deck: DeckMixer::new(sample_rate),
            start_in: None,
            sync_in: None,
            sync_error: 0,
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
            clock: Clock::new(sample_rate),
            song: Song::default(),
            song_buf: Vec::new(),
            song_text: Song::default().print(),
            song_error: None,
            codes: vec![None; SYNTHS],
            code_error: None,
            code_text: String::new(),
            knob_text: String::new(),
            song_route: [None; MAX_TRACKS],
            import_patches: false,
            note_offs: [None; NOTE_OFFS],
            live: Vec::new(),
            pending: None,
            cue: None,
            cue_name: String::new(),
            spent: None,
            taken: false,
            auto_last: [f32::NAN; MAX_AUTOS],
            mod_last: [f32::NAN; MAX_MODS],
            mod_base: [None; MAX_MODS],
            mod_state: [f32::NAN; signal::MAX_NODES],
            touched: 0,
            fold: 0,
            hits: [NO_HIT; HITS],
            hit_count: 0,
            arps: [Arp::default(); SYNTHS],
            midi_in: MidiIn::default(),
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
        if REVISED.contains(&param) || param == Param::Revision {
            self.revise(synth, param, v);
        }
    }

    /// The Revision switch of a model that has one (#343): a revision sets
    /// the switches it stands for and its drift; turning a switch away from
    /// them makes it Custom (0). Either way round, the switches and the
    /// Revision agree, so a setup loads the same in any order.
    fn revise(&mut self, synth: usize, param: Param, v: f32) {
        let Some(revisions) = self.synths.get(synth).and_then(|m| m.model.def().revisions) else {
            return;
        };
        let parts = |r: &RevisionDef| [r.vco, r.filter, r.env].map(f32::from);
        // Revisions run from 1; 0, Custom, sets nothing.
        let nth = |x: f32| {
            (x.round() as usize)
                .checked_sub(1)
                .and_then(|i| revisions.get(i))
        };
        if param == Param::Revision {
            if let Some(r) = nth(v) {
                for (p, x) in REVISED.into_iter().zip(parts(r)) {
                    self.put(synth, p, x);
                }
                self.put(synth, Param::Analog, r.analog);
                self.touch(synth);
            }
            return;
        }
        let Some(r) = nth(self.param_value(synth, Param::Revision)) else {
            return;
        };
        // 1 and 2 are the same early chips, as `FilterRev` has always had it.
        let early = |x: f32| x < 2.5;
        let agree = REVISED
            .into_iter()
            .zip(parts(r))
            .all(|(p, x)| early(self.param_value(synth, p)) == early(x));
        if !agree {
            self.put(synth, Param::Revision, 0.0);
            self.touch(synth);
        }
    }

    /// Store and apply a synth parameter, nothing else.
    fn put(&mut self, synth: usize, param: Param, v: f32) {
        let v = param.clamp(v);
        if let Some(slot) = self
            .values
            .get_mut(synth)
            .and_then(|r| r.get_mut(param as usize))
        {
            *slot = v;
        }
        if let Some(mono) = self.synths.get_mut(synth) {
            mono.set(param, v);
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
        // The model is the one whose definition holds the preset (#330).
        let model = [(Param::Model, preset.model() as u32 as f32)];
        for (p, v) in DEFAULTS.iter().chain(&model).chain(preset.changes()) {
            self.set_param(synth, *p, *v);
        }
        // A Modular preset's code; building allocates, so never from `render`.
        match preset.code() {
            Some(code) => {
                // A preset's code is the engine's own and always builds.
                if self.set_code(synth, code).is_err() {
                    self.set_graph(synth, Program::default());
                }
            }
            None => {
                if let Some(c) = self.codes.get_mut(synth) {
                    *c = None;
                }
            }
        }
    }

    /// Give Modular `synth` a SuperCollider SynthDef (ADR-0024): built here,
    /// outside `render`, its knobs set to the numbers as written. An error
    /// leaves the synth as it was.
    pub fn set_code(&mut self, synth: usize, code: &str) -> Result<(), CodeError> {
        let patch = compile(code)?;
        self.set_graph(synth, patch.program);
        for k in &patch.knobs {
            if let Some(p) = Param::ctl_param(k.ctl) {
                self.set_param(synth, p, k.default);
            }
        }
        if let Some(slot) = self.codes.get_mut(synth) {
            *slot = Some(patch);
        }
        Ok(())
    }

    /// Give Modular `synth` the SynthDef written into the song buffer (the
    /// panel's Apply); an error is kept for `code_error`.
    pub fn set_code_from_buffer(&mut self, synth: usize) -> Result<(), CodeError> {
        let code = String::from_utf8_lossy(&self.song_buf).into_owned();
        let result = self.set_code(synth, &code);
        self.code_error = result.err();
        result
    }

    pub fn code_error(&self) -> Option<CodeError> {
        self.code_error
    }

    /// Modular `synth`'s code, kept for the C ABI until the next call.
    pub fn code_text(&mut self, synth: usize) -> &str {
        self.code_text = self.code(synth).unwrap_or_default();
        &self.code_text
    }

    pub fn code_text_buf(&self) -> &str {
        &self.code_text
    }

    /// Modular `synth`'s knobs for its panel (#329), one line each:
    /// `module, UGen, name, ctl, lo, hi, exp (0/1), default`, tab-separated;
    /// empty for another model. Kept for the C ABI until the next call.
    pub fn knob_list(&mut self, synth: usize) -> &str {
        let mut text = String::new();
        if let Some(patch) = self.patch(synth) {
            for k in &patch.knobs {
                let ugen = patch.modules.get(k.module).map_or("", |m| m.name.as_str());
                text.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    k.module,
                    ugen,
                    k.name,
                    k.ctl,
                    k.lo,
                    k.hi,
                    u8::from(k.exp),
                    k.default
                ));
            }
        }
        self.knob_text = text;
        &self.knob_text
    }

    pub fn knob_list_buf(&self) -> &str {
        &self.knob_text
    }

    /// Modular `synth`'s code as it plays: its numbers are its knobs' values.
    pub fn code(&self, synth: usize) -> Option<String> {
        let patch = self.patch(synth)?;
        let values: Vec<f32> = (0..crate::params::CTLS)
            .map(|i| Param::ctl_param(i).map_or(0.0, |p| self.param_value(synth, p)))
            .collect();
        Some(patch.text(&values))
    }

    /// Modular `synth`'s knobs and modules, for its panel; none once the
    /// synth is another model.
    pub fn patch(&self, synth: usize) -> Option<&Patch> {
        let modular = self.synths.get(synth)?.model.uses_graph();
        self.codes.get(synth)?.as_ref().filter(|_| modular)
    }

    /// Give `synth` the voice a Modular synth plays (ADR-0020); each note
    /// takes it when it starts.
    pub fn set_graph(&mut self, synth: usize, graph: Program) {
        if let Some(pool) = self.pools.get_mut(synth) {
            pool.size_graph(&graph, self.sample_rate);
        }
        if let Some(p) = self.synths.get_mut(synth) {
            p.graph_cap = graph.voice_cap();
            p.graph = graph;
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

    /// Start over (#325): the song stopped and emptied, every synth, strip,
    /// arp, zone map, pad kit and global effect back to its defaults, every
    /// voice let go, and synth 0 a Modular synth on `ModularBasic`. Loaded
    /// samples stay in the store. Builds a SynthDef, so never from `render`.
    pub fn clear(&mut self) {
        self.song_stop();
        self.all_off();
        let empty = Song::default().print();
        self.song_buf.clear();
        self.song_buf.extend_from_slice(empty.as_bytes());
        // The canonical empty song always parses; should it not, no song at all.
        if self.load_song().is_err() {
            self.song = Song::default();
        }
        for (p, v) in GLOBAL_DEFAULTS {
            self.set_param(0, p, v);
        }
        for synth in 0..SYNTHS {
            self.clear_zones(synth);
            self.clear_pads(synth);
            if let Some(c) = self.codes.get_mut(synth) {
                *c = None;
            }
            self.set_graph(synth, Program::default());
        }
        self.code_error = None;
        for strip in 0..STRIPS {
            self.reset(strip);
        }
        self.preset(0, Preset::ModularBasic);
        // Every synth on screen is a track (ADR-0027).
        let _ = self.track_add(0, Preset::ModularBasic);
    }

    /// The synth MIDI input plays (#10): the one selected in the view.
    pub fn set_midi_target(&mut self, synth: usize) {
        if synth < SYNTHS {
            self.midi_in.target = synth;
        }
    }

    /// One MIDI message from a controller (#257 stage 1, #10): keys play the
    /// target synth, a key's release goes where it started, the wheels move
    /// its `PitchBend` and `ModWheel`. Other messages are ignored for now.
    pub fn midi_in(&mut self, status: u8, d1: u8, d2: u8) {
        let target = self.midi_in.target;
        match midi::decode(status, d1, d2) {
            Some(midi::Event::NoteOn { note, velocity, .. }) => {
                self.midi_in.press(note, target);
                self.note_on(target, note, velocity);
            }
            Some(midi::Event::NoteOff { note, .. }) => {
                let synth = self.midi_in.release(note);
                self.note_off(synth, note);
            }
            Some(midi::Event::Bend { value, .. }) => {
                self.set_param(target, Param::PitchBend, value)
            }
            Some(midi::Event::Control {
                number: midi::MOD_WHEEL,
                value,
                ..
            }) => self.set_param(target, Param::ModWheel, f32::from(value) / 127.0),
            _ => {}
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
    /// song track.
    fn target(&self, owner: Owner) -> Option<usize> {
        match owner {
            Owner::Live(s) => Some(usize::from(s)),
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

    // --- MIDI files (imported, #173) --------------------------------------

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

    /// The transport, the only one (ADR-0022): the clock runs the song's
    /// lanes (spec 002 Req 5). Play continues from where it paused or
    /// stopped; pause holds the place; stop goes back to the top, as a drum
    /// machine's does. Hits ring out.
    /// Start the song `frames` from now, on that exact sample, even inside a
    /// later block: how a deck starts on the master's bar (ADR-0029).
    pub fn song_play_in(&mut self, frames: usize) {
        self.start_in = Some(frames);
    }

    /// In `frames` frames the master is on a bar line: then pull this song's
    /// nearest bar line onto that sample, if it is off (sync lock, ADR-0029).
    pub fn sync_bar_in(&mut self, frames: usize) {
        self.sync_in = Some(frames);
    }

    /// How far the last sync found this song off the master's bar, in frames:
    /// positive when it was ahead, within `SYNC_TOLERANCE` when it was on it.
    pub fn sync_error(&self) -> i64 {
        self.sync_error
    }

    fn sync_to_bar(&mut self) {
        if !self.clock.playing() {
            return;
        }
        let bar = STEPS_PER_BAR as f64;
        let k = ((self.clock.step_position() / bar).round() * bar).max(0.0) as u64;
        let error = self.clock.position() as i64 - self.clock.step_sample(k) as i64;
        self.sync_error = error;
        if error.abs() > SYNC_TOLERANCE {
            self.clock.align_to_step(k);
        }
    }

    /// Frames from now to the next multiple of `every` steps at least
    /// `at_least` frames away, while the song plays: where to cue a deck.
    pub fn cue_frames(&self, every: u64, at_least: u64) -> Option<u64> {
        self.clock.frames_to_multiple(every, at_least)
    }

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

    /// Halt the song where it is; play goes on from here. Its notes let go.
    pub fn song_pause(&mut self) {
        self.commit_song();
        self.hand_arps_over();
        self.release_song_notes();
        self.clock.stop();
        self.hit_count = 0;
    }

    pub fn song_stop(&mut self) {
        self.commit_song();
        self.cue = None;
        self.start_in = None;
        self.sync_in = None;
        self.hand_arps_over();
        self.release_song_notes();
        self.clock.stop();
        self.clock.seek(0);
        self.hit_count = 0;
        // Each modulation puts back the value it found; a per-voice one never
        // wrote the synth's, so its voices just go back to it (ADR-0023).
        for m in 0..MAX_MODS {
            let target = self
                .song
                .mods
                .get(m)
                .filter(|md| !md.signal.per_voice())
                .map(|md| (md.target, md.param));
            let base = self.mod_base.get_mut(m).and_then(Option::take);
            if let (Some((t, p)), Some(base)) = (target, base) {
                self.automate(t, p, base);
            }
        }
        for pool in self.pools.iter_mut() {
            pool.clear_all_voices();
        }
        // From the top every lane and modulation writes again.
        self.auto_last = [f32::NAN; MAX_AUTOS];
        self.mod_last = [f32::NAN; MAX_MODS];
        self.mod_state = [f32::NAN; signal::MAX_NODES];
    }

    /// Play fragment `frag` alone, looping from its first bar, on the song's
    /// clock (#375): no arrangement, scenes or automation lanes, only its
    /// lanes or notes and the song's modulations. `None`, or a fragment the
    /// song does not have, stops it and leaves the song as it was. Stop
    /// clears the cue too.
    pub fn song_cue(&mut self, frag: Option<usize>) {
        self.song_stop();
        let Some((f, name)) = frag.and_then(|f| self.song.frags.get(f).map(|x| (f, &x.name)))
        else {
            return;
        };
        self.cue_name.clone_from(name);
        self.cue = Some(f);
        self.song_play();
    }

    /// The fragment playing alone, if one is cued.
    pub fn song_cued(&self) -> Option<usize> {
        self.cue
    }

    /// Where clock step `k` falls: in the song's arrangement, or free while a
    /// fragment is cued (#375).
    fn place(&self, k: u64) -> At {
        if self.cue.is_some() {
            At::Free(k)
        } else {
            self.song.at(k)
        }
    }

    /// Whether fragment `f` is silent because another one is cued.
    fn cued_out(&self, f: usize) -> bool {
        self.cue.is_some_and(|c| c != f)
    }

    /// Move the song to the first step of `bar` (from 0); the clock fires it next.
    pub fn song_seek_bar(&mut self, bar: u64) {
        self.clock.seek_step(bar.saturating_mul(STEPS_PER_BAR));
        self.hit_count = 0;
    }

    /// Where the last fired step fell: the arrangement entry and the steps into
    /// it, or `None` without an arrangement or before the first step.
    pub fn song_place(&self) -> Option<(usize, u64)> {
        match self.place(self.clock.step()?) {
            At::In { entry, local, .. } => Some((entry, local)),
            _ => None,
        }
    }

    fn fire_due_events(&mut self) {
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
        self.fire_hits();
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
        let (from, section) = match self.place(step) {
            At::Free(_) => (j, None),
            At::In { section, local, .. } => {
                (local * TICKS_PER_STEP + j % TICKS_PER_STEP, Some(section))
            }
            At::End => return,
        };
        for f in 0..self.song.frags.len() {
            if self.cued_out(f) {
                continue;
            }
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
            if !self.song.heard(usize::from(track)) {
                continue;
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
    fn play_step(&mut self, step: u64) {
        match self.place(step) {
            At::In {
                section, local: 0, ..
            } => self.apply_scenes(section),
            At::End => {
                self.song_stop();
                return;
            }
            _ => {}
        }
        self.lane_hits(step, false);
        // The grace strokes of the next step's flams and drags fall before
        // it (#353): queued now, from what the song holds now.
        self.lane_hits(step + 1, true);
    }

    /// The lanes' hits from clock step `step` up to the next (#353): a lane
    /// of `g` steps a bar has its step n at n·16/g clock steps. A hit on the
    /// step plays now, one between steps waits in the queue. With `ahead`,
    /// only the grace strokes that fall before the step are queued; without,
    /// the hits and the graces that fall after it.
    fn lane_hits(&mut self, step: u64, ahead: bool) {
        let (k, section) = match self.place(step) {
            At::Free(k) => (k, None),
            At::In { section, local, .. } => (local, Some(section)),
            At::End => return,
        };
        let on_step = self.clock.step_sample(step);
        let before = step.checked_sub(1).map(|p| self.clock.step_sample(p));
        let ms = f64::from(self.sample_rate) / 1000.0;
        for f in 0..self.song.frags.len() {
            if self.cued_out(f) {
                continue;
            }
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
            if !self.song.heard(frag.track) {
                continue;
            }
            let owner = Owner::Track(u8::try_from(frag.track).unwrap_or(u8::MAX));
            let g = u64::from(frag.grid.max(1));
            let (first, end) = ((k * g).div_ceil(16), ((k + 1) * g).div_ceil(16));
            for l in 0..frag.lanes.len() {
                for n in first..end {
                    let hit = self
                        .song
                        .frags
                        .get(f)
                        .and_then(|fr| fr.lanes.get(l))
                        .and_then(|lane| {
                            let len = lane.steps.len() as u64;
                            let st = lane.steps.get(usize::try_from(n % len.max(1)).ok()?)?;
                            Some((lane.pad.note(), *st))
                        });
                    let Some((note, st)) = hit else {
                        continue;
                    };
                    let off = n * 16 - k * g;
                    let at = if off == 0 {
                        on_step
                    } else {
                        self.clock.between_sample(step, off as f64 / g as f64)
                    };
                    if let (Some(velocity), false) = (st.velocity(), ahead) {
                        if off == 0 {
                            self.start_voice(owner, note, velocity);
                        } else {
                            self.queue_hit(Hit {
                                at,
                                owner,
                                note,
                                velocity,
                            });
                        }
                    }
                    // Graces shrink together to fit after the lane's hit
                    // before and after the clock step before (the furthest
                    // the queue looks ahead), so they stay in order.
                    let graces = st.graces();
                    let Some(widest) = graces.first().map(|g| f64::from(*g) * ms) else {
                        continue;
                    };
                    let prev = n.checked_sub(1).map(|p| {
                        let (pk, poff) = (p * 16 / g, p * 16 % g);
                        let abs = step - (k - pk);
                        self.clock.between_sample(abs, poff as f64 / g as f64)
                    });
                    let room = [prev, before]
                        .into_iter()
                        .flatten()
                        .map(|b| at.saturating_sub(b))
                        .min()
                        .map_or(0.0, |r| r as f64 * 0.9);
                    let scale = (room / widest).min(1.0);
                    for g in graces {
                        let grace = at.saturating_sub((f64::from(*g) * ms * scale).round() as u64);
                        if (grace < on_step) == ahead && grace < at {
                            self.queue_hit(Hit {
                                at: grace,
                                owner,
                                note,
                                velocity: GRACE_VELOCITY,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Hold a drum hit between steps until the clock reaches it; with the
    /// queue full it plays at once rather than not at all (#353).
    fn queue_hit(&mut self, hit: Hit) {
        match self.hits.get_mut(self.hit_count) {
            Some(slot) => {
                *slot = hit;
                self.hit_count += 1;
            }
            None => self.start_voice(hit.owner, hit.note, hit.velocity),
        }
    }

    /// Play the queued hits the clock has reached, in the order queued.
    fn fire_hits(&mut self) {
        let pos = self.clock.position();
        let mut i = 0;
        while i < self.hit_count {
            let Some(hit) = self.hits.get(i).copied() else {
                break;
            };
            if hit.at <= pos {
                self.hits.copy_within(i + 1..self.hit_count, i);
                self.hit_count -= 1;
                let heard = match hit.owner {
                    Owner::Track(t) => self.song.heard(usize::from(t)),
                    _ => true,
                };
                if heard {
                    self.start_voice(hit.owner, hit.note, hit.velocity);
                }
            } else {
                i += 1;
            }
        }
    }

    /// Frames to render before the next queued hit, at least 1, at most `remaining`.
    fn frames_until_hit(&self, remaining: usize) -> usize {
        let pos = self.clock.position();
        let next = self
            .hits
            .iter()
            .take(self.hit_count)
            .map(|h| h.at.saturating_sub(pos))
            .min();
        next.and_then(|gap| usize::try_from(gap).ok())
            .map_or(remaining, |gap| gap.clamp(1, remaining.max(1)))
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
        self.touch(strip);
    }

    /// The automation lanes at the clock's position, once per block: each
    /// writes only when its value changed. Lanes of the current section play,
    /// counted from its first step; without an arrangement every lane loops.
    fn run_automation(&mut self) {
        // A cued fragment plays without the arrangement's lanes (#375).
        if !self.clock.playing() || self.song.autos.is_empty() || self.cue.is_some() {
            return;
        }
        let pos = self.clock.step_position().max(0.0);
        let whole = pos.floor();
        let frac = pos - whole;
        let (local, section) = match self.place(whole as u64) {
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
        let section = match self.place(pos.floor() as u64) {
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
                !self.cued_out(f)
                    && section.is_none_or(|s| {
                        self.song
                            .sections
                            .get(s)
                            .is_some_and(|sec| sec.frags.contains(&f))
                    })
            });
            if md.signal.per_voice() {
                // A value per voice of the track's Mono or Poly synth (ADR-0023).
                let Target::Track(track) = target else {
                    continue;
                };
                let Some((s, pool)) = self
                    .song_route
                    .get(track)
                    .copied()
                    .flatten()
                    .and_then(|s| Some((s, self.pools.get_mut(s)?)))
                else {
                    continue;
                };
                let Some(base) = self.mod_base.get_mut(m) else {
                    continue;
                };
                if !playing {
                    if base.take().is_some() {
                        pool.clear_voices(param);
                    }
                    continue;
                }
                // Marks the knob as modulated; nothing is put back.
                *base = Some(0.0);
                let sr = self.sample_rate;
                let adsr = self.synths.get(s).map_or([0.0; 4], |p| {
                    let t = p.adsr;
                    [t.attack / sr, t.decay / sr, t.sustain, t.release / sr]
                });
                for slot in 0..MAX_VOICES {
                    let Some((on, off)) = pool.voice_times(slot, sr) else {
                        continue;
                    };
                    let mut ctx = signal::Ctx {
                        cps,
                        dt,
                        state: &mut self.mod_state,
                        voice: Some(signal::Voice {
                            slot,
                            on,
                            off,
                            adsr,
                        }),
                    };
                    let v = md.signal.eval(t, &mut ctx);
                    pool.set_voice(slot, param, v);
                }
                continue;
            }
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
                voice: None,
            };
            let v = md.signal.eval(t, &mut ctx);
            match self.mod_last.get_mut(m) {
                Some(last) if *last != v => *last = v,
                _ => continue,
            }
            self.automate(target, param, v);
        }
    }

    /// The `i`th (strip, parameter) a modulation is writing now (ADR-0019),
    /// so the view can mark its knob; `None` past the last. A track's
    /// modulation names the strip it is routed to; the master's is strip 0.
    pub fn modulated(&self, i: usize) -> Option<(usize, Param)> {
        self.song
            .mods
            .iter()
            .zip(self.mod_base.iter())
            .filter(|(_, base)| base.is_some())
            .filter_map(|(m, _)| {
                let strip = match m.target {
                    Target::Master => Some(0),
                    Target::Strip(s) => Some(s),
                    Target::Track(t) => self.song_routed(t),
                };
                strip.map(|s| (s, m.param))
            })
            .nth(i)
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

    // --- Autocommit (ADR-0027) ---------------------------------------------------

    /// A parameter a hand set (a knob, a library preset, a setup opened):
    /// set as `set_param` does, and marked to be folded into the song.
    /// Automation, scenes and modulation call `set_param` and are not.
    pub fn edit_param(&mut self, synth: usize, param: Param, value: f32) {
        self.set_param(synth, param, value);
        self.mark(synth, param);
    }

    /// A factory preset picked for `synth`: the tracks it plays take it as
    /// their preset, and its sound is folded in from there.
    pub fn edit_preset(&mut self, synth: usize, preset: Preset) {
        self.commit_song();
        self.preset(synth, preset);
        for t in 0..self.song.tracks.len() {
            if self.song_routed(t) == Some(synth) {
                self.song.set_track_preset(t, preset);
            }
        }
        self.mark_synth(synth);
    }

    /// A track for `synth`, shown without one (ADR-0027): named after it
    /// (`synth_2`, `drums_10`), on `preset`, routed to it. The synth keeps its
    /// sound; the next fold writes it into the track's setting. The track's
    /// index, or `None` past the song's room.
    pub fn track_add(&mut self, synth: usize, preset: Preset) -> Option<usize> {
        if synth >= SYNTHS {
            return None;
        }
        self.commit_song();
        let m = preset.model();
        let word = if m.uses_drums() || m.uses_pads() {
            "drums"
        } else if m.uses_sampler() {
            "sampler"
        } else {
            "synth"
        };
        let t = self
            .song
            .add_track(&format!("{word}_{}", synth + 1), preset)?;
        self.song_route(t, Some(synth));
        self.mark_synth(synth);
        self.song_text = self.song.print();
        Some(t)
    }

    /// `synth` taken off the screen (ADR-0027): its track goes when it has no
    /// music, the later tracks keeping their synths; one with music is
    /// muted, so nothing composed is lost. 1 removed, 0 muted, −1 no track.
    pub fn track_remove(&mut self, synth: usize) -> i32 {
        let Some(t) = (0..self.song.tracks.len()).find(|t| self.song_routed(*t) == Some(synth))
        else {
            return -1;
        };
        self.commit_song();
        let done = if self.song.remove_track(t) {
            self.song_route.copy_within(t + 1.., t);
            if let Some(last) = self.song_route.last_mut() {
                *last = None;
            }
            1
        } else {
            self.song_route(t, None);
            0
        };
        self.song_text = self.song.print();
        done
    }

    /// What of `synth`'s sound the song cannot hold yet (ADR-0027, #361), one
    /// bit each: `LIVE_ARP` an arpeggiator away from its defaults,
    /// `LIVE_ZONES` sample zones, `LIVE_PADS` sampled pads, `LIVE_FULL` more
    /// changes than a setting holds. 0 when the song holds all of it.
    pub fn live_only(&self, synth: usize) -> u32 {
        let mut bits = 0;
        if ARP_DEFAULTS
            .iter()
            .any(|(p, d)| (self.param_value(synth, *p) - p.clamp(*d)).abs() > 1e-6)
        {
            bits |= LIVE_ARP;
        }
        let zones = self.zones.get(synth);
        if (0..crate::sampler::ZONES).any(|z| {
            zones
                .and_then(|m| m.get(z))
                .is_some_and(|z| z.sample.is_some())
        }) {
            bits |= LIVE_ZONES;
        }
        let kit = self.synths.get(synth).map(|p| &p.pad_kit);
        if (0..crate::padsampler::PADS).any(|i| {
            kit.and_then(|k| k.pad(i))
                .is_some_and(|c| c.sample.is_some())
        }) {
            bits |= LIVE_PADS;
        }
        let track = (0..self.song.tracks.len()).find(|t| self.song_routed(*t) == Some(synth));
        if track.is_some_and(|t| self.changed_params(t).is_none()) {
            bits |= LIVE_FULL;
        }
        bits
    }

    /// Mark `synth`'s sound to fold: a preset, a voice or code it took.
    pub fn mark_synth(&mut self, synth: usize) {
        self.fold |= 1u32.checked_shl(synth as u32).unwrap_or(0);
    }

    /// Mark `synth`'s strip, or the master for a global parameter, to fold.
    pub fn mark(&mut self, synth: usize, param: Param) {
        let bit = if param.is_global() {
            FOLD_GLOBAL
        } else {
            1u32.checked_shl(synth as u32).unwrap_or(0)
        };
        self.fold |= bit;
    }

    /// Whether a lane, a scene or a modulation of the song sets `param` on
    /// `strip`: its value is the song's, never folded back.
    fn driven(&self, strip: usize, param: Param) -> bool {
        let on = |target: Target| match target {
            Target::Track(t) => self.song_routed(t) == Some(strip),
            Target::Strip(s) => s == strip,
            Target::Master => param.is_global(),
        };
        self.song
            .autos
            .iter()
            .any(|a| a.param == param && on(a.target))
            || self
                .song
                .mods
                .iter()
                .any(|m| m.param == param && on(m.target))
            || self
                .song
                .scenes
                .iter()
                .any(|sc| sc.sets.iter().any(|(tg, p, _)| *p == param && on(*tg)))
    }

    /// Fold what the hands changed since the last call into the song
    /// (ADR-0027): each changed track's sound into its own setting, the mixer
    /// into its lines, and the song printed again. True when the text
    /// changed. Allocates: the worklet calls it a few times a second, never
    /// from `render`.
    pub fn fold(&mut self) -> bool {
        let marks = std::mem::take(&mut self.fold);
        if marks == 0 {
            return false;
        }
        self.commit_song();
        let before = std::mem::take(&mut self.song_text);
        for t in 0..self.song.tracks.len() {
            let Some(s) = self.song_routed(t).filter(|s| *s < SYNTHS) else {
                continue;
            };
            if marks & (1 << s) == 0 {
                continue;
            }
            // Past a setting's room the sound stays as it plays, unfolded.
            if let Some(sets) = self.changed_params(t) {
                let code = self.changed_code(t);
                self.song.fold_sound(t, sets, code);
            }
        }
        self.write_mixer();
        self.song_text != before
    }

    /// Mark `strip`'s values as moved by the engine, for the view to fetch.
    fn touch(&mut self, strip: usize) {
        self.touched |= 1u32.checked_shl(strip as u32).unwrap_or(0);
    }

    /// The strips automation or a Revision switch changed since the last
    /// call (bit per strip), cleared.
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
        // A cued fragment is found again by its name; gone, it stops (#375).
        let lost = self.cue.is_some() && {
            self.cue = self.song.frags.iter().position(|f| f.name == self.cue_name);
            self.cue.is_none()
        };
        self.auto_last = [f32::NAN; MAX_AUTOS];
        self.mod_last = [f32::NAN; MAX_MODS];
        self.mod_state = [f32::NAN; signal::MAX_NODES];
        // A modulation the new song keeps keeps the value it found; one it
        // drops puts that value back.
        let old = std::mem::replace(&mut self.mod_base, [None; MAX_MODS]);
        let mut restore = [None; MAX_MODS];
        // Per-voice values come back from the new song's signals next block.
        for pool in self.pools.iter_mut() {
            pool.clear_all_voices();
        }
        if let Some((spent, _)) = &self.spent {
            for (o, md) in spent.mods.iter().enumerate() {
                let Some(base) = old.get(o).copied().flatten() else {
                    continue;
                };
                if md.signal.per_voice() {
                    continue;
                }
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
        if lost {
            self.song_stop();
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
                        let code = song.code(t);
                        let changed = self.import_patches
                            || routed.is_none()
                            || self.song.patch(t) != patch
                            || self.song.code(t) != code;
                        if let Some(s) = synth.filter(|_| changed) {
                            self.preset(s, preset);
                            // The parser built it already, so it builds here.
                            if let Some(code) = code {
                                self.set_code(s, code).ok();
                            }
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
    /// song's, and its tracks, one per channel in channel order, go to synths
    /// 0, 1, 2… (ADR-0022). The number of tracks, or a negative code: the MIDI file's own
    /// (`smf::Error::code`), `ImportError::code`, or −9 when the text does
    /// not parse (a bug). Allocates; never called from `render`.
    pub fn import_midi(&mut self) -> Result<usize, i32> {
        let smf = smf::parse(&self.midi).map_err(smf::Error::code)?;
        let imported = crate::midi_import::import(&smf).map_err(|e| e.code())?;
        self.song_buf = imported.text.into_bytes();
        // Its parts play on synths 0, 1, 2… (ADR-0022), each with the patch
        // the text gives it (#327): routed first, so the patch lands there.
        for (t, slot) in self.song_route.iter_mut().enumerate() {
            *slot = Some(t).filter(|s| *s < imported.channels.len() && *s < SYNTHS);
        }
        self.import_patches = true;
        let loaded = self.load_song();
        self.import_patches = false;
        loaded.map_err(|_| -9)?;
        self.commit_song();
        for t in 0..imported.channels.len() {
            if let Some(slot) = self.song_route.get_mut(t) {
                *slot = Some(t).filter(|s| *s < SYNTHS);
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

    /// Mute and solo track `t` (#355): its frags stop or play again, the
    /// notes of every track no longer heard let go, and the text says so.
    /// The synth and its strip are left alone.
    pub fn set_track_flags(&mut self, t: usize, mute: bool, solo: bool) -> bool {
        self.commit_song();
        if !self.song.set_track_flags(t, mute, solo) {
            return false;
        }
        for i in 0..self.note_offs.len() {
            let unheard = self
                .note_offs
                .get(i)
                .copied()
                .flatten()
                .filter(|(_, track, _)| !self.song.heard(usize::from(*track)));
            if let Some((_, track, note)) = unheard {
                if let Some(slot) = self.note_offs.get_mut(i) {
                    *slot = None;
                }
                self.stop_note(Owner::Track(track), note);
            }
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
        // A value the old text set and the new one leaves out goes back to its
        // default (ADR-0027): the text is the mix, so a line taken out is gone.
        let mut resets = Vec::new();
        for line in &self.song.mix {
            let (strip, defaults): (Option<usize>, &[(Param, f32)]) = match line.at {
                Mix::Track(t) => (before.get(t).copied().flatten(), &STRIP_DEFAULTS),
                Mix::Strip(i) => (Some(i), &STRIP_DEFAULTS),
                Mix::Group(g) => (Some(SYNTHS + g), &STRIP_DEFAULTS),
                Mix::Master => (Some(0), &GLOBAL_DEFAULTS),
            };
            let Some(strip) = strip else {
                continue;
            };
            for (p, _) in &line.sets {
                let kept = match line.at {
                    Mix::Track(t) => self.song.tracks.get(t).is_some_and(|old| {
                        song.tracks
                            .iter()
                            .position(|new| new.name == old.name)
                            .is_some_and(|u| song.mix_value(Mix::Track(u), *p).is_some())
                    }),
                    at => song.mix_value(at, *p).is_some(),
                };
                if let Some((_, d)) = defaults.iter().find(|(q, _)| q == p).filter(|_| !kept) {
                    resets.push((strip, *p, *d));
                }
            }
        }
        for (strip, p, d) in resets {
            self.set_param(strip, p, d);
        }
    }

    /// What the song's mixer lines say `param` is on `strip`, if they say.
    fn mix_value_at(&self, strip: usize, param: Param) -> Option<f32> {
        self.song.mix.iter().find_map(|line| {
            let at = match line.at {
                Mix::Track(t) => self.song_routed(t),
                Mix::Strip(i) => Some(i),
                Mix::Group(g) => Some(SYNTHS + g),
                Mix::Master => Some(0),
            };
            (at == Some(strip))
                .then(|| line.sets.iter().find(|(p, _)| *p == param).map(|(_, v)| *v))
                .flatten()
        })
    }

    /// Print the mixer as it is into the song (ADR-0018, Write mixer to
    /// song): a line for each track's strip, each other strip and group, and
    /// the master, holding what differs from the defaults. The song's old
    /// mixer lines are replaced; a group keeps its name.
    pub fn write_mixer(&mut self) {
        self.commit_song();
        // A value the song drives keeps what its line says (ADR-0027).
        let differs = |e: &Engine, strip: usize, defaults: &[(Param, f32)]| -> Vec<(Param, f32)> {
            defaults
                .iter()
                .filter_map(|(p, d)| {
                    let v = if e.driven(strip, *p) {
                        e.mix_value_at(strip, *p)?
                    } else {
                        e.param_value(strip, *p)
                    };
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
                let code = self.changed_code(t);
                sets.is_some_and(|sets| self.song.add_setting(t, sets, code).is_some())
            }
            _ => false,
        };
        if !ok {
            return false;
        }
        if op != 2 {
            if let (Some(s), Some((preset, sets))) = (self.song_routed(t), self.song.patch(t)) {
                let sets = sets.to_vec();
                let code = self.song.code(t).map(str::to_string);
                self.preset(s, preset);
                if let Some(code) = code {
                    self.set_code(s, &code).ok();
                }
                for (p, v) in sets {
                    self.set_param(s, p, v);
                }
            }
        }
        self.song_text = self.song.print();
        true
    }

    /// Track `t`'s Modular synth's code as it plays, its knobs' values in
    /// place of its numbers, when it is not its preset's (ADR-0024).
    fn changed_code(&self, t: usize) -> Option<String> {
        let s = self.song_routed(t)?;
        let preset = self.song.tracks.get(t)?.preset?;
        let code = self.code(s)?;
        (Some(code.as_str()) != preset.code()).then_some(code)
    }

    /// The parameters of track `t`'s synth that differ from its preset, as a
    /// setting holds them: the sound only, no model, strip, global or arp
    /// parameters, and no knob of a Modular synth with code, which its code
    /// holds. `None` without a synth or a preset, or past a setting's room.
    fn changed_params(&self, t: usize) -> Option<Vec<(Param, f32)>> {
        let s = self.song_routed(t)?;
        let preset = self.song.tracks.get(t)?.preset?;
        let coded = self.patch(s).is_some();
        let mut sets = Vec::new();
        for (p, d) in DEFAULTS.iter() {
            if *p == Param::Model || p.is_strip() || p.is_global() || p.is_arp() {
                continue;
            }
            if coded && p.ctl().is_some() {
                continue;
            }
            let base = preset
                .changes()
                .iter()
                .rev()
                .find(|(q, _)| q == p)
                .map_or(*d, |(_, v)| *v);
            // A value the song drives keeps what its setting says (ADR-0027).
            let now = if self.driven(s, *p) {
                self.song
                    .patch(t)
                    .and_then(|(_, sets)| sets.iter().find(|(q, _)| q == p).map(|(_, v)| *v))
            } else {
                Some(self.param_value(s, *p))
            };
            if let Some(now) = now.filter(|v| (v - p.clamp(base)).abs() > 1e-6) {
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
                    self.hit_count = 0;
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
    /// split at clock steps, so each lands on its exact
    /// sample.
    pub fn render(&mut self, frames: usize) {
        let n = frames.min(BLOCK);
        self.run_automation();
        self.run_mods(n);
        self.out.fill(0.0);
        self.mixer.clear(n);
        let mut t = 0;
        while t < n {
            if self.start_in == Some(0) {
                self.start_in = None;
                self.song_play();
            }
            if self.sync_in == Some(0) {
                self.sync_in = None;
                self.sync_to_bar();
            }
            self.fire_due_events();
            let chunk = self
                .clock
                .frames_until_next(n - t)
                .min(self.free_frames_until_next(n - t))
                .min(self.frames_until_hit(n - t))
                .min(self.start_in.unwrap_or(usize::MAX))
                .min(self.sync_in.unwrap_or(usize::MAX));
            for (synth, (pool, params)) in self.pools.iter_mut().zip(self.synths.iter()).enumerate()
            {
                // A-440 sounds with no key held (#308).
                if pool.active() == 0 && !params.a440 {
                    continue;
                }
                if params.model.uses_drums() {
                    // The kit's pads go to its strip or straight to a group (#162),
                    // panned either way: the synth gets a stereo bus (#364).
                    if let Some((l, r, direct)) = self.mixer.pad_outs(synth, t..t + chunk) {
                        pool.render_kit(params, &self.sine, &self.blep, (l, r), direct, t);
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
                // A stereo Modular program writes both sides of its bus.
                if params.model.uses_graph() && params.graph.is_stereo() {
                    if let Some((l, r)) = self.mixer.stereo_bus(synth, t..t + chunk) {
                        let tools = Tools {
                            sine: &self.sine,
                            blep: &self.blep,
                            ladder: &self.ladder,
                            pitch: &self.pitch,
                            tables: self.tables,
                            samples: &self.samples,
                            zones,
                        };
                        pool.render_into(params, tools, l, Some(r));
                    }
                    continue;
                }
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
            self.clock.advance(chunk);
            if !self.clock.playing() {
                self.free_pos += chunk as u64;
            }
            for pending in [&mut self.start_in, &mut self.sync_in]
                .into_iter()
                .flatten()
            {
                *pending -= chunk;
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
        // Decks B–D join after deck A's master and its meters (ADR-0029).
        let (left, right) = self.out.split_at_mut(BLOCK);
        if let (Some(l), Some(r)) = (left.get_mut(..n), right.get_mut(..n)) {
            self.deck.process(l, r);
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

    /// The deck mixer (ADR-0029): decks B–D's input blocks, levels and crossfader.
    pub fn deck(&mut self) -> &mut DeckMixer {
        &mut self.deck
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
