use super::*;

const BEAT: &str = "\
# a comment
tempo 124
swing 56
track kit drums

frag beat = kit /16
  bd  x...x...x...x...   # four on the floor
  sn  ....X.......X..x
  ch  x.x. x.x. x.x. x.x.
";

#[test]
fn a_beat_parses() {
    let s = Song::parse(BEAT).expect("parses");
    assert_eq!((s.tempo, s.swing), (124.0, 56.0));
    assert_eq!(
        s.tracks,
        vec![Track {
            name: "kit".into(),
            kind: Kind::Drums
        }]
    );
    let f = &s.frags[0];
    assert_eq!((f.name.as_str(), f.track, f.lanes.len()), ("beat", 0, 3));
    assert_eq!(f.lanes[0].pad, Pad::Bd);
    assert_eq!(f.lanes[0].steps.len(), 16);
    assert_eq!(f.lanes[1].steps[4], Step::Accent);
    assert_eq!(f.lanes[1].steps[15], Step::Hit);
    assert_eq!(f.lanes[2].steps.len(), 16, "spaces only split for reading");
}

#[test]
fn the_print_is_canonical_and_parses_back() {
    let s = Song::parse(BEAT).expect("parses");
    let text = s.print();
    assert_eq!(
        text,
        "tempo 124\nswing 56\ntrack kit drums\n\nfrag beat = kit /16\n  bd x...x...x...x...\n  sn ....X.......X..x\n  ch x.x.x.x.x.x.x.x.\n"
    );
    assert_eq!(Song::parse(&text), Ok(s));
}

#[test]
fn an_empty_text_is_an_empty_song() {
    assert_eq!(Song::parse(""), Ok(Song::default()));
    assert_eq!(Song::parse("\n  \n# nothing\n"), Ok(Song::default()));
    assert_eq!(Song::parse(&Song::default().print()), Ok(Song::default()));
}

#[test]
fn every_error_says_where() {
    let cases: [(&str, usize, usize, &str); 18] = [
        ("tempo", 1, 6, "a number goes here"),
        ("tempo 10", 1, 7, "tempo is 20 to 300"),
        ("tempo fast", 1, 7, "tempo is 20 to 300"),
        ("tempo 120 now", 1, 11, "unexpected text"),
        ("swing 80", 1, 7, "swing is 50 to 75"),
        (
            "track 9kit drums",
            1,
            7,
            "a name is a letter, then letters, digits or _",
        ),
        (
            "track kit bass",
            1,
            11,
            "a track kind is drums, synth or sampler",
        ),
        (
            "track kit drums\ntrack kit drums",
            2,
            7,
            "there is already a track with this name",
        ),
        ("frag a = kit", 1, 10, "no track has this name"),
        ("track kit drums\nfrag a kit", 2, 8, "= and a track go here"),
        (
            "track kit drums\nfrag a = kit /8\n  bd x",
            2,
            14,
            "only /16 steps for now",
        ),
        (
            "track kit drums\nfrag a = kit\n  zz x...",
            3,
            3,
            "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc or hc",
        ),
        (
            "track kit drums\nfrag a = kit\n  bd x..o",
            3,
            9,
            "a step is x, X or .",
        ),
        (
            "track kit drums\nfrag a = kit\n  bd",
            3,
            3,
            "a lane needs its steps: x, X or .",
        ),
        (
            "track kit drums\nfrag a = kit\n  bd x\n  bd x",
            4,
            3,
            "this pad already has a lane",
        ),
        (
            "track kit drums\nfrag a = kit\ntempo 120",
            2,
            1,
            "a frag needs at least one lane",
        ),
        ("  bd x...", 1, 3, "a lane goes under a frag"),
        (
            "play a",
            1,
            1,
            "a line starts with tempo, swing, scale, track, frag, section, arrange or loop",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text),
            Err(SongError { line, col, msg }),
            "{text:?}"
        );
    }
}

#[test]
fn limits_hold() {
    let long = format!("track kit drums\nfrag a = kit\n  bd {}", "x".repeat(65));
    assert_eq!(
        Song::parse(&long).map_err(|e| e.msg),
        Err("a lane has at most 64 steps")
    );
    let tracks: String = (0..17).map(|i| format!("track t{i} drums\n")).collect();
    assert_eq!(
        Song::parse(&tracks).map_err(|e| (e.line, e.msg)),
        Err((17, "a song has at most 16 tracks"))
    );
}

#[test]
fn set_step_changes_one_step() {
    let mut s = Song::parse(BEAT).expect("parses");
    assert!(s.set_step(0, 0, 1, Step::Accent));
    assert!(s.print().contains("  bd xX..x...x...x...\n"));
    assert!(!s.set_step(0, 0, 16, Step::Hit), "past the lane");
    assert!(!s.set_step(0, 3, 0, Step::Hit), "no fourth lane");
    assert!(!s.set_step(1, 0, 0, Step::Hit), "no second frag");
    assert_eq!(Step::from_level(3), None);
}

/// A small xorshift for generated cases: no new dependency, and repeatable.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        self.next() as usize % n.max(1)
    }
}

fn random_song(r: &mut Rng) -> Song {
    let mut song = Song {
        tempo: 20.0 + r.below(2801) as f32 / 10.0,
        swing: 50.0 + r.below(26) as f32,
        ..Song::default()
    };
    for t in 0..1 + r.below(3) {
        song.tracks.push(Track {
            name: format!("t{t}"),
            kind: if t == 1 { Kind::Synth } else { Kind::Drums },
        });
    }
    for f in 0..r.below(5) {
        if song.tracks.len() > 1 && r.below(3) == 0 {
            let seq = ["\"c4 [e4 g4]*2 ~\"", "c4:4 e4:8. r:8", "\"<c4 d4> g4?\""][r.below(3)];
            song.frags.push(Fragment {
                name: format!("f{f}"),
                track: 1,
                lanes: Vec::new(),
                notes: Some(notes::parse(seq, 1).expect("valid")),
                live: false,
            });
            continue;
        }
        let mut pads: Vec<Pad> = Pad::ALL.iter().map(|(p, _)| *p).collect();
        let mut lanes = Vec::new();
        for _ in 0..1 + r.below(8) {
            let pad = pads.remove(r.below(pads.len()));
            let steps = (0..1 + r.below(MAX_STEPS))
                .map(|_| [Step::Off, Step::Hit, Step::Accent][r.below(3)])
                .collect();
            lanes.push(Lane {
                pad,
                steps,
                call: None,
            });
        }
        song.frags.push(Fragment {
            name: format!("f{f}"),
            track: 0,
            lanes,
            notes: None,
            live: false,
        });
    }
    song
}

#[test]
fn print_then_parse_is_identity() {
    let mut r = Rng(0x1234_5678);
    for _ in 0..1000 {
        let song = random_song(&mut r);
        assert_eq!(Song::parse(&song.print()), Ok(song));
    }
}

#[test]
fn never_panics_on_garbage() {
    let mut r = Rng(0x9E37_79B9);
    let alphabet = b"tempo swing scale c minor track frag drums synth euclid(3,8,1) = /16 bd sn ch c4 xX.#\n\t 0123456789-+e\xc3\xa9";
    for _ in 0..3000 {
        let n = r.below(200);
        let bytes: Vec<u8> = (0..n).map(|_| alphabet[r.below(alphabet.len())]).collect();
        // Whatever parses prints back to the same song.
        if let Ok(song) = Song::parse(&String::from_utf8_lossy(&bytes)) {
            assert_eq!(Song::parse(&song.print()), Ok(song));
        }
    }
    // And valid songs with a few characters changed.
    for _ in 0..1000 {
        let mut text: Vec<char> = random_song(&mut r).print().chars().collect();
        for _ in 0..1 + r.below(4) {
            if !text.is_empty() {
                let at = r.below(text.len());
                text[at] = alphabet[r.below(alphabet.len())] as char;
            }
        }
        let text: String = text.into_iter().collect();
        if let Ok(song) = Song::parse(&text) {
            assert_eq!(Song::parse(&song.print()), Ok(song));
        }
    }
}

const RIFF: &str = "\
track bass synth
track kit drums

frag riff = bass
  \"c4 [e4 g4] ~ <c5 d5>?\"   # a sharp is not a comment: c#4
frag line = bass
  c#4:4 e4:8. [c4,e4,g4]:2 r:8
frag beat = kit
  bd x...
";

#[test]
fn note_fragments_parse_and_print_back() {
    let s = Song::parse(RIFF).expect("parses");
    assert_eq!(s.tracks[0].kind, Kind::Synth);
    assert_eq!(s.frags[0].notes.as_ref().map(|n| n.bars), Some(8));
    assert!(s.frags[0].lanes.is_empty());
    assert_eq!(s.frags[2].notes, None);
    let text = s.print();
    assert!(text.contains("track bass synth\ntrack kit drums\n"));
    assert!(text.contains("frag riff = bass\n  \"c4 [e4 g4] ~ <c5 d5>?\"\n"));
    assert!(text.contains("frag line = bass\n  c#4:4 e4:8. [c4,e4,g4]:2 r:8\n"));
    assert_eq!(Song::parse(&text), Ok(s));
}

#[test]
fn note_errors_say_line_and_column() {
    let cases: [(&str, usize, usize, &str); 8] = [
        (
            "track b synth\nfrag a = b\n  \"c4 x4\"",
            3,
            7,
            "a note is a letter a to g, maybe # or b, and an octave 0 to 9, as c4",
        ),
        (
            "track b synth\nfrag a = b\n  c4:4 \"e4\"",
            3,
            8,
            "mini-notation goes inside quotes, classic notes outside",
        ),
        (
            "track b synth\nfrag a = b\n  \"c4:4\"",
            3,
            6,
            "durations go outside the quotes, as c4:4",
        ),
        (
            "track b synth\nfrag a = b\n  c4:4 e4",
            3,
            10,
            "a classic note has a duration, as c4:4",
        ),
        (
            "track b synth\nfrag a = b\n  c4:4\n  e4:4",
            4,
            3,
            "a note frag is one line of notes",
        ),
        (
            "track b synth\nfrag a = b\nfrag c = b\n  c4:4",
            2,
            1,
            "a frag needs a line of notes",
        ),
        (
            "track b synth\nfrag a = b /16\n  c4:4",
            2,
            12,
            "a note frag has no step grid",
        ),
        (
            "track b drums\nfrag a = b\n  c4:4",
            3,
            3,
            "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc or hc",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text),
            Err(SongError { line, col, msg }),
            "{text:?}"
        );
    }
}

#[test]
fn a_comment_starts_at_a_space_not_at_a_sharp() {
    assert_eq!(strip_comment("  bd x... # kick"), "  bd x... ");
    assert_eq!(strip_comment("c#4:4 # up"), "c#4:4 ");
    assert_eq!(strip_comment("# all"), "");
}

const SAMPLED: &str = "\
track pads sampler
track keys sampler

frag hits = pads
  bd x...x...
  sn ....X...
frag tune = keys
  c4:4 e4:8 g4:8
frag mini = keys
  \"c4 [e4 g4]\"
";

#[test]
fn sampler_tracks_hold_lanes_or_notes() {
    let s = Song::parse(SAMPLED).expect("parses");
    assert_eq!(s.tracks[0].kind, Kind::Sampler);
    assert_eq!(s.frags[0].lanes.len(), 2);
    assert!(s.frags[0].notes.is_none());
    assert!(s.frags[1].lanes.is_empty() && s.frags[1].notes.is_some());
    let text = s.print();
    assert!(text.contains("track pads sampler\ntrack keys sampler\n"));
    assert!(text.contains("frag hits = pads /16\n  bd x...x...\n  sn ....X...\n"));
    assert_eq!(Song::parse(&text), Ok(s));
}

#[test]
fn sampler_errors_say_where() {
    let cases: [(&str, usize, usize, &str); 4] = [
        (
            "track p sampler\nfrag a = p\n  zz x...",
            3,
            3,
            "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc or hc",
        ),
        (
            "track p sampler\nfrag a = p\n  bd x...\n  c4:4",
            4,
            3,
            "a frag holds lanes or notes, not both",
        ),
        (
            "track p sampler\nfrag a = p\n  c4:4\n  bd x...",
            4,
            3,
            "a frag holds lanes or notes, not both",
        ),
        (
            "track p sampler\nfrag a = p\ntempo 120",
            2,
            1,
            "a frag needs lanes or a line of notes",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text),
            Err(SongError { line, col, msg }),
            "{text:?}"
        );
    }
}

const GENERATED: &str = "\
tempo 120
scale c minor
track kit drums
track lead synth

frag beat = kit
  bd euclid(3,8)
  sn euclid(5,16,2)
  ch x.x.
frag line = lead
  euclid(5,8) scale c4
frag pulse = lead
  euclid(3,8,1) g3!
";

#[test]
fn generators_and_scales_parse_and_print_back() {
    let s = Song::parse(GENERATED).expect("parses");
    assert_eq!(s.scale.map(|k| (k.root, k.mode.name())), Some((0, "minor")));
    let lanes = &s.frags[0].lanes;
    assert_eq!(lanes[0].steps.len(), 8);
    assert_eq!(
        lanes[0].steps.iter().filter(|x| **x == Step::Hit).count(),
        3
    );
    assert!(lanes[0].call.is_some() && lanes[2].call.is_none());
    let text = s.print();
    assert!(text.starts_with("tempo 120\nswing 50\nscale c minor\ntrack kit drums\n"));
    assert!(text.contains("  bd euclid(3,8)\n  sn euclid(5,16,2)\n  ch x.x.\n"));
    assert!(text.contains("  euclid(5,8) scale c4\n"));
    assert_eq!(Song::parse(&text), Ok(s));
}

#[test]
fn editing_a_generated_step_turns_it_into_written_steps() {
    let mut s = Song::parse(GENERATED).expect("parses");
    assert!(s.set_step(0, 0, 1, Step::Hit));
    assert!(s.print().contains("  bd xx.x..x.\n"));
}

#[test]
fn generator_errors_say_where() {
    let cases: [(&str, usize, usize, &str); 7] = [
        (
            "scale h minor",
            1,
            7,
            "a root is a letter a to g, maybe # or b",
        ),
        (
            "scale c funky",
            1,
            9,
            "a mode is major, minor, dorian, phrygian, lydian, mixolydian, locrian, pentatonic or blues",
        ),
        ("scale c", 1, 8, "a mode goes here: scale c minor"),
        ("scale c minor\nscale d major", 2, 1, "a song has one scale"),
        (
            "track t drums\nfrag a = t\n  bd x\nscale c minor",
            4,
            1,
            "the scale goes before the frags",
        ),
        (
            "track t drums\nfrag a = t\n  bd euclid(9,8)",
            3,
            6,
            "euclid cannot have more hits than steps",
        ),
        (
            "track t synth\nfrag a = t\n  euclid(3,8) scale c4",
            3,
            15,
            "a scale walk needs a scale line before it",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text),
            Err(SongError { line, col, msg }),
            "{text:?}"
        );
    }
}

#[test]
fn generator_calls_read_earlier_frags_and_print_back() {
    let text = "\
scale d dorian
track lead synth

frag riff = lead
  d4:4 f4:8 a4:8 c5:2
frag arp = lead
  arp([d4,f4,a4],updown,16)
frag wander = lead
  walk(d4,8,3)
frag learned = lead
  markov(1,riff,9)
frag changed = lead
  mutate(riff,40,2)
";
    let s = Song::parse(text).expect("parses");
    assert_eq!(s.frags[3].notes.as_ref().map(|n| n.events.len()), Some(4));
    let printed = s.print();
    for call in [
        "arp([d4,f4,a4],updown,16)",
        "walk(d4,8,3)",
        "markov(1,riff,9)",
        "mutate(riff,40,2)",
    ] {
        assert!(printed.contains(call), "{call}");
    }
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn a_call_cannot_read_a_later_or_missing_frag() {
    let text = "track t synth\nfrag a = t\n  mutate(b,10,1)\nfrag b = t\n  c4:4\n";
    assert_eq!(
        Song::parse(text),
        Err(SongError {
            line: 3,
            col: 10,
            msg: "no note frag with this name comes before this one"
        })
    );
    let own = "track t synth\nfrag a = t\n  markov(1,a,1)\n";
    assert!(Song::parse(own).is_err());
}

#[test]
fn a_live_frag_prints_and_parses_back() {
    let text = "scale c minor\ntrack lead synth\n\nfrag w = lead live\n  walk(c4,8,1)\nfrag s = lead\n  arp([c4,e4],up,8)\n";
    let s = Song::parse(text).expect("parses");
    assert!(s.frags[0].live && !s.frags[1].live);
    let printed = s.print();
    assert!(printed.contains("frag w = lead live\n  walk(c4,8,1)\n"));
    assert!(printed.contains("frag s = lead\n"));
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn live_errors_say_where() {
    let cases: [(&str, usize, usize, &str); 3] = [
        (
            "track k drums\nfrag a = k live\n  bd x",
            2,
            12,
            "only a note frag can be live",
        ),
        (
            "track t synth\nfrag a = t live\n  c4:4",
            3,
            3,
            "a live frag is a call: arp, walk, markov or mutate",
        ),
        (
            "track t synth\nfrag a = t live\n  euclid(3,8) c4",
            3,
            3,
            "a live frag is a call: arp, walk, markov or mutate",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text),
            Err(SongError { line, col, msg }),
            "{text:?}"
        );
    }
}

#[test]
fn freezing_replaces_the_call_with_notes() {
    let mut s = Song::parse(
        "scale c minor\ntrack t synth\nfrag w = t live\n  walk(c4,4,1)\nfrag p = t\n  c4:4\n",
    )
    .unwrap();
    assert!(!s.freeze(1, None), "a written frag has no call");
    assert!(!s.freeze(9, None), "no such frag");
    let before = s.frags[0].notes.as_ref().unwrap().events.clone();
    assert!(s.freeze(0, None));
    assert!(!s.frags[0].live);
    let printed = s.print();
    assert!(!printed.contains("walk(") && !printed.contains("live"));
    assert_eq!(
        Song::parse(&printed).unwrap().frags[0]
            .notes
            .as_ref()
            .unwrap()
            .events,
        before
    );
}

/// An electro cut in the spirit of Egyptian Lover, Green Velvet and Drexciya:
/// an 808 kit, an acid line, chord stabs and a lead that drifts.
const ELECTRO: &str = "\
tempo 124
swing 54
scale a minor

track kit drums
track bass synth
track lead synth

frag beat = kit
  bd x..x..x...x..x..
  cp ....x.......x...
  ch x.x.x.x.x.x.x.x.
  oh ..x...x...x...x.
  cb euclid(5,16,2)

frag acid = bass
  \"a1! a1 [a2 a1] ~ a1! ~ [c2 e2] a1\"

frag stabs = lead
  \"~ [a3!,c4!,e4!] ~ ~ ~ [a3,c4,e4] ~ ~\"

frag arp = lead
  arp([a3,c4,e4,g4],updown,16)

frag drift = lead live
  walk(a3,16,7)
";

#[test]
fn an_electro_example_parses_and_prints_back() {
    let s = Song::parse(ELECTRO).expect("parses");
    assert_eq!((s.tracks.len(), s.frags.len()), (3, 5));
    assert_eq!(Song::parse(&s.print()), Ok(s));
}

const ARRANGED: &str = "\
track kit drums
frag beat = kit /16
  bd x...x...x...x...
frag fill = kit /16
  sn ..x.
section intro 2: beat
section main 4 : beat fill
section gap 1:
arrange intro main main gap
loop 3 6
";

/// Spec 002 Req 4: sections, the arrangement and a loop region parse and
/// print canonically.
#[test]
fn sections_and_the_arrangement_parse_and_print() {
    let s = Song::parse(ARRANGED).expect("parses");
    assert_eq!(s.sections.len(), 3);
    assert_eq!(
        s.sections[1],
        Section {
            name: "main".into(),
            bars: 4,
            frags: vec![0, 1]
        }
    );
    assert_eq!(
        s.sections[2].frags,
        Vec::<usize>::new(),
        "an empty section is silent bars"
    );
    assert_eq!(s.arrange, vec![0, 1, 1, 2]);
    assert_eq!(s.loop_bars, Some((3, 6)));
    assert_eq!(s.bars(), 11);
    let text = s.print();
    assert!(text.ends_with("section intro 2: beat\nsection main 4: beat fill\nsection gap 1:\narrange intro main main gap\nloop 3 6\n"), "{text}");
    assert_eq!(Song::parse(&text), Ok(s));
}

#[test]
fn a_step_falls_in_its_section_and_the_loop_wraps() {
    let s = Song::parse(ARRANGED).expect("parses");
    assert_eq!(
        s.at(0),
        At::In {
            entry: 0,
            section: 0,
            local: 0
        }
    );
    assert_eq!(
        s.at(31),
        At::In {
            entry: 0,
            section: 0,
            local: 31
        }
    );
    assert_eq!(
        s.at(32),
        At::In {
            entry: 1,
            section: 1,
            local: 0
        }
    );
    // Bars 3 to 6 are steps 32..96; step 96 wraps back to 32.
    assert_eq!(
        s.at(96),
        At::In {
            entry: 1,
            section: 1,
            local: 0
        }
    );
    assert_eq!(
        s.at(96 + 63),
        At::In {
            entry: 1,
            section: 1,
            local: 63
        }
    );
    let mut no_loop = s.clone();
    no_loop.loop_bars = None;
    assert_eq!(
        no_loop.at(96),
        At::In {
            entry: 2,
            section: 1,
            local: 0
        },
        "main again, from its start"
    );
    assert_eq!(
        no_loop.at(160),
        At::In {
            entry: 3,
            section: 2,
            local: 0
        }
    );
    assert_eq!(no_loop.at(176), At::End);
    assert_eq!(
        Song::parse("track kit drums").expect("parses").at(7),
        At::Free(7)
    );
}

#[test]
fn arrangement_errors_say_where() {
    let head = "track kit drums\nfrag b = kit\n  bd x\n";
    let cases: [(&str, usize, usize, &str); 12] = [
        (
            "section 1a 2: b",
            4,
            9,
            "a name is a letter, then letters, digits or _",
        ),
        ("section a", 4, 10, "a number of bars and : go here"),
        ("section a 2 b", 4, 13, ": goes here, after the bars"),
        ("section a 0: b", 4, 11, "a section is 1 to 256 bars"),
        ("section a 300: b", 4, 11, "a section is 1 to 256 bars"),
        ("section a 2: c", 4, 14, "no frag has this name"),
        (
            "section a 2: b b",
            4,
            16,
            "this frag is already in the section",
        ),
        (
            "section a 2: b\nsection a 1: b",
            5,
            9,
            "there is already a section with this name",
        ),
        ("arrange x", 4, 9, "no section has this name"),
        (
            "section a 2: b\narrange",
            5,
            8,
            "sections go here, in the order they play",
        ),
        (
            "section a 2: b\narrange a\nloop 2 3",
            6,
            1,
            "the loop ends after the arrangement",
        ),
        ("loop 1 1", 4, 1, "a loop needs an arrange line"),
    ];
    for (tail, line, col, msg) in cases {
        let err = Song::parse(&format!("{head}{tail}")).expect_err(tail);
        assert_eq!((err.line, err.col, err.msg), (line, col, msg), "{tail}");
    }
    let err =
        Song::parse(&format!("{head}section a 1:\narrange a\nloop 3 2")).expect_err("backwards");
    assert_eq!(err.msg, "the last bar comes after the first");
    let err = Song::parse(&format!("{head}section a 1:\narrange a\narrange a")).expect_err("two");
    assert_eq!(err.msg, "a song has one arrange line");
}
