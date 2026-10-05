//! Comments in the song text (#199, ADR-0012). A `#` at the start of a line or
//! after a space starts one. The parser drops them while it reads an item, so
//! this pass keeps them beside it: each comment is tied to the item it
//! belongs to, found by the same rule in the text that was read and in the text
//! about to be printed, and the printer puts it back.
//!
//! * a comment line belongs to the next item below it (`# kick` above
//!   `bd x...`), a comment after an item to that item;
//! * an item is named by its first words: `tempo`, `frag beat`, `track kit`,
//!   `section main`, a lane `lane beat bd`, a frag's notes `notes riff`;
//! * comments after the last item stay at the end, and one whose item is gone
//!   (a lane taken out) is moved to the end, never lost.
//!
//! Layout is still the printer's: blank lines and alignment are not kept.

use std::collections::{BTreeMap, BTreeSet};

use super::{Song, strip_comment};

/// What was written about one item.
#[derive(Clone, Debug, Default)]
struct Remark {
    /// Comment lines above it, each from its `#`.
    above: Vec<String>,
    /// The comment after it on its own line, from its `#`.
    side: Option<String>,
}

/// The song's comments, by the item they belong to. They are not music: two
/// songs that play the same are equal whatever they say, so comparing songs
/// ignores them.
#[derive(Clone, Debug, Default)]
pub struct Comments {
    items: BTreeMap<String, Remark>,
    /// Comment lines after the last item.
    end: Vec<String>,
}

impl PartialEq for Comments {
    fn eq(&self, _: &Comments) -> bool {
        true
    }
}

/// The item each line is, `None` for a blank line or one with only a comment.
fn keys(lines: &[&str], song: &Song) -> Vec<Option<String>> {
    // The frag the indented lines belong to.
    let mut frag: Option<&str> = None;
    lines
        .iter()
        .map(|raw| {
            let body = strip_comment(raw);
            let mut words = body.split_whitespace();
            let first = words.next()?;
            if body.starts_with([' ', '\t']) {
                let f = frag?;
                let notes = song
                    .frags
                    .iter()
                    .find(|x| x.name == f)
                    .is_some_and(|x| x.notes.is_some());
                return Some(if notes {
                    format!("notes {f}")
                } else {
                    format!("lane {f} {first}")
                });
            }
            // `scene drop: …` names its scene up to the colon.
            let name = words.next().map(|n| n.trim_end_matches(':'));
            frag = if first == "frag" { name } else { None };
            Some(match first {
                // `master:` has no name before its colon.
                "tempo" | "swing" | "scale" | "arrange" | "loop" | "master" | "master:" => {
                    first.trim_end_matches(':').to_string()
                }
                _ => format!("{first} {}", name.unwrap_or("")),
            })
        })
        .collect()
}

/// The comment on `raw`, from its `#`.
fn comment_of(raw: &str) -> Option<String> {
    let rest = raw.get(strip_comment(raw).len()..)?.trim();
    rest.starts_with('#').then(|| rest.to_string())
}

impl Comments {
    /// The comments of `text`, which has been parsed into `song`.
    pub(super) fn collect(text: &str, song: &Song) -> Comments {
        let lines: Vec<&str> = text.lines().collect();
        let mut found = Comments::default();
        let mut pending: Vec<String> = Vec::new();
        for (raw, key) in lines.iter().zip(keys(&lines, song)) {
            let side = comment_of(raw);
            match key {
                None => pending.extend(side),
                Some(k) => {
                    let r = found.items.entry(k).or_default();
                    r.above.append(&mut pending);
                    if side.is_some() {
                        r.side = side;
                    }
                }
            }
        }
        found.end = pending;
        found
    }

    /// `printed` (the lines of `song`'s canonical text) with the comments put back.
    pub(super) fn apply(&self, printed: Vec<String>, song: &Song) -> Vec<String> {
        if self.items.is_empty() && self.end.is_empty() {
            return printed;
        }
        let refs: Vec<&str> = printed.iter().map(String::as_str).collect();
        let keys = keys(&refs, song);
        let mut used = BTreeSet::new();
        let mut out = Vec::with_capacity(printed.len());
        for (line, key) in printed.iter().zip(keys) {
            let remark = key
                .as_ref()
                .and_then(|k| self.items.get_key_value(k.as_str()));
            let Some((k, r)) = remark else {
                out.push(line.clone());
                continue;
            };
            used.insert(k.as_str());
            let indent = if k.starts_with("lane ") || k.starts_with("notes ") {
                "  "
            } else {
                ""
            };
            out.extend(r.above.iter().map(|c| format!("{indent}{c}")));
            out.push(match &r.side {
                Some(s) => format!("{line} {s}"),
                None => line.clone(),
            });
        }
        // Comments whose item is gone, then those that came last.
        let mut rest: Vec<String> = Vec::new();
        for (k, r) in &self.items {
            if !used.contains(k.as_str()) {
                rest.extend(r.above.iter().cloned());
                rest.extend(r.side.iter().cloned());
            }
        }
        rest.extend(self.end.iter().cloned());
        if !rest.is_empty() {
            out.push(String::new());
            out.extend(rest);
        }
        out
    }
}
