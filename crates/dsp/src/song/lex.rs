//! The song text as coloured spans, for the editor (#203): the engine knows
//! the notation, so the view only paints what this returns (ADR-0001).
//!
//! The lexer reads a line at a time with the little context the parser has:
//! which keyword starts the line, and the kind of the clip an indented line
//! belongs to (a drum lane, or a line of notes). It does not check the song:
//! a wrong word is still coloured by its shape, and errors come from `parse`.
//! It is total, like the parser: any text gives spans, never a panic.
//!
//! Spans count UTF-16 units, as the view indexes its strings; they are in
//! order, never overlap, and leave whitespace and unknown text uncoloured.

/// What a span is, as the view colours it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Class {
    /// `tempo`, `clip`, `scene`…, and `live`, `bars`, `ramp`, track kinds.
    Keyword = 1,
    /// A track, clip, scene, auto or snapshot, and words like `up` or `minor`.
    Name = 2,
    Number = 3,
    /// A pitch on a line of notes: `c4`, `f#3`, `bb2`.
    Note = 4,
    /// The pad a lane plays: `bd`, `sn`.
    Pad = 5,
    /// A hit in a lane: `x` or `X`.
    Step = 6,
    /// A rest: `.` in a lane, `~` or `r` in notes.
    Rest = 7,
    /// A generator: `euclid`, `arp`, `walk`, `markov`, `mutate`.
    Call = 8,
    /// A `target.Param` in an auto or a snapshot.
    Param = 9,
    /// Notation: `= : " [ ] < > ( ) , * @ ? ! /`.
    Punct = 10,
    Comment = 11,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub len: u32,
    pub class: Class,
}

use super::KEYWORDS;

/// The words after a line's keyword that are coloured as keywords.
pub(crate) const WORDS: [&str; 7] = [
    "live", "bars", "ramp", "drums", "synth", "sampler", "voicing",
];

/// What the indented lines under the open clip hold.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Under {
    Nothing,
    Lanes,
    Notes,
    /// A sampler clip: lanes, or notes when written as notes.
    Either,
    /// A Modular setting's SuperCollider code (ADR-0024).
    Code,
}

/// The spans of `text`.
pub fn lex(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    // Track names and kinds as declared so far, and the open clip's kind.
    let mut tracks: Vec<(&str, &str)> = Vec::new();
    let mut under = Under::Nothing;
    let mut at: u32 = 0;
    for raw in text.split('\n') {
        let line: Vec<char> = raw.strip_suffix('\r').unwrap_or(raw).chars().collect();
        // The UTF-16 offset of each char of the line, and one past the end.
        let mut pos = Vec::with_capacity(line.len() + 1);
        let mut p = at;
        for c in &line {
            pos.push(p);
            p += c.len_utf16() as u32;
        }
        pos.push(p);
        let mut l = Line {
            chars: &line,
            pos: &pos,
            out: &mut out,
        };
        let indented = line.first().is_some_and(|c| *c == ' ' || *c == '\t');
        // Code has no song comments: a `#` there is SuperCollider's.
        let body = if indented && under == Under::Code {
            line.len()
        } else {
            comment_at(&line)
        };
        let words = words(line.get(..body).unwrap_or(&[]));
        if let Some(&(s, e)) = words.first() {
            let first: String = line.get(s..e).unwrap_or(&[]).iter().collect();
            if indented {
                let notes = match under {
                    Under::Notes => true,
                    Under::Either => first.starts_with(['"', '[']) || first.contains(':'),
                    _ => false,
                };
                if under == Under::Code {
                    l.tokens(s, body, false);
                } else if notes {
                    l.tokens(s, body, true);
                } else {
                    l.push(s, e, Class::Pad);
                    l.lane(e, body);
                }
            } else {
                under = Under::Nothing;
                let word = |k: usize| {
                    words
                        .get(k)
                        .and_then(|&(s, e)| raw.get(byte(raw, s)..byte(raw, e)))
                };
                let kw = super::current_keyword(&first, word(1), word(2));
                match kw {
                    "track" => {
                        if let (Some(n), Some(k)) = (word(1), word(2)) {
                            tracks.push((n, k));
                        }
                    }
                    "setting" if word(3) == Some("Modular") => under = Under::Code,
                    "clip" => {
                        let kind = word(3)
                            .and_then(|t| tracks.iter().rev().find(|(n, _)| *n == t))
                            .map(|(_, k)| *k);
                        under = match kind {
                            Some("drums") => Under::Lanes,
                            Some("synth") => Under::Notes,
                            Some("sampler") => Under::Either,
                            _ => Under::Nothing,
                        };
                    }
                    _ => {}
                }
                // `master:` is the keyword and its colon, as the parser reads it.
                let e = if kw == "master:" { e - 1 } else { e };
                if KEYWORDS.contains(&kw.trim_end_matches(':')) {
                    l.push(s, e, Class::Keyword);
                    l.tokens(e, body, false);
                } else {
                    l.tokens(s, body, false);
                }
            }
        }
        if body < line.len() {
            l.push(body, line.len(), Class::Comment);
        }
        // Past the line as written, its `\r` too, and the `\n`.
        at = p + u32::from(raw.ends_with('\r')) + 1;
    }
    out
}

/// The byte offset of char `k` of `s`.
fn byte(s: &str, k: usize) -> usize {
    s.char_indices().nth(k).map_or(s.len(), |(b, _)| b)
}

/// Where the comment starts, or the line's length: a `#` at the start or after
/// a space, as the parser has it, so `c#4` stays a note.
fn comment_at(line: &[char]) -> usize {
    let mut prev_space = true;
    for (i, c) in line.iter().enumerate() {
        if *c == '#' && prev_space {
            return i;
        }
        prev_space = c.is_whitespace();
    }
    line.len()
}

/// The words of a line as char ranges.
fn words(line: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in line.iter().enumerate() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                out.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, line.len()));
    }
    out
}

/// `c4`, `f#3`, `bb2`, `c-1`: a letter a–g, maybe `#` or `b`, an octave.
fn is_note(w: &[char]) -> bool {
    let rest = match w {
        [l, r @ ..] if ('a'..='g').contains(l) => r,
        _ => return false,
    };
    let rest = match rest {
        ['#' | 'b', r @ ..] if !r.is_empty() => r,
        r => r,
    };
    let rest = rest.strip_prefix(&['-']).unwrap_or(rest);
    !rest.is_empty() && rest.iter().all(char::is_ascii_digit)
}

/// `c`, `bb`, `f#3`: a chord's root, maybe with an octave (#103).
fn is_root(w: &[char]) -> bool {
    let rest = match w {
        [l, r @ ..] if ('a'..='g').contains(l) => r,
        _ => return false,
    };
    let rest = match rest {
        ['#' | 'b', r @ ..] => r,
        r => r,
    };
    matches!(rest, [] | [_]) && rest.iter().all(char::is_ascii_digit)
}

/// `VI`, `bVII`, `viio7`, `V7`, `ivmaj7`: a roman numeral (#103).
fn is_roman(w: &[char]) -> bool {
    let w = match w {
        ['b', r @ ..] => r,
        r => r,
    };
    let n = w
        .iter()
        .take_while(|c| matches!(c, 'i' | 'v' | 'I' | 'V'))
        .count();
    let (roman, suffix) = w.split_at(n);
    let one_case =
        roman.iter().all(char::is_ascii_uppercase) || roman.iter().all(char::is_ascii_lowercase);
    let suffix: String = suffix.iter().collect();
    (1..=4).contains(&n) && one_case && matches!(suffix.as_str(), "" | "o" | "o7" | "7" | "maj7")
}

struct Line<'a> {
    chars: &'a [char],
    pos: &'a [u32],
    out: &'a mut Vec<Span>,
}

impl Line<'_> {
    fn push(&mut self, s: usize, e: usize, class: Class) {
        let (Some(a), Some(b)) = (self.pos.get(s), self.pos.get(e)) else {
            return;
        };
        if b > a {
            self.out.push(Span {
                start: *a,
                len: b - a,
                class,
            });
        }
    }

    /// Is char `s` just after the `:` of a chord's root (`c:m7`)?
    fn quality_at(&self, s: usize) -> bool {
        let (Some(colon), Some(here)) = (
            s.checked_sub(1).and_then(|k| self.pos.get(k)),
            self.pos.get(s),
        ) else {
            return false;
        };
        let n = self.out.len();
        match (
            n.checked_sub(2).and_then(|k| self.out.get(k)),
            self.out.last(),
        ) {
            (Some(root), Some(p)) => {
                p.class == Class::Punct
                    && p.start == *colon
                    && p.start + p.len == *here
                    && root.class == Class::Note
                    && root.start + root.len == p.start
            }
            _ => false,
        }
    }

    fn at(&self, i: usize) -> char {
        self.chars.get(i).copied().unwrap_or('\0')
    }

    /// The steps of a lane from `i` to `end`, or a generator call.
    fn lane(&mut self, mut i: usize, end: usize) {
        if self.chars.get(i..end).is_some_and(|w| w.contains(&'(')) {
            return self.tokens(i, end, false);
        }
        while i < end {
            let class = match self.at(i) {
                'x' | 'X' | 'o' | 'f' | 'd' | '2'..='4' => Some(Class::Step),
                '.' => Some(Class::Rest),
                _ => None,
            };
            let s = i;
            i += 1;
            while i < end && class.is_some() && self.at(i) == self.at(s) {
                i += 1;
            }
            if let Some(c) = class {
                self.push(s, i, c);
            }
        }
    }

    /// Words, numbers and notation from `i` to `end`; `notes` colours pitches.
    fn tokens(&mut self, mut i: usize, end: usize, notes: bool) {
        while i < end {
            let c = self.at(i);
            let s = i;
            if c.is_ascii_alphabetic() || c == '_' {
                let mut param = false;
                i += 1;
                while i < end {
                    let d = self.at(i);
                    let more = d.is_ascii_alphanumeric()
                        || d == '_'
                        || d == '#'
                        || (d == '-' && self.at(i + 1).is_ascii_digit());
                    if more {
                        i += 1;
                    } else if d == '.' && self.at(i + 1).is_ascii_alphabetic() {
                        param = true;
                        i += 1;
                    } else {
                        break;
                    }
                }
                let w = self.chars.get(s..i).unwrap_or(&[]);
                let word: String = w.iter().collect();
                let class = if self.at(i) == '(' {
                    Class::Call
                } else if param {
                    Class::Param
                } else if notes && (is_note(w) || is_root(w) || is_roman(w) || self.quality_at(s)) {
                    Class::Note
                } else if notes && word == "r" {
                    Class::Rest
                } else if WORDS.contains(&word.as_str()) {
                    Class::Keyword
                } else {
                    Class::Name
                };
                self.push(s, i, class);
            } else if c.is_ascii_digit()
                || (matches!(c, '-' | '.') && self.at(i + 1).is_ascii_digit())
            {
                i += 1;
                while i < end && (self.at(i).is_ascii_digit() || self.at(i) == '.') {
                    i += 1;
                }
                self.push(s, i, Class::Number);
            } else {
                i += 1;
                let class = match c {
                    '~' => Some(Class::Rest),
                    '=' | ':' | '"' | '[' | ']' | '<' | '>' | '(' | ')' | ',' | '*' | '@' | '?'
                    | '&' | '!' | '/' | '+' => Some(Class::Punct),
                    _ => None,
                };
                if let Some(class) = class {
                    self.push(s, i, class);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text and class of each span, for reading in a test.
    fn spans(text: &str) -> Vec<(String, Class)> {
        let units: Vec<u16> = text.encode_utf16().collect();
        lex(text)
            .iter()
            .map(|s| {
                let a = s.start as usize;
                let w = String::from_utf16_lossy(&units[a..a + s.len as usize]);
                (w, s.class)
            })
            .collect()
    }

    fn of(text: &str, class: Class) -> Vec<String> {
        spans(text)
            .into_iter()
            .filter(|(_, c)| *c == class)
            .map(|(w, _)| w)
            .collect()
    }

    const SONG: &str = "tempo 124 # fast
track kit drums
track lead synth
clip beat = kit /16
  bd x...X...
  sn ....x...
  ch euclid(3,8)
clip riff = lead
  \"c4 [e4 g4] ~ <c5 f#3>*2\"
setting nile = Minimoog MiniLead: Cutoff 1200
auto sweep = kit.Cutoff ramp 300 4000 /8
snapshot drop: strip1.Mute 1, master.P2Return 0.4
scene main 8: beat riff sweep [drop]
arrange main main
";

    /// #486: today's words are keywords, and so are the old ones a song
    /// written before still uses, with their lanes and notes read under them.
    #[test]
    fn ableton_words_and_the_old_ones_are_keywords() {
        let new = "track kit drums\nclip beat = kit /16\n  bd x...\nsnapshot dub: kit.Level 1\nscene a 2: beat [dub]";
        assert_eq!(
            of(new, Class::Keyword),
            ["track", "drums", "clip", "snapshot", "scene"]
        );
        assert_eq!(of(new, Class::Pad), ["bd"]);
        let old = "track kit drums\nfrag beat = kit /16\n  bd x...\nscene dub: kit.Level 1\nsection a 2: beat [dub]";
        assert_eq!(
            of(old, Class::Keyword),
            ["track", "drums", "frag", "scene", "section"]
        );
        assert_eq!(of(old, Class::Pad), ["bd"]);
    }

    #[test]
    fn keywords_names_and_numbers() {
        let k = of(SONG, Class::Keyword);
        for w in [
            "tempo", "track", "drums", "synth", "clip", "auto", "ramp", "snapshot", "scene",
            "arrange", "setting",
        ] {
            assert!(k.contains(&w.to_string()), "{w} is a keyword: {k:?}");
        }
        let n = of(SONG, Class::Name);
        for w in ["kit", "lead", "beat", "riff", "sweep", "drop", "main"] {
            assert!(n.contains(&w.to_string()), "{w} is a name: {n:?}");
        }
        assert!(of(SONG, Class::Number).contains(&"124".to_string()));
        assert_eq!(of(SONG, Class::Comment), ["# fast"]);
        assert_eq!(
            of(SONG, Class::Param),
            ["kit.Cutoff", "strip1.Mute", "master.P2Return"]
        );
    }

    #[test]
    fn lanes_and_notes() {
        assert_eq!(of(SONG, Class::Pad), ["bd", "sn", "ch"]);
        assert_eq!(of(SONG, Class::Step), ["x", "X", "x"]);
        assert_eq!(of(SONG, Class::Call), ["euclid"]);
        assert_eq!(of(SONG, Class::Note), ["c4", "e4", "g4", "c5", "f#3"]);
        assert!(of(SONG, Class::Rest).contains(&"~".to_string()));
        assert!(of(SONG, Class::Rest).contains(&"...".to_string()));
    }

    /// ADR-0024: a Modular setting's code lines are not lanes, and a `#`
    /// in them is no comment.
    #[test]
    fn a_setting_and_its_code() {
        let text = "setting s = Modular ModularBasic\n  SynthDef(\\a, { Saw.ar(440) * #[1] })\n";
        assert_eq!(of(text, Class::Keyword), ["setting"]);
        assert!(of(text, Class::Pad).is_empty());
        assert!(of(text, Class::Comment).is_empty());
        assert!(of(text, Class::Number).contains(&"440".to_string()));
    }

    #[test]
    fn a_sampler_line_is_lanes_or_notes() {
        let text = "track s sampler\nclip a = s\n  bd x.x.\nclip b = s\n  c4:4 r:4 g4:8.\n";
        assert_eq!(of(text, Class::Pad), ["bd"]);
        assert_eq!(of(text, Class::Note), ["c4", "g4"]);
        assert_eq!(of(text, Class::Rest), [".", ".", "r"]);
        assert!(of(text, Class::Number).contains(&"8.".to_string()));
    }

    #[test]
    fn chord_names_are_notes() {
        let text = "scale c minor\ntrack k synth\nclip p = k voicing\n  \"<c:m7 bb3:sus4 i VI bVII viio7> f\"\n";
        assert_eq!(
            of(text, Class::Note),
            ["c", "m7", "bb3", "sus4", "i", "VI", "bVII", "viio7", "f"]
        );
        assert!(of(text, Class::Keyword).contains(&"voicing".to_string()));
        let classic = "track k synth\nclip p = k\n  c:maj7:2 V7:4\n";
        assert_eq!(of(classic, Class::Note), ["c", "maj7", "V7"]);
    }

    #[test]
    fn a_sharp_is_not_a_comment() {
        let text = "track l synth\nclip a = l\n  c#4:4 d5@0:6:90 # end\n";
        assert_eq!(of(text, Class::Note), ["c#4", "d5"]);
        assert_eq!(of(text, Class::Comment), ["# end"]);
    }

    #[test]
    fn spans_count_utf16_and_are_ordered() {
        let text = "# café 🎹\ntempo 120\n";
        let s = spans(text);
        assert_eq!(s[0], ("# café 🎹".to_string(), Class::Comment));
        assert_eq!(s[1], ("tempo".to_string(), Class::Keyword));
        let raw = lex(text);
        assert!(raw.windows(2).all(|w| w[0].start + w[0].len <= w[1].start));
    }

    /// The parser reads `master:` as `master` and a colon; so does the lexer.
    #[test]
    fn master_and_its_colon() {
        assert_eq!(
            spans("master: MasterGain 0.8"),
            [
                ("master".to_string(), Class::Keyword),
                (":".to_string(), Class::Punct),
                ("MasterGain".to_string(), Class::Name),
                ("0.8".to_string(), Class::Number),
            ]
        );
    }

    /// A `\r\n` line end moves the spans after it by two units, not one.
    #[test]
    fn crlf_lines_keep_their_offsets() {
        let text = "tempo 120\r\nswing 50\r\n";
        assert_eq!(
            spans(text),
            [
                ("tempo".to_string(), Class::Keyword),
                ("120".to_string(), Class::Number),
                ("swing".to_string(), Class::Keyword),
                ("50".to_string(), Class::Number),
            ]
        );
    }

    /// The view colours by `CLASSES` in `web/src/audio/lex.ts`, indexed by
    /// class number: one name per class and the empty one for 0.
    #[test]
    fn the_view_has_a_name_per_class() {
        let ts = include_str!("../../../../web/src/audio/lex.ts");
        let line = ts
            .lines()
            .find(|l| l.starts_with("export const CLASSES"))
            .expect("CLASSES in lex.ts");
        assert_eq!(line.matches('\'').count() / 2, Class::Comment as usize + 1);
    }

    #[test]
    fn any_text_lexes_in_bounds() {
        let mut seed = 7u32;
        let alphabet: Vec<char> = "abcxX.~#\"[]<>(),:@*?!/=- \n\t\r09é🎹".chars().collect();
        for _ in 0..500 {
            let text: String = (0..80)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    alphabet[(seed >> 8) as usize % alphabet.len()]
                })
                .collect();
            let units = text.encode_utf16().count() as u32;
            let s = lex(&text);
            assert!(s.iter().all(|x| x.len > 0 && x.start + x.len <= units));
            assert!(s.windows(2).all(|w| w[0].start + w[0].len <= w[1].start));
        }
    }
}
