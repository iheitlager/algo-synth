use super::*;

fn ev(text: &str) -> Vec<(u32, u32, u8)> {
    parse(text, 1)
        .unwrap()
        .events
        .iter()
        .map(|e| (e.start, e.len, e.note))
        .collect()
}

#[test]
fn a_quoted_sequence_divides_the_bar() {
    assert_eq!(
        ev("\"c4 e4 g4 c5\""),
        [(0, 12, 60), (12, 12, 64), (24, 12, 67), (36, 12, 72)]
    );
}

#[test]
fn brackets_subdivide_and_three_is_a_triplet() {
    assert_eq!(
        ev("\"c4 [e4 g4]\""),
        [(0, 24, 60), (24, 12, 64), (36, 12, 67)]
    );
    assert_eq!(
        ev("\"c4 c4 c4\""),
        [(0, 16, 60), (16, 16, 60), (32, 16, 60)]
    );
    assert_eq!(
        ev("\"[c4 d4 e4] ~\""),
        [(0, 8, 60), (8, 8, 62), (16, 8, 64)]
    );
}

#[test]
fn rests_repeats_and_weights() {
    assert_eq!(ev("\"c4 ~ ~ ~\""), [(0, 12, 60)]);
    assert_eq!(ev("\"c4*2 e4\""), [(0, 12, 60), (12, 12, 60), (24, 24, 64)]);
    assert_eq!(ev("\"c4@3 e4\""), [(0, 36, 60), (36, 12, 64)]);
}

#[test]
fn a_chord_sounds_together() {
    assert_eq!(
        ev("\"[c4,eb4,g4] ~\""),
        [(0, 24, 60), (0, 24, 63), (0, 24, 67)]
    );
}

#[test]
fn an_alternation_plays_one_option_a_bar_and_repeats() {
    let n = parse("\"<c4 d4 e4>\"", 1).unwrap();
    assert_eq!(n.bars, 3);
    let got: Vec<_> = n.events.iter().map(|e| (e.start, e.note)).collect();
    assert_eq!(got, [(0, 60), (48, 62), (96, 64)]);
}

#[test]
fn a_chance_is_drawn_from_a_fixed_seed() {
    let a = parse("\"c4? e4? g4? c5?\"", 1).unwrap();
    let b = parse("\"c4? e4? g4? c5?\"", 1).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.bars, 8);
    let all = 8 * 4;
    assert!(a.events.len() > 4 && a.events.len() < all);
}

#[test]
fn classic_durations_lay_notes_one_after_another() {
    assert_eq!(
        ev("c4:4 e4:8 g4:8 c5:2"),
        [(0, 12, 60), (12, 6, 64), (18, 6, 67), (24, 24, 72)]
    );
    let n = parse("c4:4 e4:8. g4:16 r:4", 1).unwrap();
    let got: Vec<_> = n.events.iter().map(|e| (e.start, e.len)).collect();
    assert_eq!(got, [(0, 12), (12, 9), (21, 3)]);
    assert_eq!(n.bars, 1);
    assert_eq!(parse("c4:1 c4:4", 1).unwrap().bars, 2);
}

#[test]
fn accents_set_the_velocity() {
    let n = parse("\"c4! e4\"", 1).unwrap();
    assert_eq!(
        n.events.iter().map(Event::velocity).collect::<Vec<_>>(),
        [ACCENT_VELOCITY, HIT_VELOCITY]
    );
}

#[test]
fn note_names_cover_sharps_flats_and_octaves() {
    let n: Vec<u8> = ev("\"c4 c#4 db4 bb3 b3 a4 g9\"")
        .iter()
        .map(|e| e.2)
        .collect();
    assert_eq!(n, [60, 61, 61, 58, 59, 69, 127]);
    assert_eq!(note_name(60), "c4");
    assert_eq!(note_name(61), "c#4");
}

#[test]
fn print_then_parse_is_identity() {
    for text in [
        "\"c4 [e4 g4]*2 ~ <c5 d5>?\"",
        "\"[c4,e4!,g4]@2 d#3\"",
        "c4:4 e4:8. [c4,e4,g4]:2 r:4",
        "\"c4\"",
    ] {
        let n = parse(text, 1).unwrap();
        let printed = n.print();
        assert_eq!(parse(&printed, 1).unwrap(), n, "{text}");
        assert_eq!(parse(&printed, 1).unwrap().print(), printed);
    }
}

#[test]
fn every_error_says_where() {
    for (text, col, msg) in [
        ("\"c4 e4", 7, "the quote is not closed"),
        ("\"c4 [e4 g4\"", 11, "a [ or < is not closed"),
        (
            "\"c4 x4\"",
            5,
            "a note is a letter a to g, maybe # or b, and an octave 0 to 9, as c4",
        ),
        ("\"c4 g10\"", 7, "a word ends at a space"),
        ("\"c4:4\"", 4, "durations go outside the quotes, as c4:4"),
        ("c4:4 e4", 8, "a classic note has a duration, as c4:4"),
        (
            "c4:4 ~:4",
            6,
            "mini-notation goes inside quotes, classic notes outside",
        ),
        ("c4:3", 4, "a duration is 1, 2, 4, 8 or 16"),
        ("c4:16.", 4, "a dotted sixteenth does not fit the grid"),
        ("\"c4*0\"", 4, "a repeat is 1 to 16"),
        ("\"[]\"", 3, "nothing between the brackets"),
        ("\"c4\" d4", 6, "one quoted sequence to a line"),
        ("\"[c4,e4\"", 8, "a chord is notes with commas, then ]"),
        ("\"\"", 3, "a sequence needs a word"),
        ("\"g9 a9\"", 5, "this note is out of range"),
    ] {
        let e = parse(text, 1).unwrap_err();
        assert_eq!((e.col, e.msg), (col, msg), "{text}");
    }
}

#[test]
fn too_much_is_an_error_not_a_hang() {
    let long = format!("\"{}\"", "c4 ".repeat(1200));
    assert!(parse(&long, 1).is_err());
    assert!(parse("\"<a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4 a4>\"", 1).is_err());
    assert!(parse("\"[[[[[c4]]]]]\"", 1).is_err());
}

#[test]
fn it_never_panics_on_garbage() {
    let mut rng = Rng::new(42);
    let alphabet: Vec<char> = "\"[]<>~*@?!,:.#abcdefgr0123456789 é".chars().collect();
    for _ in 0..20_000 {
        let n = rng.next_u32() % 24;
        let text: String = (0..n)
            .map(|_| alphabet[(rng.next_u32() as usize) % alphabet.len()])
            .collect();
        if let Ok(notes) = parse(&text, 1) {
            assert_eq!(parse(&notes.print(), 1).unwrap(), notes, "{text}");
        }
    }
}

#[test]
fn euclid_notes_spread_hits_over_the_bar() {
    // (3,8): hits on eighths 0, 3 and 6 of the bar, each an eighth long.
    assert_eq!(ev("euclid(3,8) c4"), [(0, 6, 60), (18, 6, 60), (36, 6, 60)]);
    assert_eq!(ev("euclid(3,8,1) c4!").len(), 3);
    let n = parse("euclid(5,8) c4", 1).unwrap();
    assert_eq!((n.bars, n.events.len()), (1, 5));
}

#[test]
fn a_scale_walk_follows_the_song_scale() {
    use crate::algo::{Mode, Scale};
    let minor = Scale {
        root: 0,
        mode: Mode::Minor,
    };
    let n = parse_in("euclid(4,8) scale c4", 1, Some(&minor)).unwrap();
    let notes: Vec<u8> = n.events.iter().map(|e| e.note).collect();
    assert_eq!(notes, [60, 62, 63, 65]);
    assert_eq!(n.print(), "euclid(4,8) scale c4");
    assert_eq!(parse_in(&n.print(), 1, Some(&minor)).unwrap(), n);
    let e = parse("euclid(4,8) scale c4", 1).unwrap_err();
    assert_eq!(
        (e.col, e.msg),
        (13, "a scale walk needs a scale line before it")
    );
}

#[test]
fn euclid_prints_canonically() {
    for (text, printed) in [
        ("euclid(3,8) c4", "euclid(3,8) c4"),
        ("euclid(3,8,0)   g#3!", "euclid(3,8) g#3!"),
        ("euclid(3,8,11) c4", "euclid(3,8,3) c4"),
    ] {
        let n = parse(text, 1).unwrap();
        assert_eq!(n.print(), printed);
        assert_eq!(parse(&n.print(), 1).unwrap(), n);
    }
}

#[test]
fn euclid_errors_say_where() {
    for (text, col, msg) in [
        (
            "euclid(9,8) c4",
            1,
            "euclid cannot have more hits than steps",
        ),
        (
            "euclid(3) c4",
            1,
            "euclid takes hits, steps and maybe a rotation: euclid(3,8) or euclid(3,8,2)",
        ),
        (
            "euclid(3,8)",
            12,
            "a note goes after the call: euclid(3,8) c4",
        ),
        (
            "euclid(3,8) x4",
            13,
            "a note is a letter a to g, maybe # or b, and an octave 0 to 9, as c4",
        ),
        ("euclid(3,8) c4 e4", 16, "a word ends at a space"),
    ] {
        let e = parse(text, 1).unwrap_err();
        assert_eq!((e.col, e.msg), (col, msg), "{text}");
    }
}

fn minor() -> crate::algo::Scale {
    crate::algo::Scale {
        root: 0,
        mode: crate::algo::Mode::Minor,
    }
}

/// `riff` is `c4:4 e4:4 g4:4 e4:4` for the calls that read a fragment.
fn with_riff(text: &str) -> Result<Notes, NoteError> {
    let riff = parse("c4:4 e4:4 g4:4 e4:4 d4:4 f4:4 a4:4 c5:4", 1).unwrap();
    parse_with(text, 1, Some(&minor()), &|name| {
        (name == "riff").then(|| (riff.events.clone(), riff.bars))
    })
}

fn pitches(n: &Notes) -> Vec<u8> {
    n.events.iter().map(|e| e.note).collect()
}

#[test]
fn an_arpeggio_cycles_the_chord_at_its_rate() {
    let up = parse("arp([g4,c4,e4],up,8)", 1).unwrap();
    assert_eq!(up.bars, 1);
    assert_eq!(pitches(&up), [60, 64, 67, 60, 64, 67, 60, 64]);
    assert_eq!(up.events[1].start, 6);
    assert_eq!(up.events[0].len, 6);
    assert_eq!(
        pitches(&parse("arp([c4,e4,g4],down,4)", 1).unwrap()),
        [67, 64, 60, 67]
    );
    assert_eq!(
        pitches(&parse("arp([c4,e4,g4],updown,4)", 1).unwrap()),
        [60, 64, 67, 64]
    );
    let a = parse("arp([c4,e4,g4],random,16,7)", 1).unwrap();
    let b = parse("arp([c4,e4,g4],random,16,7)", 1).unwrap();
    let c = parse("arp([c4,e4,g4],random,16,8)", 1).unwrap();
    assert_eq!((a.events.len(), &a), (16, &b));
    assert_ne!(pitches(&a), pitches(&c));
    assert!(pitches(&a).iter().all(|n| [60, 64, 67].contains(n)));
}

#[test]
fn a_walk_stays_on_the_scale_and_in_range() {
    let a = parse_in("walk(c4,16,1)", 1, Some(&minor())).unwrap();
    assert_eq!(a.events.len(), 16);
    let again = parse_in("walk(c4,16,1)", 1, Some(&minor())).unwrap();
    assert_eq!(a, again);
    let other = parse_in("walk(c4,16,2)", 1, Some(&minor())).unwrap();
    assert_ne!(pitches(&a), pitches(&other));
    for seed in 0..50 {
        let n = parse_in(&format!("walk(c4,32,{seed})"), 1, Some(&minor())).unwrap();
        assert_eq!(n.events.len(), 32);
        for e in &n.events {
            assert!((48..=84).contains(&e.note), "{}", e.note);
            assert!(
                [0, 2, 3, 5, 7, 8, 10].contains(&(e.note % 12)),
                "{}",
                e.note
            );
        }
        assert_eq!(
            n.events.first().map(|e| e.note),
            Some(60),
            "it starts on the start note"
        );
    }
}

#[test]
fn markov_keeps_the_rhythm_and_the_pitch_set() {
    let src = [60u8, 62, 64, 65, 67, 69, 71, 72];
    let riff = with_riff("markov(1,riff,3)").unwrap();
    let want = parse("c4:4 e4:4 g4:4 e4:4 d4:4 f4:4 a4:4 c5:4", 1).unwrap();
    assert_eq!(riff.events.len(), want.events.len());
    assert!(
        riff.events
            .iter()
            .zip(&want.events)
            .all(|(a, b)| (a.start, a.len) == (b.start, b.len))
    );
    let set: Vec<u8> = pitches(&want);
    assert!(pitches(&riff).iter().all(|n| set.contains(n)));
    assert_eq!(riff, with_riff("markov(1,riff,3)").unwrap());
    let varied =
        (0..20).any(|s| pitches(&with_riff(&format!("markov(2,riff,{s})")).unwrap()) != set);
    assert!(varied, "some seed gives a new line");
    let _ = src;
}

#[test]
fn mutate_changes_about_the_amount() {
    let base = with_riff("mutate(riff,0,1)").unwrap();
    let want = parse("c4:4 e4:4 g4:4 e4:4 d4:4 f4:4 a4:4 c5:4", 1).unwrap();
    assert_eq!(base.events, want.events, "0 percent changes nothing");
    let all = with_riff("mutate(riff,100,1)").unwrap();
    assert!(all.events.len() <= want.events.len());
    assert_ne!(all.events, want.events);
    for e in &all.events {
        assert!((57..=77).contains(&e.note));
        assert!(
            [0, 2, 3, 5, 7, 8, 10].contains(&(e.note % 12)),
            "snapped to the scale"
        );
    }
    assert_eq!(all, with_riff("mutate(riff,100,1)").unwrap());
}

#[test]
fn generator_calls_print_canonically_and_parse_back() {
    for text in [
        "arp([c4,e4,g4],up,16)",
        "arp([c4!,e4],random,8,7)",
        "walk(c4,8,1)",
        "markov(2,riff,3)",
        "mutate(riff,30,5)",
    ] {
        let n = with_riff(text).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        assert_eq!(n.print(), text);
        assert_eq!(with_riff(&n.print()).unwrap(), n);
    }
    let spaced = with_riff("arp( [c4,e4] , up , 16 )").unwrap();
    assert_eq!(spaced.print(), "arp([c4,e4],up,16)");
}

#[test]
fn generator_errors_say_where() {
    for (text, col, msg) in [
        (
            "arp([c4,e4],up)",
            1,
            "arp takes a chord, a mode, a rate and for random a seed: arp([c4,e4,g4],up,16)",
        ),
        (
            "arp([c4,e4],sideways,16)",
            13,
            "a mode is up, down, updown or random",
        ),
        ("arp([c4,e4],up,5)", 16, "a rate is 2, 4, 8 or 16"),
        (
            "arp([c4,e4],up,16,1)",
            1,
            "only random takes a seed: arp([c4,e4,g4],random,16,7)",
        ),
        (
            "arp([c4,e4],random,16)",
            1,
            "only random takes a seed: arp([c4,e4,g4],random,16,7)",
        ),
        ("arp(c4,up,16)", 5, "a chord goes here, as [c4,e4,g4]"),
        ("walk(c4,0,1)", 9, "a walk is 1 to 32 notes"),
        ("walk(c4,33,1)", 9, "a walk is 1 to 32 notes"),
        ("walk(c4,8,x)", 11, "a seed is a number"),
        ("markov(4,riff,1)", 8, "an order is 1 to 3"),
        (
            "markov(1,nope,1)",
            10,
            "no note frag with this name comes before this one",
        ),
        ("mutate(riff,101,1)", 13, "a percent is 0 to 100"),
        ("mutate(riff,5,1) x", 18, "nothing goes after a call"),
        ("walk(c4,8,1", 12, "a call ends with )"),
    ] {
        let e = with_riff(text).unwrap_err();
        assert_eq!((e.col, e.msg), (col, msg), "{text}");
    }
    let e = parse("walk(c4,8,1)", 1).unwrap_err();
    assert_eq!((e.col, e.msg), (1, "a walk needs a scale line before it"));
}

#[test]
fn damaged_generator_calls_never_panic() {
    let mut rng = Rng::new(99);
    let seeds = [
        "arp([c4,e4,g4],random,16,7)",
        "walk(c4,8,1)",
        "markov(2,riff,3)",
        "mutate(riff,30,5)",
        "euclid(3,8,1) scale c4",
    ];
    let alphabet: Vec<char> = "()[],.0123456789 acdefgrwkmuiopltxyz#!".chars().collect();
    for _ in 0..20_000 {
        let mut text: Vec<char> = seeds[(rng.next_u32() as usize) % seeds.len()]
            .chars()
            .collect();
        for _ in 0..1 + rng.next_u32() % 3 {
            let at = (rng.next_u32() as usize) % text.len();
            match rng.next_u32() % 3 {
                0 => text[at] = alphabet[(rng.next_u32() as usize) % alphabet.len()],
                1 => {
                    text.remove(at);
                }
                _ => text.insert(at, alphabet[(rng.next_u32() as usize) % alphabet.len()]),
            }
            if text.is_empty() {
                text.push('(');
            }
        }
        let text: String = text.into_iter().collect();
        if let Ok(n) = with_riff(&text) {
            assert_eq!(with_riff(&n.print()).unwrap(), n, "{text}");
        }
    }
}

#[test]
fn freezing_writes_the_events_back_exactly() {
    for text in [
        "arp([c4,e4,g4],updown,16)",
        "walk(c4,5,1)", // lengths of 9 and 10 ticks: not on a classic grid
        "walk(c4,32,4)",
        "arp([c4!,e4],random,8,7)",
        "mutate(riff,50,2)",
        "markov(2,riff,6)",
        "euclid(5,8,2) c4",
    ] {
        let n = with_riff(text).unwrap();
        let frozen = freeze(&n.events, n.bars).unwrap_or_else(|| panic!("{text}"));
        assert_eq!(frozen.events, n.events, "{text}");
        assert_eq!(frozen.bars, n.bars);
        assert!(matches!(frozen.seq, Seq::Mini(_)));
        // Printed and parsed again, the same events.
        assert_eq!(parse(&frozen.print(), 1).unwrap(), frozen, "{text}");
    }
}

#[test]
fn freezing_handles_chords_rests_and_several_bars() {
    let two = parse("[c4,e4,g4]:2 r:2 d4:1", 1).unwrap();
    assert_eq!(two.bars, 2);
    let f = freeze(&two.events, two.bars).unwrap();
    assert_eq!(f.print(), "\"<[[c4,e4,g4]@24 ~@24] [d4@48]>\"");
    assert_eq!(f.events, two.events);
    let empty = freeze(&[], 1).unwrap();
    assert_eq!(empty.print(), "\"~@48\"");
}

#[test]
fn freezing_refuses_what_it_cannot_write() {
    let ev = |start, len, note| Event {
        start,
        len,
        note,
        accent: false,
    };
    // overlapping notes, a note over the bar line, a chord of two lengths
    assert!(freeze(&[ev(0, 24, 60), ev(12, 12, 62)], 1).is_none());
    assert!(freeze(&[ev(40, 20, 60)], 1).is_none());
    assert!(freeze(&[ev(0, 12, 60), ev(0, 6, 64)], 1).is_none());
    assert!(freeze(&[ev(60, 6, 60)], 1).is_none(), "after the last bar");
}

fn ev3(start: u32, len: u32, note: u8) -> Event {
    Event {
        start,
        len,
        note,
        accent: false,
    }
}

#[test]
fn an_added_note_is_a_sixteenth_and_makes_room_for_itself() {
    let base = [ev3(0, 12, 60), ev3(24, 12, 64)];
    let got = edit(&base, 1, Edit::Add { tick: 6, note: 67 }).unwrap();
    assert_eq!(
        got,
        [ev3(0, 6, 60), ev3(6, 3, 67), ev3(24, 12, 64)],
        "the note under it is cut short"
    );
    // at the same tick as another it joins it as a chord of the same length
    let chord = edit(&base, 1, Edit::Add { tick: 24, note: 67 }).unwrap();
    assert_eq!(chord, [ev3(0, 12, 60), ev3(24, 12, 64), ev3(24, 12, 67)]);
    // adding the same note again changes nothing
    assert_eq!(
        edit(&base, 1, Edit::Add { tick: 0, note: 60 }).unwrap(),
        base
    );
    // one near the next onset is cut to fit
    let tight = edit(&base, 1, Edit::Add { tick: 22, note: 55 }).unwrap();
    assert_eq!(tight[1], ev3(22, 2, 55));
    assert!(
        edit(&base, 1, Edit::Add { tick: 48, note: 60 }).is_none(),
        "past the bar"
    );
}

#[test]
fn a_length_cannot_run_over_the_next_note_or_the_bar_line() {
    let base = [ev3(0, 3, 60), ev3(12, 3, 64)];
    let got = edit(
        &base,
        1,
        Edit::Len {
            tick: 0,
            note: 60,
            len: 30,
        },
    )
    .unwrap();
    assert_eq!(got[0].len, 12);
    let got = edit(
        &base,
        1,
        Edit::Len {
            tick: 12,
            note: 64,
            len: 99,
        },
    )
    .unwrap();
    assert_eq!(got[1].len, 36, "to the bar line");
    assert!(
        edit(
            &base,
            1,
            Edit::Len {
                tick: 12,
                note: 65,
                len: 3
            }
        )
        .is_none()
    );
    assert!(
        edit(
            &base,
            1,
            Edit::Len {
                tick: 12,
                note: 64,
                len: 0
            }
        )
        .is_none()
    );
}

#[test]
fn removing_names_a_note() {
    let base = [ev3(0, 12, 60), ev3(0, 12, 64)];
    assert_eq!(
        edit(&base, 1, Edit::Remove { tick: 0, note: 64 }).unwrap(),
        [ev3(0, 12, 60)]
    );
    assert!(edit(&base, 1, Edit::Remove { tick: 0, note: 65 }).is_none());
}

#[test]
fn every_edit_can_be_written_back_and_plays_the_same() {
    let mut r = Rng::new(5);
    let mut events: Vec<Event> = Vec::new();
    for _ in 0..400 {
        let tick = r.next_u32() % 96;
        let note = 48 + (r.next_u32() % 24) as u8;
        let op = match r.next_u32() % 3 {
            0 => Edit::Add { tick, note },
            1 => Edit::Remove { tick, note },
            _ => Edit::Len {
                tick,
                note,
                len: 1 + r.next_u32() % 40,
            },
        };
        if let Some(next) = edit(&events, 2, op) {
            events = next;
        }
        let written = freeze(&events, 2).unwrap_or_else(|| panic!("{events:?}"));
        assert_eq!(written.events, events);
    }
    assert!(!events.is_empty());
}
