//! Where the words of a song's text are (#205), so the view can light the ones
//! that play. Built from the text the engine keeps, as typed or as it printed
//! it (#588), when a song loads or an edit reprints it, never in `render`.
//!
//! A clip's lines are found by their words, whatever their indent, blank lines
//! and comments. A note line written otherwise than it prints (`eb4` for `d#4`,
//! `0..3` for `0 .. 3`) is matched word for word against its print; a line
//! whose words do not line up gets no spans, so nothing lights up wrongly.
//!
//! The numbers of a mod's sequences (a parameter method's or a `mod` line's)
//! are found the same way: the signal as typed is parsed again, and gets
//! spans only when it prints as the song's does (#675).
//!
//! Positions count UTF-16 units from the start of the text, as the editor's
//! highlighting does (#203).

use super::signal::Signal;
use super::{
    Auto, Mod, Shape, Song, adsr_parts, alias, parse_methods, split_methods, strip_comment,
};
use crate::notes::TICKS_PER_BAR;
use crate::notes::pattern::Pattern;
use crate::params::Param;

/// A song's spans: each clip's words, each mod's numbers by their index in
/// its signal (#675), and each auto's values by their index (#683).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SongSpans {
    pub clips: Vec<ClipSpans>,
    pub mods: Vec<Vec<Option<(u32, u32)>>>,
    pub autos: Vec<Vec<Option<(u32, u32)>>>,
}

/// A clip's words: one span per word of its note line, in
/// `Notes::word_spans` order, or one per step of each lane; and the values
/// of its pattern methods a bar (#676), as (tick in the clip, ticks it holds,
/// span), by tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipSpans {
    pub notes: Vec<(u32, u32)>,
    pub lanes: Vec<Vec<(u32, u32)>>,
    pub methods: Vec<(u32, u32, (u32, u32))>,
}

fn u16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

/// Each clip's and mod's spans in `text`, a text that parses to `song`. One
/// whose line is not found gets none.
pub fn spans(song: &Song, text: &str) -> SongSpans {
    // Each line with where it starts, in UTF-16 units.
    let mut lines = Vec::new();
    let mut at = 0u32;
    for line in text.split('\n') {
        lines.push((at, line));
        at += u16_len(line) + 1;
    }
    let mods = song
        .mods
        .iter()
        .map(|md| mod_spans(song, md, &lines))
        .collect();
    let autos = song.autos.iter().map(|a| auto_spans(a, &lines)).collect();
    let clips = song
        .clips
        .iter()
        .map(|clip| {
            let mut out = ClipSpans::default();
            let Some(h) = lines.iter().position(|(_, l)| {
                let mut w = strip_comment(l).split_whitespace();
                w.next() == Some("clip") && w.next() == Some(clip.name.as_str())
            }) else {
                return out;
            };
            // The clip's own lines: the indented ones after its header, without
            // blank and comment lines; (start, body without indent or comment).
            let mut body = lines
                .iter()
                .skip(h + 1)
                .take_while(|(_, l)| {
                    l.starts_with([' ', '\t']) || strip_comment(l).trim().is_empty()
                })
                .filter_map(|(off, l)| {
                    let body = strip_comment(l).trim_end();
                    let word = body.trim_start();
                    (!word.is_empty())
                        .then(|| (off + u16_len(&body[..body.len() - word.len()]), word))
                });
            if let (Some(n), Some(&(base, line))) = (&clip.notes, lines.get(h)) {
                if !clip.live {
                    out.methods = method_spans(&clip.pattern, n.bars, base, line);
                }
            }
            if let Some(n) = &clip.notes {
                if let Some((base, typed)) = body.next() {
                    out.notes = typed_spans(&n.text, &n.word_spans(), typed)
                        .into_iter()
                        .map(|(s, l)| {
                            let pre = typed.get(..s).map_or(0, u16_len);
                            (base + pre, typed.get(s..s + l).map_or(0, u16_len))
                        })
                        .collect();
                }
                return out;
            }
            for lane in &clip.lanes {
                let Some((base, line)) = body.next() else {
                    break;
                };
                out.lanes
                    .push(lane_spans(lane, line).map_or_else(Vec::new, |steps| {
                        steps
                            .into_iter()
                            .map(|(s, l)| (base + line.get(..s).map_or(0, u16_len), l))
                            .collect()
                    }));
            }
            out
        })
        .collect();
    SongSpans { clips, mods, autos }
}

/// An auto's values in its line as typed (#683): each number of `Steps`, or
/// each number of a sequence parsed again as `mod_spans` does. None for a
/// ramp, or when the line isn't found or doesn't read as the song's.
fn auto_spans(a: &Auto, lines: &[(u32, &str)]) -> Vec<Option<(u32, u32)>> {
    let found = lines.iter().find_map(|&(base, l)| {
        let body = strip_comment(l).trim_end();
        let w: Vec<&str> = body.split_whitespace().collect();
        (w.first() == Some(&"auto") && w.get(1) == Some(&a.name.as_str())).then_some((base, l, w))
    });
    let Some((base, line, w)) = found else {
        return Vec::new();
    };
    let span = |word: &str| {
        let at = offset(line, word);
        Some((base + line.get(..at).map_or(0, u16_len), u16_len(word)))
    };
    // The values: the words between `target.Param` and `/bars`.
    let values = w.get(4..w.len().saturating_sub(1)).unwrap_or(&[]);
    match &a.shape {
        Shape::Steps(v) => {
            let same = values.len() == v.len()
                && values
                    .iter()
                    .zip(v)
                    .all(|(t, x)| t.parse::<f32>().ok() == Some(*x));
            if same {
                values.iter().map(|t| span(t)).collect()
            } else {
                Vec::new()
            }
        }
        Shape::Seq(sig) => {
            let (Some(first), Some(end)) = (values.first(), values.last()) else {
                return Vec::new();
            };
            let from = offset(line, first);
            let to = offset(line, end) + end.len();
            line.get(from..to)
                .map_or_else(Vec::new, |text| numbers(sig, base, line, text, false))
        }
        Shape::Ramp(..) => Vec::new(),
    }
}

/// The values of a clip's pattern methods a bar (#676) on its line as typed
/// (starting at `base`), for each bar of the clip's `bars`: (tick, ticks,
/// span), by tick. Only a method no later one moves in time, and not one
/// inside `every`, `off` or `sometimes`: their bars are not the clip's.
fn method_spans(
    pattern: &[Pattern],
    bars: u32,
    base: u32,
    line: &str,
) -> Vec<(u32, u32, (u32, u32))> {
    let body = strip_comment(line);
    let Some((_, methods)) = split_methods(body).1 else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, p) in pattern.iter().enumerate() {
        let Pattern::PerBar(text, steps) = p else {
            continue;
        };
        if !pattern.iter().skip(i + 1).all(Pattern::pitch_only) || steps.is_empty() {
            continue;
        }
        // The quoted argument as written, found after its `.name(`.
        let Some((name, arg)) = text.split_once('(') else {
            continue;
        };
        let arg = arg.strip_suffix(')').unwrap_or(arg);
        let call = format!(".{name}(");
        let Some(at) = methods.match_indices(&call).find_map(|(k, _)| {
            let inner = methods.get(k + call.len()..)?;
            let close = inner.find(')')?;
            let inside = inner.get(..close)?;
            (inside.trim() == arg)
                .then(|| offset(line, inside) + (inside.len() - inside.trim_start().len()))
        }) else {
            continue;
        };
        for b in 0..bars {
            let Some(parts) = steps.get(b as usize % steps.len()) else {
                continue;
            };
            for part in parts.iter().filter(|p| p.to > p.from) {
                let (from, len) = (at + part.at.0, part.at.1);
                let span = (
                    base + line.get(..from).map_or(0, u16_len),
                    line.get(from..from + len).map_or(0, u16_len),
                );
                out.push((b * TICKS_PER_BAR + part.from, part.to - part.from, span));
            }
        }
    }
    out.sort_by_key(|m| m.0);
    out
}

/// Where `inner`, a slice of `outer`, starts in it (bytes).
fn offset(outer: &str, inner: &str) -> usize {
    (inner.as_ptr() as usize).saturating_sub(outer.as_ptr() as usize)
}

/// The numbers of mod `md`'s signal in its line as typed, by their index in
/// the signal: a parameter method on its clip's line or a `mod` line. None
/// when the line isn't found or its signal doesn't print as `md`'s.
fn mod_spans(song: &Song, md: &Mod, lines: &[(u32, &str)]) -> Vec<Option<(u32, u32)>> {
    let named =
        |name: &str| alias(name).map(|(_, p)| p).or_else(|| Param::by_name(name)) == Some(md.param);
    // The line's start, the line and its signal's text.
    let found = lines.iter().find_map(|&(base, l)| {
        let body = strip_comment(l).trim_end();
        let mut w = body.split_whitespace();
        let text = match md.clip {
            Some(f) => {
                let clip = song.clips.get(f)?;
                if (w.next(), w.next()) != (Some("clip"), Some(clip.name.as_str())) {
                    return None;
                }
                let methods = split_methods(body).1?.1;
                let parsed = parse_methods(methods).ok()?;
                // A part of `.adsr("a:d:s:r")` (#677), parsed in quotes.
                if md.alias == Some("adsr") {
                    let sig = parsed
                        .into_iter()
                        .find(|(n, _, _, _)| n.eq_ignore_ascii_case("adsr"))?
                        .2;
                    let part = adsr_parts(sig)
                        .ok()?
                        .into_iter()
                        .find(|(p, _, _)| *p == Some(md.param))?
                        .2;
                    return Some((base, l, part, true));
                }
                parsed.into_iter().find(|(name, _, _, _)| named(name))?.2
            }
            None => {
                let (target, param) = w.nth(1)?.split_once('.')?;
                let rest: Vec<&str> = body.split_whitespace().collect();
                let (Some(&"mod"), Some(&"="), Some(first)) =
                    (rest.first(), rest.get(2), rest.get(3))
                else {
                    return None;
                };
                if target != song.target_name(md.target) || !named(param) {
                    return None;
                }
                body.get(offset(body, first)..)?
            }
        };
        Some((base, l, text, false))
    });
    let Some((base, line, text, quoted)) = found else {
        return Vec::new();
    };
    numbers(&md.signal, base, line, text, quoted)
}

/// The numbers of `want`, a signal written as `text` in `line` (which starts
/// at `base`): `text` parsed again, and spans only when it prints as `want`.
/// `quoted`: `text` is a sequence written without its quotes (an `.adsr` part).
fn numbers(
    want: &Signal,
    base: u32,
    line: &str,
    text: &str,
    quoted: bool,
) -> Vec<Option<(u32, u32)>> {
    let parsed = if quoted {
        Signal::parse(&format!("\"{text}\""), &mut 0)
    } else {
        Signal::parse(text, &mut 0)
    };
    let Ok(sig) = parsed else {
        return Vec::new();
    };
    let shift = usize::from(quoted);
    if sig.to_string() != want.to_string() {
        return Vec::new();
    }
    let at = offset(line, text);
    let byte = |c: usize| text.char_indices().nth(c).map_or(text.len(), |(b, _)| b);
    (0..sig.numbers())
        .map(|v| {
            let (c, l) = sig.col(v)?;
            let (from, to) = (
                byte(c as usize - 1 - shift),
                byte((c + l) as usize - 1 - shift),
            );
            Some((
                base + line.get(..at + from).map_or(0, u16_len),
                text.get(from..to).map_or(0, u16_len),
            ))
        })
        .collect()
}

/// A lane's steps in its line (`bd x... x...`), as (byte offset, UTF-16
/// length): the steps in order, spaces between them allowed; a call lights
/// whole, whichever step it plays. None when the line is not the lane's.
fn lane_spans(lane: &super::Lane, line: &str) -> Option<Vec<(usize, u32)>> {
    let pad = lane.pad.name();
    let rest = line.strip_prefix(pad)?;
    let steps = rest.trim_start();
    if steps.len() == rest.len() {
        return None;
    }
    let mut at = line.len() - steps.len();
    if lane.call.is_some() {
        return Some(vec![(at, u16_len(steps)); lane.steps.len()]);
    }
    (0..lane.steps.len())
        .map(|n| {
            let ahead = line.get(at..)?;
            at += ahead.len() - ahead.trim_start().len();
            let step = lane.step_text(n);
            line.get(at..)?.starts_with(&step).then(|| {
                at += step.len();
                (at - step.len(), u16_len(&step))
            })
        })
        .collect()
}

/// The word spans of `printed`, a note line as the engine prints it, carried
/// over to `typed`, the same line as written: the same spans when the two are
/// alike, else word by word. Empty when their words do not line up.
fn typed_spans(printed: &str, spans: &[(usize, usize)], typed: &str) -> Vec<(usize, usize)> {
    if typed == printed {
        return spans.to_vec();
    }
    // A line printed as one word (a generator call) is one word as typed.
    if let [(0, l)] = spans {
        if *l == printed.len() {
            return vec![(0, typed.len())];
        }
    }
    let (p, t) = (words(printed), words(typed));
    if p.len() != t.len() {
        return Vec::new();
    }
    spans
        .iter()
        .map(|&(s, l)| {
            let i = p.iter().position(|&(ws, _)| ws == s)?;
            let (ps, pe) = *p.get(i)?;
            let (ts, te) = *t.get(i)?;
            // A span short of its word (`c4` of `c4@2`) stops where its word's
            // suffix starts as typed.
            let len = if s + l == pe || l == pe - ps {
                te - ts
            } else {
                head(typed.get(ts..te)?)
            };
            Some((ts, len))
        })
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
}

/// The words of a note line, as (start, end) byte offsets: split on spaces,
/// quotes and `<` `>`, and on `[` `]` around a group; a chord in brackets
/// (`[e4,g4]`) is one word, and so is a range with its dots, `0 .. 3` or `0..3`.
fn words(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let at = |i: usize| b.get(i).copied().unwrap_or(b' ');
    let sep = |c: u8| c.is_ascii_whitespace() || matches!(c, b'"' | b'<' | b'>' | b'[' | b']');
    // The end of the word from `i`.
    let word_end = |mut i: usize| {
        while i < b.len() && !sep(at(i)) {
            i += 1;
        }
        i
    };
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = at(i);
        if c != b'[' && sep(c) {
            i += 1;
            continue;
        }
        let start = i;
        if c == b'[' {
            let rest = s.get(i..).unwrap_or("");
            let chord = rest.find(']').filter(|&k| {
                let inner = rest.get(1..k).unwrap_or("");
                inner.contains(',') && !inner.contains([' ', '[', '<'])
            });
            let Some(k) = chord else {
                i += 1;
                continue;
            };
            i += k + 1;
        }
        i = word_end(i);
        // `0 .. 3`: the dots and the number after them join the word before.
        if s.get(start..i) == Some("..") {
            if let Some(prev) = out.last_mut() {
                let mut j = i;
                while j < b.len() && at(j).is_ascii_whitespace() {
                    j += 1;
                }
                i = word_end(j);
                prev.1 = i;
                continue;
            }
        }
        out.push((start, i));
    }
    out
}

/// How much of a word is its note or chord: up to its first suffix, `@2`,
/// `*2`, `?`, `&`, `!` or a classic `:8`.
fn head(word: &str) -> usize {
    let from = if word.starts_with('[') {
        word.find(']').map_or(0, |k| k + 1)
    } else {
        0
    };
    word[from..]
        .find([':', '@', '*', '?', '&', '!'])
        .map_or(word.len(), |k| from + k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at<'a>(s: &'a str, spans: &[(usize, usize)]) -> Vec<&'a str> {
        spans.iter().map(|&(a, l)| &s[a..a + l]).collect()
    }

    /// #675: a mod's numbers get spans in its line as typed, and none when
    /// the text's signal is not the song's, rather than wrong ones.
    #[test]
    fn a_mods_numbers_light_as_typed_or_not_at_all() {
        let text = "track lead synth Juno106\nclip m = lead .cutoff( \"<300 [800 1200]>\" )\n  \"c4\"\nmod lead.resonance = \"0.1 .5\"\n";
        let song = crate::song::Song::parse(text).expect("parses");
        let s = spans(&song, text);
        let named = |m: usize| -> Vec<&str> {
            s.mods[m]
                .iter()
                .flatten()
                .map(|&(a, l)| &text[a as usize..(a + l) as usize])
                .collect()
        };
        assert_eq!(named(0), ["300", "800", "1200"]);
        assert_eq!(named(1), ["0.1", ".5"]);
        let other = text.replace("1200", "1300").replace("\"0.1 .5\"", "sine");
        let s = spans(&song, &other);
        assert!(s.mods.iter().all(Vec::is_empty), "{:?}", s.mods);
    }

    #[test]
    fn words_split_as_the_notation_does() {
        let s = "\"c4 [e4 g4] ~ <c5 [d5,f5]>/2 0..3 1 .. 2\"";
        let w: Vec<&str> = words(s).iter().map(|&(a, b)| &s[a..b]).collect();
        assert_eq!(
            w,
            [
                "c4", "e4", "g4", "~", "c5", "[d5,f5]", "/2", "0..3", "1 .. 2"
            ]
        );
    }

    #[test]
    fn spans_carry_over_to_the_line_as_typed() {
        let printed = "\"c4 d#4@2 [e4,g4] 0 .. 3\"";
        let spans = [(1, 2), (4, 3), (10, 7), (18, 6)];
        assert_eq!(at(printed, &spans), ["c4", "d#4", "[e4,g4]", "0 .. 3"]);
        let typed = "\"c4  eb4@2 [e4,g4]  0..3\"";
        assert_eq!(
            at(typed, &typed_spans(printed, &spans, typed)),
            ["c4", "eb4", "[e4,g4]", "0..3"]
        );
        // Words that do not line up light nothing.
        assert!(typed_spans(printed, &spans, "\"c4 eb4 e4\"").is_empty());
        // A call is one word.
        assert_eq!(
            typed_spans("arp(c4,up)", &[(0, 10)], "arp( c4, up )"),
            [(0, 13)]
        );
    }
}
