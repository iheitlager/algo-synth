//! The acceptance gate (#453): the named rules a proposed song passes before
//! it reaches the user. A refusing rule sends the song back to the model with
//! its message; a warning rides along with the proposal to the review.
//! `propose_song` runs them in the order of `RULES`.

use std::collections::BTreeSet;

use algo_dsp::song::{Song, Step};
use serde::Serialize;

use crate::tools;

/// What a rule does with a song that breaks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Refuse,
    Warn,
}

/// One rule of the gate: its id, what it does, and what it checks.
pub struct Rule {
    pub id: &'static str,
    pub level: Level,
    pub what: &'static str,
}

/// Every rule, in the order the gate runs them. Spec 008 lists the same ids.
pub const RULES: [Rule; 11] = [
    Rule {
        id: "parse",
        level: Level::Refuse,
        what: "the song parses",
    },
    Rule {
        id: "scope",
        level: Level::Refuse,
        what: "with a track in focus, nothing else changes (#415)",
    },
    Rule {
        id: "unused-clip",
        level: Level::Refuse,
        what: "with an arrangement, every clip sits in an arranged scene",
    },
    Rule {
        id: "unplayed-track",
        level: Level::Refuse,
        what: "every heard track plays a clip",
    },
    Rule {
        id: "unused-auto",
        level: Level::Refuse,
        what: "with an arrangement, every auto lane sits in an arranged scene",
    },
    Rule {
        id: "nonfinite",
        level: Level::Refuse,
        what: "the render has no non-finite samples (#430)",
    },
    Rule {
        id: "clip",
        level: Level::Refuse,
        what: "the render's peak is at most 1.0 (#430)",
    },
    Rule {
        id: "silent-focus",
        level: Level::Refuse,
        what: "the track in focus sounds (#430)",
    },
    Rule {
        id: "silent-scene",
        level: Level::Warn,
        what: "every arranged scene plays something",
    },
    Rule {
        id: "unarranged",
        level: Level::Warn,
        what: "a song with scenes arranges them",
    },
    Rule {
        id: "removed",
        level: Level::Warn,
        what: "tracks and scenes of the current song are kept",
    },
];

/// A rule a song broke, and what to change.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Finding {
    pub rule: &'static str,
    pub message: String,
}

impl Finding {
    pub fn new(rule: &'static str, message: String) -> Finding {
        debug_assert!(RULES.iter().any(|r| r.id == rule), "no rule {rule}");
        Finding { rule, message }
    }

    pub fn refuses(&self) -> bool {
        RULES
            .iter()
            .any(|r| r.id == self.rule && r.level == Level::Refuse)
    }
}

/// The structure rules over `after`, a parsed proposal: only what `before`,
/// the current song, did not already break, so the model is not held to the
/// user's own loose ends. The track in focus is left to `silent-focus`.
pub fn lint(before: Option<&Song>, after: &Song, focus: Option<&str>) -> Vec<Finding> {
    let had = before.map(|s| structure(s, focus)).unwrap_or_default();
    let mut found: Vec<Finding> = structure(after, focus)
        .into_iter()
        .filter(|f| !had.contains(f))
        .collect();
    if let Some(before) = before {
        found.extend(removed(before, after));
    }
    let order = |f: &Finding| RULES.iter().position(|r| r.id == f.rule);
    found.sort_by_key(order);
    found
}

fn structure(song: &Song, focus: Option<&str>) -> Vec<Finding> {
    let arranged: BTreeSet<usize> = song.arrange.iter().copied().collect();
    let scenes = || arranged.iter().filter_map(|s| song.scenes.get(*s));
    let mut found = Vec::new();
    if !arranged.is_empty() {
        for (i, f) in song.clips.iter().enumerate() {
            if !scenes().any(|s| s.clips.contains(&i)) {
                found.push(Finding::new(
                    "unused-clip",
                    format!("clip `{}` plays in no arranged scene: add it to a scene in `arrange`, or remove it", f.name),
                ));
            }
        }
    }
    for (t, track) in song.tracks.iter().enumerate() {
        if song.heard(t) && !tools::plays(song, t) && focus != Some(track.name.as_str()) {
            let message = if arranged.is_empty() {
                format!(
                    "track `{}` has no clip: write one for it, or remove the track",
                    track.name
                )
            } else {
                format!(
                    "track `{}` plays in no arranged scene: add one of its clips to a scene in `arrange`, or remove the track",
                    track.name
                )
            };
            found.push(Finding::new("unplayed-track", message));
        }
    }
    if !arranged.is_empty() {
        for (i, a) in song.autos.iter().enumerate() {
            if !scenes().any(|s| s.autos.contains(&i)) {
                found.push(Finding::new(
                    "unused-auto",
                    format!("auto `{}` runs in no arranged scene: add it to a scene in `arrange`, or remove it", a.name),
                ));
            }
        }
    }
    for s in scenes() {
        let sounds = s.clips.iter().filter_map(|f| song.clips.get(*f)).any(|f| {
            f.live
                || f.notes.as_ref().is_some_and(|n| !n.events.is_empty())
                || f.lanes
                    .iter()
                    .any(|l| l.steps.iter().any(|x| *x != Step::Off))
        });
        if !sounds {
            found.push(Finding::new(
                "silent-scene",
                format!("scene `{}` plays nothing in its {} bars: give it a clip with notes, unless the break is meant", s.name, s.bars),
            ));
        }
    }
    if arranged.is_empty() && !song.scenes.is_empty() {
        let names: Vec<&str> = song.scenes.iter().map(|s| s.name.as_str()).collect();
        found.push(Finding::new(
            "unarranged",
            format!("no `arrange` line, so the scenes {} are not played and every clip loops: add `arrange {}`, or remove the scenes", names.join(", "), names.join(" ")),
        ));
    }
    found
}

/// Tracks and scenes of `before` that `after` no longer has, by name.
fn removed(before: &Song, after: &Song) -> Vec<Finding> {
    let tracks = before
        .tracks
        .iter()
        .map(|t| ("track", &t.name))
        .filter(|(_, n)| !after.tracks.iter().any(|t| t.name == **n));
    let scenes = before
        .scenes
        .iter()
        .map(|s| ("scene", &s.name))
        .filter(|(_, n)| !after.scenes.iter().any(|s| s.name == **n));
    tracks
        .chain(scenes)
        .map(|(kind, n)| Finding::new("removed", format!("the {kind} `{n}` is gone")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SONG: &str = "tempo 120
track kit drums Tr909 Kit909
track bass synth Sh101 AcidBass
clip beat = kit /16
  bd x...x...x...x...
clip low = bass
  \"c2 ~ c2 g1\"
auto open = bass.Cutoff ramp 300 4000 /4
scene a 4: beat low open
arrange a
";

    fn parse(text: &str) -> Song {
        Song::parse(text).unwrap_or_else(|e| panic!("{e:?}:\n{text}"))
    }

    fn with(from: &str, to: &str) -> String {
        assert!(SONG.contains(from), "{from}");
        SONG.replacen(from, to, 1)
    }

    /// The rules `after` breaks, as a proposal for `SONG`.
    fn fired(after: &str, focus: Option<&str>) -> Vec<&'static str> {
        lint(Some(&parse(SONG)), &parse(after), focus)
            .iter()
            .map(|f| f.rule)
            .collect()
    }

    #[test]
    fn the_song_passes_every_rule() {
        assert!(fired(SONG, None).is_empty());
        assert!(lint(None, &parse(SONG), None).is_empty());
    }

    #[test]
    fn an_unused_clip_is_refused() {
        let after = with(
            "auto open",
            "clip hat = kit /16\n  ch ..x...x...x...x.\nauto open",
        );
        assert_eq!(fired(&after, None), ["unused-clip"]);
        let f = &lint(None, &parse(&after), None)[0];
        assert!(f.refuses() && f.message.contains("`hat`"), "{}", f.message);
        // Without an arrangement every clip loops.
        let looped = after
            .replace("arrange a\n", "")
            .replace("scene a 4: beat low open\n", "");
        assert!(!fired(&looped, None).contains(&"unused-clip"));
    }

    #[test]
    fn an_unplayed_track_is_refused() {
        let after = with("clip beat", "track pad synth Sh101 AcidBass\nclip beat");
        assert_eq!(fired(&after, None), ["unplayed-track"]);
        // Muted, or in focus (an audition, #430), it is meant.
        let muted = with(
            "clip beat",
            "track pad synth Sh101 AcidBass mute\nclip beat",
        );
        assert!(fired(&muted, None).is_empty(), "{:?}", fired(&muted, None));
        assert!(fired(&after, Some("pad")).is_empty());
    }

    #[test]
    fn an_unused_auto_is_refused() {
        let after = with(
            "scene a",
            "auto shut = bass.Cutoff ramp 4000 300 /4\nscene a",
        );
        assert_eq!(fired(&after, None), ["unused-auto"]);
        let used = after.replace("beat low open", "beat low open shut");
        assert!(fired(&used, None).is_empty());
    }

    #[test]
    fn a_silent_scene_is_a_warning() {
        let after = with("arrange a", "scene gap 1:\narrange a gap");
        assert_eq!(fired(&after, None), ["silent-scene"]);
        let f = &lint(None, &parse(&after), None)[0];
        assert!(!f.refuses() && f.message.contains("`gap`"), "{}", f.message);
        let rests = with(
            "arrange a",
            "clip hush = kit /16\n  bd ................\nscene gap 1: hush\narrange a gap",
        );
        assert_eq!(fired(&rests, None), ["silent-scene"]);
        let plays = with("arrange a", "scene b 1: beat\narrange a b");
        assert!(fired(&plays, None).is_empty());
    }

    #[test]
    fn scenes_without_an_arrangement_are_a_warning() {
        let after = with("arrange a\n", "");
        assert_eq!(fired(&after, None), ["unarranged"]);
        let f = &lint(None, &parse(&after), None)[0];
        assert!(
            !f.refuses() && f.message.contains("`arrange a`"),
            "{}",
            f.message
        );
        let looped = after.replace("scene a 4: beat low open\n", "");
        assert!(lint(None, &parse(&looped), None).is_empty());
    }

    #[test]
    fn removed_tracks_and_scenes_are_warnings() {
        let after =
            "tempo 120\ntrack kit drums Tr909 Kit909\nclip beat = kit /16\n  bd x...x...x...x...\n";
        let found = lint(Some(&parse(SONG)), &parse(after), None);
        let messages: Vec<&str> = found.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(
            messages,
            ["the track `bass` is gone", "the scene `a` is gone"]
        );
        assert!(found.iter().all(|f| f.rule == "removed" && !f.refuses()));
    }

    #[test]
    fn what_the_current_song_already_breaks_is_not_held_against_it() {
        let loose = with("arrange a", "scene gap 1:\narrange a gap");
        let found = lint(
            Some(&parse(&loose)),
            &parse(&loose.replace("tempo 120", "tempo 124")),
            None,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn the_rules_are_ordered_and_named_in_the_spec() {
        let spec = include_str!("../../../.openspec/specs/008-assist/spec.md");
        for r in &RULES {
            assert!(
                spec.contains(&format!("`{}`", r.id)),
                "spec 008 names `{}`",
                r.id
            );
        }
        let after = with("clip beat", "track pad synth Sh101 AcidBass\nclip beat")
            .replace("arrange a", "scene gap 1:\narrange a gap");
        assert_eq!(fired(&after, None), ["unplayed-track", "silent-scene"]);
    }
}
