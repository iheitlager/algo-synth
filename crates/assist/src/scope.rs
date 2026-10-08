//! A focus on one instrument (#415): with a track in focus, a proposed song
//! may change that track and what is aimed at it (its `track` line, its strip
//! line, its frags, and autos, mods and scene values on it), nothing else.

use algo_dsp::song::{Mix, Song, Target};

/// Why `after` changed more than the track `track` of `before`, or `Ok` when
/// it did not. A `before` that does not parse or has no such track leaves
/// nothing to hold the proposal to.
pub fn check(before: &str, after: &str, track: &str) -> Result<(), String> {
    let Ok(old) = Song::parse(before) else {
        return Ok(());
    };
    let Some(t) = old.tracks.iter().position(|x| x.name == track) else {
        return Ok(());
    };
    let new = Song::parse(after).map_err(|e| e.msg.to_string())?;
    let names = |s: &Song| s.tracks.iter().map(|x| x.name.clone()).collect::<Vec<_>>();
    if names(&old) != names(&new) {
        return Err(format!(
            "only the track `{track}` may change: keep the tracks {} as they are, in that order",
            names(&old).join(", ")
        ));
    }
    let (was, is) = (outside(&old, t), outside(&new, t));
    let mut changed: Vec<&str> = was
        .iter()
        .filter(|i| !is.contains(i))
        .chain(is.iter().filter(|i| !was.contains(i)))
        .map(|(label, _)| label.as_str())
        .collect();
    changed.dedup();
    if changed.is_empty() {
        return Ok(());
    }
    Err(format!(
        "only the track `{track}` may change (its track line, its strip, its frags, and autos, mods and scene values on it), but this changed: {}; put those back as they were",
        changed.join(", ")
    ))
}

/// Everything in `song` that is not about track `t`, as labelled items to
/// compare. Indices are read as names, so a frag added to `t` moves nothing.
fn outside(song: &Song, t: usize) -> Vec<(String, String)> {
    let mut items = vec![
        ("tempo".into(), format!("{:?}", song.tempo)),
        ("swing".into(), format!("{:?}", song.swing)),
        ("scale".into(), format!("{:?}", song.scale)),
        (
            "arrange".into(),
            format!(
                "{:?} {:?}",
                song.arrange
                    .iter()
                    .map(|&s| song.sections.get(s).map(|x| &x.name))
                    .collect::<Vec<_>>(),
                song.loop_bars
            ),
        ),
    ];
    let setting = |i: Option<usize>| i.and_then(|i| song.settings.get(i)).map(|s| &s.name);
    for (i, tr) in song.tracks.iter().enumerate().filter(|&(i, _)| i != t) {
        items.push((
            format!("track {}", tr.name),
            format!(
                "{:?} {:?} {:?} {} {}",
                tr.kind,
                tr.preset,
                setting(tr.setting),
                tr.mute,
                tr.solo
            ),
        ));
        if let Some(s) = tr.setting.and_then(|s| song.settings.get(s)) {
            items.push((format!("setting {}", s.name), format!("{s:?} {i}")));
        }
    }
    let ours = |f: usize| song.frags.get(f).is_some_and(|f| f.track == t);
    let on_us = |target: &Target| *target == Target::Track(t);
    for f in song.frags.iter().filter(|f| f.track != t) {
        items.push((format!("frag {}", f.name), format!("{f:?}")));
    }
    for m in song.mix.iter().filter(|m| m.at != Mix::Track(t)) {
        let label = match m.at {
            Mix::Track(i) => format!("strip {}", song.tracks.get(i).map_or("?", |x| &x.name)),
            Mix::Strip(n) => format!("strip strip{}", n + 1),
            Mix::Group(n) => format!("group {}", n + 1),
            Mix::Master => "master".into(),
        };
        items.push((label, format!("{m:?}")));
    }
    for a in song.autos.iter().filter(|a| !on_us(&a.target)) {
        items.push((format!("auto {}", a.name), format!("{a:?}")));
    }
    for s in &song.scenes {
        let sets: Vec<_> = s
            .sets
            .iter()
            .filter(|(target, ..)| !on_us(target))
            .collect();
        items.push((format!("scene {}", s.name), format!("{sets:?}")));
    }
    for m in song
        .mods
        .iter()
        .filter(|m| !on_us(&m.target) && !m.frag.is_some_and(ours))
    {
        let frag = m.frag.and_then(|f| song.frags.get(f)).map(|f| &f.name);
        items.push((
            format!("mod on {:?} {:?}", m.target, m.param),
            format!("{:?} {frag:?}", m.signal),
        ));
    }
    for s in &song.sections {
        let frags: Vec<_> = s
            .frags
            .iter()
            .filter(|&&f| !ours(f))
            .filter_map(|&f| song.frags.get(f).map(|f| &f.name))
            .collect();
        let autos: Vec<_> = s
            .autos
            .iter()
            .filter_map(|&a| song.autos.get(a))
            .filter(|a| !on_us(&a.target))
            .map(|a| &a.name)
            .collect();
        let scenes: Vec<_> = s
            .scenes
            .iter()
            .filter_map(|&c| song.scenes.get(c).map(|c| &c.name))
            .collect();
        items.push((
            format!("section {}", s.name),
            format!("{} {frags:?} {autos:?} {scenes:?}", s.bars),
        ));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    const SONG: &str = "tempo 120
track kit drums Tr909 Kit909
track bass synth Sh101 AcidBass
strip kit: Level 0.9
strip bass: Level 0.8
frag beat = kit /16
  bd x...x...x...x...
frag low = bass
  \"c2 ~ c2 g1\"
auto open = bass.Cutoff ramp 300 4000 /4
section a 4: beat low open
arrange a
";

    fn with(from: &str, to: &str) -> String {
        assert!(SONG.contains(from), "{from}");
        SONG.replacen(from, to, 1)
    }

    #[test]
    fn the_focused_track_may_change() {
        Song::parse(SONG).expect("the test song parses");
        for after in [
            with("Sh101 AcidBass", "Minimoog MiniBass"),
            with("strip bass: Level 0.8", "strip bass: Level 0.5, Pan -0.3"),
            with("\"c2 ~ c2 g1\"", "\"c2 c3 c2 g1\""),
            with("ramp 300 4000", "ramp 200 5000"),
            with("auto open", "frag high = bass\n  \"c3 ~ g2 ~\"\nauto open")
                .replace("beat low open", "beat low high open"),
        ] {
            Song::parse(&after).expect("the changed song parses");
            assert_eq!(check(SONG, &after, "bass"), Ok(()), "{after}");
        }
    }

    #[test]
    fn anything_else_is_named_and_refused() {
        for (after, named) in [
            (with("tempo 120", "tempo 124"), "tempo"),
            (with("Tr909 Kit909", "Tr808 Kit808"), "track kit"),
            (
                with("strip kit: Level 0.9", "strip kit: Level 0.7"),
                "strip kit",
            ),
            (
                with("bd x...x...x...x...", "bd x.x.x...x...x."),
                "frag beat",
            ),
            (
                with("section a 4: beat low open", "section a 4: low open"),
                "section a",
            ),
            (
                with("track bass", "track lead synth\ntrack bass"),
                "keep the tracks",
            ),
        ] {
            Song::parse(&after).expect("the changed song parses");
            let err = check(SONG, &after, "bass").expect_err(&after);
            assert!(err.contains(named), "{named}: {err}");
        }
    }

    #[test]
    fn no_such_track_holds_nothing() {
        let after = with("tempo 120", "tempo 124");
        assert_eq!(check(SONG, &after, "pad"), Ok(()));
    }
}
