//! The voice pool (spec 006, ADR-0011): each synth owns `MAX_VOICES` voices.
//!
//! A *monophonic* synth (`Polyphony` 1) keeps one voice per owner (live keys,
//! each MIDI channel), exactly as before the pool: the voice holds that owner's
//! key stack and glides, and its noise seed is the owner's. A *polyphonic* synth
//! presses every note on a voice of its own: a free one, rotating from the last
//! used, else the one in release that has sounded longest, else the oldest held
//! note. A voice remembers its owner and note, so a note-off finds its own
//! voice and live keys and MIDI channels do not release each other.
//!
//! A drum kit (`Model::Tr808`) hits pads instead: a key plays the pad General
//! MIDI puts there, on the voice already playing that pad or a new one, with
//! no note-off, and the closed hat chokes the open hat.
//!
//! A poly synth's pool runs one LFO and one sample-and-hold for all its voices.
//! Everything is allocated up front (ADR-0002): the voices are an enum with no
//! heap data, so a model change rebuilds one without allocating.

use crate::chip::nes_voice::NesVoice;
use crate::chip::sid_voice::SidVoice;
use crate::chip::vl_voice::VlVoice;
use crate::drums::{ACCENT_VELOCITY, Pad, PadVoice};
use crate::engine::{BLOCK, SYNTHS};
use crate::fm::FmVoice;
use crate::la::LaVoice;
use crate::mixer::GROUPS;
use crate::modular::{GraphVoice, VoiceState};
use crate::mono::MonoParams;
use crate::mono::lfo::Lfo;
use crate::mono::noise::Noise;
use crate::mono::osc::Blep;
use crate::mono::voice::{MonoCtx, MonoVoice, SharedMod, Tools, VoiceSet};
use crate::padsampler::{PadVoice as SampledPad, pad_of};
use crate::params::Param;
use crate::sample::Playback;
use crate::sample::SampleStore;
use crate::sampler::SamplerVoice;
use crate::synth::Engine;
use crate::voice::{Owner, lookup, wrap};

/// Voices in a pool.
pub const MAX_VOICES: usize = 16;
/// Level of the A-440 reference tone (#308): a sine well under one VCO.
const A440_LEVEL: f32 = 0.25;
/// Voices that may sound at once across every synth (spec 006 Req 5).
pub const VOICE_BUDGET: usize = 64;

/// One voice of a pool: the Mono voice now; other engines join as further variants.
#[derive(Clone, Copy)]
// Boxing the larger variant would allocate when a voice is rebuilt on a model change; the
// pool is allocated once and its voices are plain values (ADR-0011).
#[allow(clippy::large_enum_variant)]
pub enum PolyVoice {
    Mono(MonoVoice),
    /// The D-50's two-partial voice.
    La(LaVoice),
    /// The DX7's six-operator FM voice.
    Fm(FmVoice),
    /// A pad of the drum kit.
    Drum(PadVoice),
    /// The multisampler's voice.
    Sampler(SamplerVoice),
    /// A pad of the drum/pad sampler.
    Pad(SampledPad),
    /// A Modular synth's voice: a graph of unit generators (ADR-0020).
    Graph(GraphVoice),
    /// A SID chip (#112).
    Sid(SidVoice),
    /// A NES 2A03 (#112).
    Nes(NesVoice),
    /// A Casio VL-1 (#723).
    Vl(VlVoice),
}

impl PolyVoice {
    /// A voice of the kind a synth's model plays with.
    fn new(seed: u32, p: Option<&MonoParams>) -> PolyVoice {
        let Some(p) = p else {
            return PolyVoice::Mono(MonoVoice::new(seed));
        };
        match p.model.def().engine {
            Engine::Mono => PolyVoice::Mono(MonoVoice::new(seed)),
            Engine::La => PolyVoice::La(LaVoice::new()),
            Engine::Fm => PolyVoice::Fm(FmVoice::new(p.sample_rate())),
            Engine::Drums(_) => PolyVoice::Drum(PadVoice::new(Pad::Bd, seed)),
            Engine::Sampler => PolyVoice::Sampler(SamplerVoice::new()),
            Engine::Pads => PolyVoice::Pad(SampledPad::default()),
            Engine::Graph => PolyVoice::Graph(GraphVoice::new(seed)),
            Engine::Sid => PolyVoice::Sid(SidVoice::new()),
            Engine::Nes => PolyVoice::Nes(NesVoice::new()),
            Engine::Vl => PolyVoice::Vl(VlVoice::new()),
        }
    }

    /// Whether this voice is of the kind `p`'s model needs.
    fn fits(&self, p: &MonoParams) -> bool {
        matches!(
            (self, p.model.def().engine),
            (PolyVoice::Mono(_), Engine::Mono)
                | (PolyVoice::La(_), Engine::La)
                | (PolyVoice::Fm(_), Engine::Fm)
                | (PolyVoice::Drum(_), Engine::Drums(_))
                | (PolyVoice::Sampler(_), Engine::Sampler)
                | (PolyVoice::Pad(_), Engine::Pads)
                | (PolyVoice::Graph(_), Engine::Graph)
                | (PolyVoice::Sid(_), Engine::Sid)
                | (PolyVoice::Nes(_), Engine::Nes)
                | (PolyVoice::Vl(_), Engine::Vl)
        )
    }

    /// Sounding: gated, releasing, or about to start.
    pub fn active(&self) -> bool {
        match self {
            PolyVoice::Mono(v) => v.active(),
            PolyVoice::La(v) => v.active(),
            PolyVoice::Fm(v) => v.active(),
            PolyVoice::Drum(v) => v.active(),
            PolyVoice::Sampler(v) => v.active(),
            PolyVoice::Pad(v) => v.active(),
            PolyVoice::Graph(v) => v.active(),
            PolyVoice::Sid(v) => v.active(),
            PolyVoice::Nes(v) => v.active(),
            PolyVoice::Vl(v) => v.active(),
        }
    }

    /// A key is held on it.
    pub fn gated(&self) -> bool {
        match self {
            PolyVoice::Mono(v) => v.gated(),
            PolyVoice::La(v) => v.gated(),
            PolyVoice::Fm(v) => v.gated(),
            // A hit has no key to hold.
            PolyVoice::Drum(_) => false,
            PolyVoice::Sampler(v) => v.gated(),
            // A pad has no key to hold.
            PolyVoice::Pad(_) => false,
            PolyVoice::Graph(v) => v.gated(),
            PolyVoice::Sid(v) => v.gated(),
            PolyVoice::Nes(v) => v.gated(),
            PolyVoice::Vl(v) => v.gated(),
        }
    }

    fn release_all(&mut self) {
        match self {
            PolyVoice::Mono(v) => v.release_all(),
            PolyVoice::La(v) => v.release_all(),
            PolyVoice::Fm(v) => v.release_all(),
            // A hit rings out.
            PolyVoice::Drum(_) => {}
            PolyVoice::Sampler(v) => v.release_all(),
            PolyVoice::Pad(v) => v.release_all(),
            PolyVoice::Graph(v) => v.release_all(),
            PolyVoice::Sid(v) => v.release_all(),
            PolyVoice::Nes(v) => v.release_all(),
            PolyVoice::Vl(v) => v.release_all(),
        }
    }

    /// Press a key on a voice of the right kind (a voice of another kind is replaced).
    /// `slot` is the voice's place in its pool, for a Modular voice's lists.
    fn press(&mut self, note: u8, velocity: f32, p: &MonoParams, seed: u32, slot: usize) {
        if !self.fits(p) {
            *self = PolyVoice::new(seed, Some(p));
        }
        match self {
            PolyVoice::Mono(v) => v.press(note, velocity, p),
            PolyVoice::La(v) => v.press(note, velocity),
            PolyVoice::Fm(v) => v.press(note, velocity, p),
            PolyVoice::Drum(v) => strike(v, note, velocity, p),
            PolyVoice::Sampler(v) => v.press(note, velocity),
            PolyVoice::Pad(v) => v.press(note, velocity, &p.pad_kit, p.level[0], p.sample_rate()),
            PolyVoice::Graph(v) => v.press(note, velocity, &p.graph, p.sample_rate(), slot),
            PolyVoice::Sid(v) => v.press(note, velocity),
            PolyVoice::Nes(v) => v.press(note, velocity),
            PolyVoice::Vl(v) => v.press(note, velocity),
        }
    }

    fn release(&mut self, note: u8, p: &MonoParams) {
        match self {
            PolyVoice::Mono(v) => v.release(note, p),
            PolyVoice::La(v) => v.release_all(),
            PolyVoice::Fm(v) => v.release_all(),
            PolyVoice::Drum(_) => {}
            PolyVoice::Sampler(v) => v.release_all(),
            PolyVoice::Pad(v) => v.release(note),
            PolyVoice::Graph(v) => v.release_all(),
            PolyVoice::Sid(v) => v.release_all(),
            PolyVoice::Nes(v) => v.release_all(),
            PolyVoice::Vl(v) => v.release_all(),
        }
    }

    fn set_trim(&mut self, trim: f32, cutoff: f32) {
        match self {
            PolyVoice::Mono(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            PolyVoice::La(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            PolyVoice::Fm(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            // The kit's pads are tuned by their knobs, not by drift.
            PolyVoice::Drum(_) => {}
            PolyVoice::Sampler(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            // A pad is tuned by its own knob.
            PolyVoice::Pad(_) => {}
            PolyVoice::Graph(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            PolyVoice::Sid(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            PolyVoice::Nes(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            PolyVoice::Vl(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
        }
    }
}

/// Hit the pad `note` plays on `v` at `velocity`, accented from `ACCENT_VELOCITY` up.
fn strike(v: &mut PadVoice, note: u8, velocity: f32, p: &MonoParams) {
    let pad = Pad::from_gm(note);
    // A machine plays its nearest voice for a pad it lacks, with that voice's knobs (#148).
    let machine = p.model.drum_machine();
    let knobs = p
        .drums
        .get(machine.voice(pad) as usize)
        .copied()
        .unwrap_or_default();
    v.set_machine(machine);
    let gain = knobs.gain(velocity, velocity >= ACCENT_VELOCITY, p.drum_accent) * machine.makeup();
    v.set_pad(pad);
    v.trigger(&knobs, gain, p.sample_rate());
}

/// Who a voice plays for and what it plays.
#[derive(Clone, Copy, Default)]
struct Slot {
    owner: Option<Owner>,
    note: u8,
    /// The engine's note counter when it started: lower is older.
    age: u64,
    /// This voice's unison detune in cents (0 outside unison).
    cents: f32,
    /// Cents its note plays off the key (#699): `.add(0.1)`, `.add(perlin)`.
    fine: f32,
    /// Where its note sounds, −1 to 1, and its left and right gains, set
    /// when it is pressed; 0 and `None` where the strip puts it (#701).
    pan: f32,
    gains: Option<(f32, f32)>,
    /// Frames since its note began, and since its key was let go (ADR-0023).
    since: u32,
    released: Option<u32>,
    /// Its note's own pitch and filter envelopes (#744).
    shape: Shape,
    /// Its pitch and cutoff trim this block before its pitch envelope, so a
    /// sub-block can add the envelope's value there (#754).
    base: f32,
    cut: f32,
}

/// Samples in a sub-block of a voice whose pitch envelope is moving (#754):
/// its trim is set again before each, a quarter of the control rate's
/// block (`BLOCK`), 0.67 ms at 48 kHz.
pub const PITCH_SUB: usize = 32;

/// A note's own pitch and filter envelopes, as Strudel's `penv` and `lpenv`
/// (#744): depths in semitones and octaves, times in seconds. Set when the
/// note is pressed, and added to its voices' pitch and cutoff trim each
/// block, so every model that reads the trim plays them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shape {
    pub penv: f32,
    pub pattack: f32,
    pub pdecay: f32,
    pub lpenv: f32,
    pub lpattack: f32,
    pub lpdecay: f32,
    pub lpsustain: f32,
    pub lprelease: f32,
}

impl Shape {
    /// No envelope, its times what a depth alone gets: a pitch that lands
    /// on its note in 0.1 s, and Strudel's filter envelope.
    pub const PLAIN: Shape = Shape {
        penv: 0.0,
        pattack: 0.0,
        pdecay: 0.1,
        lpenv: 0.0,
        lpattack: 0.005,
        lpdecay: 0.14,
        lpsustain: 0.0,
        lprelease: 0.1,
    };

    /// Semitones of pitch and of cutoff `since` seconds into the note, its
    /// key up for the last `released` seconds. The pitch goes `penv` off
    /// over the attack and back to the note over the decay, so with no
    /// attack it starts there; the cutoff's envelope rises, falls to its
    /// sustain and, once the key is up, to nothing over the release.
    pub fn at(&self, since: f32, released: Option<f32>) -> (f32, f32) {
        let pitch = if self.penv == 0.0 {
            0.0
        } else {
            self.penv * ads(since, self.pattack, self.pdecay, 0.0)
        };
        let cut = if self.lpenv == 0.0 {
            0.0
        } else {
            let level = match released {
                None => ads(since, self.lpattack, self.lpdecay, self.lpsustain),
                Some(r) => {
                    let held = ads(
                        (since - r).max(0.0),
                        self.lpattack,
                        self.lpdecay,
                        self.lpsustain,
                    );
                    if self.lprelease > 0.0 {
                        held * (1.0 - r / self.lprelease).max(0.0)
                    } else {
                        0.0
                    }
                }
            };
            12.0 * self.lpenv * level
        };
        (pitch, cut)
    }
}

/// An attack-decay-sustain level, 0 to 1, `t` seconds in: up over `a`, down
/// to `s` over `d`, then `s`.
fn ads(t: f32, a: f32, d: f32, s: f32) -> f32 {
    if t < a {
        t / a
    } else if t < a + d {
        1.0 - (1.0 - s) * (t - a) / d
    } else {
        s
    }
}

/// A number in −1..=1 that depends only on `i` and `salt`: a voice's own, repeatable offset.
fn unit(i: usize, salt: u32) -> f32 {
    let mut x = (i as u32 + 1).wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// The gains of the middle, equal-power: where a mono strip puts a voice
/// at its centre, so a synth gone stereo for `jux` keeps its level (#701).
const MIDDLE: (f32, f32) = (
    std::f32::consts::FRAC_1_SQRT_2,
    std::f32::consts::FRAC_1_SQRT_2,
);

/// The largest static detune of an analog voice, in cents, and of its drift; and its cutoff offset in semitones.
const ANALOG_CENTS: f32 = 6.0;
const DRIFT_CENTS: f32 = 3.0;
const ANALOG_CUTOFF: f32 = 0.5;

/// The seed of an owner's noise, as it was when each owner had a voice of its own.
fn owner_seed(owner: Owner) -> u32 {
    let index = match owner {
        Owner::Live(s) => usize::from(s),
        // 16 past the synths: the MIDI player's channels were seeded there (ADR-0022).
        Owner::Track(t) => SYNTHS + 16 + usize::from(t),
    };
    (index as u32 + 1).wrapping_mul(2_654_435_761)
}

pub struct Pool {
    voices: [PolyVoice; MAX_VOICES],
    slots: [Slot; MAX_VOICES],
    /// The voice a poly note took last, so the next rotates on from it.
    last: usize,
    /// The shared LFO and sample-and-hold of a poly synth.
    lfo: Lfo,
    noise: Noise,
    shared: SharedMod,
    /// Each voice's slow drift in cents, and the generator that moves it.
    drift: [f32; MAX_VOICES],
    rng: u32,
    /// The keys a unison synth holds, oldest first, to fall back on when the sounding one ends.
    keys: [(Option<Owner>, u8, f32); MAX_VOICES],
    /// One block for a drum pad that goes to a group (#162), before it is panned there.
    scratch: [f32; BLOCK],
    /// A Modular synth's voices' state, one per slot, sized with its
    /// program outside `render` (ADR-0024).
    graph: Vec<VoiceState>,
    nkeys: usize,
    /// Each voice slot's own values, from per-voice signals (ADR-0023).
    sets: [VoiceSet; MAX_VOICES],
    /// Phase of the A-440 reference tone (#308).
    a440: f32,
    /// The pan of the note about to be pressed, so a note re-uses only a
    /// voice on its own side (#701).
    aim: f32,
}

impl Pool {
    pub fn new(index: usize) -> Pool {
        let seed = |i: usize| ((index * MAX_VOICES + i) as u32 + 1).wrapping_mul(0x9E37_79B1);
        Pool {
            voices: std::array::from_fn(|i| PolyVoice::new(seed(i), None)),
            slots: [Slot::default(); MAX_VOICES],
            last: MAX_VOICES - 1,
            lfo: Lfo::default(),
            noise: Noise::new(seed(MAX_VOICES) | 1),
            shared: SharedMod::new(),
            drift: [0.0; MAX_VOICES],
            rng: seed(MAX_VOICES + 1) | 1,
            keys: [(None, 0, 0.0); MAX_VOICES],
            scratch: [0.0; BLOCK],
            // Room for the saw a Modular synth plays before it has a program.
            graph: vec![VoiceState::for_program(&crate::modular::Program::default()); MAX_VOICES],
            nkeys: 0,
            sets: [VoiceSet::default(); MAX_VOICES],
            a440: 0.0,
            aim: 0.0,
        }
    }

    /// Voice slot `i` while it sounds: seconds since its note began, and
    /// since its key was let go (ADR-0023).
    pub fn voice_times(&self, i: usize, sample_rate: f32) -> Option<(f32, Option<f32>)> {
        let (v, s) = (self.voices.get(i)?, self.slots.get(i)?);
        if !v.active() {
            return None;
        }
        let secs = |n: u32| n as f32 / sample_rate;
        Some((secs(s.since), s.released.map(secs)))
    }

    /// Give voice slot `i` its own value of `param`; false for a parameter a
    /// voice has no slot for.
    pub fn set_voice(&mut self, i: usize, param: Param, value: f32) -> bool {
        self.sets.get_mut(i).is_some_and(|s| s.write(param, value))
    }

    /// Voice slot `i`'s own value of `param`, as the voice reads it.
    #[cfg(test)]
    pub fn voice_value(&self, i: usize, param: Param) -> Option<f32> {
        self.sets.get(i)?.get(param)
    }

    /// The voice slots the note numbered `clock` pressed: one, or its unison.
    pub fn pressed(&self, clock: u64) -> impl Iterator<Item = usize> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter(move |(_, s)| s.owner.is_some() && s.age == clock)
            .map(|(i, _)| i)
    }

    /// The next note pressed sounds at `pan` (#701): `place` it after.
    pub fn aim(&mut self, pan: f32) {
        self.aim = pan;
    }

    /// The note numbered `clock` plays `cents` off its key (#699) and at
    /// `pan`, −1 to 1, its gains equal-power, worked out here once (#701).
    pub fn place(&mut self, clock: u64, cents: f32, pan: f32) {
        self.aim = 0.0;
        let gains = (pan != 0.0).then(|| {
            let a = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
            (a.cos(), a.sin())
        });
        for s in &mut self.slots {
            if s.owner.is_some() && s.age == clock {
                s.fine = cents;
                s.pan = pan;
                s.gains = gains;
                s.shape = Shape::default();
            }
        }
    }

    /// The note numbered `clock` bends its pitch and cutoff as `shape` says
    /// (#744): `place` it first.
    pub fn shape(&mut self, clock: u64, shape: Shape) {
        for s in &mut self.slots {
            if s.owner.is_some() && s.age == clock {
                s.shape = shape;
            }
        }
    }

    /// The note numbered `clock` plays its sample as `play` says (#716–
    /// #718): on the pad sampler and the sampler, which find their frames as
    /// they start; other voices play on.
    pub fn play(&mut self, clock: u64, play: &Playback) {
        for (s, v) in self.slots.iter().zip(self.voices.iter_mut()) {
            if s.owner.is_some() && s.age == clock {
                match v {
                    PolyVoice::Pad(p) => p.set_play(play),
                    PolyVoice::Sampler(p) => p.set_play(play),
                    _ => {}
                }
            }
        }
    }

    /// Whether a sounding note has a pan of its own, so the pool renders
    /// to a stereo bus (#701).
    pub fn panned(&self) -> bool {
        self.slots
            .iter()
            .zip(&self.voices)
            .any(|(s, v)| s.gains.is_some() && v.active())
    }

    /// The slots the note numbered `clock` pressed drop the envelope times
    /// of the note they played before (ADR-0038).
    pub fn fresh_note_times(&mut self, clock: u64) {
        for (slot, set) in self.slots.iter().zip(self.sets.iter_mut()) {
            if slot.owner.is_some() && slot.age == clock {
                set.clear_note_times();
            }
        }
    }

    /// Every voice goes back to the synth's value of `param`.
    pub fn clear_voices(&mut self, param: Param) {
        for s in self.sets.iter_mut() {
            s.clear(param);
        }
    }

    /// Every voice goes back to the synth's values.
    pub fn clear_all_voices(&mut self) {
        self.sets = [VoiceSet::default(); MAX_VOICES];
    }

    /// Room in every slot for a Modular program's state; allocates, so it
    /// runs when the program is set, never in `render`.
    pub fn size_graph(&mut self, prog: &crate::modular::Program, sample_rate: f32) {
        for st in self.graph.iter_mut() {
            st.grow(prog, sample_rate);
        }
    }

    /// Voices sounding now.
    pub fn active(&self) -> usize {
        self.voices.iter().filter(|v| v.active()).count()
    }

    /// The notes held on the voices, lowest first (for tests).
    #[cfg(test)]
    pub fn held_notes(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self
            .slots
            .iter()
            .zip(&self.voices)
            .filter(|(_, v)| v.gated())
            .map(|(s, _)| s.note)
            .collect();
        v.sort_unstable();
        v
    }

    /// Voices with a key held (for tests).
    pub fn held(&self) -> usize {
        self.voices.iter().filter(|v| v.gated()).count()
    }

    /// The Mono voice playing for `owner`, if it has one.
    pub fn voice(&self, owner: Owner) -> Option<&MonoVoice> {
        let i = self.slots.iter().position(|s| s.owner == Some(owner))?;
        match self.voices.get(i)? {
            PolyVoice::Mono(v) => Some(v),
            PolyVoice::La(_)
            | PolyVoice::Fm(_)
            | PolyVoice::Drum(_)
            | PolyVoice::Sampler(_)
            | PolyVoice::Pad(_)
            | PolyVoice::Graph(_)
            | PolyVoice::Sid(_)
            | PolyVoice::Nes(_)
            | PolyVoice::Vl(_) => None,
        }
    }

    /// The voice in release that has sounded longest: its slot and age.
    pub fn oldest_release(&self) -> Option<(usize, u64)> {
        self.slots
            .iter()
            .zip(&self.voices)
            .enumerate()
            .filter(|(_, (_, v))| v.active() && !v.gated())
            .map(|(i, (s, _))| (i, s.age))
            .min_by_key(|(_, age)| *age)
    }

    /// The held voice that has sounded longest: its slot.
    fn oldest_held(&self, limit: usize) -> Option<usize> {
        self.slots
            .iter()
            .zip(&self.voices)
            .enumerate()
            .take(limit)
            .filter(|(_, (_, v))| v.gated())
            .map(|(i, (s, _))| (i, s.age))
            .min_by_key(|(_, age)| *age)
            .map(|(i, _)| i)
    }

    /// Silence a voice at once and free its slot (a stolen voice restarts clean).
    pub fn silence(&mut self, i: usize) {
        if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
            *v = PolyVoice::new(owner_seed_of_slot(i, s.owner), None);
            *s = Slot::default();
        }
    }

    /// Silence the oldest held note; false when nothing is held.
    pub fn silence_oldest_held(&mut self) -> bool {
        match self.oldest_held(MAX_VOICES) {
            Some(i) => {
                self.silence(i);
                true
            }
            None => false,
        }
    }

    /// Whether pressing `note` for `owner` would add a sounding voice, rather than
    /// re-use or steal one of this pool's own.
    pub fn adds_a_voice(&self, owner: Owner, note: u8, p: &MonoParams) -> bool {
        if p.model.uses_drums() {
            return self.playing(Pad::from_gm(note)).is_none();
        }
        if p.model.uses_pads() {
            return pad_of(note).is_some_and(|pad| self.playing_pad(pad).is_none());
        }
        let limit = p.voices();
        if limit == 1 {
            return !self
                .slots
                .iter()
                .zip(&self.voices)
                .any(|(s, v)| s.owner == Some(owner) && v.active());
        }
        let again = self
            .slots
            .iter()
            .zip(&self.voices)
            .take(limit)
            .any(|(s, v)| s.owner == Some(owner) && s.note == note && v.gated());
        !again && self.voices.iter().take(limit).any(|v| !v.active())
    }

    /// Press `note` for `owner`. A monophonic synth keeps the owner's voice and
    /// its key stack; a polyphonic one gives the note a voice of its own.
    pub fn note_on(&mut self, owner: Owner, note: u8, velocity: f32, p: &MonoParams, clock: u64) {
        let limit = p.voices();
        if p.model.uses_drums() {
            self.hit(owner, note, velocity, p, clock);
        } else if p.model.uses_pads() {
            self.hit_pad(owner, note, velocity, p, clock);
        } else if limit == 1 {
            self.press_mono(owner, note, velocity, p, clock);
        } else if p.unison {
            self.remember(owner, note, velocity);
            self.sound_unison(owner, note, velocity, p, clock, limit);
        } else {
            self.press_poly(owner, note, velocity, p, clock, limit);
        }
    }

    /// Hold `note` in the unison key stack (a note held again moves to the top).
    fn remember(&mut self, owner: Owner, note: u8, velocity: f32) {
        self.forget(owner, note);
        if self.nkeys == MAX_VOICES {
            self.keys.copy_within(1.., 0);
            self.nkeys -= 1;
        }
        if let Some(k) = self.keys.get_mut(self.nkeys) {
            *k = (Some(owner), note, velocity);
            self.nkeys += 1;
        }
    }

    fn forget(&mut self, owner: Owner, note: u8) {
        let held = self.keys.get(..self.nkeys).unwrap_or(&[]);
        if let Some(i) = held
            .iter()
            .position(|(o, n, _)| *o == Some(owner) && *n == note)
        {
            if let Some(tail) = self.keys.get_mut(i..self.nkeys) {
                tail.rotate_left(1);
            }
            self.nkeys -= 1;
        }
    }

    /// Every voice of the pool plays `note`, spread in pitch by the unison detune.
    fn sound_unison(
        &mut self,
        owner: Owner,
        note: u8,
        velocity: f32,
        p: &MonoParams,
        clock: u64,
        limit: usize,
    ) {
        for i in 0..limit {
            let cents = if limit > 1 {
                let mid = (limit - 1) as f32 / 2.0;
                p.unison_cents * (i as f32 - mid) / mid
            } else {
                0.0
            };
            if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
                v.release_all();
                v.press(note, velocity, p, owner_seed(owner), i);
                *s = Slot {
                    owner: Some(owner),
                    note,
                    age: clock,
                    cents,
                    ..Slot::default()
                };
            }
        }
        self.last = limit - 1;
    }

    /// The owner's voice, or a free one made for it (else the oldest).
    fn press_mono(&mut self, owner: Owner, note: u8, velocity: f32, p: &MonoParams, clock: u64) {
        let i = match self.slots.iter().position(|s| s.owner == Some(owner)) {
            Some(i) => i,
            None => {
                let i = self
                    .voices
                    .iter()
                    .position(|v| !v.active())
                    .or_else(|| self.oldest_release().map(|(i, _)| i))
                    .or_else(|| self.oldest_held(MAX_VOICES))
                    .unwrap_or(0);
                if let Some(v) = self.voices.get_mut(i) {
                    *v = PolyVoice::new(owner_seed(owner), Some(p));
                }
                i
            }
        };
        if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
            v.press(note, velocity, p, owner_seed(owner), i);
            *s = Slot {
                owner: Some(owner),
                note,
                age: clock,
                cents: 0.0,
                ..Slot::default()
            };
        }
    }

    fn press_poly(
        &mut self,
        owner: Owner,
        note: u8,
        velocity: f32,
        p: &MonoParams,
        clock: u64,
        limit: usize,
    ) {
        // The same note again on its owner re-uses its voice, and retriggers.
        let again = self
            .slots
            .iter()
            .zip(&self.voices)
            .take(limit)
            .position(|(s, v)| {
                s.owner == Some(owner) && s.note == note && s.pan == self.aim && v.gated()
            });
        let i = match again {
            Some(i) => {
                if let Some(v) = self.voices.get_mut(i) {
                    v.release_all();
                }
                i
            }
            None => self.allocate(limit),
        };
        if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
            v.press(note, velocity, p, owner_seed(owner), i);
            *s = Slot {
                owner: Some(owner),
                note,
                age: clock,
                cents: 0.0,
                ..Slot::default()
            };
        }
        self.last = i;
    }

    /// The voice sounding sampled pad `pad`, if one is.
    fn playing_pad(&self, pad: usize) -> Option<usize> {
        self.voices
            .iter()
            .position(|v| matches!(v, PolyVoice::Pad(d) if d.active() && d.pad() == pad))
    }

    /// Hit the sampled pad `note` plays: again on the voice already playing it, else on
    /// a new one; the other pads of its choke group fall silent.
    fn hit_pad(&mut self, owner: Owner, note: u8, velocity: f32, p: &MonoParams, clock: u64) {
        let Some(pad) = pad_of(note) else {
            return;
        };
        let group = p.pad_kit.pad(pad).map_or(0, |c| c.choke);
        if group != 0 {
            for v in self.voices.iter_mut() {
                if let PolyVoice::Pad(d) = v {
                    if d.pad() != pad && d.choke_group() == group {
                        d.choke();
                    }
                }
            }
        }
        let i = match self.playing_pad(pad) {
            Some(i) => i,
            None => self.allocate(MAX_VOICES),
        };
        if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
            v.press(note, velocity, p, owner_seed(owner), i);
            *s = Slot {
                owner: Some(owner),
                note,
                age: clock,
                cents: 0.0,
                ..Slot::default()
            };
        }
        self.last = i;
    }

    /// The drum kit's voices (#162): each pad panned by its `PadOut`, into
    /// the kit's stereo bus or into its group's direct input from `at` (#364).
    pub fn render_kit(
        &mut self,
        p: &MonoParams,
        sine: &[f32],
        blep: &Blep,
        (left, right): (&mut [f32], &mut [f32]),
        direct: &mut [[[f32; BLOCK]; 2]; GROUPS],
        at: usize,
    ) {
        let n = left.len();
        for v in self.voices.iter_mut() {
            let PolyVoice::Drum(d) = v else {
                continue;
            };
            if !d.active() {
                continue;
            }
            let out = p
                .pad_outs
                .get(p.model.drum_machine().voice(d.pad()) as usize)
                .copied()
                .unwrap_or_default();
            // Panned into the kit's own strip, or straight into a group (#364).
            let (gl, gr) = match out.group.checked_sub(1) {
                None => (left.get_mut(..n), right.get_mut(..n)),
                Some(group) => match direct.get_mut(group) {
                    Some([gl, gr]) => (gl.get_mut(at..at + n), gr.get_mut(at..at + n)),
                    None => continue,
                },
            };
            let (Some(s), Some(gl), Some(gr)) = (self.scratch.get_mut(..n), gl, gr) else {
                continue;
            };
            s.fill(0.0);
            d.render(sine, blep, s);
            let [pl, pr] = out.gains;
            for ((x, l), r) in s.iter().zip(gl.iter_mut()).zip(gr.iter_mut()) {
                *l += x * pl;
                *r += x * pr;
            }
        }
    }

    /// Add every sounding pad into `left` and `right` (the drum/pad sampler's stereo bus).
    pub fn render_pads(
        &mut self,
        samples: &SampleStore,
        left: &mut [f32],
        right: &mut [f32],
        direct: &mut [[[f32; BLOCK]; 2]; GROUPS],
        at: usize,
    ) {
        let n = left.len();
        for v in self.voices.iter_mut() {
            let PolyVoice::Pad(d) = v else {
                continue;
            };
            if !d.active() {
                continue;
            }
            // A pad with a group as its out goes straight in, panned as it is (#220).
            let Some(group) = d.out().checked_sub(1) else {
                d.render(samples, left, right);
                continue;
            };
            let Some([gl, gr]) = direct.get_mut(group) else {
                continue;
            };
            let (Some(gl), Some(gr)) = (gl.get_mut(at..at + n), gr.get_mut(at..at + n)) else {
                continue;
            };
            d.render(samples, gl, gr);
        }
    }

    /// The voice sounding `pad`, if one is.
    fn playing(&self, pad: Pad) -> Option<usize> {
        self.voices
            .iter()
            .position(|v| matches!(v, PolyVoice::Drum(d) if d.active() && d.pad() == pad))
    }

    /// Hit the pad `note` plays: again on the voice already playing it (a pad
    /// retriggers, as on the 808), else on a new one. A closed hat chokes the
    /// open hat.
    fn hit(&mut self, owner: Owner, note: u8, velocity: f32, p: &MonoParams, clock: u64) {
        let pad = Pad::from_gm(note);
        if pad == Pad::Ch {
            for v in self.voices.iter_mut() {
                if let PolyVoice::Drum(d) = v {
                    if d.pad() == Pad::Oh {
                        d.choke();
                    }
                }
            }
        }
        let i = match self.playing(pad) {
            Some(i) => i,
            None => self.allocate(MAX_VOICES),
        };
        if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
            v.press(note, velocity, p, owner_seed(owner), i);
            *s = Slot {
                owner: Some(owner),
                note,
                age: clock,
                cents: 0.0,
                ..Slot::default()
            };
        }
        self.last = i;
    }

    /// A voice for a new note among the first `limit`: free, rotating on from the
    /// last one used; else the one in release longest; else the oldest held note.
    fn allocate(&mut self, limit: usize) -> usize {
        let free = (1..=limit)
            .map(|k| (self.last + k) % limit)
            .find(|i| self.voices.get(*i).is_some_and(|v| !v.active()));
        if let Some(i) = free {
            return i;
        }
        let steal = self
            .slots
            .iter()
            .zip(&self.voices)
            .enumerate()
            .take(limit)
            .filter(|(_, (_, v))| !v.gated())
            .map(|(i, (s, _))| (i, s.age))
            .min_by_key(|(_, age)| *age)
            .map(|(i, _)| i)
            .or_else(|| self.oldest_held(limit))
            .unwrap_or(0);
        self.silence(steal);
        steal
    }

    /// Release `note` for `owner`: a monophonic owner's voice falls back to its next
    /// held key; a polyphonic note releases the voice that plays it.
    pub fn note_off(&mut self, owner: Owner, note: u8, p: &MonoParams) {
        if p.model.uses_drums() {
            return;
        }
        if p.model.uses_pads() {
            for v in self.voices.iter_mut() {
                v.release(note, p);
            }
            return;
        }
        if p.voices() > 1 && p.unison {
            self.forget(owner, note);
            let sounding = self
                .slots
                .iter()
                .zip(&self.voices)
                .any(|(s, v)| s.owner == Some(owner) && s.note == note && v.gated());
            if !sounding {
                return;
            }
            // The last key still held takes over; with none, the voices release.
            let next = self
                .keys
                .get(..self.nkeys)
                .unwrap_or(&[])
                .iter()
                .rev()
                .find(|(o, _, _)| *o == Some(owner))
                .copied();
            match next {
                Some((_, n, vel)) => {
                    let age = self.slots.iter().map(|s| s.age).max().unwrap_or(0);
                    self.sound_unison(owner, n, vel, p, age, p.voices());
                }
                None => self.release_owner(owner),
            }
            return;
        }
        if p.voices() <= 1 {
            if let Some(i) = self.slots.iter().position(|s| s.owner == Some(owner)) {
                if let Some(v) = self.voices.get_mut(i) {
                    v.release(note, p);
                }
            }
            return;
        }
        let held = self
            .slots
            .iter()
            .zip(&self.voices)
            .position(|(s, v)| s.owner == Some(owner) && s.note == note && v.gated());
        if let Some(v) = held.and_then(|i| self.voices.get_mut(i)) {
            v.release(note, p);
        }
    }

    /// Release every voice `owner` holds.
    pub fn release_owner(&mut self, owner: Owner) {
        while self
            .keys
            .get(..self.nkeys)
            .is_some_and(|k| k.iter().any(|(o, _, _)| *o == Some(owner)))
        {
            let n = self.keys.get(..self.nkeys).and_then(|k| {
                k.iter()
                    .find(|(o, _, _)| *o == Some(owner))
                    .map(|(_, n, _)| *n)
            });
            match n {
                Some(n) => self.forget(owner, n),
                None => break,
            }
        }
        for (s, v) in self.slots.iter().zip(self.voices.iter_mut()) {
            if s.owner == Some(owner) {
                v.release_all();
            }
        }
    }

    pub fn release_all(&mut self) {
        self.nkeys = 0;
        for v in self.voices.iter_mut() {
            v.release_all();
        }
    }

    /// Each voice's pitch and cutoff trim for the next block (`BLOCK`, the
    /// control rate, ADR-0042): its unison detune and,
    /// with `Analog`, its own static detune, a slow drift and a cutoff offset,
    /// as far as the model's oscillators wander (#339). A monosynth's VCOs
    /// drift too; a drum machine's pads are left alone.
    fn retrim(&mut self, p: &MonoParams, poly: bool) {
        let osc = p.osc();
        let sr = p.sample_rate().max(1.0);
        for (i, (v, s)) in self
            .voices
            .iter_mut()
            .zip(self.slots.iter_mut())
            .enumerate()
        {
            if p.model.uses_drums() {
                v.set_trim(0.0, 0.0);
                continue;
            }
            let unison = if poly { s.cents } else { 0.0 };
            // Its note's cents (#699) and the instrument's detune (#21).
            let mut trim = (s.fine + p.detune) / 100.0;
            // xorshift: a step of drift in −1..=1.
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 17;
            self.rng ^= self.rng << 5;
            let step = (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            if let Some(d) = self.drift.get_mut(i) {
                *d = (*d * 0.998 + step * 0.12).clamp(-DRIFT_CENTS, DRIFT_CENTS);
                let own = unit(i, 1) * ANALOG_CENTS * osc.detune + *d * osc.drift;
                trim += (unison + own * p.analog) / 100.0;
            }
            // A note's own envelopes (#744), in semitones.
            let (bend, open) = s
                .shape
                .at(s.since as f32 / sr, s.released.map(|r| r as f32 / sr));
            s.base = trim;
            s.cut = unit(i, 2) * ANALOG_CUTOFF * p.analog + open;
            v.set_trim(trim + bend, s.cut);
        }
    }

    /// Add every sounding voice into `out`.
    pub fn render(&mut self, p: &MonoParams, tools: Tools, out: &mut [f32]) {
        self.render_into(p, tools, out, None);
    }

    /// The same into two sides: a stereo Modular program's voices write
    /// both, every other voice the left (ADR-0024).
    pub fn render_into(
        &mut self,
        p: &MonoParams,
        tools: Tools,
        out: &mut [f32],
        mut right: Option<&mut [f32]>,
    ) {
        let poly = p.voices() > 1 && !p.model.uses_drums();
        self.retrim(p, poly);
        if poly {
            self.shared
                .fill(&mut self.lfo, &mut self.noise, p, tools.sine, out.len());
        }
        let ctx = MonoCtx {
            params: p,
            sine: tools.sine,
            blep: tools.blep,
            ladder: tools.ladder,
            pitch: tools.pitch,
            shared: if poly { Some(&self.shared) } else { None },
            tables: tools.tables,
        };
        let n = out.len() as u32;
        // With a pan of its own on any note, each voice goes through the
        // scratch block to both sides at its gains, the rest at the middle's
        // (#701); a stereo Modular program writes both sides itself.
        let spread = right.is_some() && !(p.model.uses_graph() && p.graph.is_stereo());
        let len = out.len().min(BLOCK);
        let sr = p.sample_rate().max(1.0);
        for (((v, s), set), st) in self
            .voices
            .iter_mut()
            .zip(self.slots.iter_mut())
            .zip(self.sets.iter())
            .zip(self.graph.iter_mut())
            .filter(|(((v, _), _), _)| v.active())
        {
            // A pitch envelope still moving renders in sub-blocks (#754).
            let bending =
                s.shape.penv != 0.0 && (s.since as f32) < (s.shape.pattack + s.shape.pdecay) * sr;
            let since = s.since;
            // How long the note has sounded, and since its key was let go.
            s.since = s.since.saturating_add(n);
            if !v.gated() {
                s.released = Some(s.released.map_or(0, |r| r.saturating_add(n)));
            }
            let into: &mut [f32] = match self.scratch.get_mut(..len) {
                Some(x) if spread => {
                    x.fill(0.0);
                    x
                }
                _ => &mut *out,
            };
            let side = if spread { None } else { right.as_deref_mut() };
            if bending {
                // The trim set again before each sub-block, at the envelope's
                // value there; the shared LFO read from the sub-block's start.
                let mut part = SharedMod::new();
                let mut side = side;
                for (k, chunk) in into.chunks_mut(PITCH_SUB).enumerate() {
                    let at = k * PITCH_SUB;
                    let t = (since as f32 + at as f32) / sr;
                    v.set_trim(s.base + s.shape.at(t, None).0, s.cut);
                    let shared = ctx.shared.map(|sm| {
                        for (to, from) in [(&mut part.lfo, &sm.lfo), (&mut part.held, &sm.held)] {
                            if let (Some(to), Some(from)) =
                                (to.get_mut(..chunk.len()), from.get(at..at + chunk.len()))
                            {
                                to.copy_from_slice(from);
                            }
                        }
                        &part
                    });
                    let sub = MonoCtx { shared, ..ctx };
                    let r = side
                        .as_deref_mut()
                        .and_then(|r| r.get_mut(at..at + chunk.len()));
                    render_voice(v, set, st, &sub, &tools, chunk, r);
                }
            } else {
                render_voice(v, set, st, &ctx, &tools, into, side);
            }
            if spread && let (Some(x), Some(r)) = (self.scratch.get(..len), right.as_deref_mut()) {
                let (gl, gr) = s.gains.unwrap_or(MIDDLE);
                for ((l, r), x) in out.iter_mut().zip(r.iter_mut()).zip(x) {
                    *l += x * gl;
                    *r += x * gr;
                }
            }
        }
        // The reference tone joins past the voices and their VCA, key or not.
        if p.a440 {
            let inc = 440.0 / p.sample_rate();
            let (gl, gr) = if spread { MIDDLE } else { (1.0, 0.0) };
            for (i, x) in out.iter_mut().enumerate() {
                let y = A440_LEVEL * lookup(tools.sine, self.a440);
                *x += y * gl;
                if let Some(r) = right.as_deref_mut().and_then(|r| r.get_mut(i)) {
                    *r += y * gr;
                }
                self.a440 = wrap(self.a440 + inc);
            }
        }
    }
}

/// One voice of a pool into `into` (and `right`, for a stereo Modular
/// program), with its own values `set`.
fn render_voice(
    v: &mut PolyVoice,
    set: &VoiceSet,
    st: &mut VoiceState,
    ctx: &MonoCtx,
    tools: &Tools,
    into: &mut [f32],
    right: Option<&mut [f32]>,
) {
    match v {
        PolyVoice::Mono(m) => {
            m.set = *set;
            m.render(ctx, into);
        }
        PolyVoice::La(l) => {
            l.set = *set;
            l.render(ctx, into);
        }
        PolyVoice::Fm(f) => f.render(ctx, into),
        PolyVoice::Drum(d) => d.render(ctx.sine, ctx.blep, into),
        PolyVoice::Sampler(v) => {
            v.set = *set;
            v.render(ctx, tools.samples, tools.zones, into);
        }
        // Pads write both sides: see `render_pads`.
        PolyVoice::Pad(_) => {}
        PolyVoice::Graph(g) => {
            g.set = *set;
            g.render(ctx, st, into, right);
        }
        PolyVoice::Sid(v) => v.render(ctx, into),
        PolyVoice::Nes(v) => v.render(ctx, tools.samples, into),
        PolyVoice::Vl(v) => v.render(ctx, into),
    }
}

/// The noise seed of a slot that is being cleared: its owner's if it had one.
fn owner_seed_of_slot(i: usize, owner: Option<Owner>) -> u32 {
    owner.map_or_else(|| ((i as u32) + 1).wrapping_mul(0x9E37_79B1), owner_seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::BLOCK;
    use crate::mono::ladder::LadderTables;
    use crate::mono::osc::Blep;
    use crate::mono::voice::PitchTable;
    use crate::sample::SampleStore;
    use crate::sampler::ZoneMap;
    use crate::table::Tables;
    use crate::voice::sine_table;

    const SR: f32 = 48_000.0;

    struct Rig {
        pool: Pool,
        params: MonoParams,
        sine: Vec<f32>,
        blep: Blep,
        ladder: LadderTables,
        pitch: PitchTable,
        tables: &'static Tables,
        samples: SampleStore,
        zones: ZoneMap,
        clock: u64,
    }

    impl Rig {
        fn new(polyphony: f32) -> Rig {
            let mut params = MonoParams::new(SR);
            params.set(crate::params::Param::Polyphony, polyphony);
            Rig {
                pool: Pool::new(0),
                params,
                sine: sine_table(),
                blep: Blep::new(),
                ladder: LadderTables::new(SR),
                pitch: PitchTable::new(SR),
                tables: Tables::shared(SR),
                samples: SampleStore::new(),
                zones: ZoneMap::new(),
                clock: 0,
            }
        }

        fn on(&mut self, owner: Owner, note: u8) {
            self.clock += 1;
            self.pool
                .note_on(owner, note, 1.0, &self.params, self.clock);
        }

        fn off(&mut self, owner: Owner, note: u8) {
            self.pool.note_off(owner, note, &self.params);
        }

        fn run(&mut self, blocks: usize) -> Vec<f32> {
            let mut all = Vec::new();
            for _ in 0..blocks {
                let mut out = vec![0.0; BLOCK];
                let tools = Tools {
                    sine: &self.sine,
                    blep: &self.blep,
                    ladder: &self.ladder,
                    pitch: &self.pitch,
                    tables: self.tables.into(),
                    samples: &self.samples,
                    zones: &self.zones,
                };
                self.pool.render(&self.params, tools, &mut out);
                all.extend(out);
            }
            all
        }

        fn held(&self) -> Vec<u8> {
            let mut v: Vec<u8> = self
                .pool
                .slots
                .iter()
                .zip(&self.pool.voices)
                .filter(|(_, v)| v.gated())
                .map(|(s, _)| s.note)
                .collect();
            v.sort_unstable();
            v
        }
    }

    const LIVE: Owner = Owner::Live(0);

    #[test]
    fn a_chord_sounds_one_voice_per_note_and_releases_its_own() {
        let mut r = Rig::new(6.0);
        for n in [60, 64, 67, 71] {
            r.on(LIVE, n);
        }
        r.run(2);
        assert_eq!(r.pool.active(), 4);
        r.off(LIVE, 64);
        assert_eq!(r.held(), vec![60, 67, 71]);
        r.run(400);
        assert_eq!(
            r.pool.active(),
            3,
            "the released voice fell silent, the others hold"
        );
    }

    #[test]
    fn a_monophonic_synth_keeps_one_voice_per_owner() {
        let mut r = Rig::new(1.0);
        r.on(LIVE, 60);
        r.on(LIVE, 64);
        r.on(Owner::Track(2), 67);
        r.run(2);
        assert_eq!(
            r.pool.active(),
            2,
            "live keys are one voice, the track another"
        );
        assert_eq!(
            r.pool.voice(LIVE).map(|v| v.note()),
            Some(64),
            "last note wins on the owner's voice"
        );
        r.off(LIVE, 64);
        assert_eq!(
            r.pool.voice(LIVE).map(|v| v.note()),
            Some(60),
            "and falls back"
        );
    }

    #[test]
    fn owners_do_not_release_each_other() {
        let mut r = Rig::new(6.0);
        r.on(LIVE, 60);
        r.on(Owner::Track(1), 60);
        r.off(Owner::Track(1), 60);
        assert_eq!(r.held(), vec![60], "the live key is still held");
        r.on(Owner::Track(1), 62);
        r.pool.release_owner(Owner::Track(1));
        assert_eq!(r.held(), vec![60]);
        r.pool.release_all();
        assert!(r.held().is_empty());
    }

    #[test]
    fn a_full_pool_steals_the_release_first_then_the_oldest_held() {
        let mut r = Rig::new(4.0);
        for n in [60, 62, 64, 65] {
            r.on(LIVE, n);
        }
        r.run(1);
        // All held: the oldest (60) is stolen.
        r.on(LIVE, 70);
        assert_eq!(r.held(), vec![62, 64, 65, 70]);
        // One in release: that one goes first, whoever is oldest.
        r.off(LIVE, 64);
        r.run(1);
        r.on(LIVE, 72);
        assert_eq!(
            r.held(),
            vec![62, 65, 70, 72],
            "the released 64 was taken, not the older 62"
        );
    }

    #[test]
    fn rotation_does_not_cut_a_released_tail_first() {
        let mut r = Rig::new(3.0);
        r.params.set(crate::params::Param::AdsrRelease, 2.0);
        r.on(LIVE, 60);
        r.run(1);
        r.off(LIVE, 60);
        r.run(1);
        // The released 60 still sounds; two new notes take the free voices.
        r.on(LIVE, 62);
        r.on(LIVE, 64);
        r.run(1);
        assert_eq!(
            r.pool.active(),
            3,
            "the tail of 60 and both new notes sound"
        );
    }

    #[test]
    fn the_same_note_again_retriggers_its_voice() {
        let mut r = Rig::new(4.0);
        r.on(LIVE, 60);
        r.run(1);
        r.on(LIVE, 60);
        assert_eq!(r.pool.active(), 1);
        assert_eq!(r.held(), vec![60]);
    }

    /// Spec 006 Req 4: every voice reads the one LFO, so a voice started later
    /// has the same vibrato as one already sounding.
    #[test]
    fn the_lfo_is_shared_by_every_voice() {
        use crate::params::Param;
        let mut r = Rig::new(4.0);
        for (p, v) in [
            (Param::Vibrato, 1.0),
            (Param::ModWheel, 1.0),
            (Param::LfoRate, 5.0),
            (Param::LfoWave, 3.0),
        ] {
            r.params.set(p, p.clamp(v));
        }
        r.on(LIVE, 60);
        r.run(300);
        // A quarter of an LFO cycle later (5 Hz: 0.05 s is 19 blocks of 128).
        r.on(LIVE, 64);
        r.run(19);
        let pitch_mods: Vec<f32> = r
            .pool
            .voices
            .iter()
            .filter(|v| v.active())
            .map(|v| match v {
                PolyVoice::Mono(m) => m.mods().pitch[0],
                PolyVoice::La(_)
                | PolyVoice::Fm(_)
                | PolyVoice::Drum(_)
                | PolyVoice::Sampler(_)
                | PolyVoice::Pad(_)
                | PolyVoice::Graph(_)
                | PolyVoice::Sid(_)
                | PolyVoice::Nes(_)
                | PolyVoice::Vl(_) => 0.0,
            })
            .collect();
        assert_eq!(pitch_mods.len(), 2);
        assert!(
            pitch_mods[0].abs() > 0.01,
            "the vibrato moves: {pitch_mods:?}"
        );
        assert_eq!(
            pitch_mods[0], pitch_mods[1],
            "and is the same on both voices"
        );
    }

    fn trims(r: &Rig) -> Vec<f32> {
        r.pool
            .voices
            .iter()
            .zip(&r.pool.slots)
            .filter(|(v, _)| v.active())
            .map(|(v, _)| match v {
                PolyVoice::Mono(m) => m.trim,
                PolyVoice::La(l) => l.trim,
                PolyVoice::Fm(f) => f.trim,
                PolyVoice::Drum(_) => 1.0,
                PolyVoice::Sampler(v) => v.trim,
                PolyVoice::Pad(_) => 1.0,
                PolyVoice::Graph(g) => g.trim,
                PolyVoice::Sid(s) => s.trim,
                PolyVoice::Nes(n) => n.trim,
                PolyVoice::Vl(v) => v.trim,
            })
            .collect()
    }

    /// Spec 006 Req 3: unison presses the note on every voice, spread evenly.
    #[test]
    fn unison_spreads_every_voice_around_the_note() {
        use crate::params::Param;
        let mut r = Rig::new(6.0);
        for (p, v) in [(Param::Assign, 1.0), (Param::UnisonDetune, 0.4)] {
            r.params.set(p, p.clamp(v));
        }
        r.on(LIVE, 60);
        let out = r.run(40);
        assert_eq!(r.pool.active(), 6, "all six voices play the one note");
        assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 6.0));
        let mut t = trims(&r);
        t.sort_by(f32::total_cmp);
        assert_eq!(t.len(), 6);
        // 0.4 of 50 cents: ±20 cents, evenly spaced, six distinct pitches.
        assert!(
            (t[0] + 0.2).abs() < 1.0e-5 && (t[5] - 0.2).abs() < 1.0e-5,
            "{t:?}"
        );
        for w in t.windows(2) {
            assert!((w[1] - w[0] - 0.08).abs() < 1.0e-5, "even steps: {t:?}");
        }
    }

    #[test]
    fn unison_falls_back_to_the_last_key_held() {
        use crate::params::Param;
        let mut r = Rig::new(4.0);
        r.params.set(Param::Assign, 1.0);
        r.on(LIVE, 60);
        r.on(LIVE, 64);
        assert_eq!(r.held(), vec![64, 64, 64, 64]);
        r.off(LIVE, 64);
        assert_eq!(r.held(), vec![60, 60, 60, 60], "back to the key still down");
        r.off(LIVE, 60);
        assert!(r.held().is_empty());
        // A key let go while another sounds changes nothing.
        r.on(LIVE, 60);
        r.on(LIVE, 64);
        r.off(LIVE, 60);
        assert_eq!(r.held(), vec![64, 64, 64, 64]);
    }

    /// #781: `Assign` mono plays one voice whatever `Polyphony` says, falling
    /// back to the key still held as `Polyphony` 1 does.
    #[test]
    fn assign_mono_plays_one_voice() {
        use crate::params::Param;
        let mut r = Rig::new(6.0);
        r.params.set(Param::Assign, 2.0);
        r.on(LIVE, 60);
        r.on(LIVE, 64);
        r.run(4);
        assert_eq!(r.pool.active(), 1, "one voice");
        let note = |r: &Rig| r.pool.voice(LIVE).map(|v| v.note());
        assert_eq!(note(&r), Some(64), "the last key");
        r.off(LIVE, 64);
        r.run(2);
        assert_eq!(note(&r), Some(60), "back to the key still down");
        r.params.set(Param::Assign, 0.0);
        r.on(LIVE, 67);
        assert_eq!(r.held().len(), 2, "poly again");
    }

    /// Spec 006 Req 3: `Analog` gives each voice its own bounded offsets, the same
    /// every time, and 0 is exact.
    #[test]
    fn analog_variance_is_bounded_repeatable_and_off_at_zero() {
        use crate::params::Param;
        let chord = |analog: f32| {
            let mut r = Rig::new(6.0);
            r.params.set(Param::Analog, analog);
            for n in [60, 64, 67, 71] {
                r.on(LIVE, n);
            }
            let out = r.run(300);
            (trims(&r), out)
        };
        let (exact, out0) = chord(0.0);
        assert!(exact.iter().all(|t| *t == 0.0), "{exact:?}");
        let (loose, out1) = chord(1.0);
        let (again, out2) = chord(1.0);
        assert_eq!(out1, out2, "the same seed gives the same sound");
        assert_ne!(out0, out1, "and variance is audible");
        assert_eq!(loose, again);
        assert!(loose.iter().any(|t| *t != 0.0));
        // At most 6 cents of static detune and 3 of drift.
        assert!(loose.iter().all(|t| t.abs() <= 0.0901), "{loose:?}");
        let distinct = loose
            .iter()
            .map(|t| t.to_bits())
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert_eq!(distinct, loose.len(), "every voice has its own");
    }

    #[test]
    fn every_sample_is_finite_and_bounded_at_full_polyphony() {
        let mut r = Rig::new(16.0);
        for n in 0..24 {
            r.on(LIVE, 36 + 3 * n);
        }
        let out = r.run(80);
        assert!(out.iter().all(|s| s.is_finite() && s.abs() < 16.0));
        assert_eq!(r.pool.active(), MAX_VOICES);
    }

    /// #744: a note's pitch envelope starts `penv` off and lands on its note
    /// over the decay; its filter envelope rises, falls to its sustain and
    /// goes over the release.
    #[test]
    fn a_shape_bends_pitch_and_cutoff_over_the_note() {
        let zap = Shape {
            penv: 12.0,
            pdecay: 0.05,
            ..Shape::PLAIN
        };
        assert_eq!(zap.at(0.0, None).0, 12.0);
        assert!((zap.at(0.025, None).0 - 6.0).abs() < 1e-4);
        assert_eq!(zap.at(0.06, None).0, 0.0);
        let rise = Shape {
            penv: -7.0,
            pattack: 0.1,
            pdecay: 0.1,
            ..Shape::PLAIN
        };
        assert!(
            (rise.at(0.05, None).0 + 3.5).abs() < 1e-4,
            "half way down over the attack"
        );
        let wah = Shape {
            lpenv: 2.0,
            lpattack: 0.0,
            lpdecay: 0.1,
            lpsustain: 0.5,
            lprelease: 0.2,
            ..Shape::PLAIN
        };
        assert_eq!(wah.at(0.0, None).1, 24.0, "two octaves open at once");
        assert!((wah.at(0.5, None).1 - 12.0).abs() < 1e-4, "the sustain");
        assert!(
            (wah.at(0.6, Some(0.1)).1 - 6.0).abs() < 1e-4,
            "half way through the release"
        );
        assert_eq!(wah.at(1.0, Some(0.3)).1, 0.0);
        assert_eq!(Shape::default().at(0.3, None), (0.0, 0.0), "no envelope");
        assert!(Shape::PLAIN.at(0.0, None) == (0.0, 0.0));
    }
}
