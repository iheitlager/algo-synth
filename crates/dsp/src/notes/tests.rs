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
fn a_slide_runs_one_tick_into_the_next_note() {
    // Sixteenths are 3 ticks: the first note is 4 long and ends after the second starts.
    assert_eq!(ev("\"c2& e2 ~ g2\"")[..2], [(0, 13, 36), (12, 12, 40)]);
    // Classic: the next note still starts where the plain length says.
    assert_eq!(
        ev("c2:8& e2:8 g2:4&"),
        [(0, 7, 36), (6, 6, 40), (12, 13, 43)]
    );
    // A chord slides as a whole; a repeat slides every time.
    assert_eq!(ev("\"[c2,e2]&\""), [(0, 49, 36), (0, 49, 40)]);
    assert_eq!(ev("\"c2&*2\""), [(0, 25, 36), (24, 25, 36)]);
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
        "\"c2& e2 [g2,b2]&@2 a2&*2?\"",
        "c2:8& e2:8 [c2,e2]:4.& r:4",
        "\"Am& F*2&\"",
        "Am:4& F:4",
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
        ("\"c4 ~&\"", 6, "only a note or a chord slides"),
        ("\"c4 [e4 g4]&\"", 12, "only a note or a chord slides"),
        ("r:4&", 4, "only a note or a chord slides"),
        ("\"c4&&\"", 5, "a word ends at a space"),
        (
            "c4:4 &",
            6,
            "mini-notation goes inside quotes, classic notes outside",
        ),
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
    let alphabet: Vec<char> = "\"[]<>~*@?&!,:.#abcdefgr0123456789 émIViv+o"
        .chars()
        .collect();
    let minor = Scale {
        root: 9,
        mode: crate::algo::Mode::Minor,
    };
    for _ in 0..20_000 {
        let n = rng.next_u32() % 24;
        let text: String = (0..n)
            .map(|_| alphabet[(rng.next_u32() as usize) % alphabet.len()])
            .collect();
        if let Ok(notes) = parse(&text, 1) {
            assert_eq!(parse(&notes.print(), 1).unwrap(), notes, "{text}");
        }
        // With a key, roman numerals resolve too.
        if let Ok(notes) = parse_in(&text, 1, Some(&minor)) {
            let again = parse_in(&notes.print(), 1, Some(&minor)).unwrap();
            assert_eq!(again, notes, "{text}");
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
        (
            "arp(c4,up,16)",
            5,
            "a chord goes here, as [c4,e4,g4] or c:m7",
        ),
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
        vel: 0,
    };
    // overlapping notes, a note over the bar line, a chord of two lengths
    assert!(freeze(&[ev(0, 24, 60), ev(12, 12, 62)], 1).is_none());
    assert!(freeze(&[ev(40, 20, 60)], 1).is_none());
    assert!(freeze(&[ev(0, 12, 60), ev(0, 6, 64)], 1).is_none());
    assert!(freeze(&[ev(60, 6, 60)], 1).is_none(), "after the last bar");
}

/// #173: timed notes say exactly where each note is, may overlap and carry a
/// velocity, and print back in event order.
#[test]
fn timed_notes_parse_compile_and_print() {
    let n = parse("a4@12:24 d5@0:6 f#5!@6:6:90 d5@0:48", 1).expect("parses");
    assert_eq!(n.bars, 1);
    assert_eq!(
        n.events,
        vec![
            Event {
                start: 0,
                len: 6,
                note: 74,
                accent: false,
                vel: 0
            },
            Event {
                start: 0,
                len: 48,
                note: 74,
                accent: false,
                vel: 0
            },
            Event {
                start: 6,
                len: 6,
                note: 78,
                accent: true,
                vel: 90
            },
            Event {
                start: 12,
                len: 24,
                note: 69,
                accent: false,
                vel: 0
            },
        ]
    );
    assert_eq!(n.print(), "d5@0:6 d5@0:48 f#5!@6:6:90 a4@12:24");
    assert_eq!(parse(&n.print(), 1), Ok(n.clone()));
    assert!((n.events[2].velocity() - 90.0 / 127.0).abs() < 1e-6);
    // The line ends at the bar of its last start; a note may run past it.
    let long = parse("c4@50:200", 1).expect("parses");
    assert_eq!(long.bars, 2);
    assert_eq!(long.clone().with_bars(4).map(|n| n.bars), Ok(4));
    assert!(long.clone().with_bars(1).is_err(), "shorter than its notes");
    assert!(
        parse("c4:4", 1).expect("classic").with_bars(2).is_err(),
        "only timed lines"
    );
}

#[test]
fn timed_note_errors_say_where() {
    let cases: [(&str, usize, &str); 7] = [
        ("c4@", 4, "a number goes here"),
        ("c4@0", 5, "a length in ticks goes after :, as d5@0:6"),
        ("c4@0:0", 6, "a length is 1 to 1536 ticks"),
        ("c4@0:6:0", 8, "a velocity is 1 to 127"),
        ("c4@0:6:200", 8, "a velocity is 1 to 127"),
        ("c4@1536:6", 4, "a timed note starts within 32 bars"),
        ("c4@0:6x", 7, "a word ends at a space"),
    ];
    for (text, col, msg) in cases {
        assert_eq!(parse(text, 1), Err(NoteError { col, msg }), "{text}");
    }
}

fn ev3(start: u32, len: u32, note: u8) -> Event {
    Event {
        start,
        len,
        note,
        accent: false,
        vel: 0,
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

/// #233: every call, for every seed, fits the room `max_events` reserves, so
/// a live fragment's buffers never grow on the audio thread (ADR-0002).
#[test]
fn every_call_fits_the_room_it_reserves() {
    let notes = ["c4", "e4", "g4", "b4", "d5", "f5", "a5", "c6"];
    let mut calls = Vec::new();
    for n in 1..=notes.len() {
        let chord = notes.get(..n).unwrap_or(&[]).join(",");
        for mode in ["up", "down", "updown", "random"] {
            for rate in [2, 4, 8, 16] {
                let seed = if mode == "random" { ",1" } else { "" };
                calls.push(format!("arp([{chord}],{mode},{rate}{seed})"));
            }
        }
    }
    for steps in 1..=32 {
        calls.push(format!("walk(c4,{steps},1)"));
    }
    for order in 1..=3 {
        calls.push(format!("markov({order},riff,1)"));
    }
    for amount in [0, 25, 50, 100] {
        calls.push(format!("mutate(riff,{amount},1)"));
    }
    // #103: progressions and what they feed.
    for bars in 1..=16 {
        calls.push(format!("prog({bars},1)"));
    }
    calls.push("root(riff)".to_string());
    for mode in ["up", "down", "updown", "random"] {
        for rate in [2, 4, 8, 16] {
            let seed = if mode == "random" { ",1" } else { "" };
            calls.push(format!("arp(riff,{mode},{rate}{seed})"));
        }
    }
    for text in &calls {
        let n = with_riff(text).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        let Seq::Generated(call) = &n.seq else {
            panic!("{text} is a call");
        };
        let mut out = Vec::with_capacity(call.max_events());
        let room = out.capacity();
        for seed in 0..64 {
            call.events_into(seed, Some(&minor()), &mut out);
            assert!(out.len() <= call.max_events(), "{text} seed {seed}");
            assert_eq!(out.capacity(), room, "{text} seed {seed} grew its buffer");
        }
    }
}

// --- Chords by name (#103) -----------------------------------------------------

/// The notes starting at each tick, in order of start.
fn chords_of(n: &Notes) -> Vec<(u32, Vec<u8>)> {
    let mut out: Vec<(u32, Vec<u8>)> = Vec::new();
    for e in &n.events {
        match out.last_mut() {
            Some((s, notes)) if *s == e.start => notes.push(e.note),
            _ => out.push((e.start, vec![e.note])),
        }
    }
    for (_, notes) in &mut out {
        notes.sort_unstable();
    }
    out
}

fn key(root: u8, mode: crate::algo::Mode) -> Scale {
    Scale { root, mode }
}

#[test]
fn chord_symbols_play_their_notes() {
    let n = parse("\"<c:m7 f:maj7>\"", 1).unwrap();
    assert_eq!(n.bars, 2);
    assert_eq!(
        chords_of(&n),
        [(0, vec![60, 63, 67, 70]), (48, vec![65, 69, 72, 76])]
    );
    let n = parse("\"c bb:sus4 g3:7 d:m7b5 e:dim f#:aug a:sus2 c3:maj\"", 1).unwrap();
    let got: Vec<Vec<u8>> = chords_of(&n).into_iter().map(|(_, c)| c).collect();
    assert_eq!(
        got,
        [
            vec![60, 64, 67],
            vec![70, 75, 77],
            vec![55, 59, 62, 65],
            vec![62, 65, 68, 72],
            vec![64, 67, 70],
            vec![66, 70, 74],
            vec![69, 71, 76],
            vec![48, 52, 55],
        ]
    );
}

#[test]
fn a_classic_chord_takes_its_duration_last() {
    let n = parse("c:m7:2 g:7:4 c3:4 r:4", 1).unwrap();
    assert_eq!(
        chords_of(&n),
        [
            (0, vec![60, 63, 67, 70]),
            (24, vec![67, 71, 74, 77]),
            (36, vec![48])
        ]
    );
    assert_eq!(n.events[0].len, 24);
}

#[test]
fn chord_names_print_as_written() {
    let minor = key(0, crate::algo::Mode::Minor);
    for text in [
        "\"c:m7 bb [f:maj7 eb3:6]*2 ~\"",
        "c:m7:2 bb:4. V7:8 [c4,e4]:8",
        "\"<i VI III VII> bVII viio7 III+ ivmaj7\"",
    ] {
        let n = parse_in(text, 1, Some(&minor)).unwrap();
        assert_eq!(n.print(), text);
        assert_eq!(parse_in(&n.print(), 1, Some(&minor)).unwrap(), n);
    }
}

#[test]
fn numerals_follow_the_song_key() {
    use crate::algo::Mode;
    let prog = "\"<i VI III VII>\"";
    let cm = parse_in(prog, 1, Some(&key(0, Mode::Minor))).unwrap();
    assert_eq!(
        chords_of(&cm),
        [
            (0, vec![60, 63, 67]),
            (48, vec![68, 72, 75]),
            (96, vec![63, 67, 70]),
            (144, vec![70, 74, 77]),
        ],
        "Cm Ab Eb Bb"
    );
    let am = parse_in(prog, 1, Some(&key(9, Mode::Minor))).unwrap();
    assert_eq!(
        chords_of(&am),
        [
            (0, vec![69, 72, 76]),
            (48, vec![65, 69, 72]),
            (96, vec![60, 64, 67]),
            (144, vec![67, 71, 74]),
        ],
        "Am F C G"
    );
}

#[test]
fn a_numeral_says_its_quality_in_its_case() {
    // In C major: a borrowed minor iv, a flat VII, a dominant V7, a diminished viio.
    let n = parse_in(
        "\"iv bVII V7 viio\"",
        1,
        Some(&key(0, crate::algo::Mode::Major)),
    )
    .unwrap();
    let got: Vec<Vec<u8>> = chords_of(&n).into_iter().map(|(_, c)| c).collect();
    assert_eq!(
        got,
        [
            vec![65, 68, 72],
            vec![70, 74, 77],
            vec![67, 71, 74, 77],
            vec![71, 74, 77],
        ]
    );
}

#[test]
fn chord_errors_say_where() {
    use crate::algo::Mode;
    let quality =
        "a quality is maj, m, 7, maj7, m7, m7b5, dim, dim7, aug, sus2, sus4, 6, m6, 9, m9 or add9";
    for (text, scale, col, msg) in [
        ("\"c4 c:m13\"", None, 7, quality),
        ("c:m13:4", None, 3, quality),
        (
            "\"I\"",
            None,
            2,
            "a roman numeral needs a scale line with seven notes, as scale c minor",
        ),
        (
            "\"V\"",
            Some(key(0, Mode::Pentatonic)),
            2,
            "a roman numeral needs a scale line with seven notes, as scale c minor",
        ),
        (
            "\"Iv\"",
            Some(key(0, Mode::Major)),
            2,
            "a numeral is all upper case (major) or all lower case (minor)",
        ),
        (
            "\"VIII\"",
            Some(key(0, Mode::Major)),
            2,
            "a numeral is I to VII, maybe b or #, then o, o7, +, 7 or maj7",
        ),
        (
            "\"V9\"",
            Some(key(0, Mode::Major)),
            3,
            "a numeral is I to VII, maybe b or #, then o, o7, +, 7 or maj7",
        ),
        (
            "\"c:m7:4\"",
            None,
            6,
            "durations go outside the quotes, as c4:4",
        ),
        ("c:m7", None, 3, "a number goes here"),
    ] {
        let e = parse_in(text, 1, scale.as_ref()).unwrap_err();
        assert_eq!((e.col, e.msg), (col, msg), "{text}");
    }
}

#[test]
fn voicing_moves_each_chord_to_the_nearest_inversion() {
    use crate::algo::Mode;
    let text = "\"<I vi IV V7 iii vi ii7 V>\"";
    let n = parse_in(text, 1, Some(&key(0, Mode::Major))).unwrap();
    let voiced = n.clone().voiced();
    assert_eq!(voiced.text, n.text, "the text stays as written");
    let chords = chords_of(&voiced);
    assert_eq!(chords.len(), 8);
    for pair in chords.windows(2) {
        let (prev, next) = (&pair[0].1, &pair[1].1);
        for note in next {
            let step = prev
                .iter()
                .map(|p| (i32::from(*note) - i32::from(*p)).abs())
                .min()
                .unwrap();
            assert!(step <= 7, "{note} jumps {step} from {prev:?} to {next:?}");
        }
        assert!(next.iter().all(|n| (48..=84).contains(n)));
    }
    // The same pitch classes, only moved.
    for ((_, a), (_, b)) in chords_of(&n).iter().zip(&chords) {
        let pcs = |c: &Vec<u8>| {
            let mut v: Vec<u8> = c.iter().map(|n| n % 12).collect();
            v.sort_unstable();
            v
        };
        assert_eq!(pcs(a), pcs(b));
    }
}

#[test]
fn an_arp_takes_a_chord_by_name() {
    let n = parse("arp(c:m7,up,16)", 1).unwrap();
    let first: Vec<u8> = n.events.iter().take(4).map(|e| e.note).collect();
    assert_eq!(first, [60, 63, 67, 70]);
    let k = key(0, crate::algo::Mode::Minor);
    let n = parse_in("arp(VI,down,8)", 1, Some(&k)).unwrap();
    assert_eq!(n.events.first().map(|e| e.note), Some(75));
    assert_eq!(n.print(), "arp([g#4,c5,d#5],down,8)");
}

// --- Progressions feed other parts (#103) --------------------------------------

/// A line read with frag `prog` (Cm Ab Eb Bb, a bar each) in scope, in C minor.
fn with_prog(text: &str) -> Result<Notes, NoteError> {
    let prog = parse_in("\"<i VI III VII>\"", 1, Some(&minor())).unwrap();
    parse_with(text, 1, Some(&minor()), &|name| {
        (name == "prog").then(|| (prog.events.clone(), prog.bars))
    })
}

#[test]
fn root_plays_the_bass_of_each_chord() {
    let n = with_prog("root(prog)").unwrap();
    assert_eq!(n.bars, 4);
    let got: Vec<(u32, u32, u8)> = n.events.iter().map(|e| (e.start, e.len, e.note)).collect();
    // C2, Ab2, Eb2, Bb2, a whole bar each.
    assert_eq!(
        got,
        [(0, 48, 36), (48, 48, 44), (96, 48, 39), (144, 48, 46)]
    );
    let up = with_prog("root(prog,3)").unwrap();
    assert_eq!(up.events[0].note, 48);
    for text in ["root(prog)", "root(prog,3)"] {
        assert_eq!(with_prog(text).unwrap().print(), text);
    }
}

#[test]
fn an_arp_over_a_progression_follows_its_chords() {
    let n = with_prog("arp(prog,up,4)").unwrap();
    assert_eq!(n.bars, 4);
    assert_eq!(
        pitches(&n),
        [
            60, 63, 67, 60, 68, 72, 75, 68, 63, 67, 70, 63, 70, 74, 77, 70
        ],
        "each chord from its bottom, a quarter a note"
    );
    for text in ["arp(prog,updown,16)", "arp(prog,random,8,3)"] {
        let a = with_prog(text).unwrap();
        assert_eq!(a.print(), text);
        if let Seq::Generated(g) = &a.seq {
            assert!(a.events.len() <= g.max_events());
        }
    }
    // A chord by name still works where no frag has the name.
    assert_eq!(
        pitches(&with_prog("arp(c:m,up,4)").unwrap()),
        [60, 63, 67, 60]
    );
}

#[test]
fn a_prog_walks_the_functions_from_tonic_to_dominant() {
    for seed in 0..50 {
        let text = format!("prog(8,{seed})");
        let n = parse_in(&text, 1, Some(&minor())).unwrap();
        assert_eq!((n.bars, n.events.len(), n.print()), (8, 24, text.clone()));
        let chords: Vec<Vec<u8>> = (0..8)
            .map(|b| {
                let mut c: Vec<u8> = n
                    .events
                    .iter()
                    .filter(|e| e.start == b * 48)
                    .map(|e| e.note)
                    .collect();
                c.sort_unstable();
                c
            })
            .collect();
        assert_eq!(chords[0], [60, 63, 67], "seed {seed}: starts on i");
        assert_eq!(chords[7], [67, 70, 74], "seed {seed}: ends on v");
        // Every chord is a triad of the scale.
        let scale = [0u8, 2, 3, 5, 7, 8, 10];
        assert!(
            chords.iter().flatten().all(|n| scale.contains(&(n % 12))),
            "seed {seed}"
        );
        assert_eq!(
            parse_in(&text, 1, Some(&minor())).unwrap(),
            n,
            "deterministic"
        );
    }
    let a = parse_in("prog(8,1)", 1, Some(&minor())).unwrap();
    let b = parse_in("prog(8,2)", 1, Some(&minor())).unwrap();
    assert_ne!(a.events, b.events, "the seed changes the progression");
}

#[test]
fn progression_errors_say_where() {
    for (text, col, msg) in [
        (
            "root(nope)",
            6,
            "no note frag with this name comes before this one",
        ),
        ("root(prog,9)", 11, "an octave is 0 to 7"),
        (
            "root()",
            6,
            "no note frag with this name comes before this one",
        ),
        (
            "root(prog,2,1)",
            1,
            "root takes a frag and maybe an octave: root(prog) or root(prog,3)",
        ),
        ("prog(0,1)", 6, "a prog is 1 to 16 bars"),
        ("prog(17,1)", 6, "a prog is 1 to 16 bars"),
        (
            "prog(4)",
            1,
            "prog takes a number of bars and a seed: prog(4,7)",
        ),
    ] {
        let e = with_prog(text).unwrap_err();
        assert_eq!((e.col, e.msg), (col, msg), "{text}");
    }
    let e = parse("prog(4,1)", 1).unwrap_err();
    assert_eq!(
        (e.col, e.msg),
        (
            1,
            "a prog needs a scale line with seven notes, as scale c minor"
        )
    );
}
