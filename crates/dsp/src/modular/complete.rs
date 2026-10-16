//! Completion for a Modular synth's SuperCollider code (#595, #647), in its
//! code editor and under a `Modular` setting in the song: the classes a
//! SynthDef here takes, their methods and arguments, the methods of values,
//! filter voicings, and the code's own arguments and variables. As the song's
//! completer it reads the text before the cursor, never builds it, and is
//! total. The tables are held to the builder by the tests: what they offer
//! builds.

use super::{Class, VOICINGS};
use crate::song::complete::{Completion, List, byte_at, is_word};
use crate::song::lex::Class as Colour;

/// A call of a class: its methods, its arguments, whether the builder binds
/// them by name too (`freq: 440`), and what it is.
struct Def {
    class: &'static str,
    methods: &'static [&'static str],
    args: &'static [&'static str],
    named: bool,
    what: &'static str,
}

const AR_KR: &[&str] = &["ar", "kr"];

const fn def(
    class: &'static str,
    methods: &'static [&'static str],
    args: &'static [&'static str],
    what: &'static str,
) -> Def {
    Def {
        class,
        methods,
        args,
        named: true,
        what,
    }
}

/// The same, its arguments by position only.
const fn positional(
    class: &'static str,
    methods: &'static [&'static str],
    args: &'static [&'static str],
    what: &'static str,
) -> Def {
    Def {
        class,
        methods,
        args,
        named: false,
        what,
    }
}

/// The classes a SynthDef here takes (sound-design.md, "Modular code").
const DEFS: [Def; 58] = [
    positional(
        "SynthDef",
        &["new"],
        &["name", "ugenGraphFunc"],
        "a synth definition",
    ),
    def("SinOsc", AR_KR, &["freq", "phase", "mul", "add"], "a sine"),
    def("Saw", AR_KR, &["freq", "mul", "add"], "a band-limited saw"),
    def(
        "Pulse",
        AR_KR,
        &["freq", "width", "mul", "add"],
        "a band-limited pulse",
    ),
    def(
        "LFSaw",
        AR_KR,
        &["freq", "iphase", "mul", "add"],
        "a saw LFO",
    ),
    def(
        "LFTri",
        AR_KR,
        &["freq", "iphase", "mul", "add"],
        "a triangle LFO",
    ),
    def(
        "LFPulse",
        AR_KR,
        &["freq", "iphase", "width", "mul", "add"],
        "a pulse LFO",
    ),
    def("WhiteNoise", AR_KR, &["mul", "add"], "white noise"),
    def(
        "PMOsc",
        AR_KR,
        &["carfreq", "modfreq", "pmindex", "modphase", "mul", "add"],
        "a phase-modulated sine",
    ),
    def(
        "RLPF",
        AR_KR,
        &["in", "freq", "rq", "mul", "add"],
        "a resonant low-pass",
    ),
    def(
        "RHPF",
        AR_KR,
        &["in", "freq", "rq", "mul", "add"],
        "a resonant high-pass",
    ),
    def(
        "LPF",
        AR_KR,
        &["in", "freq", "mul", "add"],
        "a 12 dB low-pass",
    ),
    def(
        "HPF",
        AR_KR,
        &["in", "freq", "mul", "add"],
        "a 12 dB high-pass",
    ),
    def(
        "BPF",
        AR_KR,
        &["in", "freq", "rq", "mul", "add"],
        "a band-pass",
    ),
    def(
        "MoogFF",
        AR_KR,
        &["in", "freq", "gain", "reset", "mul", "add", "drive"],
        "a ladder low-pass",
    ),
    def(
        "CombN",
        AR_KR,
        &["in", "maxdelaytime", "delaytime", "decaytime", "mul", "add"],
        "a comb delay",
    ),
    def(
        "CombL",
        AR_KR,
        &["in", "maxdelaytime", "delaytime", "decaytime", "mul", "add"],
        "a comb delay, interpolated",
    ),
    def(
        "CombC",
        AR_KR,
        &["in", "maxdelaytime", "delaytime", "decaytime", "mul", "add"],
        "a comb delay, cubic",
    ),
    def(
        "DelayN",
        AR_KR,
        &["in", "maxdelaytime", "delaytime", "mul", "add"],
        "a delay line",
    ),
    def(
        "DelayL",
        AR_KR,
        &["in", "maxdelaytime", "delaytime", "mul", "add"],
        "a delay line, interpolated",
    ),
    def(
        "DelayC",
        AR_KR,
        &["in", "maxdelaytime", "delaytime", "mul", "add"],
        "a delay line, cubic",
    ),
    def(
        "Rand",
        &["new", "ir"],
        &["lo", "hi"],
        "a random number, once a note",
    ),
    def(
        "ExpRand",
        &["new", "ir"],
        &["lo", "hi"],
        "a random number, once a note, exponentially",
    ),
    def(
        "LinExp",
        AR_KR,
        &["in", "srclo", "srchi", "dstlo", "dsthi"],
        "a linear range onto an exponential one",
    ),
    def(
        "EnvGen",
        AR_KR,
        &[
            "envelope",
            "gate",
            "levelScale",
            "levelBias",
            "timeScale",
            "doneAction",
        ],
        "plays an envelope",
    ),
    positional(
        "Mix",
        &["new", "ar", "kr"],
        &["array"],
        "the sum of an array",
    ),
    positional(
        "Mix",
        &["fill"],
        &["n", "function"],
        "the sum of n of a function",
    ),
    def("Pan2", AR_KR, &["in", "pos", "level"], "mono to stereo"),
    def(
        "Splay",
        AR_KR,
        &["inArray", "spread", "level", "center", "levelComp"],
        "an array spread over stereo",
    ),
    def(
        "FreeVerb",
        &["ar"],
        &["in", "mix", "room", "damp", "mul", "add"],
        "a reverb",
    ),
    def(
        "FreeVerb2",
        &["ar"],
        &["in", "in2", "mix", "room", "damp", "mul", "add"],
        "a stereo reverb",
    ),
    def(
        "Select",
        AR_KR,
        &["which", "array"],
        "one of up to 8 choices",
    ),
    def(
        "Latch",
        AR_KR,
        &["in", "trig"],
        "holds its input from trigger to trigger",
    ),
    def(
        "Decimator",
        AR_KR,
        &["in", "rate", "bits", "mul", "add"],
        "fewer samples and bits",
    ),
    def("LFNoise0", AR_KR, &["freq", "mul", "add"], "random steps"),
    def("LFNoise1", AR_KR, &["freq", "mul", "add"], "random ramps"),
    def(
        "LeakDC",
        AR_KR,
        &["in", "coef", "mul", "add"],
        "takes away a DC offset",
    ),
    def(
        "Out",
        AR_KR,
        &["bus", "channelsArray"],
        "the voice's output",
    ),
    def(
        "Impulse",
        AR_KR,
        &["freq", "phase", "mul", "add"],
        "a click a cycle",
    ),
    def(
        "Ringz",
        AR_KR,
        &["in", "freq", "decaytime", "mul", "add"],
        "a resonator",
    ),
    def(
        "Klang",
        AR_KR,
        &["specificationsArrayRef", "freqscale", "freqoffset"],
        "a bank of sines",
    ),
    def(
        "DynKlang",
        AR_KR,
        &["specificationsArrayRef", "freqscale", "freqoffset"],
        "a bank of sines, of signals",
    ),
    def(
        "Klank",
        AR_KR,
        &[
            "specificationsArrayRef",
            "input",
            "freqscale",
            "freqoffset",
            "decayscale",
        ],
        "a bank of resonators",
    ),
    def(
        "DynKlank",
        AR_KR,
        &[
            "specificationsArrayRef",
            "input",
            "freqscale",
            "freqoffset",
            "decayscale",
        ],
        "a bank of resonators, of signals",
    ),
    def(
        "VOsc",
        AR_KR,
        &["bufpos", "freq", "phase", "mul", "add"],
        "the user wavetables, crossfaded",
    ),
    def(
        "Osc",
        AR_KR,
        &["bufnum", "freq", "phase", "mul", "add"],
        "a user wavetable's first wave",
    ),
    def(
        "SidOsc",
        AR_KR,
        &[
            "freq", "wave", "width", "chip", "sync", "ring", "mul", "add",
        ],
        "the SID's oscillator",
    ),
    def(
        "SidEnv",
        AR_KR,
        &[
            "gate", "attack", "decay", "sustain", "release", "chip", "mul", "add",
        ],
        "the SID's envelope",
    ),
    def(
        "SidFilter",
        AR_KR,
        &["in", "cutoff", "res", "mode", "chip", "bias", "mul", "add"],
        "the SID's filter",
    ),
    def(
        "NesPulse",
        AR_KR,
        &["freq", "duty", "volume", "mul", "add"],
        "the NES's pulse",
    ),
    def(
        "NesTri",
        AR_KR,
        &["freq", "mul", "add"],
        "the NES's triangle",
    ),
    def(
        "NesNoise",
        AR_KR,
        &["period", "short", "volume", "mul", "add"],
        "the NES's noise",
    ),
    def(
        "NesDmc",
        AR_KR,
        &["in", "rate", "direct", "mul", "add"],
        "the NES's DMC",
    ),
    def(
        "Env",
        &["new"],
        &["levels", "times", "curve", "releaseNode", "loopNode"],
        "an envelope",
    ),
    positional(
        "Env",
        &["adsr"],
        &["attackTime", "decayTime", "sustainLevel", "releaseTime"],
        "attack, decay, sustain, release",
    ),
    positional(
        "Env",
        &["perc"],
        &["attackTime", "releaseTime"],
        "attack and release",
    ),
    positional(
        "Env",
        &["asr"],
        &["attackTime", "sustainLevel", "releaseTime"],
        "attack, sustain, release",
    ),
    positional("Array", &["fill"], &["size", "function"], "n of a function"),
];

/// What a method is called on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum On {
    Signal,
    Number,
    Array,
    Function,
    Symbol,
}

/// The methods of values: name, how it is written, what it is called on.
const METHODS: [(&str, &str, On); 43] = [
    ("range", "range(lo, hi)", On::Signal),
    ("exprange", "exprange(lo, hi)", On::Signal),
    ("tanh", "tanh", On::Signal),
    ("softclip", "softclip", On::Signal),
    ("distort", "distort", On::Signal),
    ("atan", "atan", On::Signal),
    ("midiratio", "midiratio", On::Signal),
    ("midicps", "midicps", On::Signal),
    ("neg", "neg", On::Signal),
    ("round", "round(step)", On::Signal),
    ("lag", "lag(time)", On::Signal),
    ("abs", "abs", On::Signal),
    ("sqrt", "sqrt", On::Signal),
    ("squared", "squared", On::Signal),
    ("cubed", "cubed", On::Signal),
    ("min", "min(x)", On::Signal),
    ("max", "max(x)", On::Signal),
    ("pow", "pow(x)", On::Signal),
    ("clip", "clip(lo, hi)", On::Signal),
    ("clip2", "clip2(x)", On::Signal),
    ("reciprocal", "reciprocal", On::Signal),
    ("floor", "floor", On::Signal),
    ("ceil", "ceil", On::Signal),
    ("asInteger", "asInteger", On::Number),
    ("cpsmidi", "cpsmidi", On::Signal),
    ("ratiomidi", "ratiomidi", On::Signal),
    ("dbamp", "dbamp", On::Signal),
    ("ampdb", "ampdb", On::Signal),
    ("coin", "coin", On::Number),
    ("dup", "dup(n)", On::Signal),
    ("do", "do(function)", On::Array),
    ("collect", "collect(function)", On::Array),
    ("sum", "sum", On::Array),
    ("size", "size", On::Array),
    ("reverse", "reverse", On::Array),
    ("first", "first", On::Array),
    ("last", "last", On::Array),
    ("at", "at(index)", On::Array),
    ("product", "product", On::Array),
    ("choose", "choose", On::Array),
    ("value", "value(args)", On::Function),
    ("kr", "kr(default, lag)", On::Symbol),
    ("ar", "ar(default)", On::Symbol),
];

/// The language's words.
const KEYWORDS: [&str; 7] = ["var", "arg", "true", "false", "nil", "inf", "pi"];

/// The controls a SynthDef's arguments may be, other than knobs.
const CONTROLS: [(&str, &str); 4] = [
    ("freq", "the note's pitch"),
    ("gate", "the note's gate"),
    ("amp", "the velocity times its default"),
    ("modwheel", "the mod wheel, 0..1"),
];

/// The words that fit at `cursor` (UTF-16 units) in SuperCollider `code`.
pub fn complete(code: &str, cursor: u32) -> Completion {
    let cur = byte_at(code, cursor);
    let head = code.get(..cur).unwrap_or("");
    let typed: usize = head
        .chars()
        .rev()
        .take_while(|c| is_word(*c) && *c != '-')
        .map(char::len_utf8)
        .sum();
    let (pre, word) = head.split_at(head.len() - typed);
    let mut list = List {
        prefix: word,
        items: Vec::new(),
    };
    let scan = scan(pre);
    let mut help = String::new();
    if !scan.quiet {
        let call = scan.calls.last().map(|(name, _)| *name);
        help = call.and_then(usage).unwrap_or_default();
        words(&mut list, pre, word, call);
    }
    let to = head.encode_utf16().count() as u32;
    Completion {
        from: to - word.encode_utf16().count() as u32,
        to,
        items: list.items,
        help,
        preview: String::new(),
    }
}

/// What is open at the end of the code so far.
struct Scan<'a> {
    /// The calls open, outermost first: what is called (`SinOsc.ar`,
    /// `sig.range`, `Mix`) and where its `(` is; a `{` or `[` inside one
    /// stands between, as `None`.
    calls: Vec<(&'a str, usize)>,
    /// In a comment, a string or a `'symbol'`: nothing to offer.
    quiet: bool,
}

fn scan(pre: &str) -> Scan<'_> {
    let mut open: Vec<Option<(&str, usize)>> = Vec::new();
    let mut chars = pre.char_indices().peekable();
    let mut quiet = false;
    while let Some((i, c)) = chars.next() {
        match c {
            '/' if chars.peek().is_some_and(|(_, d)| *d == '/') => {
                quiet = true;
                for (_, d) in chars.by_ref() {
                    if d == '\n' {
                        quiet = false;
                        break;
                    }
                }
            }
            '/' if chars.peek().is_some_and(|(_, d)| *d == '*') => {
                quiet = true;
                let mut star = false;
                for (_, d) in chars.by_ref() {
                    if star && d == '/' {
                        quiet = false;
                        break;
                    }
                    star = d == '*';
                }
            }
            '"' | '\'' => {
                quiet = true;
                for (_, d) in chars.by_ref() {
                    if d == c {
                        quiet = false;
                        break;
                    }
                }
            }
            '(' => {
                let head = pre.get(..i).unwrap_or("");
                let n: usize = head
                    .chars()
                    .rev()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
                    .map(char::len_utf8)
                    .sum();
                open.push(Some((head.get(head.len() - n..).unwrap_or(""), i)));
            }
            '[' | '{' => open.push(None),
            ')' | ']' | '}' => {
                open.pop();
            }
            _ => {}
        }
    }
    // Only the calls not behind a bracket or a function count.
    let calls = open
        .iter()
        .rev()
        .map_while(|o| *o)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Scan { calls, quiet }
}

/// The row of `class` with `method`; `Mix(…)` is `Mix.new`.
fn def_of(call: &str) -> Option<(&'static Def, &str)> {
    let (class, method) = call.rsplit_once('.').unwrap_or((call, "new"));
    DEFS.iter()
        .find(|d| d.class == class && d.methods.contains(&method))
        .map(|d| (d, method))
}

/// How `call` is written: `RLPF.ar(in, freq, rq, mul, add)`, or a value's
/// method's `range(lo, hi)`.
fn usage(call: &str) -> Option<String> {
    if let Some((d, method)) = def_of(call) {
        let dot = if call.contains('.') {
            format!(".{method}")
        } else {
            String::new()
        };
        return Some(format!(
            "{}{dot}({}) — {}",
            d.class,
            d.args.join(", "),
            d.what
        ));
    }
    let method = call.rsplit_once('.').map(|(_, m)| m)?;
    METHODS
        .iter()
        .find(|(m, w, _)| *m == method && w.contains('('))
        .map(|(_, w, _)| (*w).to_string())
}

/// The words for the place before `word`.
fn words(list: &mut List, pre: &str, word: &str, call: Option<&str>) {
    match pre.chars().last() {
        Some('.') => {
            let at = pre.len() - 1;
            let receiver: String = pre
                .get(..at)
                .unwrap_or("")
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '\\')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if receiver.starts_with(|c: char| c.is_ascii_uppercase()) {
                for d in DEFS.iter().filter(|d| d.class == receiver) {
                    for m in d.methods {
                        list.add(Colour::Name, m, || d.what.to_string());
                    }
                }
            } else if receiver.starts_with('\\') {
                methods(list, Some(On::Symbol));
            } else if receiver.starts_with(|c: char| c.is_ascii_digit()) {
                methods(list, Some(On::Number));
                methods(list, Some(On::Signal));
            } else {
                methods(list, None);
            }
        }
        Some('\\') => {
            let before = pre.get(..pre.len() - 1).unwrap_or("").trim_end();
            let class = call.and_then(|c| c.split('.').next());
            if let (true, Some(c)) = (before.ends_with("voicing:"), class) {
                let class = Class::of(c);
                for (v, k, m, _) in VOICINGS {
                    if k == class {
                        list.add(Colour::Param, v, || format!("as the {m:?}'s filter"));
                    }
                }
            }
        }
        _ if word.starts_with(|c: char| c.is_ascii_uppercase()) => {
            for d in &DEFS {
                list.add(Colour::Call, d.class, || d.what.to_string());
            }
        }
        _ => {
            // The code's own names first: an argument is more often a
            // variable than a keyword argument.
            for (n, what) in declared(pre) {
                list.add(Colour::Name, n, || what.to_string());
            }
            if let Some((d, _)) = call.and_then(def_of).filter(|(d, _)| d.named) {
                for a in d.args {
                    list.add(Colour::Param, &format!("{a}:"), || {
                        format!("{}'s {a}", d.class)
                    });
                }
            }
            for k in KEYWORDS {
                list.add(Colour::Keyword, k, || "the language's".to_string());
            }
        }
    }
}

fn methods(list: &mut List, on: Option<On>) {
    for (m, written, o) in METHODS {
        if on.is_none_or(|on| on == o) && (on.is_some() || o != On::Symbol) {
            list.add(Colour::Name, m, || written.to_string());
        }
    }
}

/// The arguments (`|freq = 440, cutoff|`, `arg a, b;`) and variables (`var
/// sig = …;`) of the code before the cursor, and what each is.
fn declared(pre: &str) -> Vec<(&str, &'static str)> {
    let mut out: Vec<(&str, &'static str)> = Vec::new();
    let ident = |part: &str| -> Option<usize> {
        let t = part.trim_start();
        let n = t
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(t.len());
        (n > 0 && t.starts_with(|c: char| c.is_ascii_lowercase())).then_some(n)
    };
    // `|a = 1, b|` lists.
    let mut rest = pre;
    while let Some(i) = rest.find('|') {
        let after = rest.get(i + 1..).unwrap_or("");
        let Some(j) = after.find('|') else {
            break;
        };
        for part in after.get(..j).unwrap_or("").split(',') {
            let t = part.trim_start();
            if let Some(n) = ident(t) {
                let name = t.get(..n).unwrap_or("");
                let what = CONTROLS
                    .iter()
                    .find(|(c, _)| *c == name)
                    .map_or("an argument: a knob", |(_, w)| *w);
                out.push((name, what));
            }
        }
        rest = after.get(j + 1..).unwrap_or("");
    }
    // `var a, b = 2;` and `arg a, b;`.
    for (kw, what) in [("var", "a variable"), ("arg", "an argument: a knob")] {
        let mut rest = pre;
        while let Some(i) = rest.find(kw) {
            let before_ok = rest
                .get(..i)
                .and_then(|b| b.chars().last())
                .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
            let after = rest.get(i + kw.len()..).unwrap_or("");
            rest = after;
            if !before_ok || !after.starts_with(char::is_whitespace) {
                continue;
            }
            let stmt = after.split(';').next().unwrap_or("");
            for part in stmt.split(',') {
                let t = part.trim_start();
                if let Some(n) = ident(t) {
                    out.push((t.get(..n).unwrap_or(""), what));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The words offered with the cursor at `|`.
    fn at(text: &str) -> Vec<String> {
        // The cursor is the last `|`: a SynthDef's arguments are between two.
        let (a, b) = text.rsplit_once('|').expect("a cursor");
        let c = complete(&format!("{a}{b}"), a.encode_utf16().count() as u32);
        c.items.into_iter().map(|i| i.word).collect()
    }

    fn help(text: &str) -> String {
        // The cursor is the last `|`: a SynthDef's arguments are between two.
        let (a, b) = text.rsplit_once('|').expect("a cursor");
        complete(&format!("{a}{b}"), a.encode_utf16().count() as u32).help
    }

    const HEAD: &str = "SynthDef(\\acid, { |freq = 110, cutoff = 800, gate = 1|\n    var sig = Saw.ar(freq);\n    ";

    #[test]
    fn classes_methods_and_arguments() {
        assert_eq!(at("Sin|"), ["SinOsc"]);
        assert_eq!(at("Nes|"), ["NesPulse", "NesTri", "NesNoise", "NesDmc"]);
        assert_eq!(at("SinOsc.|"), ["ar", "kr"]);
        assert_eq!(at("FreeVerb.|"), ["ar"]);
        assert_eq!(at("Env.|"), ["new", "adsr", "perc", "asr"]);
        assert_eq!(
            at(&format!("{HEAD}RLPF.ar(sig, |")),
            [
                "freq", "cutoff", "gate", "sig", "in:", "freq:", "rq:", "mul:", "add:", "var",
                "arg", "true", "false", "nil", "inf", "pi"
            ]
        );
        assert_eq!(at(&format!("{HEAD}RLPF.ar(sig, cu|")), ["cutoff"]);
        // Positional only: no names to give.
        assert!(!at(&format!("{HEAD}Env.adsr(|")).contains(&"attackTime:".to_string()));
    }

    #[test]
    fn the_methods_of_values() {
        // A variable may hold a signal or a number: both kinds' methods.
        assert_eq!(at(&format!("{HEAD}sig.ra|")), ["range", "ratiomidi"]);
        assert_eq!(at(&format!("{HEAD}SinOsc.kr(3).exp|")), ["exprange"]);
        assert_eq!(at(&format!("{HEAD}[1, 2].col|")), ["collect"]);
        assert_eq!(at("\\cutoff.|"), ["kr", "ar"]);
        assert_eq!(at("60.mid|"), ["midiratio", "midicps"]);
        assert_eq!(at("0.5.co|"), ["coin"]);
    }

    #[test]
    fn a_filter_takes_its_voicings() {
        let lp = at(&format!("{HEAD}RLPF.ar(sig, 800, 0.3, voicing: \\|"));
        assert_eq!(
            lp,
            [
                "ms20",
                "cs15",
                "polymoog",
                "jupiter8",
                "matrix12",
                "odysseyrev1"
            ]
        );
        let ladder = at(&format!("{HEAD}MoogFF.ar(sig, voicing: \\mini|"));
        assert_eq!(ladder, ["minimoog"]);
        assert!(at(&format!("{HEAD}SinOsc.ar(\\|")).is_empty());
    }

    #[test]
    fn the_codes_own_names_and_its_controls() {
        let c = complete(&format!("{HEAD}fr"), (HEAD.len() + 2) as u32);
        assert_eq!(c.items.len(), 1);
        assert_eq!(c.items[0].word, "freq");
        assert_eq!(c.items[0].detail, "the note's pitch");
        assert_eq!(at(&format!("{HEAD}s|")), ["sig"]);
        assert_eq!(at("arg a, bb;\nb|"), ["bb"]);
        // Not a variable: `variable` holds `var` but is not one.
        assert_eq!(at("variable = 1; v|"), ["var"]);
    }

    #[test]
    fn a_call_shows_how_it_is_written() {
        assert_eq!(
            help(&format!("{HEAD}RLPF.ar(sig, |")),
            "RLPF.ar(in, freq, rq, mul, add) — a resonant low-pass"
        );
        assert_eq!(
            help(&format!("{HEAD}Mix(|")),
            "Mix(array) — the sum of an array"
        );
        assert_eq!(help(&format!("{HEAD}sig.range(|")), "range(lo, hi)");
        // In a function given to a call, that call is not the one being written.
        assert_eq!(help(&format!("{HEAD}Mix.fill(4, {{ |i| |")), "");
        assert_eq!(
            help(&format!("{HEAD}Mix.fill(4, {{ |i| SinOsc.ar(|")),
            "SinOsc.ar(freq, phase, mul, add) — a sine"
        );
        // In the SynthDef's body, not in `SynthDef(`'s arguments.
        assert_eq!(complete(HEAD, HEAD.encode_utf16().count() as u32).help, "");
    }

    #[test]
    fn nothing_in_comments_and_strings() {
        assert!(at("// Sin|").is_empty());
        assert!(at("/* Sin|").is_empty());
        assert_eq!(at("/* x */ Sin|"), ["SinOsc"]);
        assert!(at("\"Sin|").is_empty());
    }

    /// What the tables offer builds: no class, method or argument name
    /// here is one the builder does not know.
    #[test]
    fn the_tables_are_the_builders() {
        let unknown = [
            "this UGen is not part of a SynthDef here",
            "this method is not part of a SynthDef here",
            "there is no argument by this name",
        ];
        let build = |body: &str| {
            let code = format!("SynthDef(\\t, {{ |freq = 440, gate = 1| {body} }}).add;");
            super::super::sc::compile(&code).err().map(|e| e.msg)
        };
        for d in &DEFS {
            for m in d.methods {
                let call = format!("{}.{m}()", d.class);
                let e = build(&format!("var x = {call}; SinOsc.ar(freq)"));
                assert!(e.is_none_or(|e| !unknown.contains(&e)), "{call}: {e:?}");
                for a in d.args.iter().filter(|_| d.named) {
                    let call = format!("{}.{m}({a}: 1)", d.class);
                    let e = build(&format!("var x = {call}; SinOsc.ar(freq)"));
                    assert!(e.is_none_or(|e| !unknown.contains(&e)), "{call}: {e:?}");
                }
            }
        }
        for (m, _, on) in METHODS {
            let value = match on {
                On::Signal => "SinOsc.ar(freq)",
                On::Number => "3",
                On::Array => "[1, 2]",
                On::Function => "{ |a| a }",
                On::Symbol => "\\cut",
            };
            let e = build(&format!("var x = {value}.{m}; SinOsc.ar(freq)"));
            assert!(
                e.is_none_or(|e| !unknown.contains(&e)),
                "{value}.{m}: {e:?}"
            );
        }
    }

    /// Every UGen the sound designer's definition lists is offered.
    #[test]
    fn every_documented_ugen_is_offered() {
        let doc = include_str!("../../../../.openspec/sound-design.md");
        let (_, rest) = doc
            .split_once("- **UGens** (`.ar`/`.kr`): `")
            .expect("the UGens");
        let (list, _) = rest.split_once('`').expect("their end");
        for u in list.split_whitespace() {
            assert!(DEFS.iter().any(|d| d.class == u), "{u}");
        }
    }

    #[test]
    fn any_text_completes_in_bounds() {
        let alphabet: Vec<char> = "aSx .(){}[],|\\:=;/*\"'\n1é".chars().collect();
        let mut seed: u32 = 11;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 16) as usize
        };
        for _ in 0..300 {
            let text: String = (0..next() % 50)
                .map(|_| alphabet[next() % alphabet.len()])
                .collect();
            let units = text.encode_utf16().count() as u32;
            for cursor in 0..=units {
                let c = complete(&text, cursor);
                assert!(c.from <= c.to && c.to <= units, "{text:?} at {cursor}");
            }
        }
    }
}
