//! A SynthDef as coloured spans, for the Modular editor (#329): the engine
//! knows the language, so the view only paints what this returns (ADR-0001).
//!
//! It reads sclang by the shape of its words, as the song lexer does, and
//! shares its classes so both editors colour alike: comments, numbers,
//! `\symbols` and `'symbols'` and `name:` keyword arguments as parameters,
//! strings and `.method` names as names, capitalised class names (`SinOsc`,
//! `Env`, `SynthDef`) as calls, `var` and `arg` and the constants as keywords,
//! and brackets and operators as punctuation. It is total: any text gives
//! spans, never a panic. Spans count UTF-16 units, in order, never
//! overlapping, whitespace and unknown text left plain.

use crate::song::lex::{Class, Span};

const KEYWORDS: [&str; 7] = ["var", "arg", "true", "false", "nil", "inf", "pi"];
const PUNCT: &str = "=()[]{},;.|:+-*/<>!?^%&@#~`";

/// The spans of `text`.
pub fn lex(text: &str) -> Vec<Span> {
    let chars: Vec<char> = text.chars().collect();
    // The UTF-16 offset of each char, and one past the end.
    let mut pos = Vec::with_capacity(chars.len() + 1);
    let mut p = 0u32;
    for c in &chars {
        pos.push(p);
        p += c.len_utf16() as u32;
    }
    pos.push(p);
    let at = |i: usize| chars.get(i).copied();
    let mut out = Vec::new();
    let mut push = |s: usize, e: usize, class: Class| {
        if let (Some(&a), Some(&b)) = (pos.get(s), pos.get(e)) {
            if b > a {
                out.push(Span {
                    start: a,
                    len: b - a,
                    class,
                });
            }
        }
    };
    // Whether the last word was a `.` before a method name.
    let mut after_dot = false;
    let mut i = 0;
    while let Some(c) = at(i) {
        let s = i;
        let dot = after_dot;
        after_dot = false;
        if c.is_whitespace() {
            i += 1;
            after_dot = dot;
            continue;
        }
        if c == '/' && at(i + 1) == Some('/') {
            while at(i).is_some_and(|c| c != '\n') {
                i += 1;
            }
            push(s, i, Class::Comment);
        } else if c == '/' && at(i + 1) == Some('*') {
            i += 2;
            while at(i).is_some() && !(at(i) == Some('*') && at(i + 1) == Some('/')) {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            push(s, i, Class::Comment);
        } else if c == '"' || c == '\'' {
            i += 1;
            while let Some(d) = at(i) {
                i += 1;
                if d == '\\' {
                    i += 1;
                } else if d == c {
                    break;
                }
            }
            i = i.min(chars.len());
            push(s, i, if c == '"' { Class::Name } else { Class::Param });
        } else if c == '\\' {
            i += 1;
            while at(i).is_some_and(|c| c.is_alphanumeric() || c == '_') {
                i += 1;
            }
            push(s, i, Class::Param);
        } else if c.is_ascii_digit() || (c == '.' && at(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            while at(i).is_some_and(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_') {
                // A `.` is the number's unless a method follows (`1.0.midiratio`).
                if at(i) == Some('.') && !at(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                    break;
                }
                i += 1;
            }
            push(s, i, Class::Number);
        } else if c.is_alphabetic() || c == '_' {
            while at(i).is_some_and(|c| c.is_alphanumeric() || c == '_') {
                i += 1;
            }
            let word: String = chars.get(s..i).unwrap_or(&[]).iter().collect();
            let keyword_arg = at(i) == Some(':') && at(i + 1) != Some(':');
            let class = if KEYWORDS.contains(&word.as_str()) {
                Some(Class::Keyword)
            } else if keyword_arg {
                Some(Class::Param)
            } else if dot {
                Some(Class::Name)
            } else if c.is_uppercase() {
                Some(Class::Call)
            } else {
                None
            };
            if let Some(class) = class {
                push(s, i, class);
            }
        } else if PUNCT.contains(c) {
            i += 1;
            push(s, i, Class::Punct);
            after_dot = c == '.';
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text of each span with its class.
    fn spans(text: &str) -> Vec<(String, Class)> {
        let units: Vec<u16> = text.encode_utf16().collect();
        lex(text)
            .iter()
            .map(|s| {
                let a = s.start as usize;
                let piece = units.get(a..a + s.len as usize).unwrap_or(&[]);
                (String::from_utf16_lossy(piece), s.class)
            })
            .collect()
    }

    fn of(text: &str, class: Class) -> Vec<String> {
        spans(text)
            .into_iter()
            .filter(|(_, c)| *c == class)
            .map(|(t, _)| t)
            .collect()
    }

    const DEF: &str = "SynthDef(\\acid, { |freq = 110, gate = 1| // a bass
    var sig = Saw.ar(freq * 0.5);
    sig = MoogFF.ar(sig, 800, 2, voicing: \\sh101); /* 24 dB */
    sig * Env.adsr(0.01, 0.2, 0.7, 0.3).kr(2, gate) * 1e-1
}).add;";

    #[test]
    fn colours_a_synthdef_by_its_words() {
        assert_eq!(of(DEF, Class::Call), ["SynthDef", "Saw", "MoogFF", "Env"]);
        let numbers = of(DEF, Class::Number);
        assert_eq!(
            numbers.get(..10).unwrap_or(&[]),
            [
                "110", "1", "0.5", "800", "2", "0.01", "0.2", "0.7", "0.3", "2"
            ]
        );
        assert_eq!(of(DEF, Class::Param), ["\\acid", "voicing", "\\sh101"]);
        assert_eq!(of(DEF, Class::Name), ["ar", "ar", "adsr", "kr", "add"]);
        assert_eq!(of(DEF, Class::Keyword), ["var"]);
        assert_eq!(of(DEF, Class::Comment), ["// a bass", "/* 24 dB */"]);
    }

    #[test]
    fn a_method_on_a_number_is_not_part_of_it() {
        assert_eq!(
            spans("1.5.midiratio"),
            [
                ("1.5".to_string(), Class::Number),
                (".".to_string(), Class::Punct),
                ("midiratio".to_string(), Class::Name)
            ]
        );
    }

    #[test]
    fn counts_utf16_units_and_never_panics() {
        // `é` is one unit, `𝄞` two: after ` // `, the newline and a space, `12` is at 9.
        let s = lex("é𝄞 // \n 12");
        let last = s.last().expect("a span");
        assert_eq!((last.start, last.len, last.class), (9, 2, Class::Number));
        for text in [
            "\"open", "/* open", "\\", "'", "1.", ".", "a:", "x::y", "\u{0}",
        ] {
            let s = lex(text);
            let end = text.encode_utf16().count() as u32;
            assert!(s.iter().all(|s| s.start + s.len <= end), "{text:?}");
        }
    }
}
