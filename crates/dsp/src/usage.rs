//! How each call of the song language is written (#595, #644): one line per
//! generator call, pattern method and signal function, `<name> takes …:
//! <example>`. The parser's errors that state a call's usage read it here,
//! and so does the editor's argument help, so the two never differ.

pub const ARP: &str = "arp takes a chord, a mode (up:2 for two octaves), a rate and for random a seed: arp([c4,e4,g4],up,16)";
pub const WALK: &str = "walk takes a start note, a count and a seed: walk(c4,8,1)";
pub const MARKOV: &str = "markov takes an order, a clip and a seed: markov(1,riff,3)";
pub const MUTATE: &str = "mutate takes a clip, a percent and a seed: mutate(riff,30,5)";
pub const ROOT: &str = "root takes a clip and maybe an octave: root(prog) or root(prog,3)";
pub const BASS: &str = "bass takes a clip, a style, maybe an octave and for walk a seed: bass(prog,fifths) or bass(prog,walk,2,5)";
pub const PROG: &str = "prog takes a number of bars, a seed and maybe a size, beats, a style and colours: prog(4,7) or prog(4,7,7th,\"4 2 2\",jazz,borrow secondary)";
pub const EUCLID: &str =
    "euclid takes hits, steps and maybe a rotation: euclid(3,8) or euclid(3,8,2)";

pub const EVERY: &str = "every takes a number and a method: every(4, rev)";
pub const OFF: &str = "off takes a time and a method: off(1/8, add(12))";
pub const SCALE: &str =
    "scale takes a root or an earlier clip, and a mode: scale(a1 minor) or scale(harm minor)";
pub const CHORD: &str = "chord takes an earlier clip: chord(harm)";
pub const VOICING: &str =
    "voicing takes a style and maybe an anchor note: voicing(drop2) or voicing(drop2,c5)";
pub const SOMETIMES_BY: &str =
    "sometimesBy takes a chance from 0 to 1 and a method: sometimesBy(0.3, add(12))";
pub const ARP_METHOD: &str =
    "arp takes chord indexes in quotes, 0 the lowest note: arp(\"0 3 <2 0> [1 3]\")";
pub const SLICE: &str = "slice takes a number of parts from 1 to 64 and indexes in quotes: slice(8, \"0 1 <2 3> [6 7]\")";
pub const SPLICE: &str = "splice takes a number of parts from 1 to 64 and indexes in quotes: splice(8, \"0 1 <2 3> [6 7]\")";
pub const VELOCITY: &str = "velocity takes a velocity from 0 to 1, one a step or a signal: velocity(0.5), velocity(\"1 0.5 ~ 0.7\") or velocity(sine.range(0.4, 1).segment(16))";
pub const CLIP: &str = "clip takes a share of the note's length, more than 0 up to 16, one a step or a signal: clip(0.5), clip(\"0.3 1 ~ 2\") or clip(perlin.range(0.2, 1))";
pub const ROT: &str = "rot takes how many onsets each takes its notes from, -64 to 64, or one a bar: rot(1) or rot(\"<0 1 2 3>\")";
pub const MIRROR: &str =
    "mirror takes a degree from -24 to 24, a note, or nothing: mirror(2), mirror(c4) or mirror()";
pub const ECHO_WITH: &str = "echoWith takes a number of layers from 1 to 16, a time of a bar and a method: echoWith(3, 1/8, add(2))";
pub const STUT_WITH: &str = "stutWith takes a number of layers from 1 to 16, a time of a bar and a method: stutWith(3, 1/8, add(2))";
pub const PLY_WITH: &str =
    "plyWith takes a number of repeats from 1 to 16 and a method: plyWith(3, add(12))";
pub const SUPERIMPOSE: &str =
    "superimpose takes a method for the copy on top: superimpose(add(12))";
pub const LAYER: &str =
    "layer takes 1 to 8 methods, one a copy, id the line as is: layer(add(7), rev)";
pub const INSIDE: &str =
    "inside takes a ratio and a method, the method on the line that much slower: inside(2, rev)";
pub const OUTSIDE: &str =
    "outside takes a ratio and a method, the method on the line that much faster: outside(2, rev)";
pub const LINGER: &str = "linger takes a part of a bar, more than 0 up to 1, or one a bar: linger(1/4) or linger(\"<1 0.25>\")";
pub const ZOOM: &str =
    "zoom takes where in the bar to start and end, 0 to 1, the start first: zoom(0.25, 0.75)";
pub const COMPRESS: &str =
    "compress takes where in the bar to start and end, 0 to 1, the start first: compress(0, 0.5)";
pub const HURRY: &str = "hurry takes a ratio, each side 1 to 16: hurry(2)";
pub const ITER_BACK: &str = "iterBack takes a whole number from 1 to 16: iterBack(4)";
pub const MASK: &str = "mask takes 1 to keep and 0 to silence, one a bar or a part of one: mask(\"<1 0 1 1>\") or mask(\"1 0 1 1\")";
pub const CHUNK: &str =
    "chunk takes a number of parts from 1 to 16 and a method: chunk(4, add(12))";
pub const WHENMOD: &str = "whenmod takes a cycle of bars from 1 to 16, the bar from which on, below it, and a method: whenmod(8, 6, rev)";
pub const SEGMENT: &str = "segment takes a number of steps from 1 to 48: segment(4)";
pub const STUT: &str = "stut takes a number of copies from 1 to 16, a gain from 0 to 1 and a time of a bar: stut(4, 0.25, 0.05)";
pub const ECHO: &str = "echo takes a number of copies from 1 to 16, a time of a bar and a gain from 0 to 1: echo(4, 0.05, 0.25)";
pub const SPREAD: &str = "spread takes 2 to 8 methods, one a bar in turn, id the bar as it is: spread(id, rev(), chop(16))";
pub const RIB: &str = "rib takes a bar to start at and a number of bars, looped: rib(46, 1)";
pub const RIBBON: &str =
    "ribbon takes a bar to start at and a number of bars, looped: ribbon(46, 1)";
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
pub const METHODS: [(&str, &str); 60] = [
    (
        "fast",
        "fast takes a ratio, each side 1 to 16, or one a bar: fast(2), fast(3/4) or fast(\"<1 2>\")",
    ),
    (
        "slow",
        "slow takes a ratio, each side 1 to 16, or one a bar: slow(2), slow(1.5) or slow(\"<1 2>\")",
    ),
    ("rev", "rev takes no value: rev()"),
    ("palindrome", "palindrome takes no value: palindrome()"),
    (
        "add",
        "add takes semitones from -48 to 48, a list for a chord, one a bar or a signal: add(7), add(0.5), add([0,4,7]), add(\"<0 3 5>\") or add(perlin.range(0, 0.5))",
    ),
    (
        "sub",
        "sub takes semitones from -48 to 48, one a bar or a signal: sub(12), sub(0.5) or sub(\"<0 12>\")",
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
    ("often", "often takes a method: often(add(12))"),
    ("rarely", "rarely takes a method: rarely(add(12))"),
    (
        "almostNever",
        "almostNever takes a method: almostNever(ply(2))",
    ),
    (
        "almostAlways",
        "almostAlways takes a method: almostAlways(add(12))",
    ),
    ("sometimesBy", SOMETIMES_BY),
    ("scale", SCALE),
    ("oct", "oct takes octaves from -4 to 4: oct(1)"),
    ("chord", CHORD),
    (
        "strum",
        "strum takes milliseconds from 1 to 100, below 0 to strum down: strum(15)",
    ),
    ("arp", ARP_METHOD),
    ("jux", "jux takes a method: jux(rev)"),
    (
        "shuffle",
        "shuffle takes a whole number from 1 to 16: shuffle(4)",
    ),
    (
        "scramble",
        "scramble takes a whole number from 1 to 16: scramble(4)",
    ),
    (
        "chop",
        "chop takes a number of parts from 1 to 64, or one a bar: chop(16) or chop(\"<8 16>\")",
    ),
    ("slice", SLICE),
    ("fit", "fit takes no value: fit()"),
    ("splice", SPLICE),
    (
        "speed",
        "speed takes a speed from -32 to 32, -1 backwards, or one a step: speed(2), speed(-1) or speed(\"1 2 -1\")",
    ),
    (
        "accelerate",
        "accelerate takes a speed from -32 to 32 to add by the note's end, or one a step: accelerate(1) or accelerate(\"1 2\")",
    ),
    (
        "coarse",
        "coarse takes a whole number from 1 to 64, or one a step: coarse(8) or coarse(\"16 32 24\")",
    ),
    ("mul", "mul takes a speed: mul(speed(-1))"),
    ("stut", STUT),
    ("echo", ECHO),
    ("spread", SPREAD),
    ("rib", RIB),
    ("ribbon", RIBBON),
    ("velocity", VELOCITY),
    ("clip", CLIP),
    ("rot", ROT),
    ("mirror", MIRROR),
    ("echoWith", ECHO_WITH),
    ("stutWith", STUT_WITH),
    ("plyWith", PLY_WITH),
    ("superimpose", SUPERIMPOSE),
    ("layer", LAYER),
    ("inside", INSIDE),
    ("outside", OUTSIDE),
    ("linger", LINGER),
    ("zoom", ZOOM),
    ("compress", COMPRESS),
    ("hurry", HURRY),
    ("iterBack", ITER_BACK),
    ("mask", MASK),
    ("chunk", CHUNK),
    ("whenmod", WHENMOD),
    ("segment", SEGMENT),
];

/// The signal sources that take arguments, and the signal methods.
pub const SIGNAL: [(&str, &str); 15] = [
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
    ("seg", "seg takes a number above 0, values a bar: seg(16)"),
    ("lag", "lag takes a number above 0, seconds: lag(0.1)"),
    ("add", "add takes a number: add(1)"),
    ("sub", "sub takes a number: sub(7)"),
    ("mul", "mul takes a number: mul(8)"),
    ("div", "div takes a number: div(2)"),
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
