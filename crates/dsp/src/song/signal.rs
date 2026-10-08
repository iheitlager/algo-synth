//! Signals (ADR-0019): numbers that move, written in the song text and
//! evaluated at control rate.
//!
//! ```text
//! sine.range(300, 3000).slow(4)
//! lfo(0.2, saw).range(-2, 2) + perlin.exprange(200, 800)
//! ```
//!
//! A signal is a function of the song's position in cycles (one cycle is a
//! bar), so it is deterministic and lands in the same place after a seek. The
//! sources run from 0 to 1: `sine saw tri square` once per cycle, `rand` a new
//! value every sixteenth, `perlin` a smooth random curve through one value per
//! cycle, `lfo(rate, shape)` at a rate in hertz. A sequence of numbers in
//! mini-notation is a signal too: `"<300 800 1200>"` one value per cycle,
//! `"0 0.5 1 0.5"` the cycle shared between them. `+ - * /` combine them;
//! `.range(a, b)` and `.exprange(a, b)` map 0..1 onto a..b, linearly or
//! exponentially; `.slow(n)` and `.fast(n)` stretch time; `.segment(n)` holds
//! n values per cycle; `.lag(s)` follows its input with a time constant of s
//! seconds, the only node that keeps state.
//!
//! Two sources have a value per voice (ADR-0023): `env(adsr)`, `env(perc)` or
//! `env(a, d, s, r)`, an envelope from 0 to 1 that restarts with each note,
//! and a list `[a, b, …]`, number *i* for voice slot *i* round the list, also
//! as `lfo([1, 3])`'s rates. A signal that uses either is per voice: the
//! engine evaluates it for each voice of a Mono or Poly synth.
//!
//! The parser compiles an expression into a flat array of nodes, children
//! before parents, and prints it back canonically. Parsing allocates, so it
//! runs at load; `eval` doesn't, so it runs in `render` (ADR-0002).

use std::fmt::{self, Write};

/// The words a signal's source may be, and its methods; no other is read.
pub const SOURCES: [&str; 8] = [
    "sine", "saw", "tri", "square", "rand", "perlin", "lfo", "env",
];
pub const METHODS: [&str; 6] = ["range", "exprange", "slow", "fast", "segment", "lag"];

/// Most nodes in all of a song's signals, and so most `lag` slots.
pub const MAX_NODES: usize = 256;
/// Deepest nesting of brackets and minus signs.
const MAX_DEPTH: usize = 32;
/// Most numbers in one sequence.
const MAX_SEQ: usize = 64;
/// The value `rand` takes changes this often per cycle: once a sixteenth.
const RAND_PER_CYCLE: f64 = 16.0;
/// Most numbers in one list, as in the Modular voice (ADR-0021).
const MAX_LIST: usize = 16;
/// `env(perc)`: attack, decay, sustain and release, as the Modular voice's (ADR-0021).
const PERC: [f32; 4] = [0.002, 0.3, 0.0, 0.3];
/// A decay or release falls 60 dB over its time.
const FALL: f64 = 6.907_755;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Saw,
    Tri,
    Square,
}

impl Wave {
    const ALL: [(Wave, &'static str); 4] = [
        (Wave::Sine, "sine"),
        (Wave::Saw, "saw"),
        (Wave::Tri, "tri"),
        (Wave::Square, "square"),
    ];

    fn named(s: &str) -> Option<Wave> {
        Self::ALL.iter().find(|(_, n)| *n == s).map(|(w, _)| *w)
    }

    fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(w, _)| *w == self)
            .map_or("", |(_, n)| n)
    }

    /// The wave at `phase` cycles, from 0 to 1.
    fn at(self, phase: f64) -> f64 {
        let p = phase - phase.floor();
        match self {
            Wave::Sine => 0.5 + 0.5 * (std::f64::consts::TAU * p).sin(),
            Wave::Saw => p,
            Wave::Tri => 1.0 - (2.0 * p - 1.0).abs(),
            Wave::Square => {
                if p < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }
}

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

    /// Binding strength: `*` and `/` bind tighter than `+` and `-`.
    fn prec(self) -> u8 {
        match self {
            Op::Add | Op::Sub => 1,
            Op::Mul | Op::Div => 2,
        }
    }
}

/// One node; `usize` fields index earlier nodes of the same signal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Node {
    Num(f32),
    Wave(Wave),
    Lfo(f32, Wave),
    /// `lfo([a, b])`: `len` rates from `from` in the values, one per voice.
    LfoList {
        from: usize,
        len: usize,
        wave: Wave,
    },
    /// `[a, b]`: `len` numbers from `from` in the values, one per voice.
    List {
        from: usize,
        len: usize,
    },
    /// An envelope per voice: the synth's ADSR (`None`), or attack, decay,
    /// sustain and release in seconds.
    Env(Option<[f32; 4]>),
    Rand,
    Perlin,
    Neg(usize),
    Bin(Op, usize, usize),
    Range {
        of: usize,
        lo: f32,
        hi: f32,
        exp: bool,
    },
    Slow(usize, f32),
    Fast(usize, f32),
    Segment(usize, f32),
    Lag {
        of: usize,
        secs: f32,
        slot: usize,
    },
    /// `len` numbers from `from` in the signal's values: one per cycle
    /// (`"<a b>"`), or sharing each cycle (`"a b"`).
    Seq {
        from: usize,
        len: usize,
        per_cycle: bool,
    },
}

/// A compiled expression: its nodes, the last one the result.
#[derive(Clone, Debug, PartialEq)]
pub struct Signal {
    nodes: Vec<Node>,
    /// The numbers of its sequences.
    values: Vec<f32>,
}

/// What `eval` needs besides the position.
pub struct Ctx<'a> {
    /// Cycles per second at the song's tempo, for `lfo`'s hertz.
    pub cps: f64,
    /// Seconds since the last evaluation, for `lag`.
    pub dt: f32,
    /// The song's `lag` slots, NaN until a lag first runs.
    pub state: &'a mut [f32],
    /// The voice a per-voice signal is evaluated for (ADR-0023); `None` per synth.
    pub voice: Option<Voice>,
}

/// One voice of a pool, for `env` and lists.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Voice {
    /// Its slot in the pool: list number `slot` round the list.
    pub slot: usize,
    /// Seconds since its note began, and since its key was let go.
    pub on: f32,
    pub off: Option<f32>,
    /// The synth's amplifier ADSR in seconds (sustain a level), for `env(adsr)`.
    pub adsr: [f32; 4],
}

impl Voice {
    /// The envelope `times` at this voice's moment, from 0 to 1: a straight
    /// attack, then decay and release that fall 60 dB over their times. It is
    /// a function of the times since the note, so it needs no state.
    fn env(&self, times: [f32; 4]) -> f64 {
        let [a, d, s, r] = times.map(f64::from);
        let s = s.clamp(0.0, 1.0);
        let gated = |t: f64| {
            if t < a {
                t / a
            } else if d > 0.0 {
                s + (1.0 - s) * (-FALL * (t - a) / d).exp()
            } else {
                s
            }
        };
        let on = f64::from(self.on.max(0.0));
        match self.off {
            None => gated(on),
            Some(off) => {
                let off = f64::from(off.max(0.0));
                let held = gated((on - off).max(0.0));
                if r > 0.0 {
                    held * (-FALL * off / r).exp()
                } else {
                    0.0
                }
            }
        }
    }
}

impl Signal {
    /// Parse `text`. `nodes` counts the nodes already in the song, so the
    /// limit holds for all of its signals and each `lag` gets its own slot.
    /// An error's column counts in chars from 1.
    pub fn parse(text: &str, nodes: &mut usize) -> Result<Signal, (usize, &'static str)> {
        let toks = lex(text, true)?;
        let mut p = Parser {
            toks: &toks,
            at: 0,
            end: text.chars().count() + 1,
            nodes: Vec::new(),
            values: Vec::new(),
            base: *nodes,
            depth: 0,
        };
        p.sum()?;
        if let Some(t) = toks.get(p.at) {
            return Err((t.col, "unexpected text"));
        }
        *nodes += p.nodes.len();
        Ok(Signal {
            nodes: p.nodes,
            values: p.values,
        })
    }

    /// Whether this signal has a value per voice: it uses `env` or a list.
    pub fn per_voice(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| matches!(n, Node::Env(_) | Node::List { .. } | Node::LfoList { .. }))
    }

    /// Whether it uses `.lag`, whose state is one for the song.
    pub fn lags(&self) -> bool {
        self.nodes.iter().any(|n| matches!(n, Node::Lag { .. }))
    }

    /// The value at `t` cycles. Never allocates.
    pub fn eval(&self, t: f64, ctx: &mut Ctx<'_>) -> f32 {
        match self.nodes.len().checked_sub(1) {
            Some(root) => self.at(root, t, ctx) as f32,
            None => 0.0,
        }
    }

    fn at(&self, i: usize, t: f64, ctx: &mut Ctx<'_>) -> f64 {
        let Some(node) = self.nodes.get(i) else {
            return 0.0;
        };
        match *node {
            Node::Num(v) => f64::from(v),
            Node::Wave(w) => w.at(t),
            Node::Lfo(rate, w) => w.at(t / ctx.cps * f64::from(rate)),
            Node::LfoList { from, len, wave } => {
                let rate = self.listed(from, len, ctx);
                wave.at(t / ctx.cps * rate)
            }
            Node::List { from, len } => self.listed(from, len, ctx),
            Node::Env(times) => ctx.voice.map_or(0.0, |v| v.env(times.unwrap_or(v.adsr))),
            Node::Rand => unit(hash((t * RAND_PER_CYCLE).floor())),
            Node::Perlin => {
                let (k, f) = (t.floor(), t - t.floor());
                let s = f * f * (3.0 - 2.0 * f);
                let (a, b) = (unit(hash(k)), unit(hash(k + 1.0)));
                a + (b - a) * s
            }
            Node::Neg(a) => -self.at(a, t, ctx),
            Node::Bin(op, a, b) => {
                let (x, y) = (self.at(a, t, ctx), self.at(b, t, ctx));
                match op {
                    Op::Add => x + y,
                    Op::Sub => x - y,
                    Op::Mul => x * y,
                    Op::Div if y == 0.0 => 0.0,
                    Op::Div => x / y,
                }
            }
            Node::Range { of, lo, hi, exp } => {
                let u = self.at(of, t, ctx);
                let (lo, hi) = (f64::from(lo), f64::from(hi));
                if exp {
                    lo * (hi / lo).powf(u)
                } else {
                    lo + (hi - lo) * u
                }
            }
            Node::Slow(a, n) => self.at(a, t / f64::from(n), ctx),
            Node::Fast(a, n) => self.at(a, t * f64::from(n), ctx),
            Node::Segment(a, n) => {
                let n = f64::from(n);
                self.at(a, (t * n).floor() / n, ctx)
            }
            Node::Seq {
                from,
                len,
                per_cycle,
            } => {
                let k = if per_cycle {
                    t.floor().rem_euclid(len as f64)
                } else {
                    ((t - t.floor()) * len as f64).floor()
                };
                let k = (k as usize).min(len.saturating_sub(1));
                self.values.get(from + k).copied().map_or(0.0, f64::from)
            }
            Node::Lag { of, secs, slot } => {
                let x = self.at(of, t, ctx);
                let Some(s) = ctx.state.get_mut(slot) else {
                    return x;
                };
                if s.is_nan() {
                    *s = x as f32;
                } else {
                    *s += (x as f32 - *s) * (1.0 - (-ctx.dt / secs).exp());
                }
                f64::from(*s)
            }
        }
    }

    /// Number `slot` round the list of `len` from `from`; the first per synth.
    fn listed(&self, from: usize, len: usize, ctx: &Ctx<'_>) -> f64 {
        let k = ctx.voice.map_or(0, |v| v.slot) % len.max(1);
        self.values.get(from + k).copied().map_or(0.0, f64::from)
    }

    fn list(&self, out: &mut String, from: usize, len: usize) -> fmt::Result {
        out.push('[');
        for (k, v) in self.values.iter().skip(from).take(len).enumerate() {
            if k > 0 {
                out.push_str(", ");
            }
            write!(out, "{v}")?;
        }
        out.push(']');
        Ok(())
    }

    fn write(&self, out: &mut String, i: usize, prec: u8) -> fmt::Result {
        let Some(node) = self.nodes.get(i) else {
            return Ok(());
        };
        // Methods bind tightest: a sum or a negation they apply to is bracketed.
        let method = |out: &mut String, a: usize, name: &str, args: &[f32]| {
            self.write(out, a, 4)?;
            write!(out, ".{name}(")?;
            for (k, v) in args.iter().enumerate() {
                if k > 0 {
                    out.push_str(", ");
                }
                write!(out, "{v}")?;
            }
            out.push(')');
            Ok(())
        };
        match *node {
            Node::Num(v) if v < 0.0 && prec > 3 => write!(out, "({v})"),
            Node::Num(v) => write!(out, "{v}"),
            Node::Wave(w) => out.write_str(w.name()),
            Node::Lfo(rate, Wave::Sine) => write!(out, "lfo({rate})"),
            Node::Lfo(rate, w) => write!(out, "lfo({rate}, {})", w.name()),
            Node::LfoList { from, len, wave } => {
                out.push_str("lfo(");
                self.list(out, from, len)?;
                if wave != Wave::Sine {
                    write!(out, ", {}", wave.name())?;
                }
                out.push(')');
                Ok(())
            }
            Node::List { from, len } => self.list(out, from, len),
            Node::Env(None) => out.write_str("env(adsr)"),
            Node::Env(Some(t)) if t == PERC => out.write_str("env(perc)"),
            Node::Env(Some([a, d, s, r])) => write!(out, "env({a}, {d}, {s}, {r})"),
            Node::Rand => out.write_str("rand"),
            Node::Perlin => out.write_str("perlin"),
            Node::Neg(a) => {
                if prec > 3 {
                    out.push('(');
                }
                out.push('-');
                self.write(out, a, 3)?;
                if prec > 3 {
                    out.push(')');
                }
                Ok(())
            }
            Node::Bin(op, a, b) => {
                let p = op.prec();
                if p < prec {
                    out.push('(');
                }
                self.write(out, a, p)?;
                write!(out, " {} ", op.char())?;
                // Left to right: `a - (b - c)` keeps its brackets.
                self.write(out, b, p + 1)?;
                if p < prec {
                    out.push(')');
                }
                Ok(())
            }
            Node::Range { of, lo, hi, exp } => {
                method(out, of, if exp { "exprange" } else { "range" }, &[lo, hi])
            }
            Node::Slow(a, n) => method(out, a, "slow", &[n]),
            Node::Fast(a, n) => method(out, a, "fast", &[n]),
            Node::Segment(a, n) => method(out, a, "segment", &[n]),
            Node::Lag { of, secs, .. } => method(out, of, "lag", &[secs]),
            Node::Seq {
                from,
                len,
                per_cycle,
            } => {
                let (open, close) = if per_cycle {
                    ("\"<", ">\"")
                } else {
                    ("\"", "\"")
                };
                out.push_str(open);
                for (k, v) in self.values.iter().skip(from).take(len).enumerate() {
                    if k > 0 {
                        out.push(' ');
                    }
                    write!(out, "{v}")?;
                }
                out.push_str(close);
                Ok(())
            }
        }
    }
}

impl fmt::Display for Signal {
    /// The canonical text: spaces around operators, brackets only where needed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        if let Some(root) = self.nodes.len().checked_sub(1) {
            self.write(&mut out, root, 0)?;
        }
        f.write_str(&out)
    }
}

/// A seeded hash of a whole number, as the generators use (ADR-0005).
fn hash(k: f64) -> u32 {
    let k = k as i64;
    crate::algo::mix(0x5167_A15E ^ (k >> 32) as u32, k as u32)
}

/// 0..1 from a hash.
fn unit(h: u32) -> f64 {
    f64::from(h) / (f64::from(u32::MAX) + 1.0)
}

/// A word of an expression; shared with the modular voice's parser.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tok<'a> {
    Num(f32),
    Word(&'a str),
    /// `( ) + - * / . ,`, `|` standing for `|>` and, in a voice, `[ ]`
    /// (ADR-0020).
    Punct(char),
    /// The text between double quotes, and the column after the first.
    Quote(&'a str, usize),
}

pub(crate) struct Token<'a> {
    pub(crate) col: usize,
    pub(crate) tok: Tok<'a>,
}

/// The words of `text`; `lists` reads `[` and `]`, which only a voice has
/// (ADR-0021: a list gives each voice its own value).
pub(crate) fn lex(text: &str, lists: bool) -> Result<Vec<Token<'_>>, (usize, &'static str)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&(byte, c)) = chars.get(i) {
        let col = i + 1;
        let run = |from: usize, ok: &dyn Fn(char) -> bool| {
            let mut j = from;
            while chars.get(j).is_some_and(|&(_, c)| ok(c)) {
                j += 1;
            }
            j
        };
        let byte_at = |j: usize| chars.get(j).map_or(text.len(), |&(b, _)| b);
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            // 12, 0.5, 2e3: a dot is a decimal point only before a digit, so
            // `2.slow` would not be read as a number.
            let mut j = run(i, &|c| c.is_ascii_digit());
            if chars.get(j).is_some_and(|&(_, c)| c == '.')
                && chars.get(j + 1).is_some_and(|&(_, c)| c.is_ascii_digit())
            {
                j = run(j + 1, &|c| c.is_ascii_digit());
            }
            if chars.get(j).is_some_and(|&(_, c)| c == 'e' || c == 'E') {
                let k = if chars.get(j + 1).is_some_and(|&(_, c)| c == '-' || c == '+') {
                    j + 2
                } else {
                    j + 1
                };
                if chars.get(k).is_some_and(|&(_, c)| c.is_ascii_digit()) {
                    j = run(k, &|c| c.is_ascii_digit());
                }
            }
            let v = text
                .get(byte..byte_at(j))
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .ok_or((col, "a number is too large"))?;
            out.push(Token {
                col,
                tok: Tok::Num(v),
            });
            i = j;
        } else if c.is_ascii_alphabetic() {
            let j = run(i, &|c| c.is_ascii_alphanumeric() || c == '_');
            out.push(Token {
                col,
                tok: Tok::Word(text.get(byte..byte_at(j)).unwrap_or("")),
            });
            i = j;
        } else if c == '|' && chars.get(i + 1).is_some_and(|&(_, c)| c == '>') {
            out.push(Token {
                col,
                tok: Tok::Punct('|'),
            });
            i += 2;
        } else if "()+-*/.,".contains(c) {
            out.push(Token {
                col,
                tok: Tok::Punct(c),
            });
            i += 1;
        } else if c == '"' {
            let j = run(i + 1, &|c| c != '"');
            if chars.get(j).is_none() {
                return Err((col, "this \" is not closed"));
            }
            out.push(Token {
                col,
                tok: Tok::Quote(text.get(byte_at(i + 1)..byte_at(j)).unwrap_or(""), col + 1),
            });
            i = j + 1;
        } else if lists && (c == '[' || c == ']') {
            out.push(Token {
                col,
                tok: Tok::Punct(c),
            });
            i += 1;
        } else if c == '[' {
            return Err((col, "a list of channels is not supported yet"));
        } else {
            return Err((col, "a signal has no such character"));
        }
    }
    Ok(out)
}

struct Parser<'a, 'b> {
    toks: &'b [Token<'a>],
    at: usize,
    /// The column just past the text, for errors at its end.
    end: usize,
    nodes: Vec<Node>,
    values: Vec<f32>,
    /// Nodes the song had before this signal.
    base: usize,
    depth: usize,
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

    fn push(&mut self, n: Node) -> Res<usize> {
        if self.base + self.nodes.len() >= MAX_NODES {
            return Err((self.col(), "a song's signals have at most 256 nodes"));
        }
        self.nodes.push(n);
        Ok(self.nodes.len() - 1)
    }

    fn deeper(&mut self) -> Res<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err((self.col(), "a signal nests at most 32 deep"));
        }
        Ok(())
    }

    fn sum(&mut self) -> Res<usize> {
        let mut a = self.product()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Punct('+')) => Op::Add,
                Some(Tok::Punct('-')) => Op::Sub,
                _ => return Ok(a),
            };
            self.at += 1;
            let b = self.product()?;
            a = self.push(Node::Bin(op, a, b))?;
        }
    }

    fn product(&mut self) -> Res<usize> {
        let mut a = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Punct('*')) => Op::Mul,
                Some(Tok::Punct('/')) => Op::Div,
                _ => return Ok(a),
            };
            self.at += 1;
            let b = self.unary()?;
            a = self.push(Node::Bin(op, a, b))?;
        }
    }

    fn unary(&mut self) -> Res<usize> {
        if self.eat('-') {
            self.deeper()?;
            let a = self.unary()?;
            self.depth -= 1;
            // A minus on a number is part of it, so `-(2)` prints as `-2`.
            if let Some(Node::Num(v)) = self.nodes.get_mut(a) {
                *v = -*v;
                return Ok(a);
            }
            return self.push(Node::Neg(a));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Res<usize> {
        let mut a = self.primary()?;
        while self.eat('.') {
            let col = self.col();
            let Some(Tok::Word(name)) = self.peek() else {
                return Err((col, "a method name goes here, e.g. .range(0, 1)"));
            };
            self.at += 1;
            if !METHODS.contains(&name) {
                return Err((col, "no such method: range exprange slow fast segment lag"));
            }
            let args = self.numbers()?;
            let one = || match args.as_slice() {
                [n] if *n > 0.0 => Ok(*n),
                [_] => Err((col, "this takes a number above 0")),
                _ => Err((col, "this takes one number")),
            };
            let node = match name {
                "range" | "exprange" => {
                    let [lo, hi] = args.as_slice() else {
                        return Err((col, "this takes two numbers: low, high"));
                    };
                    let exp = name == "exprange";
                    if exp && lo * hi <= 0.0 {
                        return Err((col, "exprange needs two numbers of one sign, not 0"));
                    }
                    Node::Range {
                        of: a,
                        lo: *lo,
                        hi: *hi,
                        exp,
                    }
                }
                "slow" => Node::Slow(a, one()?),
                "fast" => Node::Fast(a, one()?),
                "segment" => Node::Segment(a, one()?),
                "lag" => Node::Lag {
                    of: a,
                    secs: one()?,
                    slot: self.base + self.nodes.len(),
                },
                _ => return Err((col, "no such method: range exprange slow fast segment lag")),
            };
            a = self.push(node)?;
        }
        Ok(a)
    }

    /// `(n, n, …)`: the numbers of a call, maybe negative.
    fn numbers(&mut self) -> Res<Vec<f32>> {
        self.expect('(', "( goes here")?;
        let mut out = Vec::new();
        if self.eat(')') {
            return Ok(out);
        }
        loop {
            let neg = self.eat('-');
            let Some(Tok::Num(v)) = self.peek() else {
                return Err((self.col(), "a number goes here"));
            };
            self.at += 1;
            out.push(if neg { -v } else { v });
            if self.eat(')') {
                return Ok(out);
            }
            self.expect(',', ", or ) goes here")?;
        }
    }

    fn primary(&mut self) -> Res<usize> {
        let col = self.col();
        match self.peek() {
            Some(Tok::Num(v)) => {
                self.at += 1;
                self.push(Node::Num(v))
            }
            Some(Tok::Punct('(')) => {
                self.at += 1;
                self.deeper()?;
                let a = self.sum()?;
                self.expect(')', ") goes here")?;
                self.depth -= 1;
                Ok(a)
            }
            Some(Tok::Quote(q, at)) => {
                self.at += 1;
                self.seq(q, at)
            }
            Some(Tok::Word(w)) => {
                self.at += 1;
                if !SOURCES.contains(&w) {
                    return Err((
                        col,
                        "a signal is a number, sine saw tri square rand perlin, lfo(…), env(…) or [a, b]",
                    ));
                }
                if let Some(wave) = Wave::named(w) {
                    return self.push(Node::Wave(wave));
                }
                match w {
                    "rand" => self.push(Node::Rand),
                    "perlin" => self.push(Node::Perlin),
                    "lfo" => self.lfo(),
                    "env" => self.env(),
                    _ => Err((
                        col,
                        "a signal is a number, sine saw tri square rand perlin, lfo(…), env(…) or [a, b]",
                    )),
                }
            }
            Some(Tok::Punct('[')) => {
                let (from, len) = self.list()?;
                self.push(Node::List { from, len })
            }
            _ => Err((col, "a signal goes here, e.g. sine.range(300, 3000)")),
        }
    }

    /// `"<a b c>"` or `"a b c"`: numbers in mini-notation; `at` is the
    /// column of the text's first char.
    fn seq(&mut self, text: &str, at: usize) -> Res<usize> {
        let inner = text.trim();
        let (per_cycle, inner) = match inner.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
            Some(t) => (true, t),
            None => (false, inner),
        };
        if inner.contains(['<', '>']) {
            return Err((at - 1, "a sequence holds numbers, e.g. \"<300 800>\""));
        }
        let from = self.values.len();
        let mut start: Option<(usize, usize)> = None;
        // Each number and its column; a space after the last ends it.
        for (col, (b, c)) in text.char_indices().chain([(text.len(), ' ')]).enumerate() {
            match (c.is_whitespace() || c == '<' || c == '>', start) {
                (false, None) => start = Some((b, col)),
                (true, Some((s, k))) => {
                    let v = text
                        .get(s..b)
                        .and_then(|w| w.parse::<f32>().ok())
                        .filter(|v| v.is_finite())
                        .ok_or((at + k, "a sequence holds numbers, e.g. \"<300 800>\""))?;
                    if self.values.len() - from >= MAX_SEQ {
                        return Err((at + k, "a sequence has at most 64 numbers"));
                    }
                    self.values.push(v);
                    start = None;
                }
                _ => {}
            }
        }
        let len = self.values.len() - from;
        if len == 0 {
            return Err((at - 1, "a sequence holds numbers, e.g. \"<300 800>\""));
        }
        self.push(Node::Seq {
            from,
            len,
            per_cycle,
        })
    }

    /// `[a, b, …]`: numbers, maybe negative, into the values; the `[` is next.
    fn list(&mut self) -> Res<(usize, usize)> {
        self.expect('[', "[ goes here")?;
        let from = self.values.len();
        loop {
            let col = self.col();
            let neg = self.eat('-');
            let Some(Tok::Num(v)) = self.peek() else {
                return Err((self.col(), "a number goes here"));
            };
            self.at += 1;
            if self.values.len() - from >= MAX_LIST {
                return Err((col, "a list has at most 16 numbers"));
            }
            self.values.push(if neg { -v } else { v });
            if self.eat(']') {
                return Ok((from, self.values.len() - from));
            }
            self.expect(',', ", or ] goes here")?;
        }
    }

    /// `env(adsr)`, `env(perc)` or `env(a, d, s, r)` in seconds.
    fn env(&mut self) -> Res<usize> {
        self.expect('(', "( goes here: env(adsr), env(perc) or env(a, d, s, r)")?;
        let col = self.col();
        let times = match self.peek() {
            Some(Tok::Word("adsr")) => {
                self.at += 1;
                self.expect(')', ") goes here")?;
                None
            }
            Some(Tok::Word("perc")) => {
                self.at += 1;
                self.expect(')', ") goes here")?;
                Some(PERC)
            }
            _ => {
                self.at -= 1;
                let args = self.numbers()?;
                let [a, d, s, r] = args.as_slice() else {
                    return Err((col, "an envelope is adsr, perc or four times: a, d, s, r"));
                };
                if [a, d, r].iter().any(|t| **t < 0.0) || !(0.0..=1.0).contains(s) {
                    return Err((col, "times are 0 or more, the sustain from 0 to 1"));
                }
                Some([*a, *d, *s, *r])
            }
        };
        self.push(Node::Env(times))
    }

    /// `lfo(rate)`, `lfo(rate, shape)`, or a rate per voice, `lfo([a, b])`.
    fn lfo(&mut self) -> Res<usize> {
        self.expect('(', "( goes here: lfo(rate, shape)")?;
        let list = if self.peek() == Some(Tok::Punct('[')) {
            let col = self.col();
            let (from, len) = self.list()?;
            if self.values.iter().skip(from).take(len).any(|r| *r <= 0.0) {
                return Err((col, "a rate is above 0"));
            }
            Some((from, len))
        } else {
            None
        };
        let rate = match list {
            Some(_) => 0.0,
            None => {
                let Some(Tok::Num(rate)) = self.peek() else {
                    return Err((self.col(), "a rate in hertz goes here"));
                };
                if rate <= 0.0 {
                    return Err((self.col(), "a rate is above 0"));
                }
                self.at += 1;
                rate
            }
        };
        let wave = if self.eat(',') {
            let c = self.col();
            let wave = match self.peek() {
                Some(Tok::Word(w)) => Wave::named(w),
                _ => None,
            }
            .ok_or((c, "a shape is sine, saw, tri or square"))?;
            self.at += 1;
            wave
        } else {
            Wave::Sine
        };
        self.expect(')', ") goes here")?;
        match list {
            Some((from, len)) => self.push(Node::LfoList { from, len, wave }),
            None => self.push(Node::Lfo(rate, wave)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(text: &str) -> Signal {
        let mut n = 0;
        Signal::parse(text, &mut n).unwrap_or_else(|e| panic!("{text}: {e:?}"))
    }

    fn val(s: &Signal, t: f64) -> f32 {
        let mut state = [f32::NAN; MAX_NODES];
        s.eval(
            t,
            &mut Ctx {
                cps: 0.5,
                dt: 0.01,
                state: &mut state,
                voice: None,
            },
        )
    }

    /// The value for one voice: slot `slot`, `on` seconds into its note,
    /// let go `off` seconds ago.
    fn voice_val(s: &Signal, slot: usize, on: f32, off: Option<f32>) -> f32 {
        let mut state = [f32::NAN; MAX_NODES];
        s.eval(
            0.0,
            &mut Ctx {
                cps: 0.5,
                dt: 0.01,
                state: &mut state,
                voice: Some(Voice {
                    slot,
                    on,
                    off,
                    adsr: [0.01, 0.1, 0.5, 0.2],
                }),
            },
        )
    }

    /// ADR-0023: `env` and lists print back, make a signal per voice, and
    /// give each voice its own value.
    #[test]
    fn env_and_lists_are_per_voice() {
        for text in [
            "env(perc)",
            "env(adsr).range(200, 4000)",
            "env(0.01, 0.2, 0.5, 0.3)",
            "[1, -2, 3]",
            "lfo([1, 3])",
            "lfo([1, 3], saw).exprange(200, 4000)",
        ] {
            let s = sig(text);
            assert_eq!(s.to_string(), text);
            assert!(s.per_voice(), "{text}");
        }
        assert!(!sig("lfo(1) + sine").per_voice());
        let list = sig("[10, 20, 30]");
        let by_slot: Vec<f32> = (0..4).map(|i| voice_val(&list, i, 0.0, None)).collect();
        assert_eq!(
            by_slot,
            vec![10.0, 20.0, 30.0, 10.0],
            "round the list by slot"
        );
        assert_eq!(val(&list, 0.0), 10.0, "per synth, the first");
        // The envelope: up through the attack, down to the sustain, released to nothing.
        let adsr = sig("env(adsr)");
        assert!(close(voice_val(&adsr, 0, 0.005, None), 0.5));
        assert!(close(voice_val(&adsr, 0, 0.01, None), 1.0));
        assert!(close(voice_val(&adsr, 0, 2.0, None), 0.5));
        assert!(
            voice_val(&adsr, 0, 2.2, Some(0.2)) < 0.001,
            "released over its time"
        );
        let perc = sig("env(perc)");
        assert!(voice_val(&perc, 0, 0.002, None) > 0.99);
        assert!(
            voice_val(&perc, 0, 0.3, None) < 0.002,
            "a percussive one falls held"
        );
        assert_eq!(val(&perc, 0.0), 0.0, "no voice, no envelope");
        // lfo([1, 3]): each voice at its own rate.
        let lfo = sig("lfo([1, 3])");
        let mut state = [f32::NAN; MAX_NODES];
        let mut at = |slot| sig_at(&lfo, 0.125, slot, &mut state);
        assert!((at(0) - at(1)).abs() > 0.1);
    }

    fn sig_at(s: &Signal, t: f64, slot: usize, state: &mut [f32]) -> f32 {
        s.eval(
            t,
            &mut Ctx {
                cps: 0.5,
                dt: 0.01,
                state,
                voice: Some(Voice {
                    slot,
                    ..Voice::default()
                }),
            },
        )
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3 * b.abs().max(1.0)
    }

    #[test]
    fn sources_run_from_0_to_1_once_a_cycle() {
        let (sine, saw, tri, sq) = (sig("sine"), sig("saw"), sig("tri"), sig("square"));
        assert!(close(val(&sine, 0.0), 0.5) && close(val(&sine, 0.25), 1.0));
        assert!(close(val(&sine, 0.75), 0.0) && close(val(&sine, 1.25), 1.0));
        assert!(close(val(&saw, 0.25), 0.25) && close(val(&saw, 1.5), 0.5));
        assert!(close(val(&tri, 0.5), 1.0) && close(val(&tri, 0.25), 0.5));
        assert_eq!((val(&sq, 0.2), val(&sq, 0.7)), (0.0, 1.0));
        // Two seconds a cycle at 0.5 cycles a second: lfo(1) goes round twice.
        let lfo = sig("lfo(1, saw)");
        assert!(close(val(&lfo, 0.25), 0.5) && close(val(&lfo, 0.5), 0.0));
    }

    #[test]
    fn range_slow_fast_and_operators_shape_a_signal() {
        let s = sig("saw.range(300, 3000)");
        assert!(close(val(&s, 0.0), 300.0) && close(val(&s, 0.5), 1650.0));
        let e = sig("saw.exprange(100, 10000)");
        assert!(close(val(&e, 0.5), 1000.0));
        assert!(close(val(&sig("saw.slow(4)"), 1.0), 0.25));
        assert!(close(val(&sig("saw.fast(2)"), 0.25), 0.5));
        assert!(close(val(&sig("saw.segment(4)"), 0.3), 0.25));
        assert!(close(val(&sig("1 + 2 * 3"), 0.0), 7.0));
        assert!(close(val(&sig("(1 + 2) * 3"), 0.0), 9.0));
        assert!(close(val(&sig("10 - 4 - 3"), 0.0), 3.0));
        assert!(close(val(&sig("-saw.range(1, 2)"), 0.0), -1.0));
        assert_eq!(val(&sig("1 / 0"), 0.0), 0.0);
        assert!(close(val(&sig("2e3"), 0.0), 2000.0));
    }

    #[test]
    fn a_sequence_steps_per_cycle_or_within_one() {
        let alt = sig("\"<300 800 1200>\"");
        let vals: Vec<f32> = [0.0, 0.9, 1.5, 2.2, 3.0]
            .iter()
            .map(|t| val(&alt, *t))
            .collect();
        assert_eq!(vals, vec![300.0, 300.0, 800.0, 1200.0, 300.0]);
        let fast = sig("\"0 0.5 1 -0.5\"");
        let vals: Vec<f32> = [0.0, 0.3, 0.5, 0.99, 1.1]
            .iter()
            .map(|t| val(&fast, *t))
            .collect();
        assert_eq!(vals, vec![0.0, 0.5, 1.0, -0.5, 0.0]);
        assert!(close(val(&sig("\"<1 2>\" * 100"), 1.0), 200.0));
    }

    #[test]
    fn randomness_is_seeded_and_bounded() {
        let (r, p) = (sig("rand"), sig("perlin"));
        let mut distinct = 0;
        for k in 0..64 {
            let t = f64::from(k) / 16.0;
            let (a, b) = (val(&r, t), val(&p, t));
            assert!((0.0..1.0).contains(&a) && (0.0..=1.0).contains(&b));
            assert_eq!(a, val(&r, t + 0.01), "held for a sixteenth");
            if a != val(&r, t + 1.0 / 16.0) {
                distinct += 1;
            }
        }
        assert!(distinct > 56);
        // Perlin passes through its value at each whole cycle, smoothly.
        assert!((val(&p, 2.0) - val(&p, 1.999)).abs() < 1e-3);
    }

    #[test]
    fn lag_follows_its_input_and_keeps_its_state() {
        let mut nodes = 0;
        let s = Signal::parse("square.lag(0.1)", &mut nodes).expect("parses");
        let mut state = [f32::NAN; MAX_NODES];
        let mut ctx = Ctx {
            cps: 0.5,
            dt: 0.01,
            state: &mut state,
            voice: None,
        };
        assert_eq!(s.eval(0.0, &mut ctx), 0.0);
        let first = s.eval(0.6, &mut ctx);
        assert!(first > 0.05 && first < 0.15, "{first}");
        let mut v = first;
        for _ in 0..200 {
            v = s.eval(0.6, &mut ctx);
        }
        assert!(close(v, 1.0));
    }

    #[test]
    fn printing_is_canonical_and_parses_back() {
        for (text, canon) in [
            (
                "sine.range(300,3000).slow(4)",
                "sine.range(300, 3000).slow(4)",
            ),
            (
                "lfo(0.2,saw).range(-2,2)+perlin",
                "lfo(0.2, saw).range(-2, 2) + perlin",
            ),
            ("lfo( 1 , sine )", "lfo(1)"),
            ("((1+2))*3", "(1 + 2) * 3"),
            ("1+(2*3)", "1 + 2 * 3"),
            ("1-(2-3)", "1 - (2 - 3)"),
            ("(1-2)-3", "1 - 2 - 3"),
            ("-saw", "-saw"),
            ("(-saw).range(1,2)", "(-saw).range(1, 2)"),
            ("-2 * saw", "-2 * saw"),
            ("saw * -2", "saw * -2"),
            ("-(2)", "-2"),
            ("(-2).slow(2)", "(-2).slow(2)"),
            ("-2.slow(2)", "-2.slow(2)"),
            ("(saw+1).lag(0.05)", "(saw + 1).lag(0.05)"),
            ("2e3", "2000"),
            ("\" < 300  800 > \"", "\"<300 800>\""),
            ("\"1 2\".slow(2)", "\"1 2\".slow(2)"),
        ] {
            let s = sig(text);
            assert_eq!(s.to_string(), canon, "{text}");
            assert_eq!(sig(canon), s, "{canon}");
        }
    }

    #[test]
    fn errors_say_where() {
        for (text, col, msg) in [
            ("", 1, "a signal goes here, e.g. sine.range(300, 3000)"),
            (
                "sine +",
                7,
                "a signal goes here, e.g. sine.range(300, 3000)",
            ),
            (
                "cosine",
                1,
                "a signal is a number, sine saw tri square rand perlin, lfo(…), env(…) or [a, b]",
            ),
            (
                "sine.wobble(1)",
                6,
                "no such method: range exprange slow fast segment lag",
            ),
            ("sine.range(1)", 6, "this takes two numbers: low, high"),
            (
                "sine.exprange(0, 10)",
                6,
                "exprange needs two numbers of one sign, not 0",
            ),
            ("sine.slow(0)", 6, "this takes a number above 0"),
            ("sine.segment(0)", 6, "this takes a number above 0"),
            ("lfo(0)", 5, "a rate is above 0"),
            ("lfo(1, pink)", 8, "a shape is sine, saw, tri or square"),
            ("lfo([1, 0])", 5, "a rate is above 0"),
            ("[1, x]", 5, "a number goes here"),
            ("[1 2]", 4, ", or ] goes here"),
            (
                "env(1, 2)",
                5,
                "an envelope is adsr, perc or four times: a, d, s, r",
            ),
            (
                "env(0, 0.1, 2, 0)",
                5,
                "times are 0 or more, the sustain from 0 to 1",
            ),
            ("(sine", 6, ") goes here"),
            ("sine sine", 6, "unexpected text"),
            ("sine $", 6, "a signal has no such character"),
            ("1e99", 1, "a number is too large"),
            ("\"1 2", 1, "this \" is not closed"),
            ("\"1 x\"", 4, "a sequence holds numbers, e.g. \"<300 800>\""),
            ("\"\"", 1, "a sequence holds numbers, e.g. \"<300 800>\""),
            (
                "\"<1 <2>>\"",
                1,
                "a sequence holds numbers, e.g. \"<300 800>\"",
            ),
        ] {
            let mut n = 0;
            assert_eq!(Signal::parse(text, &mut n), Err((col, msg)), "{text}");
        }
        let deep = format!("{}1{}", "(".repeat(40), ")".repeat(40));
        assert_eq!(
            Signal::parse(&deep, &mut 0).map_err(|e| e.1),
            Err("a signal nests at most 32 deep")
        );
        let mut n = MAX_NODES - 1;
        assert_eq!(
            Signal::parse("1 + 2", &mut n).map_err(|e| e.1),
            Err("a song's signals have at most 256 nodes")
        );
    }

    #[test]
    fn lag_slots_are_unique_across_a_song() {
        let mut n = 0;
        let a = Signal::parse("saw.lag(1)", &mut n).expect("a");
        let b = Signal::parse("saw.lag(1)", &mut n).expect("b");
        let slot = |s: &Signal| {
            s.nodes.iter().find_map(|n| match n {
                Node::Lag { slot, .. } => Some(*slot),
                _ => None,
            })
        };
        assert_eq!((slot(&a), slot(&b), n), (Some(1), Some(3), 4));
    }
}
