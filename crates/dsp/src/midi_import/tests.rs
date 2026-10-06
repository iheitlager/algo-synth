use super::*;
use crate::smf::{self, tests::file};
use crate::song::Song;

const END: [u8; 4] = [0x00, 0xFF, 0x2F, 0x00];

/// A delta time as a MIDI variable-length quantity.
fn vlq(mut n: u32) -> Vec<u8> {
    let mut out = vec![(n & 0x7F) as u8];
    n >>= 7;
    while n > 0 {
        out.insert(0, (n & 0x7F) as u8 | 0x80);
        n >>= 7;
    }
    out
}

/// A track named `name` of (tick, on, channel, note, velocity), at 480 a quarter.
fn track(name: &str, events: &[(u32, bool, u8, u8, u8)]) -> Vec<u8> {
    let mut t = vec![0x00, 0xFF, 0x03, name.len() as u8];
    t.extend(name.as_bytes());
    let mut at = 0;
    for (tick, on, ch, note, vel) in events {
        t.extend(vlq(tick - at));
        at = *tick;
        t.extend([if *on { 0x90 } else { 0x80 } | ch, *note, *vel]);
    }
    t.extend(END);
    t
}

fn import_file(tracks: &[Vec<u8>]) -> Result<Imported, ImportError> {
    import(&smf::parse(&file(1, 480, tracks)).expect("a file"))
}

/// #173: overlapping notes keep their own lengths and velocities, on the grid.
#[test]
fn notes_land_on_the_grid_with_their_lengths() {
    let t = track(
        "Violin I",
        &[
            (0, true, 0, 74, 100),
            (240, true, 0, 78, 64), // an eighth in, overlapping
            (480, false, 0, 78, 0),
            (1920, false, 0, 74, 0), // a whole bar long
            (1935, true, 0, 69, 90), // 15 ticks of 480 late: rounds to the bar line
            (2160, false, 0, 69, 0),
        ],
    );
    let imp = import_file(&[t]).expect("imports");
    assert_eq!(imp.channels, vec![0]);
    assert!(imp.text.contains("track violin_i synth\n"), "{}", imp.text);
    assert!(
        imp.text
            .contains("frag violin_i_1 = violin_i bars 2\n  d5@0:48:100 f#5@6:6:64 a4@48:6:90\n"),
        "{}",
        imp.text
    );
    assert!(
        imp.text.contains("section s1 2: violin_i_1\narrange s1\n"),
        "{}",
        imp.text
    );
    let song = Song::parse(&imp.text).expect("the text is a song");
    assert_eq!(Song::parse(&song.print()), Ok(song), "and prints back");
}

/// Repeats in a score share their fragment and their section.
#[test]
fn identical_chunks_share_a_fragment_and_a_section() {
    let bar = 1920;
    let mut ev = Vec::new();
    for b in 0..24u32 {
        // bars 1–8 and 17–24 the same, 9–16 different
        let note = if (8..16).contains(&b) { 67 } else { 60 };
        ev.push((b * bar, true, 2, note, 80));
        ev.push((b * bar + 480, false, 2, note, 0));
    }
    let imp = import_file(&[track("Bass", &ev)]).expect("imports");
    assert!(imp.text.contains("arrange s1 s2 s1\n"), "{}", imp.text);
    assert_eq!(imp.text.matches("\nfrag ").count(), 2, "{}", imp.text);
    assert_eq!(imp.channels, vec![2]);
}

/// A dense part gets shorter chunks so a line stays within its notes.
#[test]
fn a_dense_part_gets_shorter_chunks() {
    let mut ev = Vec::new();
    // 8 bars of 96 notes a bar (sixteenth triplets at 20 ticks each): 768 in 8 bars.
    for i in 0..768u32 {
        ev.push((i * 20, true, 0, 60, 80));
        ev.push((i * 20 + 10, false, 0, 60, 0));
    }
    let imp = import_file(&[track("Fast", &ev)]).expect("imports");
    assert!(
        imp.text.contains("bars 4\n"),
        "two 4-bar chunks, not one of 768 notes"
    );
    Song::parse(&imp.text).expect("a song");
}

#[test]
fn no_notes_is_an_error_and_names_are_made_safe() {
    assert_eq!(
        import_file(&[track("Empty", &[])]),
        Err(ImportError::NoNotes)
    );
    let a = track(
        "1st & 2nd!",
        &[(0, true, 0, 60, 80), (480, false, 0, 60, 0)],
    );
    let b = track(
        "1st & 2nd!",
        &[(0, true, 1, 62, 80), (480, false, 1, 62, 0)],
    );
    let c = track("", &[(0, true, 5, 64, 80)]); // never released: ends with the file
    let imp = import_file(&[a, b, c]).expect("imports");
    assert!(
        imp.text
            .contains("track t_1st_2nd synth\ntrack t_1st_2nd_2 synth\ntrack ch6 synth\n"),
        "{}",
        imp.text
    );
    Song::parse(&imp.text).expect("a song");
}

/// Whatever the bytes, a song or an error.
#[test]
fn never_panics_on_odd_files() {
    let mut x = 0x2468_ACE1_u32;
    for len in [0, 3, 10, 40, 200] {
        let body: Vec<u8> = (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect();
        if let Ok(smf) = smf::parse(&file(1, 96, &[body])) {
            if let Ok(imp) = import(&smf) {
                Song::parse(&imp.text).expect("an import is always a song");
            }
        }
    }
}

/// #295: the song lasts until its last note ends, so a note held past the
/// last start rings out instead of being cut at that start's bar.
#[test]
fn a_held_last_note_rings_out() {
    // A note held 30 bars, and a short one in bar 1.
    let t = track(
        "Pad",
        &[
            (0, true, 0, 60, 100),
            (0, true, 0, 64, 100),
            (480, false, 0, 64, 0),
            (480 * 4 * 30, false, 0, 60, 0),
        ],
    );
    let imp = import_file(&[t]).expect("imports");
    let song = Song::parse(&imp.text).expect("parses");
    let bars: u32 = song
        .arrange
        .iter()
        .filter_map(|s| song.sections.get(*s).map(|sec| sec.bars))
        .sum();
    assert_eq!(bars, 30, "{}", imp.text);
}

/// #295: a tempo change keeps each note at its moment in the file: the grid
/// is at the first tempo, and only the bar lines drift from the file's.
#[test]
fn a_tempo_change_keeps_notes_at_their_moments() {
    let tempo = |us: u32| [0xFF, 0x51, 3, (us >> 16) as u8, (us >> 8) as u8, us as u8];
    // 120 BPM, then 60 from bar 2 (tick 1920).
    let mut map = vec![0x00];
    map.extend(tempo(500_000));
    map.extend(vlq(1920));
    map.extend(tempo(1_000_000));
    map.extend(END);
    let notes = track(
        "Lead",
        &[
            (0, true, 0, 60, 100),
            (480, false, 0, 60, 0),
            (1920, true, 0, 62, 100),
            (2400, false, 0, 62, 0), // a quarter at 60: one second
            (2400, true, 0, 64, 100),
            (2880, false, 0, 64, 0),
        ],
    );
    let imp = import_file(&[map, notes]).expect("imports");
    assert!(imp.text.starts_with("tempo 120\n"), "{}", imp.text);
    // At 120 a second is 24 ticks of the grid: d4 at bar 2, e4 a second later.
    assert!(
        imp.text.contains("c4@0:12:100 d4@48:24:100 e4@72:24:100"),
        "{}",
        imp.text
    );
}
