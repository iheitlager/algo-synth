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
fn a_bare_chance_plays_the_bars_it_always_has() {
    // Recorded before `?p` (#602): `?` alone must not move a note.
    let starts = |t: &str| -> Vec<u32> {
        parse(t, 1)
            .unwrap()
            .events
            .iter()
            .map(|e| e.start)
            .collect()
    };
    assert_eq!(
        starts("\"c4? e4? g4? c5?\""),
        [
            0, 12, 24, 72, 84, 96, 108, 120, 168, 180, 228, 240, 264, 276, 312, 324, 348, 372
        ]
    );
    assert_eq!(
        parse("\"c4?0.5\"", 1).unwrap().events,
        parse("\"c4?\"", 1).unwrap().events
    );
}

#[test]
fn a_chance_on_a_word_plays_about_that_often() {
    let count = |t: &str| parse(t, 1).unwrap().events.len();
    // 16 draws a bar over the 8 bars a chance repeats after.
    assert_eq!(count("\"[c4?0]*16\""), 0);
    assert_eq!(count("\"[c4?1]*16\""), 128);
    let some = count("\"[c4?0.3]*16\"");
    assert!((26..=51).contains(&some), "{some} of 128");
    assert_eq!(parse("\"[c4?0.3]*16\"", 1), parse("\"[c4?0.3]*16\"", 1));
    let n = parse("\"c4?0.3 e4?.25 g4?1 a4?0\"", 1).unwrap();
    assert_eq!(n.text, "\"c4?0.3 e4?0.25 g4?1 a4?0\"");
    assert_eq!(parse(&n.text, 1).unwrap(), n);
}

#[test]
fn chance_errors_say_where() {
    for (t, col) in [
        ("\"c4?1.5\"", 5),
        ("\"c4 e4?0.1234\"", 8),
        ("\"c4?0..3\"", 5),
    ] {
        assert_eq!(
            parse(t, 1).unwrap_err(),
            NoteError {
                col,
                msg: "a chance is 0 to 1, as ?0.3"
            },
            "{t}"
        );
    }
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
        "\"c:m7& f:maj7*2&\"",
        "c:m7:4& f:maj7:4",
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
    assert!(parse(&format!("\"<{}>\"", "a4 ".repeat(65)), 1).is_err());
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
        mode: crate::algo::Mode::from_name("minor").unwrap(),
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
        mode: Mode::from_name("minor").unwrap(),
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
        mode: crate::algo::Mode::from_name("minor").unwrap(),
    }
}

/// `riff` is `c4:4 e4:4 g4:4 e4:4` for the calls that read a clip.
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
            "no note clip with this name comes before this one",
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
        word: 0,
        degree: false,
        when: Default::default(),
        nudge: 0,
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
                vel: 0,
                word: 0,
                degree: false,
                when: Default::default(),
                nudge: 0,
            },
            Event {
                start: 0,
                len: 48,
                note: 74,
                accent: false,
                vel: 0,
                word: 0,
                degree: false,
                when: Default::default(),
                nudge: 0,
            },
            Event {
                start: 6,
                len: 6,
                note: 78,
                accent: true,
                vel: 90,
                word: 0,
                degree: false,
                when: Default::default(),
                nudge: 0,
            },
            Event {
                start: 12,
                len: 24,
                note: 69,
                accent: false,
                vel: 0,
                word: 0,
                degree: false,
                when: Default::default(),
                nudge: 0,
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
        ("c4@0:0", 6, "a length is 1 to 3072 ticks"),
        ("c4@0:6:0", 8, "a velocity is 1 to 127"),
        ("c4@0:6:200", 8, "a velocity is 1 to 127"),
        ("c4@3072:6", 4, "a timed note starts within 64 bars"),
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
        word: 0,
        degree: false,
        when: Default::default(),
        nudge: 0,
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
/// a live clip's buffers never grow on the audio thread (ADR-0002).
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
    let minor = key(0, crate::algo::Mode::from_name("minor").unwrap());
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
    let cm = parse_in(prog, 1, Some(&key(0, Mode::from_name("minor").unwrap()))).unwrap();
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
    let am = parse_in(prog, 1, Some(&key(9, Mode::from_name("minor").unwrap()))).unwrap();
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
        Some(&key(0, crate::algo::Mode::from_name("major").unwrap())),
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
            Some(key(0, Mode::from_name("pentatonic").unwrap())),
            2,
            "a roman numeral needs a scale line with seven notes, as scale c minor",
        ),
        (
            "\"Iv\"",
            Some(key(0, Mode::from_name("major").unwrap())),
            2,
            "a numeral is all upper case (major) or all lower case (minor)",
        ),
        (
            "\"VIII\"",
            Some(key(0, Mode::from_name("major").unwrap())),
            2,
            "a numeral is I to VII, maybe b or #, then o, o7, +, 7 or maj7",
        ),
        (
            "\"V9\"",
            Some(key(0, Mode::from_name("major").unwrap())),
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
    let n = parse_in(text, 1, Some(&key(0, Mode::from_name("major").unwrap()))).unwrap();
    let voiced = n.clone().voiced(Voicing::default(), None);
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
    let k = key(0, crate::algo::Mode::from_name("minor").unwrap());
    let n = parse_in("arp(VI,down,8)", 1, Some(&k)).unwrap();
    assert_eq!(n.events.first().map(|e| e.note), Some(75));
    assert_eq!(n.print(), "arp([g#4,c5,d#5],down,8)");
}

// --- Progressions feed other parts (#103) --------------------------------------

/// A line read with clip `prog` (Cm Ab Eb Bb, a bar each) in scope, in C minor.
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
    // A chord by name still works where no clip has the name.
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
            "no note clip with this name comes before this one",
        ),
        ("root(prog,9)", 11, "an octave is 0 to 7"),
        (
            "root()",
            6,
            "no note clip with this name comes before this one",
        ),
        (
            "root(prog,2,1)",
            1,
            "root takes a clip and maybe an octave: root(prog) or root(prog,3)",
        ),
        ("prog(0,1)", 6, "a prog is 1 to 16 bars"),
        ("prog(17,1)", 6, "a prog is 1 to 16 bars"),
        ("prog(4)", 1, crate::usage::PROG),
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

/// #570: a number is a degree, kept as one until a scale resolves it.
#[test]
fn degrees_parse_print_back_and_wait_for_a_scale() {
    let n = parse("\"0 [2 -1] 3! [0,2,4]\"", 1).unwrap();
    let got: Vec<(u32, u8, bool, bool)> = n
        .events
        .iter()
        .map(|e| (e.start, e.note, e.accent, e.degree))
        .collect();
    let z = DEGREE_ZERO;
    assert_eq!(
        got,
        [
            (0, z, false, true),
            (12, z + 2, false, true),
            (18, z - 1, false, true),
            (24, z + 3, true, true),
            (36, z, false, true),
            (36, z + 2, false, true),
            (36, z + 4, false, true),
        ]
    );
    assert_eq!(n.print(), "\"0 [2 -1] 3! [0,2,4]\"");
    assert_eq!(parse(&n.print(), 1).unwrap(), n);
    // classic and timed lines take them too
    assert_eq!(parse("0:4 -2:8", 1).unwrap().print(), "0:4 -2:8");
    assert_eq!(parse("2@0:6", 1).unwrap().print(), "2@0:6");
    // a note is still a note
    assert!(
        parse("\"c4 0\"", 1)
            .unwrap()
            .events
            .first()
            .is_some_and(|e| !e.degree)
    );

    let mut events = n.events.clone();
    let a_minor_pent = Scale {
        root: 9,
        mode: crate::algo::Mode::from_name("minor-pentatonic").unwrap(),
    };
    resolve(&mut events, &a_minor_pent, 33);
    let notes: Vec<u8> = events.iter().map(|e| e.note).collect();
    assert_eq!(notes, [33, 38, 31, 40, 33, 38, 43]);
    assert!(events.iter().all(|e| !e.degree));
}

#[test]
fn degree_errors_say_where() {
    for (text, col, msg) in [
        ("\"0 25\"", 4, "a degree is -24 to 24"),
        (
            "\"0 -\"",
            4,
            "a note is a letter a to g, maybe # or b, and an octave 0 to 9, as c4",
        ),
        ("arp([0,2],up,8)", 5, "a generator takes notes, not degrees"),
        ("walk(0,8,1)", 6, "a generator takes notes, not degrees"),
        (
            "euclid(3,8) scale 2",
            13,
            "a walk starts from a note, as c4",
        ),
    ] {
        let e = parse_in(text, 1, Some(&minor())).expect_err(text);
        assert_eq!((e.col, e.msg), (col, msg), "{text}");
    }
}

/// #575: `0 .. 12` is every degree between, sharing the word's time.
#[test]
fn a_range_of_degrees_shares_its_slot() {
    let deg = |text: &str| -> Vec<(u32, u32, i32)> {
        parse(text, 1)
            .unwrap()
            .events
            .iter()
            .map(|e| (e.start, e.len, i32::from(e.note) - i32::from(DEGREE_ZERO)))
            .collect()
    };
    assert_eq!(
        deg("\"0 .. 3\""),
        [(0, 12, 0), (12, 12, 1), (24, 12, 2), (36, 12, 3)]
    );
    assert_eq!(deg("\"0..3\""), deg("\"0 .. 3\""));
    assert_eq!(
        deg("\"0 .. 3 5\""),
        [(0, 6, 0), (6, 6, 1), (12, 6, 2), (18, 6, 3), (24, 24, 5)]
    );
    assert_eq!(
        deg("\"2 .. -1\""),
        [(0, 12, 2), (12, 12, 1), (24, 12, 0), (36, 12, -1)]
    );
    assert_eq!(deg("\"0 .. 12\"").len(), 13);
    // suffixes after the range are the range's
    assert_eq!(deg("\"[0 .. 1]*2\"").len(), 4);
    assert_eq!(
        deg("\"0 .. 1@3 7\""),
        [(0, 18, 0), (18, 18, 1), (36, 12, 7)]
    );
    let n = parse("\"0..12 <3 .. 0 ~>\"", 1).unwrap();
    assert_eq!(n.print(), "\"0 .. 12 <3 .. 0 ~>\"");
    assert_eq!(parse(&n.print(), 1).unwrap(), n);
    // the range lights as one word
    assert_eq!(n.word_spans().first(), Some(&(1, 7)));
}

#[test]
fn range_errors_say_where() {
    for (text, col, msg) in [
        (
            "\"c4 .. e4\"",
            5,
            "a range runs from one degree to another: 0 .. 12",
        ),
        (
            "\"0 .. e4\"",
            4,
            "a range runs from one degree to another: 0 .. 12",
        ),
        (
            "\"0! .. 3\"",
            5,
            "a range runs from one degree to another: 0 .. 12",
        ),
        (
            "\".. 3\"",
            2,
            "a range runs from one degree to another: 0 .. 12",
        ),
        ("\"0 .. 30\"", 7, "a degree is -24 to 24"),
        ("\"0..30\"", 2, "a degree is -24 to 24"),
        (
            "\"0..x\"",
            2,
            "a range runs from one degree to another: 0 .. 12",
        ),
    ] {
        let e = parse(text, 1).expect_err(text);
        assert_eq!((e.col, e.msg), (col, msg), "{text}");
    }
}

/// The bars (of the line's loop) each note plays in, by note.
fn bars_of(text: &str) -> Vec<(u8, Vec<u32>)> {
    let n = parse(text, 1).unwrap();
    let mut out: Vec<(u8, Vec<u32>)> = Vec::new();
    for e in &n.events {
        match out.iter_mut().find(|(note, _)| *note == e.note) {
            Some((_, bars)) => bars.push(e.start / TICKS_PER_BAR),
            None => out.push((e.note, vec![e.start / TICKS_PER_BAR])),
        }
    }
    out
}

#[test]
fn a_ratio_plays_bar_a_of_every_b() {
    let n = parse("\"c4 e4?1:4 g4?3:4 b4?not1:4\"", 1).unwrap();
    assert_eq!(n.bars, 4);
    assert_eq!(
        bars_of(&n.text),
        [
            (60, vec![0, 1, 2, 3]),
            (64, vec![0]),
            (71, vec![1, 2, 3]),
            (67, vec![2])
        ]
    );
    assert_eq!(parse("\"c4?2:3 e4?1:2\"", 1).unwrap().bars, 6);
}

#[test]
fn pre_follows_the_last_condition_before_it() {
    // `?pre` reads the ratio, and leaves it for the next `?pre`.
    assert_eq!(
        bars_of("\"c4?1:2 e4?pre g4?notpre a4?pre\""),
        [(60, vec![0]), (64, vec![0]), (69, vec![0]), (67, vec![1])]
    );
    // Before any condition in the bar, `?pre` is false.
    assert_eq!(bars_of("\"c4?pre e4?notpre\""), [(64, vec![0])]);
}

#[test]
fn first_and_last_wait_for_the_engine() {
    let n = parse("\"c4?first e4?notlast [g4 a4]?last b4?1:1 d5?notpre\"", 1).unwrap();
    let when = |note: u8| n.events.iter().find(|e| e.note == note).unwrap().when;
    let w = |first, last| When { first, last };
    assert_eq!(when(60), w(Some(true), None));
    assert_eq!(when(64), w(None, Some(false)));
    assert_eq!(when(67), w(None, Some(true)));
    assert_eq!(when(69), w(None, Some(true)));
    assert_eq!(when(71), When::default());
    assert!(
        !n.events.iter().any(|e| e.note == 74),
        "pre of a 1:1 is true"
    );
    // `?pre` after `?first` takes it over; inside one, a word adds its own.
    let n = parse(
        "\"c4?first e4?notpre [g4?last]?first [a4?notfirst]?first\"",
        1,
    )
    .unwrap();
    let got: Vec<_> = n.events.iter().map(|e| (e.note, e.when)).collect();
    assert_eq!(
        got,
        [
            (60, w(Some(true), None)),
            (64, w(Some(false), None)),
            (67, w(Some(true), Some(true)))
        ]
    );
    assert!(
        freeze(&n.events, 1).is_none(),
        "a condition does not freeze"
    );
    assert!(w(Some(true), None).holds(true, false));
    assert!(!w(Some(true), None).holds(false, false));
    assert!(w(None, Some(false)).holds(true, false));
}

#[test]
fn conditions_print_back_and_bad_ones_say_which_there_are() {
    // Canonically, as every suffix, the condition prints last.
    let text =
        "\"c4?1:4 e4*2?not3:8 g4?pre a4?notpre b4@2?first c5?notfirst d5?last e5?notlast f5?0.3\"";
    let n = parse(text, 1).unwrap();
    assert_eq!(n.text, text);
    assert_eq!(
        parse("\"e4?not3:8*2 b4?first@2\"", 1).unwrap().text,
        "\"e4*2?not3:8 b4@2?first\""
    );
    assert_eq!(parse(&n.text, 1).unwrap(), n);
    let msg = "a condition is ?0.3, ?1:4 (1 ≤ a ≤ b ≤ 16), ?pre, ?first or ?last, \
               or ?not before one of the last four";
    for (t, col) in [
        ("\"c4?foo\"", 5),
        ("\"c4 e4?5:4\"", 8),
        ("\"c4?0:4\"", 5),
        ("\"c4?1:17\"", 5),
        ("\"c4?not0.3\"", 5),
        ("\"c4?notnot\"", 5),
    ] {
        assert_eq!(parse(t, 1).unwrap_err(), NoteError { col, msg }, "{t}");
    }
}

#[test]
fn an_inline_euclid_plays_its_word_on_the_hits() {
    let line = |t: &str| parse(t, 1).unwrap().events;
    assert_eq!(line("\"c2(3,8)\""), line("euclid(3,8) c2"));
    assert_eq!(line("\"c2(3,8,2)\""), line("euclid(3,8,2) c2"));
    assert_eq!(ev("\"c2(3,8,2)\""), [(6, 6, 36), (24, 6, 36), (36, 6, 36)]);
    // In its own share: the second half bar on (3,4), x x x ~.
    assert_eq!(
        ev("\"c4 e4(3,4)\""),
        [(0, 24, 60), (24, 6, 64), (30, 6, 64), (36, 6, 64)]
    );
    // A degree, a chord name, a chord, a group and an alternation.
    let n = parse("\"0(2,4) [c4,e4](2,4) c:maj(1,2)\"", 1).unwrap();
    let got: Vec<_> = n.events.iter().map(|e| (e.start, e.degree)).collect();
    assert_eq!(got.iter().filter(|(_, d)| *d).count(), 2);
    assert_eq!(n.events.len(), 2 + 4 + 3);
    assert_eq!(ev("\"[c4 e4](1,2)\""), [(0, 12, 60), (12, 12, 64)]);
    let alt: Vec<_> = parse("\"<c4 e4>(2,4)\"", 1)
        .unwrap()
        .events
        .iter()
        .map(|e| (e.start, e.note))
        .collect();
    assert_eq!(alt, [(0, 60), (24, 60), (48, 64), (72, 64)]);
    // The other suffixes after it: each repeat has the rhythm.
    assert_eq!(ev("\"c4(1,2)*2\""), [(0, 12, 60), (24, 12, 60)]);
}

#[test]
fn an_inline_euclid_prints_back_and_says_where_it_is_wrong() {
    let text = "\"c2(3,8) <4 5>(5,8,2) [c4,e4](3,8)*2@3?0.5 c:m7(1,4)&\"";
    let n = parse(text, 1).unwrap();
    assert_eq!(n.text, text);
    assert_eq!(parse(&n.text, 1).unwrap(), n);
    assert_eq!(parse("\"c2(3,8,0)\"", 1).unwrap().text, "\"c2(3,8)\"");
    let usage = "euclid takes hits, steps and maybe a rotation: euclid(3,8) or euclid(3,8,2)";
    for (t, col, msg) in [
        ("\"c2(3)\"", 4, usage),
        (
            "\"c2 e2(9,8)\"",
            7,
            "euclid cannot have more hits than steps",
        ),
        ("\"c2(3,0)\"", 4, "euclid has 1 to 64 steps"),
        ("\"c2(3,8\"", 4, "a ( is not closed: c2(3,8)"),
        (
            "\"[c4(3,8),e4]\"",
            5,
            "euclid goes on the whole chord: [c4,e4](3,8)",
        ),
        (
            "\"[c4,e4(3,8)]\"",
            8,
            "euclid goes on the whole chord: [c4,e4](3,8)",
        ),
        (
            "\"c2*2(3,8)\"",
            6,
            "euclid comes right after the word: c2(3,8)*2",
        ),
    ] {
        assert_eq!(parse(t, 1).unwrap_err(), NoteError { col, msg }, "{t}");
    }
}

/// Events as (start, len, note, nudge, vel).
fn graced(text: &str) -> Vec<(u32, u32, u8, i16, u8)> {
    parse(text, 1)
        .unwrap()
        .events
        .iter()
        .map(|e| (e.start, e.len, e.note, e.nudge, e.vel))
        .collect()
}

#[test]
fn graces_play_soft_before_their_note() {
    assert_eq!(
        graced("\"c4^f ~\""),
        [(0, 24, 60, 0, 0), (0, 24, 60, -20, 51)]
    );
    assert_eq!(
        graced("\"c4^d ~\""),
        [
            (0, 24, 60, 0, 0),
            (0, 24, 60, -30, 51),
            (0, 24, 60, -15, 51)
        ]
    );
    // A note takes a whole tone, a degree the next degree.
    assert_eq!(
        graced("\"~ c4^u\""),
        [(24, 24, 60, 0, 0), (24, 24, 62, -30, 51)]
    );
    assert_eq!(
        graced("\"~ c4^l\""),
        [(24, 24, 58, -30, 51), (24, 24, 60, 0, 0)]
    );
    let d = parse("\"2^u 2^l\"", 1).unwrap();
    let got: Vec<_> = d
        .events
        .iter()
        .map(|e| (e.start, e.note - DEGREE_ZERO, e.degree, e.nudge))
        .collect();
    assert_eq!(
        got,
        [
            (0, 2, true, 0),
            (0, 3, true, -30),
            (24, 1, true, -30),
            (24, 2, true, 0)
        ]
    );
    // An accent stays on the note, not its grace.
    assert!(
        parse("\"c4!^f\"", 1)
            .unwrap()
            .events
            .iter()
            .all(|e| e.accent == (e.nudge == 0))
    );
}

#[test]
fn a_mordent_fits_in_the_first_sixteenth() {
    assert_eq!(
        graced("\"c4^m ~\""),
        [(0, 1, 60, 0, 0), (1, 1, 62, 0, 0), (2, 22, 60, 0, 0)]
    );
    // In a share under a sixteenth it is the note.
    assert_eq!(graced("\"[c4^m c4]*16\"").len(), 32);
}

#[test]
fn graces_print_back_and_go_on_notes_only() {
    let text = "\"c4^u 2^l& e4^f*2 g4^d@2 a4^m?0.5 c4(3,8)^f\"";
    let n = parse(text, 1).unwrap();
    assert_eq!(n.text, text);
    assert_eq!(parse(&n.text, 1).unwrap(), n);
    assert_eq!(parse("\"c4*2^f\"", 1).unwrap().text, "\"c4^f*2\"");
    let only = "a grace goes on a note or a degree";
    for (t, col, msg) in [
        ("\"c4 ~^f\"", 6, only),
        ("\"[c4 e4]^u\"", 9, only),
        ("\"[c4,e4]^f\"", 9, only),
        ("\"c:m7^f\"", 6, only),
        ("\"0 .. 3^u\"", 4, only),
        ("\"c4^x\"", 4, "a grace is ^u, ^l, ^f, ^d or ^m"),
        ("\"c4^\"", 4, "a grace is ^u, ^l, ^f, ^d or ^m"),
    ] {
        assert_eq!(parse(t, 1).unwrap_err(), NoteError { col, msg }, "{t}");
    }
}

/// #607: `/1` to `/3` move the lowest notes up an octave; `/g` puts a g at
/// the bottom, the chord's own or one under its root.
#[test]
fn inversions_and_slash_chords_put_a_note_at_the_bottom() {
    use crate::algo::Mode;
    let n = parse("\"<c:maj7/1 c:maj7/e a:m7/g c/f# c:m7/3>\"", 1).unwrap();
    let got: Vec<Vec<u8>> = chords_of(&n).into_iter().map(|(_, c)| c).collect();
    assert_eq!(
        got,
        [
            vec![64, 67, 71, 72],
            vec![64, 67, 71, 72],
            vec![79, 81, 84, 88],
            vec![54, 60, 64, 67],
            vec![70, 72, 75, 79],
        ]
    );
    let c = key(0, Mode::from_name("major").unwrap());
    let n = parse_in("\"<V7/1 IV/1 I/e bVII/c>\"", 1, Some(&c)).unwrap();
    let got: Vec<Vec<u8>> = chords_of(&n).into_iter().map(|(_, c)| c).collect();
    assert_eq!(
        got,
        [
            vec![71, 74, 77, 79],
            vec![69, 72, 77],
            vec![64, 67, 72],
            vec![60, 70, 74, 77],
        ]
    );
    // In classic notes, the duration still last.
    let n = parse("c:m7/g:2 c/e:4", 1).unwrap();
    assert_eq!(
        chords_of(&n),
        [(0, vec![67, 70, 72, 75]), (24, vec![64, 67, 72])]
    );
    for text in ["\"<c:maj7/1 a:m7/g c/f# I/e>\"", "c:m7/g:2 V7/1:4"] {
        let n = parse_in(text, 1, Some(&c)).unwrap();
        assert_eq!(n.print(), text);
    }
}

#[test]
fn a_bad_chord_ending_says_where() {
    use crate::algo::Mode;
    let bad = "after / comes an inversion 1 to 3 or a bass note a to g, as c:m7/1 or c/g";
    let c = key(0, Mode::from_name("major").unwrap());
    for (text, col, msg) in [
        ("\"c:m7/4\"", 7, bad),
        ("\"c:m7/0\"", 7, bad),
        ("\"c:m7/h\"", 7, bad),
        ("\"c:m7/\"", 7, bad),
        (
            "\"c/3\"",
            4,
            "an inversion moves fewer notes than the chord has",
        ),
        ("\"V/V\"", 4, bad),
        ("c:m7/x:4", 6, bad),
    ] {
        let e = parse_in(text, 1, Some(&c)).unwrap_err();
        assert_eq!((e.col, e.msg), (col, msg), "{text}");
    }
}

fn voiced_chords(text: &str, v: &str, scale: Option<&Scale>) -> Vec<Vec<u8>> {
    let n = parse_in(text, 1, scale).unwrap();
    let v = Voicing::parse(v).unwrap();
    chords_of(&n.voiced(v, scale))
        .into_iter()
        .map(|(_, c)| c)
        .collect()
}

/// #606: `voicing` and `voicing(close)` are today's voicing.
#[test]
fn close_is_the_plain_voicing() {
    use crate::algo::Mode;
    let c = key(0, Mode::from_name("major").unwrap());
    let text = "\"<I vi IV V7 iii vi ii7 V>\"";
    assert_eq!(
        voiced_chords(text, "voicing", Some(&c)),
        voiced_chords(text, "voicing(close)", Some(&c))
    );
}

/// #606: each style's notes on a four-note chord and on a triad.
#[test]
fn each_voicing_style_spreads_the_close_chord() {
    use crate::algo::Mode;
    let c = key(0, Mode::from_name("major").unwrap());
    let one = |text: &str, v: &str| voiced_chords(text, v, Some(&c)).remove(0);
    for chord in ["c:maj7", "d:m7", "c"] {
        let text = format!("\"{chord}\"");
        let close = one(&text, "voicing");
        let n = close.len();
        let moved = |k: usize, by: i32| {
            let mut v: Vec<u8> = close.clone();
            v[k] = u8::try_from(i32::from(v[k]) + by).unwrap();
            v.sort_unstable();
            v
        };
        assert_eq!(one(&text, "voicing(open)"), moved(1, 12), "{chord}");
        assert_eq!(one(&text, "voicing(drop2)"), moved(n - 2, -12), "{chord}");
        let drop3 = if n >= 4 {
            moved(n - 3, -12)
        } else {
            close.clone()
        };
        assert_eq!(one(&text, "voicing(drop3)"), drop3, "{chord}");
    }
    // Shell: the root at the bottom, then the third and seventh only.
    let pcs = |c: &[u8]| c.iter().map(|n| n % 12).collect::<Vec<_>>();
    let shell = one("\"c:maj7\"", "voicing(shell)");
    assert_eq!(pcs(&shell[..1]), [0]);
    let mut upper = pcs(&shell[1..]);
    upper.sort_unstable();
    assert_eq!(upper, [4, 11]);
    assert_eq!(pcs(&one("\"c\"", "voicing(shell)")).len(), 2);
    // Quartal: fourths of the scale from the root, perfect or, from f, the
    // tritone to b.
    let steps = |c: Vec<u8>| c.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
    assert_eq!(steps(one("\"d:m7\"", "voicing(quartal)")), [5, 5, 5]);
    assert_eq!(steps(one("\"f:maj7\"", "voicing(quartal)")), [6, 5, 5]);
    assert_eq!(steps(one("\"a:m\"", "voicing(quartal)")), [5, 5]);
    assert_eq!(one("\"d:m7\"", "voicing(quartal)")[0] % 12, 2);
}

/// #606: an anchor caps every chord; the chords still move little.
#[test]
fn an_anchor_caps_the_top_and_the_voices_stay_close() {
    use crate::algo::Mode;
    let c = key(0, Mode::from_name("major").unwrap());
    let text = "\"<ii7 V7 Imaj7 vi7 IV7 iii7 ii7 V7>\"";
    for v in [
        "voicing(close,c5)",
        "voicing(drop2,c5)",
        "voicing(open,g5)",
        "voicing(shell,c4)",
        "voicing(quartal,e5)",
    ] {
        let anchor = Voicing::parse(v).unwrap().anchor.unwrap();
        let chords = voiced_chords(text, v, Some(&c));
        assert_eq!(chords.len(), 8);
        for c in &chords {
            assert!(c.iter().all(|n| *n <= anchor), "{v}: {c:?}");
            assert!(c.iter().all(|n| *n + 36 > anchor), "{v}: {c:?}");
        }
        let centre = |c: &Vec<u8>| c.iter().map(|n| f32::from(*n)).sum::<f32>() / c.len() as f32;
        for pair in chords.windows(2) {
            let d = (centre(&pair[1]) - centre(&pair[0])).abs();
            assert!(d <= 7.0, "{v}: {pair:?}");
        }
    }
}

/// #607: voicing keeps a slash chord's bass at the bottom.
#[test]
fn voicing_keeps_the_bass_at_the_bottom() {
    use crate::algo::Mode;
    let c = key(0, Mode::from_name("major").unwrap());
    let text = "\"<c:maj7/e a:m7/g c/f# I/g V7/1>\"";
    for v in [
        "voicing",
        "voicing(drop2)",
        "voicing(shell)",
        "voicing(quartal,c5)",
    ] {
        let chords = voiced_chords(text, v, Some(&c));
        for (chord, bass) in chords.iter().zip([4, 7, 6, 7, 11]) {
            assert_eq!(chord[0] % 12, bass, "{v}: {chord:?}");
            assert!(chord[0] < chord[1], "{v}: {chord:?}");
        }
    }
    // Classic notes too.
    let chords = voiced_chords("c:maj7:2 a:m7/g:2", "voicing(drop2)", Some(&c));
    assert_eq!(chords[1][0] % 12, 7);
}

#[test]
fn voicing_words_parse_and_print_back() {
    for (text, style, anchor) in [
        ("voicing", None, None),
        ("voicing(close)", Some(Style::Close), None),
        ("voicing(drop2,c5)", Some(Style::Drop2), Some(72)),
        ("voicing(quartal,f#4)", Some(Style::Quartal), Some(66)),
    ] {
        let v = Voicing::parse(text).unwrap();
        assert_eq!((v.style, v.anchor), (style, anchor), "{text}");
        assert_eq!(v.print(), text);
    }
    let styles = "a voicing is close, open, drop2, drop3, shell or quartal, then maybe an anchor note, as voicing(drop2,c5)";
    for (text, msg) in [
        ("voicing(wide)", styles),
        ("voicing()", styles),
        (
            "voicing(drop2,c)",
            "an anchor is a note with its octave, as c5",
        ),
        (
            "voicing(drop2,h5)",
            "an anchor is a note with its octave, as c5",
        ),
    ] {
        assert_eq!(Voicing::parse(text), Err(msg), "{text}");
    }
}

fn major() -> Scale {
    key(0, crate::algo::Mode::from_name("major").unwrap())
}

/// A progression's chords: start, length and notes, lowest first.
fn prog_chords(text: &str, scale: &Scale) -> Vec<(u32, u32, Vec<u8>)> {
    let n = parse_in(text, 1, Some(scale)).unwrap();
    let mut out: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    for e in &n.events {
        match out.last_mut() {
            Some((s, _, c)) if *s == e.start => c.push(e.note),
            _ => out.push((e.start, e.len, vec![e.note])),
        }
    }
    for (_, _, c) in &mut out {
        c.sort_unstable();
    }
    out
}

/// The degree (0 is the tonic) each chord is built on.
fn prog_degrees(text: &str, scale: &Scale) -> Vec<usize> {
    prog_chords(text, scale)
        .iter()
        .map(|(_, _, c)| {
            (0..7)
                .find(|d| scale.degree(*d) == Some(c[0] % 12))
                .unwrap()
        })
        .collect()
}

/// #608: a size stacks more of the scale on the same degrees.
#[test]
fn a_prog_size_stacks_sevenths_ninths_and_sus() {
    let scale = minor();
    let in_scale = [0u8, 2, 3, 5, 7, 8, 10];
    for seed in 0..30 {
        let plain = format!("prog(8,{seed})");
        let triads = prog_chords(&plain, &scale);
        assert_eq!(
            prog_chords(&format!("prog(8,{seed},triad)"), &scale),
            triads
        );
        let degrees = prog_degrees(&plain, &scale);
        for (size, notes) in [("7th", 4), ("9th", 5), ("sus", 3)] {
            let text = format!("prog(8,{seed},{size})");
            assert_eq!(
                prog_degrees(&text, &scale),
                degrees,
                "{text}: the same degrees"
            );
            for ((_, _, chord), (_, _, triad)) in prog_chords(&text, &scale).iter().zip(&triads) {
                assert_eq!(chord.len(), notes, "{text}");
                assert!(chord.iter().all(|n| in_scale.contains(&(n % 12))), "{text}");
                match size {
                    // The triad, and the seventh (and ninth) above it.
                    "7th" | "9th" => assert_eq!(chord[..3], triad[..], "{text}"),
                    _ => assert_eq!((chord[0], chord[2]), (triad[0], triad[2]), "{text}"),
                }
            }
        }
    }
    // i7 in c minor: c eb g bb; i9 adds d; isus is c f g; the last chord,
    // v, is sus2 in the scale: g ab d.
    let chords = prog_chords("prog(2,1,7th)", &scale);
    assert_eq!(chords[0].2, [60, 63, 67, 70]);
    assert_eq!(
        prog_chords("prog(2,1,9th)", &scale)[0].2,
        [60, 63, 67, 70, 74]
    );
    let sus = prog_chords("prog(2,1,sus)", &scale);
    assert_eq!(
        (sus[0].2.clone(), sus[1].2.clone()),
        (vec![60, 65, 67], vec![67, 68, 74])
    );
}

/// #609: the chords last the beats given, in turn, and are cut at the end.
#[test]
fn a_prog_rhythm_sets_the_chord_lengths() {
    let scale = minor();
    let today = parse_in("prog(4,7)", 1, Some(&scale)).unwrap().events;
    for same in ["prog(4,7,4)", "prog(4,7,\"4\")"] {
        assert_eq!(
            parse_in(same, 1, Some(&scale)).unwrap().events,
            today,
            "{same}"
        );
    }
    let two = prog_chords("prog(4,7,2)", &scale);
    assert_eq!(two.len(), 8);
    assert!(
        two.iter()
            .enumerate()
            .all(|(k, c)| c.0 == 24 * k as u32 && c.1 == 24)
    );
    for seed in 0..50 {
        let text = format!("prog(4,{seed},\"4 2 2\")");
        let got = prog_chords(&text, &scale);
        let at: Vec<(u32, u32)> = got.iter().map(|c| (c.0, c.1)).collect();
        assert_eq!(
            at,
            [(0, 48), (48, 24), (72, 24), (96, 48), (144, 24), (168, 24)]
        );
        let d = prog_degrees(&text, &scale);
        assert_eq!((d[0], d[5]), (0, 4), "{text}: i to v");
    }
    // Three beats in a bar: the second chord is cut to the bar's end.
    let cut = prog_chords("prog(1,1,3)", &scale);
    assert_eq!(
        cut.iter().map(|c| (c.0, c.1)).collect::<Vec<_>>(),
        [(0, 36), (36, 12)]
    );
    // A chord on every beat of 16 bars: 64 chords of a ninth, in bounds.
    let n = parse_in("prog(16,1,9th,1)", 1, Some(&scale)).unwrap();
    assert_eq!(n.events.len(), 320);
}

/// #610: each style starts and ends where it says, for 100 seeds.
#[test]
fn a_prog_style_has_its_own_start_end_and_moves() {
    let scale = major();
    for seed in 0..100 {
        let d =
            |style: &str, bars: u32| prog_degrees(&format!("prog({bars},{seed},{style})"), &scale);
        assert_eq!(
            d("classic", 8),
            prog_degrees(&format!("prog(8,{seed})"), &scale)
        );
        let pop = d("pop", 8);
        assert_eq!(pop[0], 0, "pop starts on I");
        assert!(matches!(pop[7], 3 | 4), "pop ends on IV or V: {pop:?}");
        let jazz = d("jazz", 8);
        assert_eq!((jazz[0], jazz[6], jazz[7]), (1, 4, 0), "ii … V I: {jazz:?}");
        let modal = d("modal", 8);
        assert_eq!((modal[0], modal[7]), (0, 0), "{modal:?}");
        assert!(!modal.contains(&4), "modal has no V: {modal:?}");
        // vii is diminished in major: modal leaves it out.
        assert!(!modal.contains(&6), "{modal:?}");
        let anda = d("andalusian", 8);
        assert_eq!((anda[0], anda[7]), (0, 4), "{anda:?}");
        let steps: Vec<usize> = anda
            .iter()
            .map(|d| [0, 6, 5, 4].iter().position(|s| s == d).unwrap())
            .collect();
        assert!(
            steps.windows(2).all(|w| w[1] >= w[0]),
            "only down: {anda:?}"
        );
        assert!((0..4).all(|s| steps.contains(&s)), "every step: {anda:?}");
        for bars in 2..4 {
            let short = d("andalusian", bars);
            assert_eq!((short[0], *short.last().unwrap()), (0, 4), "{short:?}");
        }
    }
    // In a major key the descent is I vii vi V.
    assert_eq!(prog_degrees("prog(4,1,andalusian)", &scale), [0, 6, 5, 4]);
    // Spec 002's scenario: six sevenths in the rhythm, ii first, V I last.
    let text = "prog(4,7,7th,\"4 2 2\",jazz)";
    let chords = prog_chords(text, &scale);
    let beats: Vec<u32> = chords.iter().map(|c| c.0 / 12 + 1).collect();
    assert_eq!(beats, [1, 5, 7, 9, 13, 15]);
    assert!(chords.iter().all(|c| c.2.len() == 4));
    let d = prog_degrees(text, &scale);
    assert_eq!((d[0], d[4], d[5]), (1, 4, 0));
    // In either order with a rhythm.
    let a = parse_in("prog(8,3,\"2\",pop)", 1, Some(&scale)).unwrap();
    let b = parse_in("prog(8,3,pop,2)", 1, Some(&scale)).unwrap();
    assert_eq!(a.events, b.events);
    assert_eq!(prog_chords("prog(8,3,pop,2)", &scale).len(), 16);
}

#[test]
fn prog_arguments_print_back_and_errors_say_where() {
    let scale = minor();
    for (text, printed) in [
        ("prog(4,7)", "prog(4,7)"),
        ("prog(4,7,triad,4,classic)", "prog(4,7)"),
        ("prog(4,7,7th)", "prog(4,7,7th)"),
        (
            "prog(4,7,jazz,\"4 2 2\",9th)",
            "prog(4,7,9th,\"4 2 2\",jazz)",
        ),
        ("prog(4,7,\"2\")", "prog(4,7,2)"),
        ("prog(8,3,sus,3,andalusian)", "prog(8,3,sus,3,andalusian)"),
    ] {
        let n = parse_in(text, 1, Some(&scale)).unwrap();
        assert_eq!(n.print(), printed, "{text}");
        assert_eq!(parse_in(printed, 1, Some(&scale)).unwrap().events, n.events);
    }
    let args = "after the seed a prog takes a size (triad, 7th, 9th, sus), beats (2 or \"4 2 2\") or a style (classic, pop, jazz, modal, andalusian)";
    let beats = "a chord lasts 1 to 16 beats: 2, or a list in quotes as \"4 2 2\"";
    let twice = "a prog takes one size, one rhythm and one style";
    for (text, col, msg) in [
        ("prog(4,7,13th)", 10, args),
        ("prog(4,7,0)", 10, beats),
        ("prog(4,7,17)", 10, beats),
        ("prog(4,7,\"4 x 2\")", 13, beats),
        ("prog(4,7,\"\")", 10, beats),
        ("prog(4,7,7th,9th)", 14, twice),
        ("prog(4,7,pop,jazz)", 14, twice),
        ("prog(4,7,2,\"4 4\")", 12, twice),
        ("prog(4,7,7th,2,pop,1)", 1, crate::usage::PROG),
    ] {
        let e = parse_in(text, 1, Some(&scale)).unwrap_err();
        assert_eq!((e.col, e.msg), (col, msg), "{text}");
    }
}

/// #611: `fifths` and `octave` play each chord's root on its first beat.
#[test]
fn a_bass_plays_each_root_on_its_first_beat() {
    // c, ab, eb, bb in octave 2, a bar each.
    let notes = |text: &str| -> Vec<(u32, u32, u8)> {
        with_prog(text)
            .unwrap()
            .events
            .iter()
            .map(|e| (e.start, e.len, e.note))
            .collect()
    };
    let fifths = notes("bass(prog,fifths)");
    assert_eq!(
        fifths,
        [
            (0, 24, 36),
            (24, 24, 43),
            (48, 24, 44),
            (72, 24, 51),
            (96, 24, 39),
            (120, 24, 46),
            (144, 24, 46),
            (168, 24, 53),
        ]
    );
    let octave = notes("bass(prog,octave,3)");
    assert_eq!(octave.len(), 32);
    assert_eq!(&octave[..3], [(0, 6, 48), (6, 6, 60), (12, 6, 48)]);
    assert_eq!(octave[8], (48, 6, 56), "ab3 on the second bar");
}

/// #611: a walk lands on the root at every change, a step from it the beat
/// before, round the loop too; the seed picks the line.
#[test]
fn a_walking_bass_steps_into_each_root() {
    use crate::algo::Mode;
    let roots = [36u8, 44, 39, 46];
    let mut lines = Vec::new();
    for seed in 0..50 {
        let n = with_prog(&format!("bass(prog,walk,{seed})")).unwrap();
        let notes: Vec<(u32, u8)> = n.events.iter().map(|e| (e.start, e.note)).collect();
        assert_eq!(notes.len(), 16, "a note a beat");
        for (bar, root) in roots.iter().enumerate() {
            let at = 48 * u32::try_from(bar).unwrap();
            assert_eq!(notes[bar * 4], (at, *root), "seed {seed}");
            // The beat before, round the loop for the first bar.
            let before = notes[(bar * 4 + 15) % 16].1;
            let step = (i32::from(before) - i32::from(*root)).abs();
            assert!((1..=2).contains(&step), "seed {seed}: {before} into {root}");
        }
        // c minor: every note is in the scale.
        let scale = key(0, Mode::from_name("minor").unwrap());
        assert!(
            n.events.iter().all(|e| scale.snap(e.note) == e.note),
            "seed {seed}"
        );
        assert_eq!(with_prog(&format!("bass(prog,walk,{seed})")).unwrap(), n);
        lines.push(notes);
    }
    lines.sort();
    lines.dedup();
    assert!(lines.len() > 10, "the seeds give different lines");
}

#[test]
fn bass_prints_back_and_errors_say_where() {
    for text in [
        "bass(prog,fifths)",
        "bass(prog,octave,3)",
        "bass(prog,walk,5)",
        "bass(prog,walk,3,5)",
    ] {
        assert_eq!(with_prog(text).unwrap().print(), text);
    }
    assert_eq!(
        with_prog("bass(prog,fifths,2)").unwrap().print(),
        "bass(prog,fifths)"
    );
    let seeds = "only walk takes a seed, and must: bass(prog,walk,5) or bass(prog,fifths)";
    for (text, col, msg) in [
        ("bass(prog,walk)", 1, seeds),
        ("bass(prog,fifths,2,5)", 1, seeds),
        ("bass(prog,funk)", 11, "a bass is fifths, octave or walk"),
        ("bass(prog,octave,9)", 18, "an octave is 0 to 7"),
        (
            "bass(nope,fifths)",
            6,
            "no note clip with this name comes before this one",
        ),
        ("bass(prog)", 1, crate::usage::BASS),
    ] {
        let e = with_prog(text).unwrap_err();
        assert_eq!((e.col, e.msg), (col, msg), "{text}");
    }
}
