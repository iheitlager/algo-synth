//! Where the words of a song's printed text are (#205), so the view can light
//! the ones that play. Built from the text as the engine prints it, when a song
//! loads or an edit reprints it, never in `render`.
//!
//! Positions count UTF-16 units from the start of the text, as the editor's
//! highlighting does (#203).

use super::Song;

/// A clip's words: one span per word of its note line, in
/// `Notes::word_spans` order, or one per step of each lane.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipSpans {
    pub notes: Vec<(u32, u32)>,
    pub lanes: Vec<Vec<(u32, u32)>>,
}

/// Each clip's spans in `text`, which is `song` printed (comments kept).
/// A clip whose lines are not found as printed gets none.
pub fn spans(song: &Song, text: &str) -> Vec<ClipSpans> {
    // Each line with where it starts, in UTF-16 units.
    let mut lines = Vec::new();
    let mut at = 0u32;
    for line in text.split('\n') {
        lines.push((at, line));
        at += line.encode_utf16().count() as u32 + 1;
    }
    let u16_len = |s: &str| s.encode_utf16().count() as u32;
    song.clips
        .iter()
        .map(|clip| {
            let mut out = ClipSpans::default();
            let head = format!("clip {} ", clip.name);
            let Some(h) = lines.iter().position(|(_, l)| l.starts_with(&head)) else {
                return out;
            };
            // The clip's own lines follow its header; comment lines between
            // them are skipped.
            let mut body = lines
                .iter()
                .skip(h + 1)
                .filter(|(_, l)| !l.trim_start().starts_with('#'));
            if let Some(n) = &clip.notes {
                if let Some((off, line)) = body.next() {
                    let at = format!("  {}", n.text);
                    if line.starts_with(&at) {
                        let base = off + 2;
                        out.notes = n
                            .word_spans()
                            .into_iter()
                            .map(|(s, l)| {
                                let pre = n.text.get(..s).map_or(0, u16_len);
                                let word = n.text.get(s..s + l).map_or(0, u16_len);
                                (base + pre, word)
                            })
                            .collect();
                    }
                }
                return out;
            }
            for lane in &clip.lanes {
                let Some((off, line)) = body.next() else {
                    break;
                };
                let head = format!("  {} ", lane.pad.name());
                if !line.starts_with(&head) {
                    out.lanes.push(Vec::new());
                    continue;
                }
                let base = off + u16_len(&head);
                let steps = match &lane.call {
                    // A call lights whole, whichever step it plays.
                    Some(e) => vec![(base, u16_len(&e.print())); lane.steps.len()],
                    None => {
                        let mut at = base;
                        (0..lane.steps.len())
                            .map(|n| {
                                let w = u16_len(&lane.step_text(n));
                                at += w;
                                (at - w, w)
                            })
                            .collect()
                    }
                };
                out.lanes.push(steps);
            }
            out
        })
        .collect()
}
