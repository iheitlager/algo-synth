//! The modular voice (ADR-0021, ADR-0024, #216): a program of unit
//! generators, built from a SuperCollider SynthDef by `sc`, played per note
//! by the voice pool.
//!
//! A program is a fixed array of nodes, children before parents, with fixed
//! slots for the oscillators, filters, envelopes and delays; the state those
//! slots need is sized for the synth when its code is set (`VoiceState`). A
//! voice copies the program when a note starts, so a changed program takes
//! the next note while the old one finishes. `render` evaluates the array
//! once a sample with no allocation and no transcendental call: sines read
//! the sine table, and ranges and cutoffs use a fast `exp2`/`log2`
//! (ADR-0002).

use crate::mono::env::{Env, EnvTimes, Stage};
use crate::mono::ladder::{Ladder, MAX_K};
use crate::mono::model::{Filter, MOOG, Model, Setting, SvfVoicing};
use crate::mono::noise::Noise;
use crate::mono::osc::{Osc, Waveform, naive};
use crate::mono::svf::{OnePole, Svf};
use crate::mono::voice::MonoCtx;
use crate::voice::lookup;

pub mod lex;
pub mod sc;
pub mod verb;

/// Most nodes in a voice, and most of each kind of state it may hold.
pub const MAX_NODES: usize = 512;
pub const MAX_OSCS: usize = 64;
pub const MAX_PHASES: usize = 32;
pub const MAX_FILTERS: usize = 8;
pub const MAX_ENVS: usize = 8;
/// Most random numbers a voice draws when its note starts (`Rand`, `ExpRand`).
pub const MAX_RANDS: usize = 64;
/// Most reverbs a voice runs (`FreeVerb2`): each holds about 100 KB.
pub const MAX_VERBS: usize = 2;
/// Most `delay`s in a voice, and the longest one in samples (about 21 ms at
/// 48 kHz: combs, flangers and chorus, not echoes).
pub const MAX_DELAYS: usize = 32;
pub const MAX_DELAY: usize = 1024;
/// Most inputs to one `Mix` node.
const MAX_MIX: usize = 8;
/// No input: an optional argument left out.
const NONE: u16 = u16::MAX;
/// No release node in a breakpoint envelope.
const NO_RELEASE: u8 = u8::MAX;
/// Output level, as the other voices'.
const OUT_GAIN: f32 = 0.5;
/// The resonance of a filter that names none.
const RES: f32 = 0.2;
/// The modular filter: resonant, short of screaming.
const VOICING: SvfVoicing = SvfVoicing {
    osc_at: 1.0,
    k_min: 0.0,
    ceiling: 1.0,
};
/// The filter classes a voicing word is given to: `MoogFF`, the 12 dB
/// low-pass and band classes (`RLPF`, `RHPF`, `LPF`), `HPF`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    Ladder,
    Lp12,
    Hp,
}

impl Class {
    /// The class of a SuperCollider filter.
    pub(crate) fn of(ugen: &str) -> Class {
        match ugen {
            "MoogFF" => Class::Ladder,
            "HPF" => Class::Hp,
            _ => Class::Lp12,
        }
    }

    /// The words the class takes, for an error; a test holds it to `VOICINGS`.
    pub(crate) fn words(self) -> &'static str {
        match self {
            Class::Ladder => {
                "a MoogFF voicing is \\arp2600 \\minimoog \\proone \\prophet5 \\sh101 \\juno106 \\jupiter8 \\matrix12 \\ppgwave \\d50 \\odyssey \\prophet5rev1 or \\odysseyrev2"
            }
            Class::Lp12 => {
                "a filter voicing is \\ms20 \\cs15 \\polymoog \\jupiter8 \\matrix12 or \\odysseyrev1"
            }
            Class::Hp => {
                "an HPF voicing is \\ms20 \\cs15 \\odyssey \\juno106 \\jupiter8 or \\matrix12"
            }
        }
    }
}

/// Every voicing word (#307, #316, #321): a synth's lowercase name, with
/// `revN` for a revision other than its own, naming its filter of a class
/// at a setting of its switches: its ladder for `MoogFF`, its 12 dB filter
/// (the two-pole setting where it has a slope switch) for `RLPF`, `RHPF`
/// and `LPF`, its high-pass for `HPF`.
pub(crate) const VOICINGS: [(&str, Class, Model, Setting); 25] = [
    ("arp2600", Class::Ladder, Model::Arp2600, Setting::OWN),
    ("minimoog", Class::Ladder, Model::Minimoog, Setting::OWN),
    ("proone", Class::Ladder, Model::ProOne, Setting::OWN),
    ("prophet5", Class::Ladder, Model::Prophet5, Setting::OWN),
    ("sh101", Class::Ladder, Model::Sh101, Setting::OWN),
    ("juno106", Class::Ladder, Model::Juno106, Setting::OWN),
    ("jupiter8", Class::Ladder, Model::Jupiter8, Setting::OWN),
    ("matrix12", Class::Ladder, Model::Matrix12, Setting::OWN),
    ("ppgwave", Class::Ladder, Model::PpgWave, Setting::OWN),
    ("d50", Class::Ladder, Model::D50, Setting::OWN),
    ("odyssey", Class::Ladder, Model::Odyssey, Setting::OWN),
    (
        "prophet5rev1",
        Class::Ladder,
        Model::Prophet5,
        Setting::rev(1),
    ),
    (
        "odysseyrev2",
        Class::Ladder,
        Model::Odyssey,
        Setting::rev(2),
    ),
    ("ms20", Class::Lp12, Model::Ms20, Setting::OWN),
    ("cs15", Class::Lp12, Model::Cs15, Setting::OWN),
    ("polymoog", Class::Lp12, Model::PolyMoog, Setting::OWN),
    ("jupiter8", Class::Lp12, Model::Jupiter8, Setting::SLOPE12),
    ("matrix12", Class::Lp12, Model::Matrix12, Setting::SLOPE12),
    ("odysseyrev1", Class::Lp12, Model::Odyssey, Setting::rev(1)),
    ("ms20", Class::Hp, Model::Ms20, Setting::OWN),
    ("cs15", Class::Hp, Model::Cs15, Setting::OWN),
    ("odyssey", Class::Hp, Model::Odyssey, Setting::OWN),
    ("juno106", Class::Hp, Model::Juno106, Setting::OWN),
    ("jupiter8", Class::Hp, Model::Jupiter8, Setting::OWN),
    ("matrix12", Class::Hp, Model::Matrix12, Setting::OWN),
];
/// The `drive` of a ladder that names none: unity, as `Param::Drive` at 0.
const DRIVE: f32 = 0.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

/// An envelope's shape: the synth's ADSR, or the SynthDef's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Adsr,
    /// Attack, decay, sustain and release from nodes (numbers or controls),
    /// read when the gate moves, so a knob changes the next note's shape
    /// (ADR-0024).
    Nodes([u16; 4]),
    /// SuperCollider's `Env(levels, times, curves, releaseNode)`.
    Brk(Brk),
}

/// A breakpoint envelope: up to eight levels and seven segments, each with
/// a time and a curve; levels and times are nodes (numbers or knobs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brk {
    pub levels: [u16; 8],
    pub times: [u16; 7],
    /// Each segment's curve: 0 linear, 1 a curvature (`curves`), 2
    /// exponential, 3 a step.
    pub kinds: [u8; 7],
    pub curves: [f32; 7],
    pub n: u8,
    /// The node it holds at while the gate is open, `NO_RELEASE` for none.
    pub release: u8,
}

/// One unit generator; `u16` fields index earlier nodes, `slot` its state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ugen {
    Num(f32),
    Freq,
    Gate,
    Vel,
    /// A band-limited saw, triangle or pulse; `width` is `NONE` for a half.
    Osc {
        wave: Waveform,
        freq: u16,
        width: u16,
        slot: u8,
    },
    Sin {
        freq: u16,
        slot: u8,
    },
    Lfo {
        rate: u16,
        wave: Waveform,
        slot: u8,
    },
    Noise,
    Svf {
        input: u16,
        high: bool,
        cutoff: u16,
        res: u16,
        slot: u8,
        /// `res` is SuperCollider's `rq` (bandwidth over cutoff): small is sharp.
        rq: bool,
    },
    Env {
        slot: u8,
    },
    /// A synth's 6 dB high-pass (`HPF … voicing: \odyssey`, #321).
    PoleHp {
        input: u16,
        cutoff: u16,
        slot: u8,
    },
    /// The shared 24 dB ladder: `res` 0..1 up to self-oscillation.
    Ladder {
        input: u16,
        cutoff: u16,
        res: u16,
        slot: u8,
        /// `res` is `MoogFF`'s gain, 0..4: 4 is the top of the knob (#342).
        gain: bool,
        /// 0..1, 0 to +18 dB into the saturator, as `Param::Drive`.
        drive: u16,
    },
    /// A sine phase-modulated by a sine: `index` in radians. Two phases.
    Fm {
        carrier: u16,
        modulator: u16,
        index: u16,
        slot: u8,
    },
    /// A feedback comb: the input plus the output `time` seconds ago times
    /// `feedback`.
    Delay {
        input: u16,
        time: u16,
        feedback: u16,
        /// `feedback` is a `CombL` decay time in seconds: the echoes fall
        /// 60 dB in it.
        decay: bool,
        /// Its line among the voice's delays.
        slot: u8,
        /// Only the delayed signal comes out (SuperCollider's delays and
        /// combs), not the input with it.
        wet: bool,
    },
    /// SuperCollider's shapers on a signal: 0 `tanh`, 1 `softclip`, 2 `distort`.
    Clip {
        input: u16,
        kind: u8,
    },
    /// Up to eight inputs added (SuperCollider's `Mix`), or their mean.
    Mix {
        inputs: [u16; MAX_MIX],
        n: u8,
        mean: bool,
    },
    /// One of up to eight inputs, picked by `which` (SuperCollider's
    /// `Select`): truncated to a whole number and clipped to the inputs.
    /// The others run on, so switching back finds them where they were.
    Select {
        which: u16,
        inputs: [u16; MAX_MIX],
        n: u8,
    },
    /// A number drawn when the note starts, between `lo` and `hi`
    /// (SuperCollider's `Rand` and `ExpRand`), from a seeded generator.
    Rand {
        lo: u16,
        hi: u16,
        exp: bool,
        slot: u8,
    },
    /// Semitones as a frequency ratio, `2^(x/12)`.
    MidiRatio(u16),
    /// One side of an equal-power pan (`Pan2`), `pos` −1 left to 1 right.
    Pan {
        input: u16,
        pos: u16,
        right: bool,
    },
    /// A stereo reverb (`FreeVerb2`): this node is its left side and keeps
    /// the right for its `VerbR`.
    Verb {
        left: u16,
        right: u16,
        mix: u16,
        room: u16,
        damp: u16,
        slot: u8,
    },
    VerbR {
        slot: u8,
    },
    /// A knob of the code: the synth's `Param::Ctl1`… value, held in its
    /// range.
    Ctl {
        index: u8,
        lo: f32,
        hi: f32,
    },
    Neg(u16),
    Bin(Op, u16, u16),
    /// `of` mapped onto lo..hi from −1..1, or from 0..1 when `uni` (an
    /// envelope, the gate, the velocity), as SuperCollider's `range`.
    Range {
        of: u16,
        lo: u16,
        hi: u16,
        exp: bool,
        uni: bool,
    },
}

/// A compiled voice: its nodes (the last is the output) and its envelopes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Program {
    nodes: [Ugen; MAX_NODES],
    len: u16,
    /// The voice sets its own level (a SuperCollider `amp`): no velocity
    /// and output gain on top.
    own_amp: bool,
    /// The right side's node of a stereo program (the last node is the
    /// left), `NONE` for a mono one.
    right: u16,
    envs: [Shape; MAX_ENVS],
    /// Each filter slot's voicing, chosen when the program is built.
    voicings: [Filter; MAX_FILTERS],
    counts: Counts,
}

/// How many of each kind of state a program uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counts {
    oscs: u8,
    phases: u8,
    filters: u8,
    envs: u8,
    delays: u8,
    rands: u8,
    verbs: u8,
}

impl Default for Program {
    /// The voice a Modular synth plays before it is given one: a saw.
    fn default() -> Program {
        let mut nodes = [Ugen::Num(0.0); MAX_NODES];
        let [f, o, ..] = &mut nodes;
        *f = Ugen::Freq;
        *o = Ugen::Osc {
            wave: Waveform::Saw,
            freq: 0,
            width: NONE,
            slot: 0,
        };
        Program {
            nodes,
            len: 2,
            own_amp: false,
            right: NONE,
            envs: [Shape::Adsr; MAX_ENVS],
            voicings: [Filter::Ladder(MOOG); MAX_FILTERS],
            counts: Counts {
                oscs: 1,
                ..Counts::default()
            },
        }
    }
}

impl Program {
    fn node(&self, i: u16) -> Option<Ugen> {
        self.nodes.get(usize::from(i)).copied()
    }

    /// Whether the program has two sides: its synth plays a stereo bus.
    pub fn is_stereo(&self) -> bool {
        self.right != NONE
    }

    /// The most voices of this program a synth sounds at once: a budget of
    /// work per synth over a rough cost per voice (a band-limited oscillator
    /// the unit), from 1 to 16; `make bench` checks it.
    pub fn voice_cap(&self) -> usize {
        const BUDGET: f32 = 1000.0;
        let c = self.counts;
        let cost = 4.0 * f32::from(c.oscs)
            + 12.0 * f32::from(c.verbs)
            + 2.0 * f32::from(c.filters)
            + f32::from(c.phases)
            + 1.5 * f32::from(c.delays)
            + 0.25 * f32::from(self.len);
        ((BUDGET / cost.max(1.0)) as usize).clamp(1, crate::poly::MAX_VOICES)
    }
}

/// `log2(x)` for `x > 0`, within about 1e-7: the exponent from the bits,
/// and an odd series in `(m - 1) / (m + 1)` for a mantissa near 1.
pub fn fast_log2(x: f32) -> f32 {
    let bits = x.max(f32::MIN_POSITIVE).to_bits();
    let mut exp = ((bits >> 23) & 0xff) as i32 - 127;
    let mut m = f32::from_bits((bits & 0x007f_ffff) | 0x3f80_0000);
    if m > std::f32::consts::SQRT_2 {
        m *= 0.5;
        exp += 1;
    }
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    let series = t * (1.0 + t2 * (1.0 / 3.0 + t2 * (0.2 + t2 / 7.0)));
    exp as f32 + 2.0 * std::f32::consts::LOG2_E * series
}

/// `2^x`, within about 1e-5 relative: the nearest whole number into the
/// exponent, a series for the rest.
pub fn fast_exp2(x: f32) -> f32 {
    const L: f32 = std::f32::consts::LN_2;
    let x = x.clamp(-126.0, 126.0);
    let whole = x.round();
    let f = x - whole;
    let poly = 1.0
        + f * (L + f
            * (L * L / 2.0
                + f * (L * L * L / 6.0
                    + f * (L * L * L * L / 24.0 + f * L * L * L * L * L / 120.0))));
    f32::from_bits(((whole as i32 + 127) as u32) << 23) * poly
}

/// What a note of a program holds that is sized by the program: one per
/// voice slot of a pool, allocated with the synth's program, outside
/// `render` (ADR-0024). It only grows, so a sounding voice keeps its own.
#[derive(Clone, Default)]
pub struct VoiceState {
    /// The program the note plays, copied when it starts.
    prog: Program,
    vals: Vec<f32>,
    oscs: Vec<Osc>,
    phases: Vec<f32>,
    filters: Vec<Svf>,
    ladders: Vec<Ladder>,
    poles: Vec<OnePole>,
    /// Each filter's last cutoff in hertz and as a note, so a steady
    /// cutoff is converted once.
    cutoffs: Vec<(f32, f32)>,
    /// The delays' lines, `MAX_DELAY` each, and where each writes next.
    lines: Vec<f32>,
    writes: Vec<usize>,
    /// The numbers drawn when the note started, and how many notes this
    /// slot has started: a voice is rebuilt per note, its slot's state is not.
    rands: Vec<f32>,
    notes: u32,
    /// The reverbs, and the right side each left node keeps for its partner.
    verbs: Vec<verb::FreeVerb>,
    verb_r: Vec<f32>,
}

impl VoiceState {
    /// State with room for `prog`.
    pub fn for_program(prog: &Program) -> VoiceState {
        let mut st = VoiceState::default();
        st.grow(prog, 48_000.0);
        st
    }

    /// Room for `prog`, keeping what is there.
    pub fn grow(&mut self, prog: &Program, sample_rate: f32) {
        let c = prog.counts;
        while self.verbs.len() < usize::from(c.verbs) {
            self.verbs.push(verb::FreeVerb::new(sample_rate));
        }
        self.verb_r.resize(self.verbs.len(), 0.0);
        let up = |n: usize, now: usize| n.max(now);
        self.vals
            .resize(up(usize::from(prog.len), self.vals.len()), 0.0);
        self.oscs
            .resize(up(usize::from(c.oscs), self.oscs.len()), Osc::default());
        self.phases
            .resize(up(usize::from(c.phases), self.phases.len()), 0.0);
        let f = up(usize::from(c.filters), self.filters.len());
        self.filters.resize(f, Svf::new());
        self.ladders.resize(f, Ladder::new());
        self.poles.resize(f, OnePole::default());
        self.cutoffs.resize(f, (f32::NAN, 0.0));
        let d = up(usize::from(c.delays), self.writes.len());
        self.writes.resize(d, 0);
        self.lines.resize(d * MAX_DELAY, 0.0);
        self.rands
            .resize(up(usize::from(c.rands), self.rands.len()), 0.0);
    }

    fn val(&self, i: u16) -> f32 {
        self.vals.get(usize::from(i)).copied().unwrap_or(0.0)
    }

    /// The value of a number or a control node, read outside the sample loop
    /// (an envelope's times, a random number's bounds).
    fn fixed(&self, i: u16, ctx: &MonoCtx) -> f32 {
        match self.prog.node(i) {
            Some(Ugen::Num(v)) => v,
            Some(Ugen::Ctl { index, lo, hi }) => ctx
                .params
                .ctl
                .get(usize::from(index))
                .copied()
                .unwrap_or(lo)
                .clamp(lo, hi),
            _ => 0.0,
        }
    }

    /// Filter `slot`'s cutoff in hertz as a note, converted only when it
    /// changes, with the pool's trim.
    fn cutoff_note(&mut self, slot: u8, hz: f32, trim: f32) -> f32 {
        let hz = hz.max(1.0);
        let note = match self.cutoffs.get_mut(usize::from(slot)) {
            Some((last, note)) if *last == hz => *note,
            Some(cached) => {
                *cached = (hz, 69.0 + 12.0 * fast_log2(hz / 440.0));
                cached.1
            }
            None => 0.0,
        };
        note + trim
    }
}

/// A breakpoint envelope's state: the segment it is in and how it moves
/// (SuperCollider's `EnvGen`: worked out once per segment, a multiply or
/// an add per sample).
#[derive(Clone, Copy, Default)]
struct BrkState {
    seg: u8,
    left: u32,
    level: f32,
    /// Linear step, or the curve's `a2 - b1` with `b1 *= grow`, or `level *= grow`.
    inc: f32,
    a2: f32,
    b1: f32,
    grow: f32,
    kind: u8,
    target: f32,
    /// Holding at its release node with the gate open.
    holding: bool,
    done: bool,
}

/// A note playing a program; what the program sizes is in the pool's
/// [`VoiceState`] for its slot.
#[derive(Clone, Copy)]
pub struct GraphVoice {
    note: f32,
    velocity: f32,
    gate: bool,
    retrigger: bool,
    /// The note starts on a silent voice: its state starts afresh.
    fresh: bool,
    /// Pitch trim in semitones from the pool (unison, analog variance).
    pub trim: f32,
    pub cutoff_trim: f32,
    /// The voice's slot in its pool: its state, and the number of a list it takes.
    slot: usize,
    /// What `active` needs of the program: its envelopes and their kinds.
    envs_used: u8,
    shapes: [Shape; MAX_ENVS],
    envs: [Env; MAX_ENVS],
    brks: [BrkState; MAX_ENVS],
    env_vals: [f32; MAX_ENVS],
    /// The loudness of a program without `env`: the synth's ADSR.
    amp: Env,
    noise: Noise,
    /// Notes started, and the seed of this voice's random numbers.
    presses: u32,
    seed: u32,
}

const NO_TIMES: EnvTimes = EnvTimes {
    attack: 0.0,
    decay: 0.0,
    sustain: 0.0,
    release: 0.0,
};

impl GraphVoice {
    pub fn new(seed: u32) -> GraphVoice {
        GraphVoice {
            note: 0.0,
            velocity: 0.0,
            gate: false,
            retrigger: false,
            fresh: true,
            trim: 0.0,
            cutoff_trim: 0.0,
            slot: 0,
            envs_used: 0,
            shapes: [Shape::Adsr; MAX_ENVS],
            envs: [Env::default(); MAX_ENVS],
            brks: [BrkState::default(); MAX_ENVS],
            env_vals: [0.0; MAX_ENVS],
            amp: Env::default(),
            noise: Noise::new(seed | 1),
            presses: 0,
            seed,
        }
    }

    pub fn active(&self) -> bool {
        let used = usize::from(self.envs_used);
        self.retrigger
            || if used > 0 {
                self.shapes
                    .iter()
                    .zip(self.envs.iter().zip(&self.brks))
                    .take(used)
                    .any(|(shape, (e, b))| match shape {
                        Shape::Brk(_) => !b.done || b.holding,
                        _ => sounding(e),
                    })
            } else {
                self.amp.stage != Stage::Idle
            }
    }

    pub fn gated(&self) -> bool {
        self.gate
    }

    /// Start a note with the synth's program; it is copied into the voice's
    /// state when the note's first block renders.
    pub fn press(
        &mut self,
        note: u8,
        velocity: f32,
        prog: &Program,
        _sample_rate: f32,
        slot: usize,
    ) {
        self.fresh = !self.active();
        self.slot = slot;
        self.envs_used = prog.counts.envs;
        self.shapes = prog.envs;
        self.presses = self.presses.wrapping_add(1);
        self.note = f32::from(note);
        self.velocity = velocity.clamp(0.0, 1.0);
        self.gate = true;
        self.retrigger = true;
    }

    pub fn release_all(&mut self) {
        self.gate = false;
    }

    /// The note's first block: the program into the state, which starts
    /// afresh on a silent voice, and the note's random numbers drawn.
    fn start(&mut self, ctx: &MonoCtx, st: &mut VoiceState) {
        st.prog = ctx.params.graph;
        st.notes = st.notes.wrapping_add(1);
        if self.fresh {
            st.oscs.iter_mut().for_each(|o| *o = Osc::default());
            st.phases.iter_mut().for_each(|p| *p = 0.0);
            st.filters.iter_mut().for_each(|f| *f = Svf::new());
            st.ladders.iter_mut().for_each(|l| *l = Ladder::new());
            st.poles.iter_mut().for_each(|p| *p = OnePole::default());
            st.cutoffs.iter_mut().for_each(|c| *c = (f32::NAN, 0.0));
            if st.prog.counts.delays > 0 {
                st.lines.iter_mut().for_each(|x| *x = 0.0);
            }
            for v in st.verbs.iter_mut().take(usize::from(st.prog.counts.verbs)) {
                v.clear();
            }
        }
        let mut k = 0u32;
        for i in 0..usize::from(st.prog.len) {
            match st.prog.nodes.get(i).copied() {
                Some(Ugen::Osc { wave, slot, .. }) => {
                    if let Some(o) = st.oscs.get_mut(usize::from(slot)) {
                        o.wave = wave;
                    }
                }
                Some(Ugen::Rand { lo, hi, exp, slot }) => {
                    let note_seed =
                        self.seed ^ st.notes.wrapping_mul(0x9E37_79B9) ^ (self.slot as u32);
                    let h = crate::algo::mix(note_seed, k);
                    k += 1;
                    let u = h as f32 / (u32::MAX as f32 + 1.0);
                    let (l, hh) = (st.fixed(lo, ctx), st.fixed(hi, ctx));
                    let v = if exp && l * hh > 0.0 {
                        l * fast_exp2(fast_log2(hh / l) * u)
                    } else {
                        l + (hh - l) * u
                    };
                    if let Some(r) = st.rands.get_mut(usize::from(slot)) {
                        *r = v;
                    }
                }
                _ => {}
            }
        }
    }

    /// Add this voice into `out`, advancing its state in `st`.
    pub fn render(
        &mut self,
        ctx: &MonoCtx,
        st: &mut VoiceState,
        out: &mut [f32],
        mut right: Option<&mut [f32]>,
    ) {
        let p = ctx.params;
        let sr = p.sample_rate();
        let inv = 1.0 / sr.max(1.0);
        let (retrigger, gate) = (self.retrigger, self.gate);
        self.retrigger = false;
        if retrigger {
            self.start(ctx, st);
        }
        let used = usize::from(self.envs_used);
        for k in 0..used {
            let shape = self.shapes.get(k).copied().unwrap_or(Shape::Adsr);
            if let Shape::Brk(b) = shape {
                if let Some(state) = self.brks.get_mut(k) {
                    brk_gate(state, &b, st, ctx, sr, retrigger, gate);
                }
                continue;
            }
            let t = match shape {
                Shape::Nodes([a, d, s, r]) => EnvTimes {
                    attack: st.fixed(a, ctx).max(0.0) * sr,
                    decay: st.fixed(d, ctx).max(0.0) * sr,
                    sustain: st.fixed(s, ctx).clamp(0.0, 1.0),
                    release: st.fixed(r, ctx).max(0.0) * sr,
                },
                _ => p.adsr,
            };
            if let Some(env) = self.envs.get_mut(k) {
                gate_env(env, &t, retrigger, gate);
                if matches!(shape, Shape::Nodes(_)) {
                    env.set_sustain(t.sustain);
                }
            }
        }
        if used == 0 {
            gate_env(&mut self.amp, &p.adsr, retrigger, gate);
            self.amp.set_sustain(p.adsr.sustain);
        }
        let hz = ctx.pitch.at(self.note + self.trim) * sr;
        let len = usize::from(st.prog.len).min(st.vals.len());
        let _ = NO_TIMES;
        for (frame, sample) in out.iter_mut().enumerate() {
            for k in 0..used {
                let v = match self.shapes.get(k) {
                    Some(Shape::Brk(b)) => match self.brks.get_mut(k) {
                        Some(state) => brk_step(state, b, st, ctx, sr),
                        None => 0.0,
                    },
                    _ => self.envs.get_mut(k).map_or(0.0, Env::step),
                };
                if let Some(slot) = self.env_vals.get_mut(k) {
                    *slot = v;
                }
            }
            let amp = if used > 0 { 1.0 } else { self.amp.step() };
            if !self.active() {
                return;
            }
            for i in 0..len {
                let v = self.eval(st, i, hz, inv, gate, ctx);
                if let Some(slot) = st.vals.get_mut(i) {
                    *slot = v;
                }
            }
            let y = len.checked_sub(1).map_or(0.0, |i| st.val(i as u16)) * amp;
            // A SynthDef with its own `amp` plays at its own level.
            let gain = if st.prog.own_amp {
                1.0
            } else {
                OUT_GAIN * self.velocity
            };
            if y.is_finite() {
                *sample += y.clamp(-4.0, 4.0) * gain;
            }
            if let Some(r) = right.as_deref_mut().and_then(|r| r.get_mut(frame)) {
                // A stereo program's right side; a mono one in a stereo bus sounds on both.
                let yr = if st.prog.is_stereo() {
                    st.val(st.prog.right) * amp
                } else {
                    y
                };
                if yr.is_finite() {
                    *r += yr.clamp(-4.0, 4.0) * gain;
                }
            }
        }
    }

    /// Node `i` this sample, its inputs already evaluated.
    fn eval(
        &mut self,
        st: &mut VoiceState,
        i: usize,
        hz: f32,
        inv: f32,
        gate: bool,
        ctx: &MonoCtx,
    ) -> f32 {
        let Some(node) = st.prog.nodes.get(i).copied() else {
            return 0.0;
        };
        match node {
            Ugen::Num(v) => v,
            Ugen::Freq => hz,
            Ugen::Gate => f32::from(u8::from(gate)),
            Ugen::Vel => self.velocity,
            Ugen::Noise => self.noise.white(),
            Ugen::Osc {
                freq, width, slot, ..
            } => {
                let f = st.val(freq);
                let w = if width == NONE {
                    0.5
                } else {
                    st.val(width).clamp(0.05, 0.95)
                };
                let Some(o) = st.oscs.get_mut(usize::from(slot)) else {
                    return 0.0;
                };
                o.set_increment(f * inv);
                o.step(ctx.blep, ctx.sine, w, None).0
            }
            Ugen::Sin { freq, slot } => {
                let inc = st.val(freq) * inv;
                let Some(ph) = st.phases.get_mut(usize::from(slot)) else {
                    return 0.0;
                };
                *ph += inc;
                *ph -= ph.floor();
                lookup(ctx.sine, *ph)
            }
            Ugen::Lfo { rate, wave, slot } => {
                let inc = st.val(rate) * inv;
                let Some(ph) = st.phases.get_mut(usize::from(slot)) else {
                    return 0.0;
                };
                *ph += inc;
                *ph -= ph.floor();
                naive(wave, *ph, 0.5, ctx.sine)
            }
            Ugen::Svf {
                input,
                high,
                cutoff,
                res,
                slot,
                rq,
                ..
            } => {
                let x = st.val(input);
                let c = st.val(cutoff).max(1.0);
                let note = st.cutoff_note(slot, c, self.cutoff_trim);
                let r = match (res == NONE, rq) {
                    (true, _) => RES,
                    // rq 1 is gentle, 0.05 nearly singing.
                    (false, true) => 1.0 - st.val(res).clamp(0.0, 1.0),
                    (false, false) => st.val(res),
                };
                let v = match st.prog.voicings.get(usize::from(slot)) {
                    Some(Filter::Svf(v)) => *v,
                    _ => VOICING,
                };
                let Some(f) = st.filters.get_mut(usize::from(slot)) else {
                    return 0.0;
                };
                let o = f.process(ctx.ladder, &v, x, note, r);
                if high { o.hp } else { o.lp }
            }
            Ugen::PoleHp {
                input,
                cutoff,
                slot,
            } => {
                let x = st.val(input);
                let note = st.cutoff_note(slot, st.val(cutoff), self.cutoff_trim);
                let Some(p) = st.poles.get_mut(usize::from(slot)) else {
                    return 0.0;
                };
                p.process(ctx.ladder, x, note)
            }
            Ugen::Env { slot } => self.env_vals.get(usize::from(slot)).copied().unwrap_or(0.0),
            Ugen::Ctl { index, lo, hi } => ctx
                .params
                .ctl
                .get(usize::from(index))
                .copied()
                .unwrap_or(lo)
                .clamp(lo, hi),
            Ugen::Rand { slot, .. } => st.rands.get(usize::from(slot)).copied().unwrap_or(0.0),
            Ugen::MidiRatio(x) => fast_exp2(st.val(x) * (1.0 / 12.0)),
            Ugen::Pan { input, pos, right } => {
                // Equal power: cos and sin of (pos + 1)·π/4, from the sine table.
                let phase = (st.val(pos).clamp(-1.0, 1.0) + 1.0) * 0.125;
                let g = if right {
                    lookup(ctx.sine, phase)
                } else {
                    lookup(ctx.sine, phase + 0.25)
                };
                st.val(input) * g
            }
            Ugen::Verb {
                left,
                right,
                mix,
                room,
                damp,
                slot,
            } => {
                let (l, r) = (st.val(left), st.val(right));
                let (m, rm, d) = (st.val(mix), st.val(room), st.val(damp));
                let Some(v) = st.verbs.get_mut(usize::from(slot)) else {
                    return 0.0;
                };
                let (yl, yr) = v.process(l, r, m, rm, d);
                if let Some(keep) = st.verb_r.get_mut(usize::from(slot)) {
                    *keep = yr;
                }
                yl
            }
            Ugen::VerbR { slot } => st.verb_r.get(usize::from(slot)).copied().unwrap_or(0.0),
            Ugen::Ladder {
                input,
                cutoff,
                res,
                slot,
                gain,
                drive,
                ..
            } => {
                let x = st.val(input);
                let note = st.cutoff_note(slot, st.val(cutoff), self.cutoff_trim);
                let r = if res == NONE { RES } else { st.val(res) };
                // MoogFF's gain runs 0..4 and whistles at 4, as in
                // SuperCollider: its range is the knob's (#342).
                let k = if gain {
                    r.clamp(0.0, 4.0) / 4.0 * MAX_K
                } else {
                    r.clamp(0.0, 1.0) * MAX_K
                };
                let d = if drive == NONE { DRIVE } else { st.val(drive) };
                let v = match st.prog.voicings.get(usize::from(slot)) {
                    Some(Filter::Ladder(v)) => *v,
                    _ => MOOG,
                };
                let Some(l) = st.ladders.get_mut(usize::from(slot)) else {
                    return 0.0;
                };
                l.voiced(ctx.ladder, &v, x, note, k, 1.0 + 7.0 * d.clamp(0.0, 1.0))
            }
            Ugen::Fm {
                carrier,
                modulator,
                index,
                slot,
            } => {
                let (c, m, k) = (st.val(carrier), st.val(modulator), st.val(index));
                let s = usize::from(slot);
                let mph = st.phases.get(s + 1).copied().unwrap_or(0.0) + m * inv;
                let mph = mph - mph.floor();
                let cph = st.phases.get(s).copied().unwrap_or(0.0) + c * inv;
                let cph = cph - cph.floor();
                if let Some(p) = st.phases.get_mut(s + 1) {
                    *p = mph;
                }
                if let Some(p) = st.phases.get_mut(s) {
                    *p = cph;
                }
                let at = cph + k * lookup(ctx.sine, mph) * (1.0 / std::f32::consts::TAU);
                lookup(ctx.sine, at - at.floor())
            }
            Ugen::Delay {
                input,
                time,
                feedback,
                decay,
                slot,
                wet,
            } => {
                let x = st.val(input);
                let fb = match (feedback == NONE, decay) {
                    (true, _) => 0.0,
                    // A decay time: each pass loses its share of 60 dB.
                    (false, true) => {
                        let t = st.val(time).max(1e-4);
                        let dec = st.val(feedback);
                        if dec.abs() < 1e-4 {
                            0.0
                        } else {
                            (dec.signum() * fast_exp2(-9.965_784 * t / dec.abs()))
                                .clamp(-0.999, 0.999)
                        }
                    }
                    (false, false) => st.val(feedback).clamp(-0.98, 0.98),
                };
                let d = (st.val(time) / inv).clamp(1.0, (MAX_DELAY - 2) as f32);
                let k = usize::from(slot);
                let Some(write) = st.writes.get(k).copied() else {
                    return 0.0;
                };
                let base = k * MAX_DELAY;
                let back = write as f32 + MAX_DELAY as f32 - d;
                let (j, frac) = (back as usize, back - back.floor());
                let a = st.lines.get(base + j % MAX_DELAY).copied().unwrap_or(0.0);
                let b = st
                    .lines
                    .get(base + (j + 1) % MAX_DELAY)
                    .copied()
                    .unwrap_or(0.0);
                let delayed = a + (b - a) * frac;
                let stored = x + fb * delayed;
                let stored = if stored.is_finite() {
                    stored.clamp(-8.0, 8.0)
                } else {
                    0.0
                };
                if let Some(w) = st.lines.get_mut(base + write) {
                    *w = stored;
                }
                if let Some(w) = st.writes.get_mut(k) {
                    *w = (write + 1) % MAX_DELAY;
                }
                if wet { delayed } else { stored }
            }
            Ugen::Clip { input, kind } => {
                let x = st.val(input);
                match kind {
                    // A Padé tanh, exact enough below 3 and 1 above.
                    0 => {
                        let x = x.clamp(-3.0, 3.0);
                        x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
                    }
                    1 if x.abs() <= 0.5 => x,
                    1 => (x.abs() - 0.25) / x,
                    2 => x / (1.0 + x.abs()),
                    // atan, within a few thousandths: x / (1 + 0.28 x²) below 1,
                    // π/2 − 1/x … above.
                    _ => fast_atan(x),
                }
            }
            Ugen::Mix { inputs, n, mean } => {
                let n = usize::from(n).max(1);
                let sum: f32 = inputs.iter().take(n).map(|i| st.val(*i)).sum();
                if mean { sum / n as f32 } else { sum }
            }
            Ugen::Select { which, inputs, n } => {
                let last = usize::from(n).max(1) - 1;
                let w = st.val(which);
                let i = if w.is_nan() || w < 0.0 {
                    0
                } else {
                    (w as usize).min(last)
                };
                inputs.get(i).map_or(0.0, |i| st.val(*i))
            }
            Ugen::Neg(a) => -st.val(a),
            Ugen::Bin(op, a, b) => {
                let (x, y) = (st.val(a), st.val(b));
                match op {
                    Op::Add => x + y,
                    Op::Sub => x - y,
                    Op::Mul => x * y,
                    Op::Div if y == 0.0 => 0.0,
                    Op::Div => x / y,
                }
            }
            Ugen::Range {
                of,
                lo,
                hi,
                exp,
                uni,
            } => {
                let u = if uni {
                    st.val(of)
                } else {
                    (st.val(of) + 1.0) * 0.5
                };
                let (l, h) = (st.val(lo), st.val(hi));
                if !exp {
                    l + (h - l) * u
                } else if l > 0.0 && h > 0.0 {
                    l * fast_exp2(fast_log2(h / l) * u)
                } else {
                    l
                }
            }
        }
    }
}

/// `atan(x)` within about 0.005 rad, with no transcendental call.
pub fn fast_atan(x: f32) -> f32 {
    const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;
    let a = x.abs();
    let y = if a <= 1.0 {
        a / (1.0 + 0.28 * a * a)
    } else {
        HALF_PI - a / (a * a + 0.28)
    };
    y.copysign(x)
}

/// Open, close or start a breakpoint envelope at the start of a block.
fn brk_gate(
    s: &mut BrkState,
    b: &Brk,
    st: &VoiceState,
    ctx: &MonoCtx,
    sr: f32,
    retrigger: bool,
    gate: bool,
) {
    if retrigger {
        *s = BrkState {
            level: b.levels.first().map_or(0.0, |l| st.fixed(*l, ctx)),
            ..BrkState::default()
        };
        brk_segment(s, b, st, ctx, sr);
        return;
    }
    if !gate && s.holding {
        s.holding = false;
        brk_segment(s, b, st, ctx, sr);
    } else if !gate && b.release != NO_RELEASE && s.seg < b.release && !s.done {
        // Released before the release node: on from there, from where it is.
        s.seg = b.release;
        brk_segment(s, b, st, ctx, sr);
    }
}

/// Begin segment `s.seg` (from `s.level` to its level), or finish.
fn brk_segment(s: &mut BrkState, b: &Brk, st: &VoiceState, ctx: &MonoCtx, sr: f32) {
    let k = usize::from(s.seg);
    if k + 1 >= usize::from(b.n) {
        s.done = true;
        return;
    }
    let target = b.levels.get(k + 1).map_or(0.0, |l| st.fixed(*l, ctx));
    let time = b.times.get(k).map_or(0.0, |t| st.fixed(*t, ctx)).max(0.0);
    let n = (time * sr).max(1.0);
    s.left = n as u32;
    s.target = target;
    let curve = b.curves.get(k).copied().unwrap_or(0.0);
    let kind = b.kinds.get(k).copied().unwrap_or(0);
    s.kind = kind;
    match kind {
        // A curvature: SuperCollider's `a2 - b1, b1 *= grow`, once a segment.
        1 if curve.abs() > 1e-3 => {
            let a1 = (target - s.level) / (1.0 - curve.exp());
            s.a2 = s.level + a1;
            s.b1 = a1;
            s.grow = (curve / n).exp();
        }
        // Exponential, between levels of one sign.
        2 if s.level * target > 0.0 => {
            s.grow = (target / s.level).powf(1.0 / n);
        }
        3 => {
            s.level = target;
            s.inc = 0.0;
            s.kind = 3;
        }
        _ => {
            s.kind = 0;
            s.inc = (target - s.level) / n;
        }
    }
    if kind == 2 && s.level * target <= 0.0 {
        s.kind = 0;
        s.inc = (target - s.level) / n;
    }
    if kind == 1 && curve.abs() <= 1e-3 {
        s.kind = 0;
        s.inc = (target - s.level) / n;
    }
}

/// One sample of a breakpoint envelope.
fn brk_step(s: &mut BrkState, b: &Brk, st: &VoiceState, ctx: &MonoCtx, sr: f32) -> f32 {
    if s.done || s.holding {
        return s.level;
    }
    match s.kind {
        1 => {
            s.b1 *= s.grow;
            s.level = s.a2 - s.b1;
        }
        2 => s.level *= s.grow,
        3 => {}
        _ => s.level += s.inc,
    }
    s.left = s.left.saturating_sub(1);
    if s.left == 0 {
        s.level = s.target;
        s.seg += 1;
        if b.release != NO_RELEASE && s.seg == b.release {
            s.holding = true;
        } else {
            brk_segment(s, b, st, ctx, sr);
        }
    }
    s.level
}

/// Whether an envelope still shapes a sound: not idle, and not settled at
/// a sustain of nothing (`env(perc)` with the key held).
fn sounding(e: &Env) -> bool {
    match e.stage {
        Stage::Idle => false,
        Stage::Sustain => e.level() > 1e-4,
        _ => true,
    }
}

/// Open or close `env` with the gate, as the other voices do.
fn gate_env(env: &mut Env, t: &EnvTimes, retrigger: bool, gate: bool) {
    if retrigger || (gate && !env.gated()) {
        env.gate_on(t);
    } else if !gate && env.gated() {
        env.gate_off(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mono::model::Hp;

    /// The program and knobs of a SynthDef of `body`, `freq` its argument.
    fn patch(body: &str) -> sc::Patch {
        let text = format!("SynthDef(\\t, {{ |freq = 440, gate = 1| {body} }}).add;");
        sc::compile(&text).unwrap_or_else(|e| panic!("{text}: {e:?}"))
    }

    #[test]
    fn fast_math_is_close() {
        for x in [0.01_f32, 0.5, 1.0, 3.7, 440.0, 20_000.0] {
            assert!((fast_log2(x) - x.log2()).abs() < 1e-5, "log2 {x}");
        }
        for x in [-10.0_f32, -1.5, 0.0, 0.3, 1.0, 7.9] {
            let want = x.exp2();
            assert!((fast_exp2(x) - want).abs() / want < 2e-5, "exp2 {x}");
        }
    }

    /// `range` reads an envelope or the gate from 0, as SuperCollider's
    /// does, and an oscillator from −1.
    #[test]
    fn range_reads_a_unipolar_source_from_0() {
        let uni = |body: &str| {
            let p = patch(body).program;
            p.nodes
                .iter()
                .take(usize::from(p.len))
                .find_map(|n| match n {
                    Ugen::Range { uni, .. } => Some(*uni),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{body}"))
        };
        assert!(uni(
            "SinOsc.ar(EnvGen.kr(Env.perc(0.01, 0.3)).range(50, 400))"
        ));
        assert!(!uni("SinOsc.ar(Saw.ar(freq).range(50, 400))"));
    }

    /// #307, #316, #321: a voicing is a synth's lowercase name, `revN` added
    /// for another revision; each word names a filter of its class, an
    /// unknown one is refused with exactly the class's words.
    #[test]
    fn voicing_words_name_the_synths() {
        let refused = |body: &str| {
            let text = format!("SynthDef(\\t, {{ |freq = 440| {body} }}).add;");
            sc::compile(&text).expect_err(body).msg
        };
        for (class, ugen, opening) in [
            (Class::Ladder, "MoogFF", "a MoogFF voicing is"),
            (Class::Lp12, "RLPF", "a filter voicing is"),
            (Class::Hp, "HPF", "an HPF voicing is"),
        ] {
            let words: Vec<String> = VOICINGS
                .iter()
                .filter(|(_, k, ..)| *k == class)
                .map(|(w, ..)| format!("\\{w}"))
                .collect();
            let (last, rest) = words.split_last().expect("a class has words");
            let listed = class.words();
            assert_eq!(listed, format!("{opening} {} or {last}", rest.join(" ")));
            assert_eq!(
                refused(&format!("{ugen}.ar(Saw.ar(freq), 800, voicing: \\nope)")),
                listed
            );
            assert_eq!(
                refused(&format!("{ugen}.ar(Saw.ar(freq), 800, voicing: 3)")),
                listed
            );
        }
        for (word, class, m, setting) in VOICINGS {
            let name = Model::ALL
                .iter()
                .find(|(n, _)| *n == m)
                .map(|(_, name)| name.to_ascii_lowercase())
                .expect(word);
            let rev = format!("{name}rev{}", setting.rev);
            assert!(word == name || word == rev, "{word}");
            let f = m.low_pass(setting);
            match class {
                Class::Ladder => assert!(matches!(f, Filter::Ladder(_)), "{word}"),
                Class::Lp12 => assert!(matches!(f, Filter::Svf(_)), "{word}"),
                Class::Hp => assert_ne!(m.hp(), Hp::None, "{word}"),
            }
        }
    }

    /// #321: every fixed synth's filters have a word: its low-pass, each
    /// setting of its switches and its high-pass. A model added without one
    /// fails here.
    #[test]
    fn every_fixed_filter_has_a_word() {
        let filterless = [
            Model::Dx7,
            Model::Tr808,
            Model::Tr909,
            Model::Sampler,
            Model::PadSampler,
            Model::Modular,
        ];
        let named = |class: Class, f: Filter| {
            VOICINGS
                .iter()
                .any(|(_, k, m, s)| *k == class && m.low_pass(*s) == f)
        };
        for (m, name) in Model::ALL {
            if filterless.contains(&m) {
                continue;
            }
            for s in std::iter::once(Setting::OWN).chain(Setting::SWITCHED) {
                let f = m.low_pass(s);
                let class = match f {
                    Filter::Ladder(_) => Class::Ladder,
                    Filter::Svf(_) => Class::Lp12,
                };
                assert!(named(class, f), "{name} at {s:?}");
            }
            if m.hp() != Hp::None {
                let has = VOICINGS
                    .iter()
                    .any(|(_, k, h, _)| *k == Class::Hp && *h == m);
                assert!(has, "{name} high-pass");
            }
        }
    }

    /// A SynthDef rendered node by node with `MonoCtx` at 48 kHz, its knobs
    /// at their numbers.
    struct Bench {
        params: crate::mono::MonoParams,
        sine: Vec<f32>,
        blep: crate::mono::osc::Blep,
        ladder: crate::mono::ladder::LadderTables,
        pitch: crate::mono::voice::PitchTable,
        tables: &'static crate::table::Tables,
    }

    impl Bench {
        fn new() -> Bench {
            Bench {
                params: crate::mono::MonoParams::new(48_000.0),
                sine: crate::voice::sine_table(),
                blep: crate::mono::osc::Blep::new(),
                ladder: crate::mono::ladder::LadderTables::new(48_000.0),
                pitch: crate::mono::voice::PitchTable::new(48_000.0),
                tables: crate::table::Tables::shared(48_000.0),
            }
        }

        /// Each sample's value of the first oscillator (the filter's input)
        /// and of the last node, with the note held at 110 Hz.
        fn nodes(&mut self, body: &str, n: usize) -> Vec<(f32, f32)> {
            let patch = patch(body);
            for k in &patch.knobs {
                if let Some(c) = self.params.ctl.get_mut(k.ctl) {
                    *c = k.default;
                }
            }
            let ctx = MonoCtx {
                params: &self.params,
                sine: &self.sine,
                blep: &self.blep,
                ladder: &self.ladder,
                pitch: &self.pitch,
                shared: None,
                tables: self.tables,
            };
            let p = patch.program;
            let input = p
                .nodes
                .iter()
                .position(|n| matches!(n, Ugen::Osc { .. } | Ugen::Sin { .. }))
                .expect("an oscillator");
            let mut v = GraphVoice::new(1);
            v.press(45, 1.0, &p, 48_000.0, 0);
            let mut st = VoiceState::for_program(&p);
            st.prog = p;
            let len = usize::from(p.len);
            (0..n)
                .map(|_| {
                    for i in 0..len {
                        let y = v.eval(&mut st, i, 110.0, 1.0 / 48_000.0, true, &ctx);
                        st.vals[i] = y;
                    }
                    (st.vals[input], st.vals[len - 1])
                })
                .collect()
        }
    }

    /// #307, #316, #321: a voiced filter is the fixed synth's filter: the
    /// same samples out for the same samples in, for every word. Without a
    /// voicing `MoogFF` is the Moog's ladder at unity drive, per-stage
    /// saturation and all (#306).
    #[test]
    fn a_voiced_filter_renders_as_the_synths() {
        let mut b = Bench::new();
        let note = 69.0 + 12.0 * fast_log2(800.0 / 440.0);
        for (word, class, m, setting) in VOICINGS {
            let body = match class {
                Class::Ladder => {
                    format!("MoogFF.ar(Saw.ar(freq), 800, 3, voicing: \\{word}, drive: 0.5)")
                }
                Class::Lp12 => format!("RLPF.ar(Saw.ar(freq), 800, 0.1, voicing: \\{word})"),
                Class::Hp => format!("HPF.ar(Saw.ar(freq), 800, voicing: \\{word})"),
            };
            let (mut l, mut f, mut p) = (Ladder::new(), Svf::new(), OnePole::default());
            for (i, (x, y)) in b.nodes(&body, 4_800).into_iter().enumerate() {
                let want = match (class, m.low_pass(setting), m.hp()) {
                    (Class::Ladder, Filter::Ladder(v), _) => {
                        // MoogFF's gain 3 is three quarters of the knob (#342).
                        l.voiced(&b.ladder, &v, x, note, 0.75 * MAX_K, 4.5)
                    }
                    (Class::Lp12, Filter::Svf(v), _) => {
                        f.process(&b.ladder, &v, x, note, 1.0 - 0.1).lp
                    }
                    (Class::Hp, _, Hp::OnePole) => p.process(&b.ladder, x, note),
                    (Class::Hp, Filter::Svf(v), Hp::Svf) => {
                        f.process(&b.ladder, &v, x, note, 0.0).hp
                    }
                    other => panic!("{word}: {other:?}"),
                };
                assert_eq!(y, want, "{word} at {i}");
            }
        }
        let mut l = Ladder::new();
        for (x, y) in b.nodes("MoogFF.ar(Saw.ar(freq), 800, 3)", 4_800) {
            assert_eq!(y, l.voiced(&b.ladder, &MOOG, x, note, 0.75 * MAX_K, 1.0));
        }
        // RHPF takes the 12 dB filter's high output.
        for (word, class, m, setting) in VOICINGS {
            let Filter::Svf(v) = m.low_pass(setting) else {
                continue;
            };
            if class != Class::Lp12 {
                continue;
            }
            let body = format!("RHPF.ar(Saw.ar(freq), 800, 0.1, voicing: \\{word})");
            let mut f = Svf::new();
            for (x, y) in b.nodes(&body, 4_800) {
                assert_eq!(y, f.process(&b.ladder, &v, x, note, 1.0 - 0.1).hp, "{word}");
            }
        }
    }

    /// A `MoogFF`'s drive pushes the signal into the ladder's knee.
    #[test]
    fn drive_shapes_the_sound() {
        let mut b = Bench::new();
        let mut rms = |body: &str| {
            let y: Vec<f32> = b.nodes(body, 24_000).into_iter().map(|(_, y)| y).collect();
            assert!(y.iter().all(|s| s.is_finite() && s.abs() <= 2.0), "{body}");
            let tail = &y[4_800..];
            (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt()
        };
        let soft = rms("MoogFF.ar(SinOsc.ar(freq) * 0.1, 5000, 0, voicing: \\minimoog, drive: 0)");
        let hot = rms("MoogFF.ar(SinOsc.ar(freq) * 0.1, 5000, 0, voicing: \\minimoog, drive: 1)");
        assert!(
            hot > 4.0 * soft,
            "drive raises the level into the knee: {soft} -> {hot}"
        );
    }

    #[test]
    fn the_default_program_is_a_saw() {
        let p = Program::default();
        assert_eq!(p.len, 2);
        assert!(matches!(
            p.node(1),
            Some(Ugen::Osc {
                wave: Waveform::Saw,
                freq: 0,
                ..
            })
        ));
        assert_eq!(p.node(0), Some(Ugen::Freq));
    }
}
