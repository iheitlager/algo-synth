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
        ("\"c4*0\"", 4, "a repeat or weight is 1 to 16"),
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
    let long = format!("\"{}\"", "c4 ".repeat(200));
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
