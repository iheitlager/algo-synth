//! The modular voice (ADR-0020, ADR-0021, #216): a voice that is a graph of
//! unit generators written in the song text, compiled once and played per
//! note by the voice pool.
//!
//! ```text
//! voice hoover = { sin(saw(freq * 0.5).range(freq, freq * 3)) |> svf(lp, lfo(0.3).exprange(300, 3000)) * env(adsr) }
//! ```
//!
//! Unit generators are calls and run at audio rate, bipolar (−1..1) and in
//! hertz: `sin(f) saw(f) tri(f) pulse(f, width) noise() lfo(rate, shape)`,
//! the shared state-variable filter `svf(mode, cutoff, res)` (`lp` or `hp`)
//! and `env(adsr)`, `env(perc)` or `env(a, d, s, r)`. `freq gate vel` come
//! from the note. `+ - * /` combine signals; `.range(a, b)` and
//! `.exprange(a, b)` map −1..1 onto a..b, as SuperCollider's do; `x |> f(…)`
//! passes `x` into a filter as its input. A voice without `env` sounds
//! through the synth's ADSR.
//!
//! The parser compiles a voice into a fixed array of nodes, children before
//! parents, with fixed slots for the oscillators, filters and envelopes
//! (ADR-0021's limits), and prints it back canonically. A voice copies the
//! program when a note starts, so a changed graph takes the next note while
//! the old one finishes. `render` evaluates the array once a sample with no
//! allocation and no transcendental call: `sin` reads the sine table, and
//! `exprange` and the filter's cutoff use a fast `exp2`/`log2` (ADR-0002).

use std::fmt::Write;

use crate::mono::env::{Env, EnvTimes, Stage};
use crate::mono::ladder::{Ladder, MAX_K};
use crate::mono::model::SvfVoicing;
use crate::mono::noise::Noise;
use crate::mono::osc::{Osc, Waveform, naive};
use crate::mono::svf::Svf;
use crate::mono::voice::MonoCtx;
use crate::song::signal::{Tok, Token, lex};
use crate::voice::lookup;

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
/// The old voice language's limits (ADR-0021), until it goes (ADR-0024).
const OLD_NODES: usize = 32;
const OLD_OSCS: usize = 8;
const OLD_PHASES: usize = 8;
const OLD_FILTERS: usize = 4;
const OLD_ENVS: usize = 4;
const OLD_DELAYS: usize = 1;
/// Most `delay`s in a voice, and the longest one in samples (about 21 ms at
/// 48 kHz: combs, flangers and chorus, not echoes).
pub const MAX_DELAYS: usize = 32;
pub const MAX_DELAY: usize = 1024;
/// Most numbers in all of a voice's lists, and inputs to one `mix`.
pub const MAX_LIST: usize = 16;
const MAX_MIX: usize = 8;
/// Deepest nesting of brackets and minus signs.
const MAX_DEPTH: usize = 16;
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
/// What a voice is made of, for the error that names it.
const UNITS: &str = "a voice is made of sin saw tri pulse noise lfo fm svf ladder delay drive mix env, numbers, lists and freq gate vel";

/// `env(perc)`: a click of attack, a third of a second to nothing.
const PERC: Shape = Shape::Times(0.002, 0.3, 0.0, 0.3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    fn char(self) -> char {
        match self {
            Op::Add => '+',
            Op::Sub => '-',
            Op::Mul => '*',
            Op::Div => '/',
        }
    }

    fn prec(self) -> u8 {
        match self {
            Op::Add | Op::Sub => 1,
            Op::Mul | Op::Div => 2,
        }
    }
}

/// An envelope's shape: the synth's ADSR, or its own times in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Adsr,
    Times(f32, f32, f32, f32),
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

/// One unit generator; `u8` fields index earlier nodes, `slot` its state.
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
    /// The shared 24 dB ladder: `res` 0..1 up to self-oscillation.
    Ladder {
        input: u16,
        cutoff: u16,
        res: u16,
        slot: u8,
        /// `res` is `MoogFF`'s gain, 0..4, the feedback itself.
        gain: bool,
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
    /// A soft clip that keeps a full-scale input at full scale.
    Drive {
        input: u16,
        amount: u16,
    },
    /// Up to eight inputs added (SuperCollider's `Mix`), or their mean.
    Mix {
        inputs: [u16; MAX_MIX],
        n: u8,
        mean: bool,
    },
    /// A list of numbers: each voice slot takes its own, round the list.
    List {
        from: u8,
        len: u8,
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
    /// A control of the voice (`ctl`): the synth's `Param::Ctl1`… value,
    /// held in its range.
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
    /// The numbers of the voice's lists.
    lists: [f32; MAX_LIST],
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
    lists: u8,
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
            lists: [0.0; MAX_LIST],
            counts: Counts {
                oscs: 1,
                ..Counts::default()
            },
        }
    }
}

impl Program {
    /// Compile `text`, the inside of a voice's braces. An error's column
    /// counts in chars from 1.
    pub fn parse(text: &str) -> Result<Program, (usize, &'static str)> {
        Program::parse_with(text, &[])
    }

    /// Compile `text` with the voice's controls, by name and range.
    pub fn parse_with(text: &str, ctls: &[Control<'_>]) -> Result<Program, (usize, &'static str)> {
        let toks = lex(text, true)?;
        let mut p = Parser {
            toks: &toks,
            at: 0,
            end: text.chars().count() + 1,
            prog: Program {
                nodes: [Ugen::Num(0.0); MAX_NODES],
                len: 0,
                own_amp: false,
                right: NONE,
                envs: [Shape::Adsr; MAX_ENVS],
                lists: [0.0; MAX_LIST],
                counts: Counts::default(),
            },
            depth: 0,
            ctls,
        };
        // Children go before parents, so the root is the last node.
        p.sum()?;
        if let Some(t) = toks.get(p.at) {
            return Err((t.col, "unexpected text"));
        }
        Ok(p.prog)
    }

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

    fn write(&self, out: &mut String, names: &[&str], i: u16, prec: u8) -> std::fmt::Result {
        let Some(node) = self.node(i) else {
            return Ok(());
        };
        let wave = |w: Waveform| match w {
            Waveform::Saw => "saw",
            Waveform::Pulse => "pulse",
            Waveform::Triangle => "tri",
            Waveform::Sine => "sine",
        };
        match node {
            Ugen::Num(v) if v < 0.0 && prec > 3 => write!(out, "({v})"),
            Ugen::Num(v) => write!(out, "{v}"),
            Ugen::Freq => out.write_str("freq"),
            Ugen::Gate => out.write_str("gate"),
            Ugen::Vel => out.write_str("vel"),
            Ugen::Rand { .. } => out.write_str("rand"),
            Ugen::Pan { .. } => out.write_str("pan"),
            Ugen::Verb { .. } | Ugen::VerbR { .. } => out.write_str("verb"),
            Ugen::MidiRatio(x) => {
                self.write(out, names, x, 4)?;
                out.write_str(".midiratio")
            }
            Ugen::Ctl { index, .. } => match names.get(usize::from(index)) {
                Some(n) => out.write_str(n),
                None => write!(out, "ctl{}", index + 1),
            },
            Ugen::Noise => out.write_str("noise()"),
            Ugen::Osc {
                wave: w,
                freq,
                width,
                ..
            } => {
                write!(out, "{}(", wave(w))?;
                self.write(out, names, freq, 0)?;
                if width != NONE {
                    out.push_str(", ");
                    self.write(out, names, width, 0)?;
                }
                out.push(')');
                Ok(())
            }
            Ugen::Sin { freq, .. } => {
                out.push_str("sin(");
                self.write(out, names, freq, 0)?;
                out.push(')');
                Ok(())
            }
            Ugen::Lfo { rate, wave: w, .. } => {
                out.push_str("lfo(");
                self.write(out, names, rate, 0)?;
                if w != Waveform::Sine {
                    let name = if w == Waveform::Pulse {
                        "square"
                    } else {
                        wave(w)
                    };
                    write!(out, ", {name}")?;
                }
                out.push(')');
                Ok(())
            }
            Ugen::Svf {
                input,
                high,
                cutoff,
                res,
                ..
            } => {
                self.write(out, names, input, 4)?;
                write!(out, " |> svf({}, ", if high { "hp" } else { "lp" })?;
                self.write(out, names, cutoff, 0)?;
                if res != NONE {
                    out.push_str(", ");
                    self.write(out, names, res, 0)?;
                }
                out.push(')');
                Ok(())
            }
            Ugen::Env { slot } => match self.envs.get(usize::from(slot)) {
                Some(Shape::Adsr) | None => out.write_str("env(adsr)"),
                Some(s) if *s == PERC => out.write_str("env(perc)"),
                Some(Shape::Times(a, d, s, r)) => write!(out, "env({a}, {d}, {s}, {r})"),
                Some(Shape::Nodes(_) | Shape::Brk(_)) => out.write_str("env(adsr)"),
            },
            Ugen::Ladder {
                input, cutoff, res, ..
            } => {
                self.write(out, names, input, 4)?;
                out.push_str(" |> ladder(");
                self.write(out, names, cutoff, 0)?;
                if res != NONE {
                    out.push_str(", ");
                    self.write(out, names, res, 0)?;
                }
                out.push(')');
                Ok(())
            }
            Ugen::Delay {
                input,
                time,
                feedback,
                ..
            } => {
                self.write(out, names, input, 4)?;
                out.push_str(" |> delay(");
                self.write(out, names, time, 0)?;
                if feedback != NONE {
                    out.push_str(", ");
                    self.write(out, names, feedback, 0)?;
                }
                out.push(')');
                Ok(())
            }
            Ugen::Clip { input, kind } => {
                self.write(out, names, input, 4)?;
                let name = ["tanh", "softclip", "distort"]
                    .get(usize::from(kind))
                    .copied()
                    .unwrap_or("tanh");
                write!(out, ".{name}")
            }
            Ugen::Drive { input, amount } => {
                self.write(out, names, input, 4)?;
                out.push_str(" |> drive(");
                self.write(out, names, amount, 0)?;
                out.push(')');
                Ok(())
            }
            Ugen::Fm {
                carrier,
                modulator,
                index,
                ..
            } => {
                out.push_str("fm(");
                self.write(out, names, carrier, 0)?;
                out.push_str(", ");
                self.write(out, names, modulator, 0)?;
                out.push_str(", ");
                self.write(out, names, index, 0)?;
                out.push(')');
                Ok(())
            }
            Ugen::Mix { inputs, n, .. } => {
                out.push_str("mix(");
                for (k, i) in inputs.iter().take(usize::from(n)).enumerate() {
                    if k > 0 {
                        out.push_str(", ");
                    }
                    self.write(out, names, *i, 0)?;
                }
                out.push(')');
                Ok(())
            }
            Ugen::List { from, len } => {
                out.push('[');
                let from = usize::from(from);
                for (k, v) in self
                    .lists
                    .iter()
                    .skip(from)
                    .take(usize::from(len))
                    .enumerate()
                {
                    if k > 0 {
                        out.push_str(", ");
                    }
                    write!(out, "{v}")?;
                }
                out.push(']');
                Ok(())
            }
            Ugen::Neg(a) => {
                if prec > 3 {
                    out.push('(');
                }
                out.push('-');
                self.write(out, names, a, 3)?;
                if prec > 3 {
                    out.push(')');
                }
                Ok(())
            }
            Ugen::Bin(op, a, b) => {
                let p = op.prec();
                if p < prec {
                    out.push('(');
                }
                self.write(out, names, a, p)?;
                write!(out, " {} ", op.char())?;
                self.write(out, names, b, p + 1)?;
                if p < prec {
                    out.push(')');
                }
                Ok(())
            }
            Ugen::Range {
                of, lo, hi, exp, ..
            } => {
                self.write(out, names, of, 4)?;
                out.push_str(if exp { ".exprange(" } else { ".range(" });
                self.write(out, names, lo, 0)?;
                out.push_str(", ");
                self.write(out, names, hi, 0)?;
                out.push(')');
                Ok(())
            }
        }
    }
}

impl Program {
    /// The canonical text, without the braces, naming the controls `names`.
    pub fn print(&self, names: &[&str]) -> String {
        let mut out = String::new();
        if let Some(root) = self.len.checked_sub(1) {
            // Writing into a `String` does not fail.
            if self.write(&mut out, names, root, 0).is_err() {
                out.clear();
            }
        }
        out
    }
}

impl std::fmt::Display for Program {
    /// The canonical text of a voice without controls.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.print(&[]))
    }
}

struct Parser<'a, 'b> {
    toks: &'b [Token<'a>],
    at: usize,
    end: usize,
    prog: Program,
    depth: usize,
    ctls: &'b [Control<'b>],
}

/// A control as the parser sees it: its name and range.
#[derive(Clone, Copy, Debug)]
pub struct Control<'a> {
    pub name: &'a str,
    pub lo: f32,
    pub hi: f32,
}

type Res<T> = Result<T, (usize, &'static str)>;

impl<'a> Parser<'a, '_> {
    fn peek(&self) -> Option<Tok<'a>> {
        self.toks.get(self.at).map(|t| t.tok)
    }

    fn col(&self) -> usize {
        self.toks.get(self.at).map_or(self.end, |t| t.col)
    }

    fn eat(&mut self, c: char) -> bool {
        let hit = self.peek() == Some(Tok::Punct(c));
        if hit {
            self.at += 1;
        }
        hit
    }

    fn expect(&mut self, c: char, msg: &'static str) -> Res<()> {
        if self.eat(c) {
            Ok(())
        } else {
            Err((self.col(), msg))
        }
    }

    fn push(&mut self, u: Ugen) -> Res<u16> {
        let len = usize::from(self.prog.len);
        let Some(slot) = self.prog.nodes.get_mut(len).filter(|_| len < OLD_NODES) else {
            return Err((self.col(), "a voice has at most 32 nodes"));
        };
        *slot = u;
        self.prog.len += 1;
        Ok(self.prog.len - 1)
    }

    fn num(&mut self, v: f32) -> Res<u16> {
        self.push(Ugen::Num(v))
    }

    /// The next slot of a kind of state, or the error that says its limit.
    fn slot(count: &mut u8, limit: usize, col: usize, msg: &'static str) -> Res<u8> {
        if usize::from(*count) >= limit {
            return Err((col, msg));
        }
        *count += 1;
        Ok(*count - 1)
    }

    fn deeper(&mut self) -> Res<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err((self.col(), "a voice nests at most 16 deep"));
        }
        Ok(())
    }

    fn sum(&mut self) -> Res<u16> {
        let mut a = self.product()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Punct('+')) => Op::Add,
                Some(Tok::Punct('-')) => Op::Sub,
                _ => return Ok(a),
            };
            self.at += 1;
            let b = self.product()?;
            a = self.push(Ugen::Bin(op, a, b))?;
        }
    }

    fn product(&mut self) -> Res<u16> {
        let mut a = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Punct('*')) => Op::Mul,
                Some(Tok::Punct('/')) => Op::Div,
                _ => return Ok(a),
            };
            self.at += 1;
            let b = self.unary()?;
            a = self.push(Ugen::Bin(op, a, b))?;
        }
    }

    fn unary(&mut self) -> Res<u16> {
        if !self.eat('-') {
            return self.postfix();
        }
        self.deeper()?;
        let a = self.unary()?;
        self.depth -= 1;
        // A minus on a number is part of it, so `-(2)` prints as `-2`.
        if let Some(Ugen::Num(v)) = self.prog.nodes.get_mut(usize::from(a)) {
            *v = -*v;
            return Ok(a);
        }
        self.push(Ugen::Neg(a))
    }

    /// Methods and `|>` after a primary, left to right.
    fn postfix(&mut self) -> Res<u16> {
        let mut a = self.primary()?;
        loop {
            if self.eat('.') {
                let col = self.col();
                let exp = match self.peek() {
                    Some(Tok::Word("range")) => false,
                    Some(Tok::Word("exprange")) => true,
                    _ => return Err((col, "a method is range or exprange")),
                };
                self.at += 1;
                self.expect('(', "( goes here")?;
                let lo = self.sum()?;
                self.expect(',', ", and a high value go here")?;
                let hi = self.sum()?;
                self.expect(')', ") goes here")?;
                if exp
                    && let (Some(Ugen::Num(l)), Some(Ugen::Num(h))) =
                        (self.prog.node(lo), self.prog.node(hi))
                    && l * h <= 0.0
                {
                    return Err((col, "exprange needs two numbers of one sign, not 0"));
                }
                let uni = matches!(
                    self.prog.node(a),
                    Some(Ugen::Env { .. } | Ugen::Gate | Ugen::Vel)
                );
                a = self.push(Ugen::Range {
                    of: a,
                    lo,
                    hi,
                    exp,
                    uni,
                })?;
            } else if self.eat('|') {
                let col = self.col();
                let Some(Tok::Word(name)) = self.peek() else {
                    return Err((col, "a filter goes after |>, e.g. svf(lp, 800)"));
                };
                self.at += 1;
                a = match name {
                    "svf" => self.svf(Some(a), col)?,
                    "ladder" | "delay" | "drive" => self.effect(name, Some(a), col)?,
                    _ => {
                        return Err((
                            col,
                            "only svf, ladder, delay and drive take a signal through |>",
                        ));
                    }
                };
            } else {
                return Ok(a);
            }
        }
    }

    fn primary(&mut self) -> Res<u16> {
        let col = self.col();
        match self.peek() {
            Some(Tok::Num(v)) => {
                self.at += 1;
                self.num(v)
            }
            Some(Tok::Punct('(')) => {
                self.at += 1;
                self.deeper()?;
                let a = self.sum()?;
                self.expect(')', ") goes here")?;
                self.depth -= 1;
                Ok(a)
            }
            Some(Tok::Punct('[')) => {
                self.at += 1;
                self.list(col)
            }
            Some(Tok::Word(w)) => {
                self.at += 1;
                match w {
                    "freq" => self.push(Ugen::Freq),
                    "gate" => self.push(Ugen::Gate),
                    "vel" => self.push(Ugen::Vel),
                    _ => match self.ctls.iter().position(|c| c.name == w) {
                        Some(i) => {
                            let c = self.ctls.get(i).copied();
                            let (lo, hi) = c.map_or((0.0, 0.0), |c| (c.lo, c.hi));
                            self.push(Ugen::Ctl {
                                index: u8::try_from(i).unwrap_or(0),
                                lo,
                                hi,
                            })
                        }
                        None => self.call(w, col),
                    },
                }
            }
            _ => Err((col, "a signal goes here, e.g. saw(freq)")),
        }
    }

    /// `name(…)`: a unit generator.
    fn call(&mut self, name: &str, col: usize) -> Res<u16> {
        let wave = match name {
            "saw" => Some(Waveform::Saw),
            "tri" => Some(Waveform::Triangle),
            "pulse" => Some(Waveform::Pulse),
            _ => None,
        };
        if !matches!(
            name,
            "saw"
                | "tri"
                | "pulse"
                | "sin"
                | "noise"
                | "lfo"
                | "svf"
                | "env"
                | "ladder"
                | "delay"
                | "drive"
                | "fm"
                | "mix"
        ) {
            return Err((col, UNITS));
        }
        self.expect('(', "( goes here")?;
        if name == "svf" {
            self.depth += 1;
            let r = self.svf(None, col);
            self.depth -= 1;
            return r;
        }
        if matches!(name, "ladder" | "delay" | "drive") {
            self.depth += 1;
            let r = self.effect(name, None, col);
            self.depth -= 1;
            return r;
        }
        if name == "fm" {
            self.deeper()?;
            let carrier = self.sum()?;
            self.expect(',', "fm takes a carrier, a modulator in hertz and an index")?;
            let modulator = self.sum()?;
            self.expect(',', "fm takes a carrier, a modulator in hertz and an index")?;
            let index = self.sum()?;
            self.expect(')', ") goes here")?;
            self.depth -= 1;
            let slot = Self::slot(
                &mut self.prog.counts.phases,
                OLD_PHASES - 1,
                col,
                "a voice has at most 8 sin and lfo, an fm counting two",
            )?;
            self.prog.counts.phases += 1;
            return self.push(Ugen::Fm {
                carrier,
                modulator,
                index,
                slot,
            });
        }
        if name == "mix" {
            self.deeper()?;
            let mut inputs = [0u16; MAX_MIX];
            let mut n = 0usize;
            loop {
                let a = self.sum()?;
                let Some(slot) = inputs.get_mut(n).filter(|_| n < 4) else {
                    return Err((col, "mix takes at most 4 inputs"));
                };
                *slot = a;
                n += 1;
                if !self.eat(',') {
                    break;
                }
            }
            self.expect(')', ") goes here")?;
            self.depth -= 1;
            return self.push(Ugen::Mix {
                inputs,
                n: u8::try_from(n).unwrap_or(0),
                mean: true,
            });
        }
        if name == "env" {
            return self.env(col);
        }
        if name == "noise" {
            self.expect(')', "noise takes nothing: noise()")?;
            return self.push(Ugen::Noise);
        }
        self.deeper()?;
        let first = self.sum()?;
        let node = match (name, wave) {
            ("lfo", _) => {
                let w = if self.eat(',') {
                    let c = self.col();
                    let w = match self.peek() {
                        Some(Tok::Word("sine")) => Waveform::Sine,
                        Some(Tok::Word("saw")) => Waveform::Saw,
                        Some(Tok::Word("tri")) => Waveform::Triangle,
                        Some(Tok::Word("square")) => Waveform::Pulse,
                        _ => return Err((c, "a shape is sine, saw, tri or square")),
                    };
                    self.at += 1;
                    w
                } else {
                    Waveform::Sine
                };
                let slot = Self::slot(
                    &mut self.prog.counts.phases,
                    OLD_PHASES,
                    col,
                    "a voice has at most 8 sin and lfo",
                )?;
                Ugen::Lfo {
                    rate: first,
                    wave: w,
                    slot,
                }
            }
            ("sin", _) => Ugen::Sin {
                freq: first,
                slot: Self::slot(
                    &mut self.prog.counts.phases,
                    OLD_PHASES,
                    col,
                    "a voice has at most 8 sin and lfo",
                )?,
            },
            (_, Some(w)) => {
                let width = if w == Waveform::Pulse && self.eat(',') {
                    self.sum()?
                } else {
                    NONE
                };
                Ugen::Osc {
                    wave: w,
                    freq: first,
                    width,
                    slot: Self::slot(
                        &mut self.prog.counts.oscs,
                        OLD_OSCS,
                        col,
                        "a voice has at most 8 saw, tri and pulse",
                    )?,
                }
            }
            _ => return Err((col, "no such unit generator")),
        };
        self.expect(')', ") goes here")?;
        self.depth -= 1;
        self.push(node)
    }

    /// `ladder(cutoff, res)`, `delay(time, feedback)` or `drive(amount)`
    /// after `|>`, or with the input first; the `(` is read when not piped.
    fn effect(&mut self, name: &str, piped: Option<u16>, col: usize) -> Res<u16> {
        if piped.is_some() {
            self.expect('(', "( goes here")?;
        }
        let input = match piped {
            Some(i) => i,
            None => {
                let i = self.sum()?;
                self.expect(',', ", and the effect's values go here")?;
                i
            }
        };
        let first = self.sum()?;
        let second = if name != "drive" && self.eat(',') {
            self.sum()?
        } else {
            NONE
        };
        self.expect(')', ") goes here")?;
        let node = match name {
            "ladder" => Ugen::Ladder {
                input,
                cutoff: first,
                res: second,
                gain: false,
                slot: Self::slot(
                    &mut self.prog.counts.filters,
                    OLD_FILTERS,
                    col,
                    "a voice has at most 4 filters",
                )?,
            },
            "delay" => {
                let slot = Self::slot(
                    &mut self.prog.counts.delays,
                    OLD_DELAYS,
                    col,
                    "a voice has one delay",
                )?;
                Ugen::Delay {
                    input,
                    time: first,
                    feedback: second,
                    decay: false,
                    slot,
                    wet: false,
                }
            }
            _ => Ugen::Drive {
                input,
                amount: first,
            },
        };
        self.push(node)
    }

    /// `[a, b, …]`: numbers, one for each voice slot in turn; the `[` is read.
    fn list(&mut self, col: usize) -> Res<u16> {
        let from = self.prog.counts.lists;
        loop {
            let neg = self.eat('-');
            let c = self.col();
            let Some(Tok::Num(v)) = self.peek() else {
                return Err((c, "a list holds numbers, e.g. [1, 3]"));
            };
            self.at += 1;
            let Some(slot) = self.prog.lists.get_mut(usize::from(self.prog.counts.lists)) else {
                return Err((col, "a voice's lists hold at most 16 numbers"));
            };
            *slot = if neg { -v } else { v };
            self.prog.counts.lists += 1;
            if !self.eat(',') {
                break;
            }
        }
        self.expect(']', "] goes here")?;
        self.push(Ugen::List {
            from,
            len: self.prog.counts.lists - from,
        })
    }

    /// `svf(mode, cutoff, res)` after `|>`, or `svf(input, mode, cutoff, res)`;
    /// the `(` is read.
    fn svf(&mut self, piped: Option<u16>, col: usize) -> Res<u16> {
        if piped.is_some() {
            self.expect('(', "( goes here")?;
        }
        let input = match piped {
            Some(i) => i,
            None => {
                let i = self.sum()?;
                self.expect(',', ", and a mode go here: lp or hp")?;
                i
            }
        };
        let c = self.col();
        let high = match self.peek() {
            Some(Tok::Word("lp")) => false,
            Some(Tok::Word("hp")) => true,
            _ => return Err((c, "a filter mode is lp or hp")),
        };
        self.at += 1;
        self.expect(',', ", and a cutoff in hertz go here")?;
        let cutoff = self.sum()?;
        let res = if self.eat(',') { self.sum()? } else { NONE };
        self.expect(')', ") goes here")?;
        let slot = Self::slot(
            &mut self.prog.counts.filters,
            OLD_FILTERS,
            col,
            "a voice has at most 4 filters",
        )?;
        self.push(Ugen::Svf {
            input,
            high,
            cutoff,
            res,
            slot,
            rq: false,
        })
    }

    /// `env(adsr)`, `env(perc)` or `env(a, d, s, r)`; the `(` is read.
    fn env(&mut self, col: usize) -> Res<u16> {
        let shape = match self.peek() {
            Some(Tok::Word("adsr")) => {
                self.at += 1;
                Shape::Adsr
            }
            Some(Tok::Word("perc")) => {
                self.at += 1;
                PERC
            }
            _ => {
                let mut v = [0.0_f32; 4];
                for (k, x) in v.iter_mut().enumerate() {
                    if k > 0 {
                        self.expect(',', "an envelope is adsr, perc or four times: a, d, s, r")?;
                    }
                    let c = self.col();
                    let Some(Tok::Num(n)) = self.peek() else {
                        return Err((c, "an envelope is adsr, perc or four times: a, d, s, r"));
                    };
                    self.at += 1;
                    *x = n;
                }
                let [a, d, s, r] = v;
                if a > 10.0 || d > 10.0 || r > 10.0 || s > 1.0 {
                    return Err((col, "times are up to 10 seconds, the sustain up to 1"));
                }
                Shape::Times(a, d, s, r)
            }
        };
        self.expect(')', ") goes here")?;
        let slot = Self::slot(
            &mut self.prog.counts.envs,
            OLD_ENVS,
            col,
            "a voice has at most 4 envelopes",
        )?;
        if let Some(e) = self.prog.envs.get_mut(usize::from(slot)) {
            *e = shape;
        }
        self.push(Ugen::Env { slot })
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
                Shape::Times(a, d, s, r) => EnvTimes {
                    attack: a * sr,
                    decay: d * sr,
                    sustain: s,
                    release: r * sr,
                },
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
                let Some(f) = st.filters.get_mut(usize::from(slot)) else {
                    return 0.0;
                };
                let o = f.process(ctx.ladder, &VOICING, x, note, r);
                if high { o.hp } else { o.lp }
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
            } => {
                let x = st.val(input);
                let note = st.cutoff_note(slot, st.val(cutoff), self.cutoff_trim);
                let r = if res == NONE { RES } else { st.val(res) };
                let k = if gain {
                    r.clamp(0.0, MAX_K)
                } else {
                    r.clamp(0.0, 1.0) * MAX_K
                };
                let Some(l) = st.ladders.get_mut(usize::from(slot)) else {
                    return 0.0;
                };
                l.process(ctx.ladder, x, note, k, 1.0)
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
            Ugen::Drive { input, amount } => {
                let a = st.val(amount).max(0.0);
                let x = st.val(input);
                (1.0 + a) * x / (1.0 + a * x.abs())
            }
            Ugen::Mix { inputs, n, mean } => {
                let n = usize::from(n).max(1);
                let sum: f32 = inputs.iter().take(n).map(|i| st.val(*i)).sum();
                if mean { sum / n as f32 } else { sum }
            }
            Ugen::List { from, len } => {
                let len = usize::from(len).max(1);
                st.prog
                    .lists
                    .get(usize::from(from) + self.slot % len)
                    .copied()
                    .unwrap_or(0.0)
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

    fn prog(text: &str) -> Program {
        Program::parse(text).unwrap_or_else(|e| panic!("{text}: {e:?}"))
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

    #[test]
    fn a_voice_prints_canonically_and_parses_back() {
        for (text, canon) in [
            (
                "sin(saw(freq*0.5).range(freq,freq*3)) |> svf(lp, lfo(0.3).exprange(300,3000)) * env(adsr)",
                "sin(saw(freq * 0.5).range(freq, freq * 3)) |> svf(lp, lfo(0.3).exprange(300, 3000)) * env(adsr)",
            ),
            ("saw(freq)", "saw(freq)"),
            (
                "pulse(freq, 0.3) + noise() * 0.1",
                "pulse(freq, 0.3) + noise() * 0.1",
            ),
            (
                "svf(saw(freq), hp, 200, 0.5)",
                "saw(freq) |> svf(hp, 200, 0.5)",
            ),
            (
                "(saw(freq) + tri(freq*2)) |> svf(lp, 900)",
                "(saw(freq) + tri(freq * 2)) |> svf(lp, 900)",
            ),
            ("sin(freq) * env(perc)", "sin(freq) * env(perc)"),
            (
                "sin(freq) * env(0.01, 0.2, 0.5, 1)",
                "sin(freq) * env(0.01, 0.2, 0.5, 1)",
            ),
            ("lfo(5, square) * -0.5", "lfo(5, square) * -0.5"),
            ("freq", "freq"),
            ("0.25", "0.25"),
            (
                "saw(freq) |> ladder(800, 0.5)",
                "saw(freq) |> ladder(800, 0.5)",
            ),
            ("ladder(saw(freq), 800)", "saw(freq) |> ladder(800)"),
            (
                "pulse(freq) |> delay(0.004, 0.6) |> drive(3)",
                "pulse(freq) |> delay(0.004, 0.6) |> drive(3)",
            ),
            (
                "fm(freq, freq*2, env(perc).range(0, 5))",
                "fm(freq, freq * 2, env(perc).range(0, 5))",
            ),
            (
                "mix(saw(freq), tri(freq*2), noise())",
                "mix(saw(freq), tri(freq * 2), noise())",
            ),
            ("sin([220, 330, -1])", "sin([220, 330, -1])"),
        ] {
            let p = prog(text);
            assert_eq!(p.to_string(), canon, "{text}");
            assert_eq!(prog(canon), p, "{canon}");
        }
    }

    #[test]
    fn errors_say_where() {
        for (text, col, msg) in [
            ("", 1, "a signal goes here, e.g. saw(freq)"),
            (
                "square(freq)",
                1,
                "a voice is made of sin saw tri pulse noise lfo fm svf ladder delay drive mix env, numbers, lists and freq gate vel",
            ),
            ("saw(freq", 9, ") goes here"),
            (
                "saw(freq) |> sin(3)",
                14,
                "only svf, ladder, delay and drive take a signal through |>",
            ),
            ("saw(freq) |> svf(bp, 3)", 18, "a filter mode is lp or hp"),
            ("saw(freq).wobble(1)", 11, "a method is range or exprange"),
            (
                "saw(freq).exprange(0, 10)",
                11,
                "exprange needs two numbers of one sign, not 0",
            ),
            (
                "env(fast)",
                5,
                "an envelope is adsr, perc or four times: a, d, s, r",
            ),
            (
                "env(1, 2, 3, 4)",
                1,
                "times are up to 10 seconds, the sustain up to 1",
            ),
            ("lfo(1, pink)", 8, "a shape is sine, saw, tri or square"),
            ("noise(1)", 7, "noise takes nothing: noise()"),
            ("saw(freq) saw(freq)", 11, "unexpected text"),
            (
                "saw(freq) |> sin(3)",
                14,
                "only svf, ladder, delay and drive take a signal through |>",
            ),
            ("mix(1, 2, 3, 4, 5)", 1, "mix takes at most 4 inputs"),
            (
                "fm(1, 2)",
                8,
                "fm takes a carrier, a modulator in hertz and an index",
            ),
            ("sin([1, x])", 9, "a list holds numbers, e.g. [1, 3]"),
            ("sin([1, 2)", 10, "] goes here"),
            (
                "saw(1) |> delay(0.1) |> delay(0.1)",
                25,
                "a voice has one delay",
            ),
        ] {
            assert_eq!(Program::parse(text), Err((col, msg)), "{text}");
        }
        let many = ["saw(freq)"; 9].join(" + ");
        assert_eq!(
            Program::parse(&many).map_err(|e| e.1),
            Err("a voice has at most 8 saw, tri and pulse")
        );
        let long = ["1"; 40].join(" + ");
        assert_eq!(
            Program::parse(&long).map_err(|e| e.1),
            Err("a voice has at most 32 nodes")
        );
        let filters = format!("saw(freq){}", " |> svf(lp, 500)".repeat(5));
        assert_eq!(
            Program::parse(&filters).map_err(|e| e.1),
            Err("a voice has at most 4 filters")
        );
    }

    #[test]
    fn range_reads_a_unipolar_source_from_0() {
        let range = |text: &str| match prog(text).node(prog(text).len - 1) {
            Some(Ugen::Range { uni, .. }) => uni,
            _ => panic!("{text}"),
        };
        assert!(range("env(perc).range(50, 400)"));
        assert!(range("vel.range(0, 1)"));
        assert!(!range("saw(freq).range(50, 400)"));
        let long = ["1"; 17].join(", ");
        assert_eq!(
            Program::parse(&format!("sin([{long}])")).map_err(|e| e.1),
            Err("a voice's lists hold at most 16 numbers")
        );
    }

    #[test]
    fn the_default_program_is_a_saw() {
        assert_eq!(Program::default(), prog("saw(freq)"));
    }
}
