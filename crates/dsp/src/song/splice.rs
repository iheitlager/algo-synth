//! An edit from outside the text (a step click, a fold, the piano roll) into
//! the text as typed (#588): only the lines of the items it changed are
//! written again, as the engine prints them; every other line, blank line and
//! comment stays as it was. Never in `render`: it allocates.
//!
//! An item is what `comments::keys` names: a top-level line with the code
//! lines of a Modular setting under it, and each lane or note line of a clip
//! on its own. The items of the typed text, of its song printed and of the
//! edited song printed are matched by name; an item that changed replaces its
//! lines in the typed text, keeping their indent and side comment, a new one
//! goes after the item before it, a gone one goes.

use std::collections::HashMap;

use super::Song;
use super::comments::{CODE, comment_of, keys};
use super::strip_comment;

/// One item's lines, `[start, end)` in its text.
struct Item {
    key: String,
    start: usize,
    end: usize,
}

/// The items of `lines`, in order. Two items of one name are told apart by
/// their count, `lane a bd#1`.
fn items(lines: &[&str], song: &Song) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (i, key) in keys(lines, song).into_iter().enumerate() {
        match key {
            Some(k) if k == CODE => {
                if let Some(last) = out.last_mut() {
                    last.end = i + 1;
                }
            }
            Some(k) => {
                let n = seen.entry(k.clone()).or_insert(0);
                *n += 1;
                let key = if *n == 1 { k } else { format!("{k}#{n}") };
                out.push(Item {
                    key,
                    start: i,
                    end: i + 1,
                });
            }
            None => {}
        }
    }
    out
}

/// An item's lines as printed, without their comments (a code line keeps its
/// `#`, which is SuperCollider's).
fn body(lines: &[&str], it: &Item) -> Vec<String> {
    lines
        .get(it.start..it.end)
        .unwrap_or(&[])
        .iter()
        .enumerate()
        .map(|(k, l)| {
            if k == 0 {
                strip_comment(l).trim_end()
            } else {
                l.trim_end()
            }
            .to_string()
        })
        .collect()
}

/// `typed`, a text that parses to `old`, with the items `new` changed written
/// again. An item the print has and the typed text leaves out (`swing 50`)
/// is written only when the edit changes it. None when two of the typed
/// text's items have one name. The caller checks the result parses to `new`.
pub fn splice(typed: &str, old: &Song, new: &Song) -> Option<String> {
    let t: Vec<&str> = typed.split('\n').collect();
    let old_text = old.print();
    let new_text = new.print();
    let o: Vec<&str> = old_text.split('\n').collect();
    let n: Vec<&str> = new_text.split('\n').collect();
    let (ti, oi, ni) = (items(&t, old), items(&o, old), items(&n, new));
    let at: HashMap<&str, &Item> = ti.iter().map(|it| (it.key.as_str(), it)).collect();
    let was: HashMap<&str, Vec<String>> = oi
        .iter()
        .map(|it| (it.key.as_str(), body(&o, it)))
        .collect();
    if at.len() != ti.len() {
        return None;
    }
    // (start, end, lines): each a range of the typed lines and what replaces it.
    let mut edits: Vec<(usize, usize, Vec<String>)> = Vec::new();
    let mut after = 0;
    for it in &ni {
        let lines = body(&n, it);
        match (at.get(it.key.as_str()), was.get(it.key.as_str())) {
            (Some(&typed_it), before) => {
                if before != Some(&lines) {
                    edits.push((typed_it.start, typed_it.end, written(&t, typed_it, lines)));
                }
                after = typed_it.end;
            }
            (None, Some(before)) if *before == lines => {}
            (None, _) => edits.push((after, after, lines)),
        }
    }
    let kept: Vec<&str> = ni.iter().map(|it| it.key.as_str()).collect();
    for it in &ti {
        if was.contains_key(it.key.as_str()) && !kept.contains(&it.key.as_str()) {
            edits.push((it.start, it.end, Vec::new()));
        }
    }
    // From the bottom up, so each range is still where it was; an insertion
    // after a replacement of the same place goes below it.
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let mut out: Vec<String> = t.iter().map(|l| (*l).to_string()).collect();
    for (start, end, lines) in edits {
        out.splice(start..end, lines);
    }
    Some(out.join("\n"))
}

/// An item's new lines as the typed one was written: its indent for an
/// indented line, its side comment after the first line.
fn written(t: &[&str], it: &Item, mut lines: Vec<String>) -> Vec<String> {
    let first = t.get(it.start).copied().unwrap_or("");
    if let Some(l) = lines.first_mut() {
        let indent = &first[..first.len() - first.trim_start().len()];
        if l.starts_with([' ', '\t']) && !indent.is_empty() {
            *l = format!("{indent}{}", l.trim_start());
        }
        if let Some(c) = comment_of(first) {
            l.push(' ');
            l.push_str(&c);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spliced(typed: &str, edit: impl FnOnce(&mut Song)) -> String {
        let old = Song::parse(typed).expect("parses");
        let mut new = old.clone();
        edit(&mut new);
        let out = splice(typed, &old, &new).expect("splices");
        assert_eq!(Song::parse(&out).expect("parses"), new, "{out}");
        out
    }

    const TYPED: &str = "tempo 120\n\n# the kit\ntrack kit drums\ntrack lead synth\n\n\
        clip beat = kit /16\n    bd x...x...x...x...   # kick\n\n    sn ....x.......x...\n\
        clip riff = lead\n    \"c4 eb4 g4\"\n";

    /// #649: a note edit of a clip another is made from writes its line
    /// only, and the other follows it.
    #[test]
    fn an_edit_of_a_source_keeps_the_clip_made_from_it() {
        let typed = "track lead synth\nclip a = lead\n  \"c4 d4\"\nclip b = lead a .rev() # back\n";
        let out = spliced(typed, |s| {
            assert!(s.edit_note(0, crate::notes::Edit::Add { tick: 6, note: 64 }));
        });
        assert!(out.ends_with("clip b = lead a .rev() # back\n"), "{out}");
        let s = Song::parse(&out).expect("parses");
        assert_eq!(s.clips[1].notes.as_ref().expect("notes").events.len(), 3);
    }

    /// #653: a step edit of a drum clip another is made from writes its
    /// lane only.
    #[test]
    fn a_step_edit_of_a_source_keeps_the_drum_clip_made_from_it() {
        let typed = "track kit drums\nclip a = kit\n  bd x...\nclip b = kit a .rev()\n";
        let out = spliced(typed, |s| {
            assert!(s.set_step(0, 0, 1, crate::song::Step::Hit));
        });
        assert_eq!(out, typed.replace("bd x...", "bd xx.."));
    }

    #[test]
    fn an_edit_writes_only_its_lines() {
        let out = spliced(TYPED, |s| {
            s.set_step(0, 0, 1, crate::song::Step::Hit);
        });
        assert_eq!(
            out,
            TYPED.replace("bd x...x...x...x...   # kick", "bd xx..x...x...x... # kick")
        );
        let out = spliced(TYPED, |s| s.tempo = 90.0);
        assert_eq!(out, TYPED.replace("tempo 120", "tempo 90"));
    }

    #[test]
    fn a_new_item_goes_after_the_one_before_it_and_a_gone_one_goes() {
        let out = spliced(TYPED, |s| {
            s.swing = 60.0;
        });
        assert_eq!(out, TYPED.replace("tempo 120\n", "tempo 120\nswing 60\n"));
        let out = spliced(TYPED, |s| {
            s.clips.remove(1);
        });
        assert_eq!(
            out,
            TYPED.replace("clip riff = lead\n    \"c4 eb4 g4\"\n", "")
        );
    }
}
