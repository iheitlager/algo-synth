//! SuperCollider SynthDefs for the Modular synth (ADR-0024).
//!
//! ```text
//! SynthDef(\acid, { |freq = 110, gate = 1|
//!     var sig = Saw.ar(freq);
//!     sig = RLPF.ar(sig, 800, 0.3);
//!     sig * EnvGen.kr(Env.adsr(0.01, 0.2, 0.7, 0.3), gate)
//! }).add;
//! ```
//!
//! As SuperCollider runs a SynthDef's function once to build its graph, so
//! does this module, when a synth is given its code (never in `render`): a
//! subset of sclang (numbers, symbols, arrays, functions and closures, `var`,
//! `value`, `dup` and `!`, `do`, `collect`, `sum`, `Array.fill`, `Mix.fill`,
//! indexing, `if` on numbers) builds the fixed [`Program`] the voice plays.
//! Binary operators run left to right with no precedence, as in sclang.
//!
//! Every number written as a UGen's argument, an envelope's time or a
//! control's default becomes a knob: a live `Param::Ctl1`… of the synth, at
//! most [`CTLS`]. The [`Patch`] keeps the code as written and where each
//! knob's number is in it, so the text it hands out shows the knobs' values.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::{
    MAX_DELAY, MAX_DELAYS, MAX_ENVS, MAX_FILTERS, MAX_MIX, MAX_OSCS, MAX_PHASES, NONE, Op, Program,
    Shape, Ugen,
};
use crate::mono::osc::Waveform;
use crate::params::CTLS;

/// Why a SynthDef does not build, and where: line and column from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeError {
    pub line: usize,
    pub col: usize,
    pub msg: &'static str,
}

/// A knob: one number of the code, live as `Param::Ctl1` + `ctl`.
#[derive(Clone, Debug, PartialEq)]
pub struct Knob {
    /// The module it is on, and the UGen input (or control) it sets.
    pub module: usize,
    pub name: String,
    pub ctl: usize,
    pub lo: f32,
    pub hi: f32,
    pub exp: bool,
    /// The number as written, and where it is in the code (bytes).
    pub default: f32,
    pub span: (usize, usize),
}

/// A UGen of the code as the panel draws it: its class and its knobs.
#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub name: String,
    pub line: usize,
    pub col: usize,
    pub knobs: Vec<usize>,
}

/// A built SynthDef: the program the voice plays, its knobs and modules,
/// and the code as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    pub program: Program,
    pub knobs: Vec<Knob>,
    pub modules: Vec<Module>,
    pub source: String,
}

impl Patch {
    /// The code with each knob's number replaced by `values[ctl]` where the
    /// knob was turned; the layout and comments stay.
    pub fn text(&self, values: &[f32]) -> String {
        let mut spans: Vec<(usize, usize, String)> = self
            .knobs
            .iter()
            .filter_map(|k| {
                let v = values.get(k.ctl).copied()?;
                ((v - k.default).abs() > 1e-6 * k.default.abs().max(1.0))
                    .then(|| (k.span.0, k.span.1, number(v)))
            })
            .collect();
        spans.sort_by_key(|s| std::cmp::Reverse(s.0));
        let mut out = self.source.clone();
        for (a, b, text) in spans {
            if out.is_char_boundary(a) && out.is_char_boundary(b) && a <= b && b <= out.len() {
                out.replace_range(a..b, &text);
            }
        }
        out
    }
}

/// A number as the code writes it: no trailing zeros, four places at most.
fn number(v: f32) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Build `source` into a patch.
pub fn compile(source: &str) -> Result<Patch, CodeError> {
    let toks = lex(source)?;
    let mut p = Parser {
        toks: &toks,
        at: 0,
        end: end_pos(source),
    };
    let body = p.seq(None)?;
    if let Some(t) = toks.get(p.at) {
        return Err(t.err("unexpected text"));
    }
    let mut b = Builder::new();
    // Where each literal is in the code; a negative one from its minus.
    b.spans = toks
        .iter()
        .enumerate()
        .map(|(k, t)| match (&t.t, toks.get(k + 1)) {
            (T::P("-"), Some(n)) => (t.start, n.end),
            _ => (t.start, t.end),
        })
        .collect();
    let top = Rc::new(Scope::default());
    let value = b.seq(&body, &top)?;
    let (def, at) = match value {
        V::Def(c) | V::Func(c) => (c, (1, 1)),
        _ => {
            return Err(CodeError {
                line: 1,
                col: 1,
                msg: "the code is a SynthDef(\\name, { … }) or a function { … }",
            });
        }
    };
    let out = b.build(&def, at)?;
    let root = b.output(&out, at)?;
    // The output is the last node.
    if usize::from(root) + 1 != usize::from(b.prog.len) {
        let zero = b.push(Ugen::Num(0.0), at)?;
        b.push(Ugen::Bin(Op::Add, root, zero), at)?;
    }
    Ok(Patch {
        program: b.prog,
        knobs: b.knobs,
        modules: b.modules,
        source: source.to_string(),
    })
}

// --- Lexing ------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum T {
    Num(f64),
    Name(String),
    Class(String),
    Sym(String),
    P(&'static str),
}

#[derive(Clone, Debug)]
struct Tok {
    t: T,
    start: usize,
    end: usize,
    line: usize,
    col: usize,
}

impl Tok {
    fn err(&self, msg: &'static str) -> CodeError {
        CodeError {
            line: self.line,
            col: self.col,
            msg,
        }
    }
}

fn end_pos(src: &str) -> (usize, usize) {
    let line = src.matches('\n').count() + 1;
    let col = src.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

const PUNCT2: [&str; 5] = ["==", "!=", "<=", ">=", "**"];
const PUNCT1: [&str; 20] = [
    "(", ")", "{", "}", "[", "]", ",", ";", ".", "|", "=", "+", "-", "*", "/", "%", "<", ">", "!",
    ":",
];

fn lex(src: &str) -> Result<Vec<Tok>, CodeError> {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let byte = |j: usize| chars.get(j).map_or(src.len(), |&(b, _)| b);
    let at = |j: usize| chars.get(j).map(|&(_, c)| c);
    let (mut i, mut line, mut col) = (0, 1, 1);
    let mut out = Vec::new();
    while let Some(c) = at(i) {
        let start = (i, line, col);
        let advance = |n: usize, i: &mut usize, line: &mut usize, col: &mut usize| {
            for _ in 0..n {
                if at(*i) == Some('\n') {
                    *line += 1;
                    *col = 1;
                } else {
                    *col += 1;
                }
                *i += 1;
            }
        };
        if c.is_whitespace() {
            advance(1, &mut i, &mut line, &mut col);
            continue;
        }
        if c == '/' && at(i + 1) == Some('/') {
            while at(i).is_some_and(|c| c != '\n') {
                advance(1, &mut i, &mut line, &mut col);
            }
            continue;
        }
        if c == '/' && at(i + 1) == Some('*') {
            advance(2, &mut i, &mut line, &mut col);
            while at(i).is_some() && !(at(i) == Some('*') && at(i + 1) == Some('/')) {
                advance(1, &mut i, &mut line, &mut col);
            }
            if at(i).is_none() {
                return Err(CodeError {
                    line: start.1,
                    col: start.2,
                    msg: "this comment is not closed",
                });
            }
            advance(2, &mut i, &mut line, &mut col);
            continue;
        }
        let t = if c.is_ascii_digit() {
            let mut j = i;
            while at(j).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
            }
            if at(j) == Some('.') && at(j + 1).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
                while at(j).is_some_and(|c| c.is_ascii_digit()) {
                    j += 1;
                }
            }
            if at(j).is_some_and(|c| c == 'e' || c == 'E') {
                let k = if at(j + 1).is_some_and(|c| c == '-' || c == '+') {
                    j + 2
                } else {
                    j + 1
                };
                if at(k).is_some_and(|c| c.is_ascii_digit()) {
                    j = k;
                    while at(j).is_some_and(|c| c.is_ascii_digit()) {
                        j += 1;
                    }
                }
            }
            let v = src
                .get(byte(i)..byte(j))
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .ok_or(CodeError {
                    line,
                    col,
                    msg: "a number is too large",
                })?;
            let n = j - i;
            advance(n, &mut i, &mut line, &mut col);
            T::Num(v)
        } else if c.is_alphabetic() || c == '_' || c == '\\' || c == '\'' {
            // `\sym`, `'sym'`, a name or a Class.
            let quoted = c == '\'';
            let sym = c == '\\' || quoted;
            let mut j = if sym { i + 1 } else { i };
            while at(j).is_some_and(|c| {
                if quoted {
                    c != '\''
                } else {
                    c.is_alphanumeric() || c == '_'
                }
            }) {
                j += 1;
            }
            let text = src
                .get(byte(if sym { i + 1 } else { i })..byte(j))
                .unwrap_or("")
                .to_string();
            if quoted {
                if at(j) != Some('\'') {
                    return Err(CodeError {
                        line,
                        col,
                        msg: "this symbol is not closed",
                    });
                }
                j += 1;
            }
            let n = j - i;
            advance(n, &mut i, &mut line, &mut col);
            if sym {
                T::Sym(text)
            } else if text.starts_with(char::is_uppercase) {
                T::Class(text)
            } else {
                T::Name(text)
            }
        } else if c == '"' {
            return Err(CodeError {
                line,
                col,
                msg: "strings are not part of a SynthDef here",
            });
        } else {
            let two: String = [c, at(i + 1).unwrap_or(' ')].iter().collect();
            if let Some(p) = PUNCT2.iter().find(|p| **p == two) {
                advance(2, &mut i, &mut line, &mut col);
                T::P(p)
            } else if let Some(p) = PUNCT1.iter().find(|p| p.starts_with(c)) {
                advance(1, &mut i, &mut line, &mut col);
                T::P(p)
            } else {
                return Err(CodeError {
                    line,
                    col,
                    msg: "this character is not part of sclang here",
                });
            }
        };
        out.push(Tok {
            t,
            start: byte(start.0),
            end: byte(i),
            line: start.1,
            col: start.2,
        });
    }
    Ok(out)
}

// --- Parsing -----------------------------------------------------------------

type Pos = (usize, usize);

#[derive(Debug)]
enum E {
    /// A number and, when written as a literal, its token (for its knob).
    Num(f64, Option<usize>),
    Name(String, Pos),
    Class(String),
    Sym(String),
    Arr(Vec<E>),
    Func(Rc<FuncDef>),
    Bin(&'static str, Box<E>, Box<E>, Pos),
    Neg(Box<E>, Pos),
    Call {
        recv: Box<E>,
        name: String,
        args: Vec<E>,
        kws: Vec<(String, E)>,
        pos: Pos,
    },
    Index(Box<E>, Box<E>, Pos),
    Assign(String, Box<E>),
    Var(Vec<(String, Option<E>)>),
    Lit(Lit),
}

#[derive(Debug, Clone, Copy)]
enum Lit {
    True,
    False,
    Nil,
}

#[derive(Debug)]
struct FuncDef {
    params: Vec<(String, Option<E>, Pos)>,
    body: Vec<E>,
}

struct Parser<'a> {
    toks: &'a [Tok],
    at: usize,
    end: Pos,
}

type R<T> = Result<T, CodeError>;
/// A call's positional and keyword arguments.
type Args = (Vec<E>, Vec<(String, E)>);

impl Parser<'_> {
    fn peek(&self) -> Option<&T> {
        self.toks.get(self.at).map(|t| &t.t)
    }

    fn pos(&self) -> Pos {
        self.toks.get(self.at).map_or(self.end, |t| (t.line, t.col))
    }

    fn err(&self, msg: &'static str) -> CodeError {
        let (line, col) = self.pos();
        CodeError { line, col, msg }
    }

    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), Some(T::P(q)) if *q == p)
    }

    fn eat(&mut self, p: &str) -> bool {
        let hit = self.is(p);
        if hit {
            self.at += 1;
        }
        hit
    }

    fn expect(&mut self, p: &str, msg: &'static str) -> R<()> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(self.err(msg))
        }
    }

    /// Statements separated by `;` up to `close` (or the end).
    fn seq(&mut self, close: Option<&str>) -> R<Vec<E>> {
        let mut out = Vec::new();
        loop {
            while self.eat(";") {}
            if self.peek().is_none() || close.is_some_and(|c| self.is(c)) {
                return Ok(out);
            }
            out.push(self.stmt()?);
            if !(self.eat(";") || self.peek().is_none() || close.is_some_and(|c| self.is(c))) {
                return Err(self.err("; goes here"));
            }
        }
    }

    fn stmt(&mut self) -> R<E> {
        if matches!(self.peek(), Some(T::Name(n)) if n == "var") {
            self.at += 1;
            let mut vars = Vec::new();
            loop {
                let Some(T::Name(n)) = self.peek().cloned() else {
                    return Err(self.err("a variable name goes here"));
                };
                self.at += 1;
                let init = if self.eat("=") {
                    Some(self.expr()?)
                } else {
                    None
                };
                vars.push((n, init));
                if !self.eat(",") {
                    return Ok(E::Var(vars));
                }
            }
        }
        self.expr()
    }

    fn expr(&mut self) -> R<E> {
        if let (Some(T::Name(n)), Some(T::P("="))) = (
            self.peek().cloned(),
            self.toks.get(self.at + 1).map(|t| &t.t),
        ) {
            let pos = self.pos();
            self.at += 2;
            let _ = pos;
            return Ok(E::Assign(n, Box::new(self.expr()?)));
        }
        let mut a = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(T::P(p))
                    if [
                        "+", "-", "*", "/", "%", "**", "<", ">", "<=", ">=", "==", "!=", "!",
                    ]
                    .contains(p) =>
                {
                    *p
                }
                _ => return Ok(a),
            };
            let pos = self.pos();
            self.at += 1;
            let b = self.unary()?;
            a = E::Bin(op, Box::new(a), Box::new(b), pos);
        }
    }

    fn unary(&mut self) -> R<E> {
        if self.is("-") {
            let pos = self.pos();
            let minus = self.at;
            self.at += 1;
            // `-3` is a literal of its own, with its sign in its span.
            if let Some(T::Num(v)) = self.peek().cloned() {
                if !matches!(self.toks.get(self.at + 1).map(|t| &t.t), Some(T::P("."))) {
                    self.at += 1;
                    return Ok(E::Num(-v, Some(minus)));
                }
            }
            return Ok(E::Neg(Box::new(self.unary()?), pos));
        }
        self.postfix()
    }

    fn args(&mut self) -> R<Args> {
        let (mut args, mut kws) = (Vec::new(), Vec::new());
        if self.eat(")") {
            return Ok((args, kws));
        }
        loop {
            if let (Some(T::Name(n)), Some(T::P(":"))) = (
                self.peek().cloned(),
                self.toks.get(self.at + 1).map(|t| &t.t),
            ) {
                self.at += 2;
                kws.push((n, self.expr()?));
            } else {
                args.push(self.expr()?);
            }
            if self.eat(")") {
                return Ok((args, kws));
            }
            self.expect(",", ", or ) goes here")?;
        }
    }

    /// Function literals written after a call: `if(c) { … } { … }`.
    fn trailing(&mut self, args: &mut Vec<E>) -> R<()> {
        while self.is("{") {
            self.at += 1;
            args.push(E::Func(self.func()?));
        }
        Ok(())
    }

    fn postfix(&mut self) -> R<E> {
        let start = self.pos();
        let mut a = self.primary()?;
        loop {
            if self.is(".") {
                self.at += 1;
                // `Class.ar(…)` is where its class is written.
                let pos = if matches!(a, E::Class(_)) {
                    start
                } else {
                    self.pos()
                };
                let Some(T::Name(name)) = self.peek().cloned() else {
                    return Err(self.err("a method name goes here"));
                };
                self.at += 1;
                let (mut args, kws) = if self.eat("(") {
                    self.args()?
                } else {
                    (Vec::new(), Vec::new())
                };
                self.trailing(&mut args)?;
                a = E::Call {
                    recv: Box::new(a),
                    name,
                    args,
                    kws,
                    pos,
                };
            } else if self.is("[") {
                let pos = self.pos();
                self.at += 1;
                let i = self.expr()?;
                self.expect("]", "] goes here")?;
                a = E::Index(Box::new(a), Box::new(i), pos);
            } else {
                return Ok(a);
            }
        }
    }

    fn primary(&mut self) -> R<E> {
        let pos = self.pos();
        let at = self.at;
        let Some(t) = self.peek().cloned() else {
            return Err(self.err("an expression goes here"));
        };
        self.at += 1;
        match t {
            T::Num(v) => Ok(E::Num(v, Some(at))),
            T::Sym(s) => Ok(E::Sym(s)),
            T::Name(n) => match n.as_str() {
                "true" => Ok(E::Lit(Lit::True)),
                "false" => Ok(E::Lit(Lit::False)),
                "nil" => Ok(E::Lit(Lit::Nil)),
                "pi" => Ok(E::Num(std::f64::consts::PI, None)),
                _ if self.is("(") => {
                    // `name(a, b)` is `a.name(b)`, as sclang reads it.
                    self.at += 1;
                    let (mut args, kws) = self.args()?;
                    self.trailing(&mut args)?;
                    if args.is_empty() {
                        return Err(CodeError {
                            line: pos.0,
                            col: pos.1,
                            msg: "a call like this needs a receiver",
                        });
                    }
                    let recv = args.remove(0);
                    Ok(E::Call {
                        recv: Box::new(recv),
                        name: n,
                        args,
                        kws,
                        pos,
                    })
                }
                _ => Ok(E::Name(n, pos)),
            },
            T::Class(c) => {
                if self.is("(") {
                    // `Class(…)` is `Class.new(…)`.
                    self.at += 1;
                    let (mut args, kws) = self.args()?;
                    self.trailing(&mut args)?;
                    return Ok(E::Call {
                        recv: Box::new(E::Class(c)),
                        name: "new".into(),
                        args,
                        kws,
                        pos,
                    });
                }
                Ok(E::Class(c))
            }
            T::P("(") => {
                let mut body = self.seq(Some(")"))?;
                self.expect(")", ") goes here")?;
                match body.len() {
                    1 => Ok(body.remove(0)),
                    _ => Err(CodeError {
                        line: pos.0,
                        col: pos.1,
                        msg: "brackets hold one expression",
                    }),
                }
            }
            T::P("[") => {
                let mut items = Vec::new();
                if !self.eat("]") {
                    loop {
                        items.push(self.expr()?);
                        if self.eat("]") {
                            break;
                        }
                        self.expect(",", ", or ] goes here")?;
                    }
                }
                Ok(E::Arr(items))
            }
            T::P("{") => Ok(E::Func(self.func()?)),
            _ => {
                self.at -= 1;
                Err(self.err("an expression goes here"))
            }
        }
    }

    /// A function after its `{`: `|args|` or `arg …;`, then statements, `}`.
    fn func(&mut self) -> R<Rc<FuncDef>> {
        let mut params = Vec::new();
        let bars = self.eat("|");
        let arg_kw = !bars && matches!(self.peek(), Some(T::Name(n)) if n == "arg");
        if arg_kw {
            self.at += 1;
        }
        if bars || arg_kw {
            loop {
                if bars && self.eat("|") {
                    break;
                }
                let pos = self.pos();
                let Some(T::Name(n)) = self.peek().cloned() else {
                    return Err(self.err("an argument name goes here"));
                };
                self.at += 1;
                let default = if self.eat("=") {
                    Some(self.unary()?)
                } else {
                    None
                };
                params.push((n, default, pos));
                if self.eat(",") {
                    continue;
                }
                if bars {
                    self.expect("|", "| goes here, after the arguments")?;
                } else {
                    self.expect(";", "; goes here, after the arguments")?;
                }
                break;
            }
        }
        let body = self.seq(Some("}"))?;
        self.expect("}", "} goes here")?;
        Ok(Rc::new(FuncDef { params, body }))
    }
}

// --- Building ----------------------------------------------------------------

/// A value at build time.
#[derive(Clone, Debug)]
enum V {
    /// A number and the token it was written as, if a literal.
    Num(f64, Option<usize>),
    /// A node of the graph; `uni` when it runs 0..1 (an envelope, the gate).
    Sig(u8, bool),
    Arr(Vec<V>),
    Func(Rc<Closure>),
    Def(Rc<Closure>),
    Sym(String),
    Bool(bool),
    Nil,
    Env(EnvSpec),
    Class(String),
}

#[derive(Debug)]
struct Closure {
    def: Rc<FuncDef>,
    scope: Rc<Scope>,
}

#[derive(Debug, Default)]
struct Scope {
    vars: RefCell<HashMap<String, V>>,
    parent: Option<Rc<Scope>>,
}

impl Scope {
    fn child(parent: &Rc<Scope>) -> Rc<Scope> {
        Rc::new(Scope {
            vars: RefCell::default(),
            parent: Some(parent.clone()),
        })
    }

    fn get(&self, name: &str) -> Option<V> {
        match self.vars.borrow().get(name) {
            Some(v) => Some(v.clone()),
            None => self.parent.as_ref().and_then(|p| p.get(name)),
        }
    }

    fn set(&self, name: &str, v: V) -> bool {
        if let Some(slot) = self.vars.borrow_mut().get_mut(name) {
            *slot = v;
            return true;
        }
        self.parent.as_ref().is_some_and(|p| p.set(name, v))
    }

    fn define(&self, name: &str, v: V) {
        self.vars.borrow_mut().insert(name.to_string(), v);
    }
}

/// An envelope described by `Env.adsr`, `Env.perc` or `Env.asr`, and where.
#[derive(Clone, Debug)]
struct EnvSpec {
    kind: EnvKind,
    args: Vec<V>,
    pos: Pos,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EnvKind {
    Adsr,
    Perc,
    Asr,
}

/// A knob's range and taper.
#[derive(Clone, Copy, Debug)]
struct Spec {
    lo: f32,
    hi: f32,
    exp: bool,
}

const FREQ: Spec = Spec {
    lo: 20.0,
    hi: 20_000.0,
    exp: true,
};
const RATE: Spec = Spec {
    lo: 0.01,
    hi: 100.0,
    exp: true,
};
const RQ: Spec = Spec {
    lo: 0.05,
    hi: 2.0,
    exp: true,
};
const UNIT: Spec = Spec {
    lo: 0.0,
    hi: 1.0,
    exp: false,
};
const MUL: Spec = Spec {
    lo: 0.0,
    hi: 2.0,
    exp: false,
};
const ADD: Spec = Spec {
    lo: -1.0,
    hi: 1.0,
    exp: false,
};
const TIME: Spec = Spec {
    lo: 0.001,
    hi: 10.0,
    exp: true,
};
const GAIN: Spec = Spec {
    lo: 0.0,
    hi: 4.0,
    exp: false,
};
const INDEX: Spec = Spec {
    lo: 0.0,
    hi: 20.0,
    exp: false,
};
const DELAY: Spec = Spec {
    lo: 0.0001,
    hi: 0.02,
    exp: true,
};
const DECAY: Spec = Spec {
    lo: 0.01,
    hi: 10.0,
    exp: true,
};
const PHASE: Spec = Spec {
    lo: 0.0,
    hi: std::f32::consts::TAU,
    exp: false,
};
const ANY: Spec = Spec {
    lo: -1.0,
    hi: 1.0,
    exp: false,
};

/// The range of a control by its name and default, as SuperCollider's
/// `ControlSpec` names guess them.
fn control_spec(name: &str, default: f32) -> Spec {
    let n = name.to_ascii_lowercase();
    if n.contains("freq") || n.contains("cutoff") || n == "ffreq" {
        FREQ
    } else if n == "rq" {
        RQ
    } else if n.contains("att")
        || n.contains("rel")
        || n.contains("dec")
        || n.contains("time")
        || n == "dur"
    {
        TIME
    } else if n.contains("amp")
        || n.contains("res")
        || n.contains("mix")
        || n == "level"
        || n == "width"
    {
        UNIT
    } else if n == "pan" {
        ANY
    } else {
        let top = (default.abs() * 2.0).max(1.0);
        Spec {
            lo: if default < 0.0 { -top } else { 0.0 },
            hi: top,
            exp: false,
        }
    }
}

/// How evaluation is bounded.
const MAX_STEPS: usize = 200_000;
const MAX_CALLS: usize = 64;
const MAX_COUNT: usize = 256;

struct Builder {
    prog: Program,
    knobs: Vec<Knob>,
    /// The knob of each literal token.
    knob_of: HashMap<usize, (usize, u8)>,
    modules: Vec<Module>,
    module_at: HashMap<Pos, usize>,
    /// Controls by name (`\freq.kr`, `|cutoff|`), built once.
    controls: HashMap<String, V>,
    /// The literal tokens, for their spans.
    spans: Vec<(usize, usize)>,
    steps: usize,
    calls: usize,
}

impl Builder {
    fn new() -> Builder {
        Builder {
            prog: Program {
                len: 0,
                counts: super::Counts::default(),
                ..Program::default()
            },
            knobs: Vec::new(),
            knob_of: HashMap::new(),
            modules: Vec::new(),
            module_at: HashMap::new(),
            controls: HashMap::new(),
            spans: Vec::new(),
            steps: 0,
            calls: 0,
        }
    }

    fn push(&mut self, u: Ugen, at: Pos) -> R<u8> {
        let len = usize::from(self.prog.len);
        let Some(slot) = self.prog.nodes.get_mut(len) else {
            return Err(CodeError {
                line: at.0,
                col: at.1,
                msg: "the voice needs more than 64 nodes",
            });
        };
        *slot = u;
        self.prog.len += 1;
        Ok(self.prog.len - 1)
    }

    fn slot(count: &mut u8, limit: usize, at: Pos, msg: &'static str) -> R<u8> {
        if usize::from(*count) >= limit {
            return Err(CodeError {
                line: at.0,
                col: at.1,
                msg,
            });
        }
        *count += 1;
        Ok(*count - 1)
    }

    fn module(&mut self, name: &str, at: Pos) -> usize {
        if let Some(m) = self.module_at.get(&at) {
            return *m;
        }
        self.modules.push(Module {
            name: name.to_string(),
            line: at.0,
            col: at.1,
            knobs: Vec::new(),
        });
        let m = self.modules.len() - 1;
        self.module_at.insert(at, m);
        m
    }

    /// The node for `v` as UGen input `name` of `module`: a literal number
    /// becomes a knob (one per literal, however often it is built).
    fn input(&mut self, v: &V, module: usize, name: &str, spec: Spec, at: Pos) -> R<u8> {
        match v {
            V::Num(x, Some(tok)) => self.knob(*tok, *x as f32, module, name, spec, at),
            V::Num(x, None) => self.push(Ugen::Num(*x as f32), at),
            V::Bool(b) => self.push(Ugen::Num(f32::from(u8::from(*b))), at),
            V::Sig(n, _) => Ok(*n),
            V::Arr(_) => Err(CodeError {
                line: at.0,
                col: at.1,
                msg: "an array here needs one channel",
            }),
            _ => Err(CodeError {
                line: at.0,
                col: at.1,
                msg: "a UGen input is a number or a signal",
            }),
        }
    }

    fn knob(
        &mut self,
        tok: usize,
        x: f32,
        module: usize,
        name: &str,
        spec: Spec,
        at: Pos,
    ) -> R<u8> {
        if let Some((_, node)) = self.knob_of.get(&tok) {
            return Ok(*node);
        }
        if self.knobs.len() >= CTLS {
            return self.push(Ugen::Num(x), at);
        }
        let (lo, hi) = (spec.lo.min(x), spec.hi.max(x));
        let exp = spec.exp && lo > 0.0;
        let ctl = self.knobs.len();
        let node = self.push(
            Ugen::Ctl {
                index: u8::try_from(ctl).unwrap_or(0),
                lo,
                hi,
            },
            at,
        )?;
        let span = self.spans.get(tok).copied().unwrap_or((0, 0));
        self.knobs.push(Knob {
            module,
            name: name.to_string(),
            ctl,
            lo,
            hi,
            exp,
            default: x,
            span,
        });
        if let Some(m) = self.modules.get_mut(module) {
            m.knobs.push(ctl);
        }
        self.knob_of.insert(tok, (ctl, node));
        Ok(node)
    }

    fn step(&mut self, at: Pos) -> R<()> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(CodeError {
                line: at.0,
                col: at.1,
                msg: "the SynthDef does too much to build",
            });
        }
        Ok(())
    }

    fn seq(&mut self, body: &[E], scope: &Rc<Scope>) -> R<V> {
        let mut last = V::Nil;
        for e in body {
            last = self.eval(e, scope)?;
        }
        Ok(last)
    }

    fn eval(&mut self, e: &E, scope: &Rc<Scope>) -> R<V> {
        match e {
            E::Num(v, tok) => Ok(V::Num(*v, *tok)),
            E::Sym(s) => Ok(V::Sym(s.clone())),
            E::Lit(Lit::True) => Ok(V::Bool(true)),
            E::Lit(Lit::False) => Ok(V::Bool(false)),
            E::Lit(Lit::Nil) => Ok(V::Nil),
            E::Class(c) => Ok(V::Class(c.clone())),
            E::Name(n, at) => {
                self.step(*at)?;
                scope.get(n).ok_or(CodeError {
                    line: at.0,
                    col: at.1,
                    msg: "this name is not defined",
                })
            }
            E::Arr(items) => Ok(V::Arr(
                items
                    .iter()
                    .map(|i| self.eval(i, scope))
                    .collect::<R<_>>()?,
            )),
            E::Func(def) => Ok(V::Func(Rc::new(Closure {
                def: def.clone(),
                scope: scope.clone(),
            }))),
            E::Var(vars) => {
                for (n, init) in vars {
                    let v = match init {
                        Some(e) => self.eval(e, scope)?,
                        None => V::Nil,
                    };
                    scope.define(n, v);
                }
                Ok(V::Nil)
            }
            E::Assign(n, e) => {
                let v = self.eval(e, scope)?;
                if !scope.set(n, v.clone()) {
                    scope.define(n, v.clone());
                }
                Ok(v)
            }
            E::Neg(e, at) => {
                let v = self.eval(e, scope)?;
                self.unop(v, "neg", *at)
            }
            E::Bin(op, a, b, at) => {
                self.step(*at)?;
                let a = self.eval(a, scope)?;
                let b = self.eval(b, scope)?;
                self.binop(op, a, b, *at)
            }
            E::Index(a, i, at) => {
                let a = self.eval(a, scope)?;
                let i = self.eval(i, scope)?;
                match (a, i) {
                    (V::Arr(items), V::Num(k, _)) => {
                        items.get(k.max(0.0) as usize).cloned().ok_or(CodeError {
                            line: at.0,
                            col: at.1,
                            msg: "this index is past the array's end",
                        })
                    }
                    _ => Err(CodeError {
                        line: at.0,
                        col: at.1,
                        msg: "an index is a number into an array",
                    }),
                }
            }
            E::Call {
                recv,
                name,
                args,
                kws,
                pos,
            } => {
                self.step(*pos)?;
                let r = self.eval(recv, scope)?;
                let args: Vec<V> = args.iter().map(|a| self.eval(a, scope)).collect::<R<_>>()?;
                let kws: Vec<(String, V)> = kws
                    .iter()
                    .map(|(k, e)| Ok((k.clone(), self.eval(e, scope)?)))
                    .collect::<R<_>>()?;
                self.call(r, name, args, kws, *pos)
            }
        }
    }

    /// Call `f` with `args`; parameters left out take their defaults.
    fn apply(&mut self, f: &Closure, args: &[V], at: Pos) -> R<V> {
        self.calls += 1;
        if self.calls > MAX_CALLS {
            return Err(CodeError {
                line: at.0,
                col: at.1,
                msg: "functions call each other too deep",
            });
        }
        let scope = Scope::child(&f.scope);
        for (k, (name, default, _)) in f.def.params.iter().enumerate() {
            let v = match (args.get(k), default) {
                (Some(v), _) => v.clone(),
                (None, Some(d)) => self.eval(d, &scope)?,
                (None, None) => V::Nil,
            };
            scope.define(name, v);
        }
        let out = self.seq(&f.def.body, &scope);
        self.calls -= 1;
        out
    }

    fn count(v: &V, at: Pos) -> R<usize> {
        match v {
            V::Num(n, _) if *n >= 0.0 && (*n as usize) <= MAX_COUNT => Ok(*n as usize),
            _ => Err(CodeError {
                line: at.0,
                col: at.1,
                msg: "a count is a whole number up to 256",
            }),
        }
    }

    fn call(&mut self, r: V, name: &str, args: Vec<V>, kws: Vec<(String, V)>, at: Pos) -> R<V> {
        let err = |msg| CodeError {
            line: at.0,
            col: at.1,
            msg,
        };
        match (&r, name) {
            (V::Class(c), _) => self.class(c, name, args, kws, at),
            // Controls written where they are used: `\cutoff.kr(800)`.
            (V::Sym(s), "kr" | "ar" | "ir") => {
                let default = args.first().cloned().unwrap_or(V::Num(0.0, None));
                self.control(s, &default, at)
            }
            (
                V::Def(_) | V::Func(_),
                "add" | "play" | "store" | "send" | "load" | "writeDefFile",
            ) => Ok(r),
            (V::Func(f), "value") => self.apply(f, &args, at),
            (V::Func(f), "dup") => {
                let n = Self::count(args.first().unwrap_or(&V::Num(2.0, None)), at)?;
                let mut out = Vec::with_capacity(n);
                for i in 0..n {
                    out.push(self.apply(f, &[V::Num(i as f64, None)], at)?);
                }
                Ok(V::Arr(out))
            }
            (V::Bool(c), "if") => {
                let branch = if *c { args.first() } else { args.get(1) };
                match branch {
                    Some(V::Func(f)) => self.apply(f, &[], at),
                    Some(v) => Ok(v.clone()),
                    None => Ok(V::Nil),
                }
            }
            (V::Num(c, _), "if") => self.call(V::Bool(*c != 0.0), name, args, kws, at),
            (V::Sig(..), "if") => Err(err("an if on a signal needs Select, which is not here yet")),
            (V::Num(n, _), "do") => {
                let n = Self::count(&V::Num(*n, None), at)?;
                if let Some(V::Func(f)) = args.first() {
                    for i in 0..n {
                        self.apply(f, &[V::Num(i as f64, None), V::Num(i as f64, None)], at)?;
                    }
                }
                Ok(r)
            }
            (V::Arr(items), "do" | "collect") => {
                let Some(V::Func(f)) = args.first() else {
                    return Err(err("this takes a function"));
                };
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.iter().enumerate() {
                    out.push(self.apply(f, &[v.clone(), V::Num(i as f64, None)], at)?);
                }
                Ok(if name == "do" { r.clone() } else { V::Arr(out) })
            }
            (V::Arr(items), "sum") => self.mix(items.clone(), at),
            (V::Arr(items), "size") => Ok(V::Num(items.len() as f64, None)),
            (V::Arr(items), "reverse") => Ok(V::Arr(items.iter().rev().cloned().collect())),
            (V::Arr(items), "first") => Ok(items.first().cloned().unwrap_or(V::Nil)),
            (V::Arr(items), "last") => Ok(items.last().cloned().unwrap_or(V::Nil)),
            (V::Arr(items), "at") => match args.first() {
                Some(V::Num(k, _)) => items
                    .get(k.max(0.0) as usize)
                    .cloned()
                    .ok_or(err("this index is past the array's end")),
                _ => Err(err("an index is a number into an array")),
            },
            (_, "dup") => {
                let n = Self::count(args.first().unwrap_or(&V::Num(2.0, None)), at)?;
                Ok(V::Arr(vec![r.clone(); n]))
            }
            (V::Env(spec), "kr" | "ar") => {
                // `env.kr(doneAction, gate)` is `EnvGen.kr(env, gate)`.
                let gate = args.get(1).cloned();
                self.envgen(spec, gate, at)
            }
            (_, "value") => Ok(r),
            (_, "max" | "min") if matches!(r, V::Num(..)) => match args.first() {
                Some(V::Num(b, _)) => {
                    let a = num(&r);
                    Ok(V::Num(
                        if name == "max" { a.max(*b) } else { a.min(*b) },
                        None,
                    ))
                }
                _ => Err(err("this takes a number")),
            },
            (V::Sig(..), "range" | "exprange") | (V::Arr(_), "range" | "exprange") => {
                let lo = args.first().cloned().unwrap_or(V::Num(0.0, None));
                let hi = args.get(1).cloned().unwrap_or(V::Num(1.0, None));
                self.expand(&[r, lo, hi], &mut |b, v| {
                    let [s, lo, hi] = v else {
                        return Err(err("range takes a low and a high"));
                    };
                    let (V::Sig(of, uni), Some(m)) = (s, Some(0usize)) else {
                        return Err(err("range is a method of a signal"));
                    };
                    let _ = m;
                    let exp = name == "exprange";
                    if exp && num_or(lo) * num_or(hi) <= 0.0 {
                        return Err(err("exprange needs two numbers of one sign, not 0"));
                    }
                    let module = b.module(if exp { "exprange" } else { "range" }, at);
                    let lo = b.input(lo, module, "lo", ANY_SPEC(lo), at)?;
                    let hi = b.input(hi, module, "hi", ANY_SPEC(hi), at)?;
                    Ok(V::Sig(
                        b.push(
                            Ugen::Range {
                                of: *of,
                                lo,
                                hi,
                                exp,
                                uni: *uni,
                            },
                            at,
                        )?,
                        false,
                    ))
                })
            }
            _ => self.unop(r, name, at),
        }
    }

    /// One-argument methods: on numbers worked out now, on signals as nodes.
    fn unop(&mut self, v: V, name: &str, at: Pos) -> R<V> {
        let err = |msg| CodeError {
            line: at.0,
            col: at.1,
            msg,
        };
        match v {
            V::Num(x, _) => {
                let y = match name {
                    "neg" => -x,
                    "abs" => x.abs(),
                    "reciprocal" => 1.0 / x,
                    "squared" => x * x,
                    "cubed" => x * x * x,
                    "sqrt" => x.sqrt(),
                    "round" => x.round(),
                    "floor" => x.floor(),
                    "ceil" => x.ceil(),
                    "asInteger" => x.trunc(),
                    "midicps" => 440.0 * ((x - 69.0) / 12.0).exp2(),
                    "cpsmidi" => 69.0 + 12.0 * (x / 440.0).log2(),
                    "midiratio" => (x / 12.0).exp2(),
                    "ratiomidi" => 12.0 * x.log2(),
                    "dbamp" => 10f64.powf(x / 20.0),
                    "ampdb" => 20.0 * x.log10(),
                    "tanh" => x.tanh(),
                    "atan" => x.atan(),
                    "distort" => x / (1.0 + x.abs()),
                    "softclip" => {
                        if x.abs() <= 0.5 {
                            x
                        } else {
                            (x.abs() - 0.25) / x
                        }
                    }
                    _ => return Err(err("this method is not part of a SynthDef here")),
                };
                Ok(V::Num(y, None))
            }
            V::Sig(n, uni) => {
                let kind = match name {
                    "neg" => return Ok(V::Sig(self.push(Ugen::Neg(n), at)?, false)),
                    "tanh" => 0,
                    "softclip" => 1,
                    "distort" => 2,
                    _ => return Err(err("this method is not supported on a signal yet")),
                };
                let _ = uni;
                Ok(V::Sig(self.push(Ugen::Clip { input: n, kind }, at)?, false))
            }
            V::Arr(items) => Ok(V::Arr(
                items
                    .into_iter()
                    .map(|i| self.unop(i, name, at))
                    .collect::<R<_>>()?,
            )),
            _ => Err(err("this method is not part of a SynthDef here")),
        }
    }

    fn binop(&mut self, op: &str, a: V, b: V, at: Pos) -> R<V> {
        let err = |msg| CodeError {
            line: at.0,
            col: at.1,
            msg,
        };
        if op == "!" {
            let n = Self::count(&b, at)?;
            return match a {
                V::Func(f) => {
                    let mut out = Vec::with_capacity(n);
                    for i in 0..n {
                        out.push(self.apply(&f, &[V::Num(i as f64, None)], at)?);
                    }
                    Ok(V::Arr(out))
                }
                v => Ok(V::Arr(vec![v; n])),
            };
        }
        match (a, b) {
            (V::Arr(x), V::Arr(y)) => {
                let n = x.len().max(y.len());
                let mut out = Vec::with_capacity(n);
                for i in 0..n {
                    let (p, q) = (
                        x.get(i % x.len().max(1)).cloned(),
                        y.get(i % y.len().max(1)).cloned(),
                    );
                    out.push(self.binop(op, p.unwrap_or(V::Nil), q.unwrap_or(V::Nil), at)?);
                }
                Ok(V::Arr(out))
            }
            (V::Arr(x), b) => Ok(V::Arr(
                x.into_iter()
                    .map(|p| self.binop(op, p, b.clone(), at))
                    .collect::<R<_>>()?,
            )),
            (a, V::Arr(y)) => Ok(V::Arr(
                y.into_iter()
                    .map(|q| self.binop(op, a.clone(), q, at))
                    .collect::<R<_>>()?,
            )),
            (V::Num(x, _), V::Num(y, _)) => Ok(match op {
                "+" => V::Num(x + y, None),
                "-" => V::Num(x - y, None),
                "*" => V::Num(x * y, None),
                "/" => V::Num(if y == 0.0 { 0.0 } else { x / y }, None),
                "%" => V::Num(if y == 0.0 { 0.0 } else { x.rem_euclid(y) }, None),
                "**" => V::Num(x.powf(y), None),
                "<" => V::Bool(x < y),
                ">" => V::Bool(x > y),
                "<=" => V::Bool(x <= y),
                ">=" => V::Bool(x >= y),
                "==" => V::Bool(x == y),
                "!=" => V::Bool(x != y),
                _ => return Err(err("this operator is not part of a SynthDef here")),
            }),
            (a @ (V::Sig(..) | V::Num(..)), b @ (V::Sig(..) | V::Num(..))) => {
                let op = match op {
                    "+" => Op::Add,
                    "-" => Op::Sub,
                    "*" => Op::Mul,
                    "/" => Op::Div,
                    _ => return Err(err("this operator does not work on a signal here")),
                };
                let x = self.node(&a, at)?;
                let y = self.node(&b, at)?;
                Ok(V::Sig(self.push(Ugen::Bin(op, x, y), at)?, false))
            }
            _ => Err(err("this operator does not work on these values")),
        }
    }

    /// A number or a signal as a node, without a knob.
    fn node(&mut self, v: &V, at: Pos) -> R<u8> {
        match v {
            V::Num(x, _) => self.push(Ugen::Num(*x as f32), at),
            V::Sig(n, _) => Ok(*n),
            V::Bool(b) => self.push(Ugen::Num(f32::from(u8::from(*b))), at),
            _ => Err(CodeError {
                line: at.0,
                col: at.1,
                msg: "this is not a signal",
            }),
        }
    }

    /// A control by name: the note's `freq`, `gate` and `amp`, or a knob.
    fn control(&mut self, name: &str, default: &V, at: Pos) -> R<V> {
        if let Some(v) = self.controls.get(name) {
            return Ok(v.clone());
        }
        let v = match name {
            "freq" => V::Sig(self.push(Ugen::Freq, at)?, false),
            "gate" => V::Sig(self.push(Ugen::Gate, at)?, true),
            "out" | "bus" | "outBus" | "i_out" => V::Num(0.0, None),
            "amp" => {
                // The note's velocity times the default: the voice's level.
                self.prog.own_amp = true;
                let module = self.module("Controls", (0, 0));
                let level = self.input(default, module, "amp", UNIT, at)?;
                let vel = self.push(Ugen::Vel, at)?;
                V::Sig(self.push(Ugen::Bin(Op::Mul, vel, level), at)?, true)
            }
            _ => {
                let module = self.module("Controls", (0, 0));
                let x = num_or(default) as f32;
                V::Sig(
                    self.input(default, module, name, control_spec(name, x), at)?,
                    false,
                )
            }
        };
        self.controls.insert(name.to_string(), v.clone());
        Ok(v)
    }

    /// Build the SynthDef's function: its arguments are controls.
    fn build(&mut self, def: &Closure, at: Pos) -> R<V> {
        let mut args = Vec::new();
        for (name, default, pos) in &def.def.params {
            let d = match default {
                Some(e) => self.eval(e, &def.scope)?,
                None => V::Num(0.0, None),
            };
            args.push(self.control(name, &d, *pos)?);
        }
        let _ = at;
        self.apply(def, &args, at)
    }

    /// The node the voice plays: a signal, a number, or an array summed.
    fn output(&mut self, v: &V, at: Pos) -> R<u8> {
        match v {
            V::Sig(n, _) => Ok(*n),
            V::Num(x, _) => self.push(Ugen::Num(*x as f32), at),
            V::Arr(items) => match self.mix(items.clone(), at)? {
                V::Sig(n, _) => Ok(n),
                other => self.output(&other, at),
            },
            _ => Err(CodeError {
                line: at.0,
                col: at.1,
                msg: "the SynthDef gives no sound: its last line is a signal",
            }),
        }
    }

    /// `Mix`: the signals of an array, flattened, added in nodes of eight.
    fn mix(&mut self, items: Vec<V>, at: Pos) -> R<V> {
        let mut flat = Vec::new();
        flatten(items, &mut flat);
        let mut nodes: Vec<u8> = flat.iter().map(|v| self.node(v, at)).collect::<R<_>>()?;
        if nodes.is_empty() {
            return Ok(V::Num(0.0, None));
        }
        while nodes.len() > 1 {
            let mut next = Vec::new();
            for chunk in nodes.chunks(MAX_MIX) {
                if let [one] = chunk {
                    next.push(*one);
                    continue;
                }
                let mut inputs = [0u8; MAX_MIX];
                for (slot, n) in inputs.iter_mut().zip(chunk) {
                    *slot = *n;
                }
                let n = u8::try_from(chunk.len()).unwrap_or(0);
                next.push(self.push(
                    Ugen::Mix {
                        inputs,
                        n,
                        mean: false,
                    },
                    at,
                )?);
            }
            nodes = next;
        }
        Ok(V::Sig(nodes.first().copied().unwrap_or(0), false))
    }

    /// Run `f` once per channel when an input is an array (multichannel
    /// expansion), else once.
    fn expand(&mut self, inputs: &[V], f: &mut dyn FnMut(&mut Builder, &[V]) -> R<V>) -> R<V> {
        let n = inputs
            .iter()
            .map(|v| if let V::Arr(a) = v { a.len() } else { 0 })
            .max()
            .unwrap_or(0);
        if n == 0 {
            return f(self, inputs);
        }
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            let one: Vec<V> = inputs
                .iter()
                .map(|v| match v {
                    V::Arr(a) if !a.is_empty() => a.get(k % a.len()).cloned().unwrap_or(V::Nil),
                    other => other.clone(),
                })
                .collect();
            out.push(self.expand(&one, f)?);
        }
        Ok(V::Arr(out))
    }

    fn class(
        &mut self,
        c: &str,
        method: &str,
        args: Vec<V>,
        kws: Vec<(String, V)>,
        at: Pos,
    ) -> R<V> {
        let err = |msg| CodeError {
            line: at.0,
            col: at.1,
            msg,
        };
        match (c, method) {
            ("SynthDef", "new") => match args.get(1) {
                Some(V::Func(f)) => Ok(V::Def(f.clone())),
                _ => Err(err("a SynthDef is SynthDef(\\name, { … })")),
            },
            ("Env", "adsr" | "perc" | "asr") => {
                let kind = match method {
                    "adsr" => EnvKind::Adsr,
                    "perc" => EnvKind::Perc,
                    _ => EnvKind::Asr,
                };
                Ok(V::Env(EnvSpec {
                    kind,
                    args,
                    pos: at,
                }))
            }
            ("Env", "new") => Err(err("Env(levels, times, curves) is not supported yet")),
            ("Array", "fill") => {
                let n = Self::count(args.first().unwrap_or(&V::Nil), at)?;
                let Some(V::Func(f)) = args.get(1) else {
                    return Err(err("Array.fill takes a count and a function"));
                };
                let mut out = Vec::with_capacity(n);
                for i in 0..n {
                    out.push(self.apply(f, &[V::Num(i as f64, None)], at)?);
                }
                Ok(V::Arr(out))
            }
            ("Mix", "new" | "ar" | "kr") => match args.into_iter().next() {
                Some(V::Arr(items)) => self.mix(items, at),
                Some(v) => Ok(v),
                None => Err(err("Mix takes an array")),
            },
            ("Mix", "fill") => {
                let n = Self::count(args.first().unwrap_or(&V::Nil), at)?;
                let Some(V::Func(f)) = args.get(1) else {
                    return Err(err("Mix.fill takes a count and a function"));
                };
                let mut out = Vec::with_capacity(n);
                for i in 0..n {
                    out.push(self.apply(f, &[V::Num(i as f64, None)], at)?);
                }
                self.mix(out, at)
            }
            ("Out" | "OffsetOut" | "ReplaceOut", "ar" | "kr") => {
                let bound =
                    bind(&["bus", "channelsArray"], &[None, None], &args, &kws).map_err(err)?;
                Ok(bound.get(1).cloned().flatten().unwrap_or(V::Nil))
            }
            ("Pan2" | "Splay" | "FreeVerb2" | "FreeVerb", _) => {
                Err(err("this UGen needs the stereo synth bus (#288)"))
            }
            ("DelayN" | "DelayL" | "DelayC" | "Rand" | "ExpRand" | "IRand", _) => Err(err(
                "this UGen comes with the next step of the Modular synth",
            )),
            ("EnvGen", "kr" | "ar") => {
                let bound = bind(
                    &[
                        "envelope",
                        "gate",
                        "levelScale",
                        "levelBias",
                        "timeScale",
                        "doneAction",
                    ],
                    &[None, Some(1.0), Some(1.0), Some(0.0), Some(1.0), Some(0.0)],
                    &args,
                    &kws,
                )
                .map_err(err)?;
                match bound.first().cloned().flatten() {
                    Some(V::Env(spec)) => {
                        let env = self.envgen(&spec, bound.get(1).cloned().flatten(), at)?;
                        // levelScale as a multiplier, when written.
                        match bound.get(2).cloned().flatten() {
                            Some(V::Num(1.0, None)) => Ok(env),
                            Some(scale) => self.binop("*", env, scale, at),
                            None => Ok(env),
                        }
                    }
                    _ => Err(err("EnvGen plays an Env: Env.adsr, Env.perc or Env.asr")),
                }
            }
            (_, "ar" | "kr") => self.ugen(c, method == "kr", args, kws, at),
            _ => Err(err("this class is not part of a SynthDef here")),
        }
    }

    /// An `EnvGen` of `spec`: its times are knobs on its module.
    fn envgen(&mut self, spec: &EnvSpec, _gate: Option<V>, at: Pos) -> R<V> {
        let module = self.module("EnvGen", spec.pos);
        let a = |k: usize, d: f64| spec.args.get(k).cloned().unwrap_or(V::Num(d, None));
        let (times, level) = match spec.kind {
            EnvKind::Adsr => (
                [
                    ("attack", a(0, 0.01)),
                    ("decay", a(1, 0.3)),
                    ("sustain", a(2, 0.5)),
                    ("release", a(3, 1.0)),
                ],
                None,
            ),
            EnvKind::Perc => (
                [
                    ("attack", a(0, 0.01)),
                    ("release", a(1, 1.0)),
                    ("", V::Num(0.0, None)),
                    ("", V::Nil),
                ],
                Some(a(2, 1.0)),
            ),
            EnvKind::Asr => (
                [
                    ("attack", a(0, 0.01)),
                    ("", V::Num(0.001, None)),
                    ("", V::Num(1.0, None)),
                    ("release", a(2, 1.0)),
                ],
                Some(a(1, 1.0)),
            ),
        };
        let mut nodes = [NONE; 4];
        for (k, (name, v)) in times.iter().enumerate() {
            let s = if *name == "sustain" { UNIT } else { TIME };
            let v = if matches!(v, V::Nil) {
                times
                    .get(1)
                    .map(|t| t.1.clone())
                    .unwrap_or(V::Num(1.0, None))
            } else {
                v.clone()
            };
            let node = if name.is_empty() {
                self.node(&v, at)?
            } else {
                self.input(&v, module, name, s, at)?
            };
            if !matches!(self.prog.node(node), Some(Ugen::Num(_) | Ugen::Ctl { .. })) {
                return Err(CodeError {
                    line: at.0,
                    col: at.1,
                    msg: "an envelope's times are numbers or controls",
                });
            }
            if let Some(slot) = nodes.get_mut(k) {
                *slot = node;
            }
        }
        // A percussive envelope releases with its decay, so it ends held.
        if spec.kind == EnvKind::Perc {
            nodes[3] = nodes[1];
        }
        let slot = Self::slot(
            &mut self.prog.counts.envs,
            MAX_ENVS,
            at,
            "a voice has at most 4 envelopes",
        )?;
        if let Some(e) = self.prog.envs.get_mut(usize::from(slot)) {
            *e = Shape::Nodes(nodes);
        }
        let env = V::Sig(self.push(Ugen::Env { slot }, at)?, true);
        match level {
            Some(V::Num(1.0, None)) => Ok(env),
            Some(l @ V::Num(_, Some(_))) => {
                let n = self.input(&l, module, "level", UNIT, at)?;
                Ok(V::Sig(
                    self.push(Ugen::Bin(Op::Mul, node_of(&env), n), at)?,
                    true,
                ))
            }
            Some(l) => self.binop("*", env, l, at),
            None => Ok(env),
        }
    }

    /// A UGen by class: its arguments bound by position and name, expanded
    /// over arrays, its literal numbers knobs on its module.
    fn ugen(&mut self, c: &str, kr: bool, args: Vec<V>, kws: Vec<(String, V)>, at: Pos) -> R<V> {
        let err = |msg| CodeError {
            line: at.0,
            col: at.1,
            msg,
        };
        let (names, defaults, specs): (&[&str], &[Option<f64>], &[Spec]) = match c {
            "SinOsc" => (
                &["freq", "phase", "mul", "add"],
                &[Some(440.0), Some(0.0), Some(1.0), Some(0.0)],
                &[FREQ, PHASE, MUL, ADD],
            ),
            "Saw" => (
                &["freq", "mul", "add"],
                &[Some(440.0), Some(1.0), Some(0.0)],
                &[FREQ, MUL, ADD],
            ),
            "Pulse" => (
                &["freq", "width", "mul", "add"],
                &[Some(440.0), Some(0.5), Some(1.0), Some(0.0)],
                &[FREQ, UNIT, MUL, ADD],
            ),
            "LFSaw" | "LFTri" => (
                &["freq", "iphase", "mul", "add"],
                &[Some(440.0), Some(0.0), Some(1.0), Some(0.0)],
                &[FREQ, PHASE, MUL, ADD],
            ),
            "LFPulse" => (
                &["freq", "iphase", "width", "mul", "add"],
                &[Some(440.0), Some(0.0), Some(0.5), Some(1.0), Some(0.0)],
                &[FREQ, PHASE, UNIT, MUL, ADD],
            ),
            "WhiteNoise" => (&["mul", "add"], &[Some(1.0), Some(0.0)], &[MUL, ADD]),
            "PMOsc" => (
                &["carfreq", "modfreq", "pmindex", "modphase", "mul", "add"],
                &[None, None, Some(0.0), Some(0.0), Some(1.0), Some(0.0)],
                &[FREQ, FREQ, INDEX, PHASE, MUL, ADD],
            ),
            "RLPF" | "RHPF" => (
                &["in", "freq", "rq", "mul", "add"],
                &[None, Some(440.0), Some(1.0), Some(1.0), Some(0.0)],
                &[ANY, FREQ, RQ, MUL, ADD],
            ),
            "LPF" | "HPF" => (
                &["in", "freq", "mul", "add"],
                &[None, Some(440.0), Some(1.0), Some(0.0)],
                &[ANY, FREQ, MUL, ADD],
            ),
            "MoogFF" => (
                &["in", "freq", "gain", "reset", "mul", "add"],
                &[
                    None,
                    Some(100.0),
                    Some(2.0),
                    Some(0.0),
                    Some(1.0),
                    Some(0.0),
                ],
                &[ANY, FREQ, GAIN, ANY, MUL, ADD],
            ),
            "CombL" | "CombN" | "CombC" => (
                &["in", "maxdelaytime", "delaytime", "decaytime", "mul", "add"],
                &[None, Some(0.2), Some(0.2), Some(1.0), Some(1.0), Some(0.0)],
                &[ANY, DELAY, DELAY, DECAY, MUL, ADD],
            ),
            _ => return Err(err("this UGen is not part of a SynthDef here")),
        };
        let bound = bind(names, defaults, &args, &kws).map_err(err)?;
        let inputs: Vec<V> = bound.iter().map(|v| v.clone().unwrap_or(V::Nil)).collect();
        let freq_spec = if kr { RATE } else { FREQ };
        let c = c.to_string();
        self.expand(&inputs, &mut |b, v| {
            let module = b.module(&c, at);
            let named = |k: usize| names.get(k).copied().unwrap_or("");
            let inp = |b: &mut Builder, k: usize| -> R<u8> {
                let v = v.get(k).cloned().unwrap_or(V::Nil);
                if matches!(v, V::Nil) {
                    return Err(CodeError {
                        line: at.0,
                        col: at.1,
                        msg: "this UGen needs its input",
                    });
                }
                let spec = match named(k) {
                    "freq" | "carfreq" | "modfreq" => freq_spec,
                    _ => specs.get(k).copied().unwrap_or(ANY),
                };
                b.input(&v, module, named(k), spec, at)
            };
            let osc = |b: &mut Builder, wave: Waveform, freq: u8, width: u8| -> R<u8> {
                let slot = Self::slot(
                    &mut b.prog.counts.oscs,
                    MAX_OSCS,
                    at,
                    "a voice has at most 8 oscillators",
                )?;
                b.push(
                    Ugen::Osc {
                        wave,
                        freq,
                        width,
                        slot,
                    },
                    at,
                )
            };
            let lfo = |b: &mut Builder, wave: Waveform, rate: u8| -> R<u8> {
                let slot = Self::slot(
                    &mut b.prog.counts.phases,
                    MAX_PHASES,
                    at,
                    "a voice has at most 8 sines and LFOs",
                )?;
                b.push(Ugen::Lfo { rate, wave, slot }, at)
            };
            let (node, uni, rest) = match c.as_str() {
                "SinOsc" => {
                    let f = inp(b, 0)?;
                    if num_or(v.get(1).unwrap_or(&V::Nil)) != 0.0 {
                        return Err(CodeError {
                            line: at.0,
                            col: at.1,
                            msg: "a SinOsc's phase is not supported yet",
                        });
                    }
                    let slot = Self::slot(
                        &mut b.prog.counts.phases,
                        MAX_PHASES,
                        at,
                        "a voice has at most 8 sines and LFOs",
                    )?;
                    (b.push(Ugen::Sin { freq: f, slot }, at)?, false, 2)
                }
                "Saw" | "LFSaw" if !kr || c == "Saw" => {
                    let f = inp(b, 0)?;
                    (
                        osc(b, Waveform::Saw, f, NONE)?,
                        false,
                        if c == "Saw" { 1 } else { 2 },
                    )
                }
                "LFSaw" => {
                    let f = inp(b, 0)?;
                    (lfo(b, Waveform::Saw, f)?, false, 2)
                }
                "Pulse" => {
                    let f = inp(b, 0)?;
                    let w = inp(b, 1)?;
                    (osc(b, Waveform::Pulse, f, w)?, false, 2)
                }
                "LFTri" if !kr => {
                    let f = inp(b, 0)?;
                    (osc(b, Waveform::Triangle, f, NONE)?, false, 2)
                }
                "LFTri" => {
                    let f = inp(b, 0)?;
                    (lfo(b, Waveform::Triangle, f)?, false, 2)
                }
                "LFPulse" => {
                    // 0..1, as SuperCollider's LFPulse.
                    let f = inp(b, 0)?;
                    let sq = lfo(b, Waveform::Pulse, f)?;
                    let one = b.push(Ugen::Num(1.0), at)?;
                    let half = b.push(Ugen::Num(0.5), at)?;
                    let up = b.push(Ugen::Bin(Op::Add, sq, one), at)?;
                    (b.push(Ugen::Bin(Op::Mul, up, half), at)?, true, 3)
                }
                "WhiteNoise" => (b.push(Ugen::Noise, at)?, false, 0),
                "PMOsc" => {
                    let (car, md, ix) = (inp(b, 0)?, inp(b, 1)?, inp(b, 2)?);
                    let slot = Self::slot(
                        &mut b.prog.counts.phases,
                        MAX_PHASES - 1,
                        at,
                        "a voice has at most 8 sines and LFOs, a PMOsc counting two",
                    )?;
                    b.prog.counts.phases += 1;
                    (
                        b.push(
                            Ugen::Fm {
                                carrier: car,
                                modulator: md,
                                index: ix,
                                slot,
                            },
                            at,
                        )?,
                        false,
                        4,
                    )
                }
                "RLPF" | "RHPF" | "LPF" | "HPF" => {
                    let x = inp(b, 0)?;
                    let f = inp(b, 1)?;
                    let resonant = c.starts_with('R');
                    let res = if resonant {
                        inp(b, 2)?
                    } else {
                        b.push(Ugen::Num(0.0), at)?
                    };
                    let slot = Self::slot(
                        &mut b.prog.counts.filters,
                        MAX_FILTERS,
                        at,
                        "a voice has at most 4 filters",
                    )?;
                    let high = c.ends_with("HPF");
                    (
                        b.push(
                            Ugen::Svf {
                                input: x,
                                high,
                                cutoff: f,
                                res,
                                slot,
                                rq: resonant,
                                voicing: NONE,
                            },
                            at,
                        )?,
                        false,
                        if resonant { 3 } else { 2 },
                    )
                }
                "MoogFF" => {
                    let (x, f, g) = (inp(b, 0)?, inp(b, 1)?, inp(b, 2)?);
                    let slot = Self::slot(
                        &mut b.prog.counts.filters,
                        MAX_FILTERS,
                        at,
                        "a voice has at most 4 filters",
                    )?;
                    (
                        b.push(
                            Ugen::Ladder {
                                input: x,
                                cutoff: f,
                                res: g,
                                slot,
                                gain: true,
                                voicing: NONE,
                                drive: NONE,
                            },
                            at,
                        )?,
                        false,
                        4,
                    )
                }
                _ => {
                    // CombL, CombN, CombC.
                    let max = num_or(v.get(1).unwrap_or(&V::Nil)) as f32;
                    if max * 48_000.0 > (MAX_DELAY - 2) as f32 {
                        return Err(CodeError {
                            line: at.0,
                            col: at.1,
                            msg: "a comb holds at most 0.02 seconds here",
                        });
                    }
                    let x = inp(b, 0)?;
                    let t = inp(b, 2)?;
                    let d = inp(b, 3)?;
                    Self::slot(
                        &mut b.prog.counts.delays,
                        MAX_DELAYS,
                        at,
                        "a voice has one comb",
                    )?;
                    (
                        b.push(
                            Ugen::Delay {
                                input: x,
                                time: t,
                                feedback: d,
                                decay: true,
                            },
                            at,
                        )?,
                        false,
                        4,
                    )
                }
            };
            // mul and add, the last two arguments, when written.
            let mut out = node;
            let mul_at = names.iter().position(|n| *n == "mul");
            let _ = rest;
            if let Some(k) = mul_at {
                for (j, op) in [(k, Op::Mul), (k + 1, Op::Add)] {
                    let given = bound.get(j).is_some_and(Option::is_some)
                        && args_given(&args, &kws, names, j);
                    if given {
                        let n = inp(b, j)?;
                        out = b.push(Ugen::Bin(op, out, n), at)?;
                    }
                }
            }
            Ok(V::Sig(out, uni))
        })
    }
}

/// Whether argument `k` of `names` was written, by position or by name.
fn args_given(args: &[V], kws: &[(String, V)], names: &[&str], k: usize) -> bool {
    k < args.len()
        || names
            .get(k)
            .is_some_and(|n| kws.iter().any(|(kw, _)| kw == n))
}

/// Arguments bound to `names` by position, then by keyword, then defaults.
fn bind(
    names: &[&str],
    defaults: &[Option<f64>],
    args: &[V],
    kws: &[(String, V)],
) -> Result<Vec<Option<V>>, &'static str> {
    if args.len() > names.len() {
        return Err("this takes fewer arguments");
    }
    let mut out: Vec<Option<V>> = names
        .iter()
        .enumerate()
        .map(|(k, _)| args.get(k).cloned())
        .collect();
    for (kw, v) in kws {
        let Some(k) = names.iter().position(|n| n == kw) else {
            return Err("there is no argument by this name");
        };
        if let Some(slot) = out.get_mut(k) {
            *slot = Some(v.clone());
        }
    }
    for (slot, d) in out.iter_mut().zip(defaults) {
        if slot.is_none() {
            *slot = d.map(|d| V::Num(d, None));
        }
    }
    Ok(out)
}

fn flatten(items: Vec<V>, out: &mut Vec<V>) {
    for v in items {
        match v {
            V::Arr(inner) => flatten(inner, out),
            other => out.push(other),
        }
    }
}

fn num(v: &V) -> f64 {
    num_or(v)
}

fn num_or(v: &V) -> f64 {
    match v {
        V::Num(x, _) => *x,
        _ => 0.0,
    }
}

fn node_of(v: &V) -> u8 {
    match v {
        V::Sig(n, _) => *n,
        _ => 0,
    }
}

/// A knob's range for a `range` bound, from its number.
#[allow(non_snake_case)]
fn ANY_SPEC(v: &V) -> Spec {
    let x = num_or(v) as f32;
    if x > 0.0 {
        Spec {
            lo: x / 10.0,
            hi: x * 10.0,
            exp: true,
        }
    } else {
        let top = (x.abs() * 2.0).max(1.0);
        Spec {
            lo: -top,
            hi: top,
            exp: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(src: &str) -> Patch {
        compile(src).unwrap_or_else(|e| panic!("{src}: {e:?}"))
    }

    fn nodes(p: &Patch) -> Vec<Ugen> {
        p.program
            .nodes
            .iter()
            .take(usize::from(p.program.len))
            .copied()
            .collect()
    }

    fn count(p: &Patch, f: impl Fn(&Ugen) -> bool) -> usize {
        nodes(p).iter().filter(|u| f(u)).count()
    }

    #[test]
    fn a_synthdef_builds_with_a_knob_per_number() {
        let p =
            patch("SynthDef(\\a, { |freq = 440, gate = 1| RLPF.ar(Saw.ar(freq), 800, 0.3) }).add;");
        let names: Vec<(&str, &str, f32)> = p
            .knobs
            .iter()
            .map(|k| {
                (
                    p.modules[k.module].name.as_str(),
                    k.name.as_str(),
                    k.default,
                )
            })
            .collect();
        assert_eq!(names, vec![("RLPF", "freq", 800.0), ("RLPF", "rq", 0.3)]);
        assert!(matches!(
            nodes(&p).last(),
            Some(Ugen::Svf {
                rq: true,
                high: false,
                ..
            })
        ));
        assert_eq!(count(&p, |u| matches!(u, Ugen::Freq)), 1);
        assert!(p.knobs[0].exp && p.knobs[0].lo <= 20.0 && p.knobs[0].hi >= 20_000.0);
    }

    #[test]
    fn the_text_shows_the_knobs_values() {
        let src = "{ |freq|\n  // a comment\n  RLPF.ar(Saw.ar(freq), 800, 0.30)\n}";
        let p = patch(src);
        assert_eq!(
            p.text(&[800.0, 0.3]),
            src,
            "unturned knobs leave the text as written"
        );
        assert_eq!(p.text(&[1250.5, 0.3]), src.replace("800", "1250.5"));
        let neg = patch("{ |freq| SinOsc.ar(freq, 0, 0.5, -0.1) }");
        let add = neg.knobs.iter().find(|k| k.name == "add").expect("add");
        assert_eq!(&neg.source[add.span.0..add.span.1], "-0.1");
    }

    #[test]
    fn operators_run_left_to_right_as_in_sclang() {
        let p = patch("{ SinOsc.ar(1 + 2 * 3) }");
        assert!(nodes(&p).contains(&Ugen::Num(9.0)), "{:?}", nodes(&p));
        assert!(p.knobs.is_empty(), "a worked-out number is no knob");
    }

    #[test]
    fn functions_loops_and_vars_build_a_graph() {
        let p = patch("{ |freq| Mix.fill(3, { |i| Saw.ar(freq * (i + 1)) }) }");
        assert_eq!(count(&p, |u| matches!(u, Ugen::Osc { .. })), 3);
        assert!(matches!(
            nodes(&p).last(),
            Some(Ugen::Mix {
                n: 3,
                mean: false,
                ..
            })
        ));
        let v = patch("{ |freq| var sig = Saw.ar(freq); sig = LPF.ar(sig, 500); sig * 0.5 }");
        let osc = nodes(&v)
            .iter()
            .position(|u| matches!(u, Ugen::Osc { .. }))
            .expect("osc") as u8;
        assert!(
            nodes(&v)
                .iter()
                .any(|u| matches!(u, Ugen::Svf { input, .. } if *input == osc))
        );
        // A number in a loop's body is one knob for every copy.
        let l = patch("{ |freq| Mix.fill(4, { LPF.ar(Saw.ar(freq), 900) }) }");
        assert_eq!(l.knobs.len(), 1);
        assert_eq!(count(&l, |u| matches!(u, Ugen::Ctl { .. })), 1);
        let d = patch("{ |freq| ({ Saw.ar(freq) } ! 2).sum }");
        assert_eq!(count(&d, |u| matches!(u, Ugen::Osc { .. })), 2);
    }

    #[test]
    fn controls_by_name_and_by_argument() {
        let p = patch(
            "{ var f = \\freq.kr(440); RLPF.ar(Saw.ar(f), \\cutoff.kr(800), 0.5) * \\cutoff.kr(800) / 800 }",
        );
        assert_eq!(
            p.knobs.iter().filter(|k| k.name == "cutoff").count(),
            1,
            "one control, used twice"
        );
        assert_eq!(count(&p, |u| matches!(u, Ugen::Freq)), 1);
        let a = patch("{ |freq, amp = 0.2| Saw.ar(freq) * amp }");
        assert!(a.program.own_amp);
        assert_eq!(
            a.knobs.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(),
            vec!["amp"]
        );
    }

    #[test]
    fn if_picks_a_branch_at_build_time() {
        let p = patch("{ |freq| if(2 > 1) { Saw.ar(freq) } { Pulse.ar(freq) } }");
        assert!(matches!(
            nodes(&p).last(),
            Some(Ugen::Osc {
                wave: Waveform::Saw,
                ..
            })
        ));
        let e = compile("{ |freq, gate| if(gate, { Saw.ar(freq) }, { Pulse.ar(freq) }) }")
            .expect_err("signal");
        assert_eq!(
            e.msg,
            "an if on a signal needs Select, which is not here yet"
        );
    }

    #[test]
    fn an_envelopes_times_are_knobs() {
        let p = patch(
            "{ |freq, gate = 1| Saw.ar(freq) * EnvGen.kr(Env.adsr(0.01, 0.2, 0.7, 0.3), gate) }",
        );
        let env: Vec<&str> = p.knobs.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(env, vec!["attack", "decay", "sustain", "release"]);
        assert!(matches!(p.program.envs[0], Shape::Nodes(_)));
        let perc = patch("{ SinOsc.ar(Env.perc(0.001, 0.12).kr.exprange(48, 900)) }");
        let names: Vec<&str> = perc.knobs.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, vec!["attack", "release", "lo", "hi"]);
    }

    #[test]
    fn arrays_expand_into_channels() {
        let p = patch("{ SinOsc.ar([220, 330], 0, 0.3) }");
        assert_eq!(count(&p, |u| matches!(u, Ugen::Sin { .. })), 2);
        assert!(
            matches!(nodes(&p).last(), Some(Ugen::Mix { n: 2, .. })),
            "summed to mono"
        );
        assert_eq!(p.knobs.len(), 3, "220, 330 and the shared mul");
    }

    #[test]
    fn errors_say_where() {
        for (src, line, col, msg) in [
            (
                "{ Sawtooth.ar(440) }",
                1,
                3,
                "this UGen is not part of a SynthDef here",
            ),
            ("{ Saw.ar(fr) }", 1, 10, "this name is not defined"),
            ("{ Saw.ar(440) ", 1, 15, "} goes here"),
            (
                "{ |freq|\n  Pan2.ar(Saw.ar(freq), 0) }",
                2,
                3,
                "this UGen needs the stereo synth bus (#288)",
            ),
            (
                "{ DelayN.ar(Saw.ar(440), 0.01, 0.01) }",
                1,
                3,
                "this UGen comes with the next step of the Modular synth",
            ),
            (
                "{ Env([0, 1], [0.1]).kr }",
                1,
                3,
                "Env(levels, times, curves) is not supported yet",
            ),
            (
                "42",
                1,
                1,
                "the code is a SynthDef(\\name, { … }) or a function { … }",
            ),
            (
                "{ |freq| { Saw.ar(freq) }.dup(9).sum }",
                1,
                12,
                "a voice has at most 8 oscillators",
            ),
            (
                "{ CombL.ar(Saw.ar(440), 0.2, 0.1, 1) }",
                1,
                3,
                "a comb holds at most 0.02 seconds here",
            ),
            (
                "{ \"text\" }",
                1,
                3,
                "strings are not part of a SynthDef here",
            ),
        ] {
            let e = compile(src).expect_err(src);
            assert_eq!((e.line, e.col, e.msg), (line, col, msg), "{src}");
        }
    }
}
