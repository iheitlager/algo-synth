//! How each call of the song language is written (#595, #644): one line per
//! generator call, pattern method and signal function, `<name> takes …:
//! <example>`. The parser's errors that state a call's usage read it here,
//! and so does the editor's argument help, so the two never differ.

pub const ARP: &str =
    "arp takes a chord, a mode, a rate and for random a seed: arp([c4,e4,g4],up,16)";
pub const WALK: &str = "walk takes a start note, a count and a seed: walk(c4,8,1)";
pub const MARKOV: &str = "markov takes an order, a clip and a seed: markov(1,riff,3)";
pub const MUTATE: &str = "mutate takes a clip, a percent and a seed: mutate(riff,30,5)";
pub const ROOT: &str = "root takes a clip and maybe an octave: root(prog) or root(prog,3)";
pub const BASS: &str = "bass takes a clip, a style, maybe an octave and for walk a seed: bass(prog,fifths) or bass(prog,walk,2,5)";
pub const PROG: &str = "prog takes a number of bars, a seed and maybe a size, beats and a style: prog(4,7) or prog(4,7,7th,\"4 2 2\",jazz)";
pub const EUCLID: &str =
    "euclid takes hits, steps and maybe a rotation: euclid(3,8) or euclid(3,8,2)";

pub const EVERY: &str = "every takes a number and a method: every(4, rev)";
pub const OFF: &str = "off takes a time and a method: off(1/8, add(12))";
pub const SCALE: &str =
    "scale takes a root or an earlier clip, and a mode: scale(a1 minor) or scale(harm minor)";
pub const CHORD: &str = "chord takes an earlier clip: chord(harm)";
pub const VOICING: &str =
    "voicing takes a style and maybe an anchor note: voicing(drop2) or voicing(drop2,c5)";
pub const STRUCT: &str = "struct takes a rhythm of x and ~ in quotes or an earlier clip: struct(\"x ~ x x\") or struct(beat)";

/// The calls of a line of notes, and `euclid` on a lane.
pub const CALLS: [(&str, &str); 8] = [
    ("arp", ARP),
    ("walk", WALK),
    ("markov", MARKOV),
    ("mutate", MUTATE),
    ("root", ROOT),
    ("prog", PROG),
    ("bass", BASS),
    ("euclid", EUCLID),
];

/// The pattern methods, in `Pattern::NAMES` order.
pub const METHODS: [(&str, &str); 18] = [
    (
        "fast",
        "fast takes a whole number from 1 to 16, or one a bar: fast(2) or fast(\"<1 2>\")",
    ),
    (
        "slow",
        "slow takes a whole number from 1 to 16, or one a bar: slow(2) or slow(\"<1 2>\")",
    ),
    ("rev", "rev takes no value: rev()"),
    ("palindrome", "palindrome takes no value: palindrome()"),
    (
        "add",
        "add takes semitones from -48 to 48, a list for a chord, or one a bar: add(7), add([0,4,7]) or add(\"<0 3 5>\")",
    ),
    (
        "sub",
        "sub takes semitones from -48 to 48, or one a bar: sub(12) or sub(\"<0 12>\")",
    ),
    (
        "ply",
        "ply takes a whole number from 1 to 16, or one a bar: ply(2) or ply(\"<1 2>\")",
    ),
    ("iter", "iter takes a whole number from 1 to 16: iter(4)"),
    (
        "degrade",
        "degrade takes maybe a chance from 0 to 1, or one a bar: degrade(0.2) or degrade(\"<0 0.5>\")",
    ),
    ("every", EVERY),
    ("off", OFF),
    ("struct", STRUCT),
    ("euclid", EUCLID),
    ("sometimes", "sometimes takes a method: sometimes(add(12))"),
    ("scale", SCALE),
    ("oct", "oct takes octaves from -4 to 4: oct(1)"),
    ("chord", CHORD),
    (
        "strum",
        "strum takes milliseconds from 1 to 100, below 0 to strum down: strum(15)",
    ),
];

/// The signal sources that take arguments, and the signal methods.
pub const SIGNAL: [(&str, &str); 10] = [
    (
        "lfo",
        "lfo takes a rate in Hz and maybe a shape: lfo(3) or lfo(3, saw)",
    ),
    ("irand", "irand takes a whole number from 1 to 64: irand(8)"),
    (
        "brandBy",
        "brandBy takes a chance from 0 to 1: brandBy(0.3)",
    ),
    (
        "env",
        "env takes adsr, perc or four times a, d, s, r: env(perc) or env(0.01, 0.2, 0.5, 0.3)",
    ),
    (
        "range",
        "range takes two numbers, low and high: range(300, 3000)",
    ),
    (
        "exprange",
        "exprange takes two numbers of one sign, low and high: exprange(100, 4000)",
    ),
    ("slow", "slow takes a number above 0: slow(4)"),
    ("fast", "fast takes a number above 0: fast(2)"),
    (
        "segment",
        "segment takes a number above 0, values a bar: segment(4)",
    ),
    ("lag", "lag takes a number above 0, seconds: lag(0.1)"),
];

/// How `name` is written, in `table`.
pub fn of(table: &[(&str, &'static str)], name: &str) -> Option<&'static str> {
    table.iter().find(|(n, _)| *n == name).map(|(_, u)| *u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::pattern::Pattern;
    use crate::song::signal;

    /// Each line starts with its name and gives an example of it.
    #[test]
    fn each_usage_names_its_call_and_shows_it() {
        let voicing = [("voicing", VOICING)];
        for (name, usage) in CALLS.iter().chain(&METHODS).chain(&SIGNAL).chain(&voicing) {
            assert!(usage.starts_with(&format!("{name} takes ")), "{usage}");
            let (_, example) = usage.split_once(": ").expect("an example");
            assert!(example.starts_with(&format!("{name}(")), "{usage}");
        }
    }

    #[test]
    fn the_tables_cover_the_language() {
        fn names(t: &[(&'static str, &str)]) -> Vec<&'static str> {
            t.iter().map(|e| e.0).collect()
        }
        assert_eq!(names(&CALLS)[..7], crate::notes::CALLS);
        assert_eq!(names(&METHODS), Pattern::NAMES);
        for (n, _) in SIGNAL {
            assert!(
                signal::SOURCES.contains(&n) || signal::METHODS.contains(&n),
                "{n}"
            );
        }
    }
}
