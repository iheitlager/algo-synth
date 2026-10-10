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
            kind: Kind::Drums,
            preset: Some(Preset::Kit808),
            setting: None,
            picked: true,
            mute: false,
            solo: false,
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
        "# a comment\ntempo 124\nswing 56\ntrack kit drums Tr808 Kit808\n\nfrag beat = kit /16\n  bd x...x...x...x... # four on the floor\n  sn ....X.......X..x\n  ch x.x.x.x.x.x.x.x.\n"
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
            "a drum grid is /12, /16, /24, /32 or /48",
        ),
        (
            "track kit drums\nfrag a = kit\n  zz x...",
            3,
            3,
            "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr or rd",
        ),
        (
            "track kit drums\nfrag a = kit\n  bd x..z",
            3,
            9,
            "a step is x, X, o, f, d or .",
        ),
        (
            "track kit drums\nfrag a = kit\n  bd",
            3,
            3,
            "a lane needs its steps: x, X, o, f, d or .",
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
            "a line starts with tempo, swing, scale, setting, track, strip, group, master, frag, auto, scene, mod, section, arrange or loop",
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
    assert!(
        s.print()
            .contains("  bd xX..x...x...x... # four on the floor\n")
    );
    assert!(!s.set_step(0, 0, 16, Step::Hit), "past the lane");
    assert!(!s.set_step(0, 3, 0, Step::Hit), "no fourth lane");
    assert!(!s.set_step(1, 0, 0, Step::Hit), "no second frag");
    assert_eq!(Step::from_level(9), None);
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
            // A written preset prints and parses back as itself.
            preset: Some(match (t, r.below(2)) {
                (1, 0) => Preset::MiniLead,
                (1, _) => Preset::JunoPad,
                (_, 0) => Preset::Kit808,
                _ => Preset::Hard909,
            }),
            setting: None,
            picked: false,
            mute: false,
            solo: false,
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
                voicing: false,
                pattern: Vec::new(),
                grid: 16,
            });
            continue;
        }
        let mut pads: Vec<Pad> = Pad::ALL.iter().map(|(p, _)| *p).collect();
        let mut lanes = Vec::new();
        for _ in 0..1 + r.below(8) {
            let pad = pads.remove(r.below(pads.len()));
            let steps = (0..1 + r.below(MAX_STEPS))
                .map(|_| [Step::Off, Step::Hit, Step::Accent, Step::Ghost][r.below(4)])
                .collect();
            lanes.push(Lane {
                pad,
                steps,
                call: None,
                ratchets: Vec::new(),
            });
        }
        song.frags.push(Fragment {
            name: format!("f{f}"),
            track: 0,
            lanes,
            notes: None,
            live: false,
            voicing: false,
            pattern: Vec::new(),
            grid: GRIDS[r.below(GRIDS.len())],
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
    assert!(text.contains("track bass synth Minimoog MiniBass\ntrack kit drums Tr808 Kit808\n"));
    assert!(text.contains(
        "frag riff = bass\n  \"c4 [e4 g4] ~ <c5 d5>?\" # a sharp is not a comment: c#4\n"
    ));
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
            "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr or rd",
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
    let cases: [(&str, usize, usize, &str); 5] = [
        (
            "track p sampler\nfrag a = p\n  zz x...",
            3,
            3,
            "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr or rd",
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
        // #395: a grid is for lanes; a frag of notes has none, as on a synth.
        (
            "track s sampler Sampler\nfrag f = s /24\n  \"c4 e4 g4\"",
            2,
            12,
            "a note frag has no step grid",
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
fn sampler_tracks_hold_generators() {
    // #251: a call is notes on a sampler track too, never a lane.
    let text = "scale c minor\ntrack keys sampler\n\nfrag w = keys live\n  walk(c4,8,1)\nfrag a = keys\n  arp([c4,e4,g4],up,16)\nfrag e = keys\n  euclid(3,8) c4\n";
    let s = Song::parse(text).expect("parses");
    assert!(
        s.frags
            .iter()
            .all(|f| f.notes.is_some() && f.lanes.is_empty())
    );
    assert!(s.frags[0].live);
    let printed = s.print();
    assert!(printed.contains("frag w = keys live\n  walk(c4,8,1)\n"));
    assert!(printed.contains("  euclid(3,8) c4\n"));
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn bars_is_not_for_lanes() {
    // #251: `bars N` before lanes was dropped; it is an error at the `bars`.
    assert_eq!(
        Song::parse("track p sampler\nfrag a = p bars 2\n  bd x..."),
        Err(SongError {
            line: 2,
            col: 12,
            msg: "bars N is for a line of timed notes"
        })
    );
}

#[test]
fn a_song_has_one_tempo_and_one_swing() {
    // #251: as with scale and arrange, a second line is an error, not a winner.
    let cases = [
        ("tempo 120\ntempo 130", "a song has one tempo"),
        ("swing 50\ntempo 120\nswing 60", "a song has one swing"),
    ];
    for (text, msg) in cases {
        let line = text.lines().count();
        assert_eq!(
            Song::parse(text),
            Err(SongError { line, col: 1, msg }),
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
    assert!(text.starts_with("tempo 120\nswing 50\nscale c minor\ntrack kit drums Tr808 Kit808\n"));
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
            "a mode is major, minor, dorian, phrygian, lydian, mixolydian, locrian, pentatonic, blues, phrygian-dominant or harmonic-minor",
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
            "a live frag is a call: arp, walk, markov, mutate, root or prog",
        ),
        (
            "track t synth\nfrag a = t live\n  euclid(3,8) c4",
            3,
            3,
            "a live frag is a call: arp, walk, markov, mutate, root or prog",
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

/// The songs in `examples/` parse, print back equal and are arranged; all but
/// a drum study move a filter's cutoff and resonance, by lane or modulation,
/// and the SH-101 plays in most of them.
#[test]
fn the_example_songs_parse_and_print_back() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let (mut seen, mut sh101) = (0, 0);
    for entry in std::fs::read_dir(&dir).expect("examples dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "song") {
            continue;
        }
        let name = path.display().to_string();
        let text = std::fs::read_to_string(&path).expect("reads");
        let s = Song::parse(&text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(Song::parse(&s.print()), Ok(s.clone()), "{name}");
        assert!(!s.arrange.is_empty(), "{name}: arranged");
        seen += 1;
        // A drum study (all its tracks drums) has no bass to automate.
        if s.tracks.iter().all(|t| t.kind == Kind::Drums) {
            continue;
        }
        if s.tracks
            .iter()
            .any(|t| t.preset.is_some_and(|p| p.model() == Model::Sh101))
        {
            sh101 += 1;
        }
        // A lane or a modulation (a `mod` line or a frag's method).
        let moves = |param| {
            s.autos.iter().any(|a| a.param == param) || s.mods.iter().any(|m| m.param == param)
        };
        assert!(moves(Param::Cutoff), "{name}: the cutoff moves");
        assert!(moves(Param::Resonance), "{name}: the resonance moves");
    }
    assert_eq!(seen, 15, "fifteen examples");
    assert!(2 * sh101 > seen, "the SH-101 in most: {sh101} of {seen}");
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
            frags: vec![0, 1],
            autos: vec![],
            scenes: vec![],
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
        (
            "section a 2: c",
            4,
            14,
            "no frag, auto or [scene] has this name",
        ),
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

const AUTOMATED: &str = "\
track kit drums
frag beat = kit /16
  bd x...
auto sweep = kit.Cutoff ramp 300 4000 /2
auto duck = strip3.Level 1 0.5 0.25 1 /1
auto wet = master.P2Return 0 0.4 /4
scene drop: strip1.Mute 1, group2.Send1 0.5, master.P2Return 0.4
section a 2: beat sweep [drop]
section b 1: duck wet
arrange a b
";

/// ADR-0015: automation lanes and scenes, by parameter name on a target,
/// parse and print canonically.
#[test]
fn automation_and_scenes_parse_and_print() {
    let s = Song::parse(AUTOMATED).expect("parses");
    assert_eq!(s.autos.len(), 3);
    assert_eq!(s.autos[0].target, Target::Track(0));
    assert_eq!(s.autos[0].param, Param::Cutoff);
    assert_eq!(s.autos[0].shape, Shape::Ramp(300.0, 4000.0));
    assert_eq!(s.autos[1].target, Target::Strip(2));
    assert_eq!(s.autos[1].shape, Shape::Steps(vec![1.0, 0.5, 0.25, 1.0]));
    assert_eq!(s.autos[2].target, Target::Master);
    assert_eq!(
        s.scenes[0].sets,
        vec![
            (Target::Strip(0), Param::Mute, 1.0),
            (Target::Strip(17), Param::Send1, 0.5),
            (Target::Master, Param::P2Return, 0.4)
        ]
    );
    assert_eq!(
        (s.sections[0].autos.clone(), s.sections[0].scenes.clone()),
        (vec![0], vec![0])
    );
    assert_eq!(s.sections[1].autos, vec![1, 2]);
    let text = s.print();
    assert!(
        text.contains("auto sweep = kit.Cutoff ramp 300 4000 /2\n"),
        "{text}"
    );
    assert!(
        text.contains("scene drop: strip1.Mute 1, group2.Send1 0.5, master.P2Return 0.4\n"),
        "{text}"
    );
    assert!(text.contains("section a 2: beat sweep [drop]\n"), "{text}");
    assert_eq!(Song::parse(&text), Ok(s));
}

/// ADR-0019: a parameter is named in any case, as `kit.cutoff` or
/// `kit.Cutoff`, and prints by its registry name.
#[test]
fn a_parameter_name_is_read_in_any_case() {
    let lower = AUTOMATED
        .replace("kit.Cutoff", "kit.cutoff")
        .replace("master.P2Return 0.4", "master.p2return 0.4");
    assert_eq!(Song::parse(&lower), Song::parse(AUTOMATED));
}

#[test]
fn a_lane_steps_or_ramps_over_its_length_and_loops() {
    let s = Song::parse(AUTOMATED).expect("parses");
    let (ramp, steps) = (&s.autos[0], &s.autos[1]);
    assert_eq!(ramp.value_at(0.0), 300.0);
    assert_eq!(ramp.value_at(16.0), 2150.0, "half way over two bars");
    assert!((ramp.value_at(31.999) - 4000.0).abs() < 1.0);
    assert_eq!(ramp.value_at(32.0), 300.0, "and round again");
    assert_eq!(
        [0.0, 3.9, 4.0, 8.0, 12.0, 15.9].map(|t| steps.value_at(t)),
        [1.0, 1.0, 0.5, 0.25, 1.0, 1.0]
    );
}

#[test]
fn automation_errors_say_where() {
    let head = "track kit drums\nfrag b = kit\n  bd x\n";
    let cases: [(&str, usize, usize, &str); 11] = [
        (
            "auto a = strip1.Level 1 /0",
            4,
            25,
            "a length in bars goes last: /1 to /256",
        ),
        (
            "auto a = strip1.Level /1",
            4,
            23,
            "values or a ramp go before the length",
        ),
        (
            "auto a = strip1.Nope 1 /1",
            4,
            10,
            "no parameter has this name",
        ),
        (
            "auto a = strip17.Level 1 /1",
            4,
            10,
            "a target is a track, strip1–16, group1–8 or master",
        ),
        (
            "auto a = group1.Cutoff 1 /1",
            4,
            10,
            "this parameter does not belong to this target",
        ),
        (
            "auto a = master.Level 1 /1",
            4,
            10,
            "this parameter does not belong to this target",
        ),
        (
            "auto a = kit.Model 1 /1",
            4,
            10,
            "the model and the routing can't be automated",
        ),
        (
            "auto a = kit.Cutoff ramp 1 /1",
            4,
            21,
            "a ramp goes from one value to another",
        ),
        (
            "auto b = kit.Cutoff 1 /1",
            4,
            6,
            "there is already a frag or auto with this name",
        ),
        (
            "scene s: strip1.Mute 1 strip2.Mute 1",
            4,
            24,
            "a comma goes between settings",
        ),
        ("section a 1: [none]", 4, 14, "no scene has this name"),
    ];
    for (tail, line, col, msg) in cases {
        let err = Song::parse(&format!("{head}{tail}")).expect_err(tail);
        assert_eq!((err.line, err.col, err.msg), (line, col, msg), "{tail}");
    }
}

/// #171: arranger edits keep the song valid and print back as text.
#[test]
fn arranger_edits_change_the_song_and_its_text() {
    let mut s = Song::parse(AUTOMATED).expect("parses");
    // Toggle: take the beat out of section a, put the lane duck in, the scene out.
    assert!(s.toggle(0, 0, 0));
    assert!(s.toggle(0, 1, 1));
    assert!(s.toggle(0, 2, 0));
    assert_eq!(
        (
            s.sections[0].frags.clone(),
            s.sections[0].autos.clone(),
            s.sections[0].scenes.clone()
        ),
        (vec![], vec![0, 1], vec![])
    );
    assert!(
        !s.toggle(0, 0, 9) && !s.toggle(9, 0, 0) && !s.toggle(0, 3, 0),
        "no such item, section or kind"
    );
    // Add a section: named partN, appended to the arrangement.
    assert_eq!(s.add_section(4), Some(2));
    assert_eq!(
        (s.sections[2].name.as_str(), s.arrange.clone()),
        ("part1", vec![0, 1, 2])
    );
    assert_eq!(s.add_section(0), None);
    // Insert, move and remove entries.
    assert!(s.arrange_insert(0, 1));
    assert_eq!(s.arrange, vec![1, 0, 1, 2]);
    assert!(s.arrange_move(0, 3));
    assert_eq!(s.arrange, vec![0, 1, 2, 1]);
    assert!(!s.arrange_insert(9, 0) && !s.arrange_insert(0, 9) && !s.arrange_move(0, 9));
    assert_eq!(s.bars(), 2 + 1 + 4 + 1);
    // Loop, then shrink the song under it: the loop goes.
    assert!(s.set_loop(3, 7) && !s.set_loop(3, 9) && !s.set_loop(0, 2));
    assert!(s.arrange_remove(2));
    assert_eq!(s.loop_bars, None, "bar 7 is gone");
    assert!(s.set_loop(1, 2) && s.set_loop(0, 0));
    assert_eq!(s.loop_bars, None);
    assert!(s.set_bars(2, 8) && !s.set_bars(2, 0));
    // The text says it all, and parses back to the same song.
    let text = s.print();
    assert!(
        text.contains("section a 2: sweep duck\n")
            && text.contains("section part1 8:\n")
            && text.contains("arrange a b b\n"),
        "{text}"
    );
    assert_eq!(Song::parse(&text), Ok(s));
}

#[test]
fn editing_a_note_rewrites_the_text() {
    use crate::notes::Edit;
    let mut s = Song::parse("track t synth\nfrag a = t\n  c4:4 e4:4 g4:2\n").unwrap();
    assert!(s.edit_note(0, Edit::Add { tick: 36, note: 72 }));
    let text = s.print();
    assert!(
        text.contains("frag a = t\n  \"c4@12 e4@12 g4@12 c5@3 ~@9\"\n"),
        "{text}"
    );
    assert_eq!(Song::parse(&text), Ok(s.clone()));
    assert!(s.edit_note(0, Edit::Remove { tick: 0, note: 60 }));
    assert!(s.print().contains("\"~@12 e4@12 g4@12 c5@3 ~@9\""));
    assert!(
        !s.edit_note(0, Edit::Remove { tick: 0, note: 60 }),
        "no such note"
    );
    assert!(
        !s.edit_note(5, Edit::Add { tick: 0, note: 60 }),
        "no such frag"
    );
}

#[test]
fn a_generated_frag_is_frozen_before_it_is_edited() {
    use crate::notes::Edit;
    let mut s = Song::parse(
        "scale c minor\ntrack t synth\nfrag a = t\n  euclid(3,8) c4\nfrag b = t\n  walk(c4,4,1)\n",
    )
    .unwrap();
    assert!(!s.edit_note(0, Edit::Add { tick: 3, note: 60 }));
    assert!(!s.edit_note(1, Edit::Add { tick: 3, note: 60 }));
    assert!(s.freeze(0, None));
    assert!(s.edit_note(0, Edit::Add { tick: 3, note: 60 }));
}

/// #173 with #168: editing an imported line of timed notes keeps it timed,
/// with its overlaps, velocities and bars.
#[test]
fn an_edit_of_timed_notes_stays_timed() {
    let text = "track v synth\nfrag a = v bars 2\n  d5@0:48:100 f#5@6:6:64\n";
    let mut s = Song::parse(text).expect("parses");
    assert!(s.edit_note(0, notes::Edit::Add { tick: 24, note: 69 }));
    let printed = s.print();
    assert!(
        printed.contains("frag a = v bars 2\n  d5@0:48:100 f#5@6:6:64 a4@24:3\n"),
        "a sixteenth, nothing shortened: {printed}"
    );
    assert!(s.edit_note(
        0,
        notes::Edit::Len {
            tick: 24,
            note: 69,
            len: 30
        }
    ));
    assert!(s.edit_note(0, notes::Edit::Remove { tick: 6, note: 78 }));
    assert!(
        s.print().contains("  d5@0:48:100 a4@24:30\n"),
        "{}",
        s.print()
    );
    assert!(
        !s.edit_note(0, notes::Edit::Add { tick: 96, note: 60 }),
        "past its bars"
    );
    assert_eq!(Song::parse(&s.print()), Ok(s));
}

const NILE: &str = "\
tempo 116
scale e phrygian
track kit drums
track lead synth
track bass synth
track pad synth
track low synth

frag beat = kit /16
  bd x..x..x...x..x..
frag sub = bass
  \"e2 ~ ~ e2\"
frag nile = lead
  \"e4 f4 g#4 ~\"
frag sand = lead live
  arp([e4,g#4,b4],updown,16)
frag hold = pad
  \"[e3,g#3,b3]\"
frag deep = low
  \"e1 f1 e2 b1\"
";

/// #210: a track left without a model is given one from its role.
#[test]
fn a_track_without_a_model_gets_one_for_its_role() {
    let s = Song::parse(NILE).expect("parses");
    let presets: Vec<Option<Preset>> = s.tracks.iter().map(|t| t.preset).collect();
    assert_eq!(
        presets,
        vec![
            Some(Preset::Kit808),
            Some(Preset::ProLead),
            Some(Preset::MiniBass),
            Some(Preset::JunoPad),
            Some(Preset::MiniBass),
        ],
        "kit, a lead by name, a bass by name, a pad by name, a bass by register"
    );
    let text = s.print();
    assert!(text.contains("track lead synth ProOne ProLead\ntrack bass synth Minimoog MiniBass\n"));
    assert_eq!(Song::parse(&text), Ok(s));
    let roles = Song::parse(
        "track a synth\ntrack b synth\nfrag x = a\n  \"[c4,e4]\"\nfrag y = b live\n  arp([c4,e4],up,16)\n",
    )
    .expect("parses");
    assert_eq!(roles.tracks[0].preset, Some(Preset::JunoPad), "chords");
    assert_eq!(roles.tracks[1].preset, Some(Preset::ShArp), "only arps");
    let drums = Song::parse("track tr909 drums\n").expect("parses");
    assert_eq!(
        drums.tracks[0].preset,
        Some(Preset::Kit909),
        "the name says 909"
    );
    let sampler = Song::parse("track s sampler\n").expect("parses");
    assert_eq!(
        sampler.tracks[0].preset, None,
        "a sampler keeps its samples"
    );
    assert_eq!(sampler.print(), "tempo 120\nswing 50\ntrack s sampler\n");
}

/// #210: a model alone gets its preset for the role, or its first.
#[test]
fn a_model_alone_gets_its_preset_for_the_role() {
    let s =
        Song::parse("track bass synth Juno106\ntrack lead synth Juno106\ntrack k drums Tr909\n")
            .expect("parses");
    assert_eq!(s.tracks[0].preset, Some(Preset::JunoBass));
    assert_eq!(
        s.tracks[1].preset,
        Some(Preset::JunoPad),
        "no Juno lead: its first"
    );
    assert_eq!(s.tracks[2].preset, Some(Preset::Kit909));
    let s = Song::parse("track lead synth Sh101 AcidBass\n").expect("parses");
    assert_eq!(
        s.tracks[0].preset,
        Some(Preset::AcidBass),
        "a written preset wins"
    );
}

/// #210: a setting is a preset with changes; a track plays it by name.
#[test]
fn settings_parse_and_print_back() {
    let text = "\
setting nile = Minimoog MiniLead: Cutoff 1200, Resonance 0.5
setting plain = Tr909 Hard909
track lead synth nile
track kit drums plain
track bass synth
";
    let s = Song::parse(text).expect("parses");
    assert_eq!(s.settings.len(), 2);
    assert_eq!(s.settings[0].preset, Preset::MiniLead);
    assert_eq!(
        s.settings[0].sets,
        vec![(Param::Cutoff, 1200.0), (Param::Resonance, 0.5)]
    );
    assert_eq!(
        (s.tracks[0].setting, s.tracks[0].preset),
        (Some(0), Some(Preset::MiniLead))
    );
    assert_eq!(
        s.patch(0),
        Some((Preset::MiniLead, &s.settings[0].sets[..]))
    );
    assert_eq!(s.patch(1), Some((Preset::Hard909, &[][..])));
    let printed = s.print();
    assert!(printed.starts_with(
        "tempo 120\nswing 50\nsetting nile = Minimoog MiniLead: Cutoff 1200, Resonance 0.5\nsetting plain = Tr909 Hard909\ntrack lead synth nile\ntrack kit drums plain\ntrack bass synth Minimoog MiniBass\n"
    ));
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn model_and_setting_errors_say_where() {
    let at = |text: &str| {
        Song::parse(text)
            .map(|_| ())
            .map_err(|e| (e.line, e.col, e.msg))
    };
    assert_eq!(
        at("track a synth Moog\n"),
        Err((
            1,
            15,
            "a model (as Minimoog or Tr808) or a setting goes here"
        ))
    );
    assert_eq!(
        at("track a synth Minimoog ProLead\n"),
        Err((1, 24, "this preset is for another model"))
    );
    assert_eq!(
        at("track a synth Tr808\n"),
        Err((1, 15, "this model does not play this kind of track"))
    );
    assert_eq!(
        at("track a synth Minimoog MiniLead x\n"),
        Err((1, 33, "unexpected text"))
    );
    assert_eq!(
        at("setting s = Minimoog MiniLead: Level 1\n"),
        Err((1, 32, "a setting changes the synth's own parameters"))
    );
    assert_eq!(
        at("setting s = Minimoog MiniLead: Cutoff 1 Resonance 1\n"),
        Err((1, 41, "a comma goes between changes"))
    );
    assert_eq!(
        at("track a synth\nsetting s = Minimoog MiniLead\n"),
        Err((2, 1, "a setting goes before the tracks"))
    );
    assert_eq!(
        at("setting s = Tr808 Kit808\ntrack a synth s\n"),
        Err((2, 15, "this model does not play this kind of track"))
    );
    assert_eq!(
        at("setting Minimoog = Minimoog MiniLead\n"),
        Err((
            1,
            9,
            "a name is a letter, then letters, digits or _, and not a model's"
        ))
    );
}

// --- comments are kept (#199) -------------------------------------------------

const REMARKED: &str = "\
# Voodoo, after Gerald
tempo 118   # not too fast
scale a minor
track kit drums
# the bass
track bass synth

# the beat
frag beat = kit /16
  # four on the floor
  bd x...x...x...x...
  cp ....x.......x...   # clap on 2 and 4

frag line = bass
  # a sharp stays a sharp: c#4
  \"a2 ~ a2 [a2 c3]\"   # root and fifth

# the arrangement
section intro 4: beat line   # four bars
arrange intro
# the end
";

#[test]
fn comments_survive_print_and_parse() {
    let s = Song::parse(REMARKED).expect("parses");
    let text = s.print();
    for kept in [
        "# Voodoo, after Gerald\ntempo 118 # not too fast\n",
        "# the bass\ntrack bass synth",
        "# the beat\nfrag beat = kit /16\n",
        "  # four on the floor\n  bd x...x...x...x...\n",
        "  cp ....x.......x... # clap on 2 and 4\n",
        "frag line = bass\n  # a sharp stays a sharp: c#4\n  \"a2 ~ a2 [a2 c3]\" # root and fifth\n",
        "# the arrangement\nsection intro 4: beat line # four bars\n",
    ] {
        assert!(text.contains(kept), "{kept:?} in\n{text}");
    }
    assert!(text.ends_with("arrange intro\n\n# the end\n"), "{text}");
    // Printing is stable: what is printed prints the same, and is the same song.
    let again = Song::parse(&text).expect("parses back");
    assert_eq!(again.print(), text);
    assert_eq!(again, s);
}

#[test]
fn a_comment_stays_with_its_item_through_an_edit() {
    let mut s = Song::parse(REMARKED).expect("parses");
    assert!(s.set_step(0, 0, 1, Step::Accent));
    assert!(s.set_step(0, 1, 0, Step::Accent));
    let text = s.print();
    assert!(
        text.contains("  # four on the floor\n  bd xX..x...x...x...\n"),
        "{text}"
    );
    assert!(
        text.contains("  cp X...x.......x... # clap on 2 and 4\n"),
        "{text}"
    );
}

#[test]
fn a_comment_whose_item_is_gone_moves_to_the_end() {
    let with =
        Song::parse("track k drums\nfrag a = k\n  # the kick\n  bd x...\n  sn ..x. # snare\n")
            .expect("parses");
    let without = Song::parse("track k drums\nfrag a = k\n  sn ..x.\n").expect("parses");
    let text = Comments::apply(
        &with.comments,
        without.print().lines().map(String::from).collect(),
        &without,
    )
    .join("\n");
    assert!(text.contains("  sn ..x. # snare\n"), "{text}");
    assert!(text.ends_with("\n\n# the kick"), "{text}");
}

#[test]
fn a_song_without_comments_prints_as_before() {
    let s = Song::parse("tempo 100\ntrack k drums\nfrag a = k\n  bd x...\n").expect("parses");
    assert_eq!(
        s.print(),
        "tempo 100\nswing 50\ntrack k drums Tr808 Kit808\n\nfrag a = k /16\n  bd x...\n"
    );
}

/// Every ```song block of a document parses, and its print parses back equal;
/// how many there were.
fn song_blocks_parse(name: &str, doc: &str) -> usize {
    let mut blocks = Vec::new();
    let mut open: Option<(usize, String)> = None;
    for (i, line) in doc.lines().enumerate() {
        match (&mut open, line.trim_end()) {
            (None, "```song") => open = Some((i + 1, String::new())),
            (Some(_), "```") => blocks.extend(open.take()),
            (Some((_, text)), l) => {
                text.push_str(l);
                text.push('\n');
            }
            _ => {}
        }
    }
    assert!(open.is_none(), "{name}: a song block is not closed");
    for (at, text) in &blocks {
        let s = Song::parse(text).unwrap_or_else(|e| panic!("{name}:{at}: {e:?}"));
        let printed = s.print();
        assert_eq!(Song::parse(&printed), Ok(s), "{name}:{at}:\n{printed}");
    }
    blocks.len()
}

/// The normative definition of the language (#383).
const LANGUAGE: &str = include_str!("../../../../.openspec/language.md");

/// #383: every ```song block of the language's definition parses and prints
/// back equal.
#[test]
fn the_language_examples_parse() {
    let n = song_blocks_parse(".openspec/language.md", LANGUAGE);
    assert!(n >= 20, "{n} song blocks");
}

/// #383: the definition grows with the language. Every line keyword has a
/// heading; every word the parser reads from a fixed list (track kinds and
/// frag words, generator calls, pattern and signal methods, signal sources,
/// pads, steps, grids, models, modes, chord qualities, arp modes, insert and
/// processor types, filter voicings) is written in its code or grammar.
#[test]
fn the_language_definition_covers_the_parser() {
    // Words in backticks and in fenced blocks, split at what can't be in one.
    let (mut code, mut prose) = (String::new(), String::new());
    let mut fenced = false;
    let mut headings = Vec::new();
    for line in LANGUAGE.lines() {
        if line.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if line.starts_with('#') && !fenced {
            headings.push(line);
        }
        let to = if fenced { &mut code } else { &mut prose };
        to.push_str(line);
        to.push('\n');
    }
    // A span in backticks may run over a line break.
    for (k, span) in prose.split('`').enumerate() {
        if k % 2 == 1 {
            code.push_str(span);
            code.push('\n');
        }
    }
    let words: std::collections::HashSet<&str> = code
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .collect();
    let in_a_heading = |k: &str| headings.iter().any(|h| h.split([' ', ',']).any(|w| w == k));
    let mut missing: Vec<String> = KEYWORDS
        .iter()
        .filter(|k| !in_a_heading(k))
        .map(|k| format!("heading {k}"))
        .collect();
    let grids: Vec<String> = GRIDS.iter().map(|g| g.to_string()).collect();
    let lists: [Vec<&str>; 15] = [
        lex::WORDS.to_vec(),
        notes::CALLS.to_vec(),
        Pattern::NAMES.to_vec(),
        signal::SOURCES.to_vec(),
        signal::METHODS.to_vec(),
        ALIASES.iter().map(|(a, _)| *a).collect(),
        Pad::ALL.iter().map(|(_, n)| *n).collect(),
        Model::ALL.iter().map(|(_, n)| *n).collect(),
        crate::algo::Mode::ALL.iter().map(|m| m.1).collect(),
        notes::QUALITIES
            .iter()
            .map(|q| q.0)
            .filter(|q| !q.is_empty())
            .collect(),
        notes::ArpMode::ALL.iter().map(|m| m.1).collect(),
        InsertType::ALL.iter().map(|(_, n)| *n).collect(),
        ProcType::ALL.iter().map(|(_, n)| *n).collect(),
        crate::modular::VOICINGS.iter().map(|v| v.0).collect(),
        grids
            .iter()
            .map(String::as_str)
            .chain(["x", "X", "o", "f", "d", "mute", "solo", "euclid"])
            .collect(),
    ];
    for list in &lists {
        for w in list {
            if !words.contains(w) {
                missing.push((*w).to_string());
            }
        }
    }
    assert!(
        missing.is_empty(),
        ".openspec/language.md does not define: {missing:?}"
    );
}

/// #383: the keywords are the parser's: each starts a line it reads, and any
/// other word is refused naming them all.
#[test]
fn the_keywords_are_the_parsers() {
    let err = Song::parse("tune 120").expect_err("no such keyword");
    for k in KEYWORDS {
        assert!(err.msg.contains(k), "{k} in {}", err.msg);
        let e = Song::parse(k).err();
        assert!(
            e.is_none_or(|e| !e.msg.starts_with("a line starts with")),
            "{k}: {e:?}"
        );
    }
}

/// #103: a chord frag in the song's key, voiced, prints back as written.
#[test]
fn a_voiced_progression_in_the_key_prints_back() {
    let text =
        "scale a minor\ntrack chords synth\n\nfrag prog = chords voicing\n  \"<i VI III VII>\"\n";
    let s = Song::parse(text).expect("parses");
    let f = &s.frags[0];
    assert!(f.voicing);
    assert!(s.print().contains("frag prog = chords"));
    assert!(s.print().contains("voicing\n  \"<i VI III VII>\""));
    assert_eq!(Song::parse(&s.print()).expect("parses back"), s);
    // Voiced: every chord within C3 to C6, the first Am around middle C.
    let notes: Vec<u8> = f
        .notes
        .as_ref()
        .unwrap()
        .events
        .iter()
        .map(|e| e.note)
        .collect();
    assert!(notes.iter().all(|n| (48..=84).contains(n)), "{notes:?}");
    // Chords make the track a pad (spec 002 Req 11).
    assert_eq!(s.tracks[0].preset, Some(Preset::JunoPad));
}

#[test]
fn voicing_errors_say_where() {
    for (text, line, col, msg) in [
        (
            "track kit drums\nfrag b = kit /16 voicing\n  bd x...\n",
            2,
            18,
            "voicing is for a frag of notes",
        ),
        (
            "scale c minor\ntrack l synth\nfrag w = l live voicing\n  walk(c4,8,1)\n",
            3,
            17,
            "a live frag is not voiced",
        ),
    ] {
        assert_eq!(
            Song::parse(text),
            Err(SongError { line, col, msg }),
            "{text}"
        );
    }
}

/// #103: one progression feeds the pad, the bass and the arp, and each track
/// picks a synth for its role.
#[test]
fn a_progression_feeds_pad_bass_and_arp() {
    let text = "scale c minor\ntrack pad synth\ntrack bass synth\ntrack arp synth\n\n\
                frag chords = pad voicing\n  prog(4,7)\n\
                frag low = bass\n  root(chords)\n\
                frag ripple = arp\n  arp(chords,updown,16)\n";
    let s = Song::parse(text).expect("parses");
    let printed = s.print();
    for line in [
        "  prog(4,7)",
        "  root(chords)",
        "  arp(chords,updown,16)",
        "frag chords = pad",
    ] {
        assert!(printed.contains(line), "{line} in\n{printed}");
    }
    assert_eq!(Song::parse(&printed).expect("parses back"), s);
    let bars: Vec<u32> = s
        .frags
        .iter()
        .map(|f| f.notes.as_ref().unwrap().bars)
        .collect();
    assert_eq!(bars, [4, 4, 4]);
    // The bass plays the roots of the voiced chords: the same pitch classes.
    let chords = &s.frags[0].notes.as_ref().unwrap().events;
    for e in &s.frags[1].notes.as_ref().unwrap().events {
        assert!(
            chords
                .iter()
                .any(|c| c.start == e.start && c.note % 12 == e.note % 12)
        );
        assert!(e.note < 48, "below C3");
    }
}

// --- Mixer lines (ADR-0018) -----------------------------------------------------

const MIXED: &str = "track kit drums\ntrack bass synth\n\
    strip bass: Level 0.8, Pan -0.2, I1Type Overdrive, I1A 0.6, Out group1\n\
    strip strip5: Mute 1\n\
    group 1 drums: Level 0.9, Out group3\n\
    group 3: Out none\n\
    master: MasterGain 0.6, P1Type Echo, P1Return 0.25, P3Type Chorus\n";

#[test]
fn mixer_lines_parse_and_print_back() {
    let s = Song::parse(MIXED).expect("parses");
    assert_eq!(s.mix.len(), 5);
    assert_eq!(s.mix[0].at, Mix::Track(1));
    assert_eq!(s.mix[1].at, Mix::Strip(4));
    assert_eq!(
        (s.mix[2].at, s.mix[2].name.as_deref()),
        (Mix::Group(0), Some("drums"))
    );
    assert_eq!(
        s.mix_value(Mix::Track(1), Param::I1Type),
        Some(1.0),
        "Overdrive"
    );
    assert_eq!(s.mix_value(Mix::Track(1), Param::Out), Some(1.0), "group1");
    assert_eq!(s.mix_value(Mix::Group(2), Param::Out), Some(9.0), "none");
    assert_eq!(s.mix_value(Mix::Master, Param::P3Type), Some(3.0), "Chorus");
    let printed = s.print();
    for line in [
        "strip bass: Level 0.8, Pan -0.2, I1Type Overdrive, I1A 0.6, Out group1",
        "strip strip5: Mute 1",
        "group 1 drums: Level 0.9, Out group3",
        "group 3: Out none",
        "master: MasterGain 0.6, P1Type Echo, P1Return 0.25, P3Type Chorus",
    ] {
        assert!(printed.contains(line), "{line}\n{printed}");
    }
    assert_eq!(Song::parse(&printed), Ok(s));
    // `master :` and `strip bass :` read the same.
    let spaced = Song::parse("track bass synth\nstrip bass : Level 0.5\nmaster : MasterGain 0.4\n");
    assert_eq!(spaced.expect("parses").mix.len(), 2);
}

#[test]
fn mixer_errors_say_where() {
    let head = "track bass synth\n";
    for (line, col, msg) in [
        (
            "strip nope: Level 1",
            7,
            "a strip is a track's name or strip1 to strip16",
        ),
        (
            "strip strip17: Level 1",
            7,
            "a strip is a track's name or strip1 to strip16",
        ),
        (
            "group 9: Level 1",
            7,
            "a group is 1 to 8, then maybe its name",
        ),
        (
            "group 2 my group: Level 1",
            7,
            "a group is 1 to 8, then maybe its name",
        ),
        (
            "strip bass: Cutoff 300",
            13,
            "a strip or group takes its strip's parameters",
        ),
        (
            "master: Level 1",
            9,
            "the master takes the global parameters",
        ),
        ("strip bass: Lvl 1", 13, "no parameter has this name"),
        (
            "strip bass: I1Type Delay",
            20,
            "an insert is Off, Overdrive, Distortion, Fuzz, Eq, Comp or Vocoder",
        ),
        (
            "master: P1Type Delay",
            16,
            "a processor is Off, Echo, Reverb, Chorus or Flanger",
        ),
        (
            "strip bass: Out bus2",
            17,
            "an out is master, group1 to group8 or none",
        ),
        (
            "group 3: Out group2",
            14,
            "a group goes only to a higher group or the master",
        ),
        (
            "strip bass: Level 1, Level 2",
            22,
            "this parameter is already on the line",
        ),
        (
            "strip bass: Level 1 Pan 0",
            21,
            "a comma goes between values",
        ),
        ("strip bass: Level", 18, "a value goes here"),
        ("strip bass:", 12, "values go here: Param value, …"),
        ("strip bass Level 1", 19, ": goes here, then Param value, …"),
    ] {
        let text = format!("{head}{line}\n");
        assert_eq!(
            Song::parse(&text),
            Err(SongError { line: 2, col, msg }),
            "{line}"
        );
    }
    let twice = format!("{head}strip bass: Level 1\nstrip bass: Pan 0\n");
    assert_eq!(
        Song::parse(&twice),
        Err(SongError {
            line: 3,
            col: 1,
            msg: "this strip already has a line"
        })
    );
}

#[test]
fn a_comment_on_the_master_line_stays_with_it() {
    let text = "track bass synth\nmaster: MasterGain 0.6 # louder\n";
    let s = Song::parse(text).expect("parses");
    let mut changed = s.clone();
    changed.mix[0].sets[0].1 = 0.7;
    assert!(
        changed.print().contains("master: MasterGain 0.7 # louder"),
        "{}",
        changed.print()
    );
}

/// ADR-0019: `mod target.param = signal` parses, holds its target and
/// parameter, and prints canonically, the parameter in lower case.
#[test]
fn a_mod_line_parses_and_prints() {
    let text = "track kit drums\nfrag b = kit\n  bd x\n\
        mod kit.Cutoff = sine.range(300,3000).slow(4)   # sweep\n\
        mod master.p2return = lfo(0.2, saw).range(0, 0.5) + perlin * 0.1\n";
    let s = Song::parse(text).expect("parses");
    assert_eq!(s.mods.len(), 2);
    assert_eq!(
        (s.mods[0].target, s.mods[0].param),
        (Target::Track(0), Param::Cutoff)
    );
    assert_eq!(
        (s.mods[1].target, s.mods[1].param),
        (Target::Master, Param::P2Return)
    );
    let printed = s.print();
    assert!(
        printed.contains("\nmod kit.cutoff = sine.range(300, 3000).slow(4) # sweep\n"),
        "{printed}"
    );
    assert!(
        printed.contains("\nmod master.p2return = lfo(0.2, saw).range(0, 0.5) + perlin * 0.1\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn mod_errors_say_where() {
    let head = "track kit drums\nfrag b = kit\n  bd x\n";
    for (line, col, msg) in [
        ("mod", 4, "a target.param goes here"),
        ("mod kit.nope = 1", 5, "no parameter has this name"),
        (
            "mod group1.cutoff = 1",
            5,
            "this parameter does not belong to this target",
        ),
        (
            "mod kit.model = 1",
            5,
            "the model and the routing can't be automated",
        ),
        ("mod kit.cutoff 1", 16, "= and a signal go here"),
        (
            "mod kit.cutoff =",
            17,
            "a signal goes here, e.g. sine.range(300, 3000)",
        ),
        (
            "mod kit.cutoff =  sine.wobble(2)",
            24,
            "no such method: range exprange slow fast segment lag",
        ),
        (
            "mod kit.cutoff = lfo(1, pink)",
            25,
            "a shape is sine, cosine, saw, tri or square",
        ),
    ] {
        let err = Song::parse(&format!("{head}{line}")).expect_err(line);
        assert_eq!((err.line, err.col, err.msg), (4, col, msg), "{line}");
    }
    let twice = format!("{head}mod kit.cutoff = 1\nmod kit.Cutoff = 2");
    let err = Song::parse(&twice).expect_err("twice");
    assert_eq!(
        (err.line, err.col, err.msg),
        (5, 5, "this parameter already has a mod")
    );
    let params = ["level", "pan", "send1"];
    let many: String = (0..33)
        .map(|n| format!("mod strip{}.{} = 1\n", n % 16 + 1, params[n / 16]))
        .collect();
    let err = Song::parse(&format!("{head}{many}")).expect_err("many");
    assert_eq!((err.line, err.msg), (36, "a song has at most 32 mods"));
    let big = format!("{head}mod kit.cutoff = {}1", "1 + ".repeat(130));
    let err = Song::parse(&big).expect_err("big");
    assert_eq!(err.msg, "a song's signals have at most 256 nodes");
}

/// ADR-0023: a signal with `env` or a list goes on a Mono or Poly track's
/// voice parameters, without `.lag`, and says where when it does not.
#[test]
fn per_voice_mod_errors_say_where() {
    let head = "track kit drums\ntrack lead synth Juno106 JunoPad\n\
        track buzzer synth Modular ModularBasic\nfrag b = kit\n  bd x\n";
    for (line, col, msg) in [
        (
            "mod strip1.level = [0.5, 1]",
            20,
            "env and lists give each voice its own value: they modulate a track",
        ),
        (
            "mod lead.pan = env(perc)",
            16,
            "per voice: cutoff, resonance, vco1level, vco2level, vco3level, noiselevel, ringlevel or sublevel",
        ),
        (
            "mod lead.cutoff = env(perc).range(200, 900).lag(0.1)",
            19,
            ".lag follows one value for the song, not one per voice",
        ),
        (
            "mod kit.cutoff = env(perc)",
            18,
            "env and lists need a Mono or Poly synth; a Modular voice writes them in its graph",
        ),
        (
            "mod buzzer.cutoff = [300, 600]",
            21,
            "env and lists need a Mono or Poly synth; a Modular voice writes them in its graph",
        ),
    ] {
        let err = Song::parse(&format!("{head}{line}")).expect_err(line);
        assert_eq!((err.line, err.col, err.msg), (6, col, msg), "{line}");
    }
    // On the track's voice parameters it parses, and so does a method.
    let ok = format!(
        "{head}frag r = lead .resonance(lfo([1, 3]).range(0, 0.6))\n  \"c3\"\n\
        mod lead.cutoff = env(perc).exprange(200, 4000)\nmod lead.vco1level = [1, 0.5]\n"
    );
    let song = Song::parse(&ok).expect("parses");
    assert!(song.mods.iter().all(|m| m.signal.per_voice()));
    assert_eq!(Song::parse(&song.print()), Ok(song));
}

/// #204: parameter methods on a fragment's line parse with it, belong to its
/// track, and print after the rest of the line.
#[test]
fn fragment_methods_parse_and_print() {
    let text = "track kit drums\ntrack lead synth\ntrack pad synth\n\
        frag b = kit /16 .Level(0.5)\n  bd x...\n\
        frag r = lead live  .cutoff(sine.slow(4).range(300,3000)) .resonance( 0.7 )   # acid\n  arp([c4,e4,g4],up,8)\n\
        frag p = pad voicing .pan(lfo(0.25).range(-1, 1)) .cutoff( \"<300  800>\" )\n  \"[c3,e3,g3] [f3,a3,c4]\"\n\
        mod lead.cutoff = 900\n";
    let s = Song::parse(text).expect("parses");
    let scoped: Vec<_> = s.mods.iter().map(|m| (m.target, m.param, m.frag)).collect();
    assert_eq!(
        scoped,
        vec![
            (Target::Track(0), Param::Level, Some(0)),
            (Target::Track(1), Param::Cutoff, Some(1)),
            (Target::Track(1), Param::Resonance, Some(1)),
            (Target::Track(2), Param::Pan, Some(2)),
            (Target::Track(2), Param::Cutoff, Some(2)),
            (Target::Track(1), Param::Cutoff, None),
        ]
    );
    assert!(s.frags[1].live && s.frags[2].voicing);
    let printed = s.print();
    for line in [
        "frag b = kit /16 .level(0.5)\n",
        "frag r = lead live .cutoff(sine.slow(4).range(300, 3000)) .resonance(0.7) # acid\n",
        "frag p = pad voicing .pan(lfo(0.25).range(-1, 1)) .cutoff(\"<300 800>\")\n",
        "\nmod lead.cutoff = 900\n",
    ] {
        assert!(printed.contains(line), "{line}in\n{printed}");
    }
    assert_eq!(Song::parse(&printed), Ok(s));
}

/// #298: Strudel's parameter names reach their registry parameters on
/// parameter methods and `mod` lines, and print back as written.
#[test]
fn strudels_parameter_names_are_aliases() {
    let text = "track bass synth\n\
        frag a = bass .lpf(sine.range(300, 3000)) .lpq(0.6) .attack(0.01) .room(0.3)\n  \"c2 ~ c2 ~\"\n\
        mod bass.gain = 0.8\n";
    let s = Song::parse(text).expect("parses");
    let params: Vec<_> = s.mods.iter().map(|m| (m.param, m.alias)).collect();
    assert_eq!(
        params,
        vec![
            (Param::Cutoff, Some("lpf")),
            (Param::Resonance, Some("lpq")),
            (Param::AdsrAttack, Some("attack")),
            (Param::Send2, Some("room")),
            (Param::Level, Some("gain")),
        ]
    );
    let printed = s.print();
    assert!(
        printed.contains(".lpf(sine.range(300, 3000)) .lpq(0.6) .attack(0.01) .room(0.3)"),
        "{printed}"
    );
    assert!(printed.contains("mod bass.gain = 0.8"), "{printed}");
    assert_eq!(Song::parse(&printed), Ok(s));
    // An alias names no registry parameter but its own, nor a pattern method.
    for (a, p) in ALIASES {
        assert!(Param::by_name(a).is_none_or(|q| q == p), "{a}");
        assert!(!Pattern::NAMES.contains(&a), "{a}");
    }
    let e = Song::parse("track bass synth\nfrag a = bass .distort(0.5)\n  \"c2\"\n")
        .expect_err("no alias");
    assert_eq!(e.msg, "no parameter has this name");
}

#[test]
fn fragment_method_errors_say_where() {
    let head = "track kit drums\n";
    for (line, col, msg) in [
        ("frag b = kit .nope(1)", 15, "no parameter has this name"),
        (
            "frag b = kit .mastergain(1)",
            15,
            "this parameter does not belong to this target",
        ),
        (
            "frag b = kit .model(1)",
            15,
            "the model and the routing can't be automated",
        ),
        (
            "frag b = kit .cutoff",
            21,
            "( and a value go here, e.g. .cutoff(800)",
        ),
        (
            "frag b = kit .(1)",
            15,
            "a parameter name goes here, e.g. .cutoff(800)",
        ),
        ("frag b = kit .cutoff(sine", 21, "this ( is not closed"),
        (
            "frag b = kit .cutoff(sine.wobble(1))",
            27,
            "no such method: range exprange slow fast segment lag",
        ),
        (
            "frag b = kit .cutoff()",
            22,
            "a signal goes here, e.g. sine.range(300, 3000)",
        ),
        (
            "frag b = kit .cutoff(1) x",
            25,
            "a parameter method goes here, e.g. .cutoff(800)",
        ),
        (
            "frag b = kit .cutoff(1) .Cutoff(2)",
            26,
            "this parameter already has a method",
        ),
    ] {
        let err = Song::parse(&format!("{head}{line}\n  bd x")).expect_err(line);
        assert_eq!((err.line, err.col, err.msg), (2, col, msg), "{line}");
    }
}

/// ADR-0019, #215: pattern methods on a frag's line transform its notes when
/// the song loads, and print before its parameter methods.
#[test]
fn pattern_methods_transform_a_frags_notes() {
    let text = "track lead synth\n\
        frag r = lead .cutoff(900) .fast(2) .every(2, rev) .off( 0.125 ,add(12))\n  \"c4 d4 e4 f4\"\n";
    let s = Song::parse(text).expect("parses");
    let f = &s.frags[0];
    assert_eq!(f.pattern.len(), 3);
    let n = f.notes.as_ref().expect("notes");
    assert_eq!(n.text, "\"c4 d4 e4 f4\"", "the line stays as written");
    assert_eq!(n.bars, 2);
    assert_eq!(n.events.len(), 32);
    // Bar 0 reversed in eighths: f4 then e4, each with the octave of the
    // note an eighth before (the last of bar 1 wraps round); bar 1 as written.
    let at = |t: u32| {
        n.events
            .iter()
            .filter(|e| e.start == t)
            .map(|e| e.note)
            .collect::<Vec<_>>()
    };
    assert_eq!((at(0), at(6)), (vec![65, 77], vec![64, 77]));
    assert_eq!(at(48), vec![60, 72]);
    let printed = s.print();
    assert!(
        printed.contains("frag r = lead .fast(2) .every(2, rev) .off(1/8, add(12)) .cutoff(900)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn pattern_method_errors_say_where() {
    for (text, line, col, msg) in [
        (
            "track kit drums\nfrag b = kit .fast(2)\n  bd x",
            2,
            15,
            "pattern methods are for a frag of notes",
        ),
        (
            "track l synth\nfrag w = l live .rev()\n  walk(c4,8,1)",
            2,
            18,
            "a live frag takes no pattern methods yet",
        ),
        (
            "track l synth\nfrag r = l .fast(0)\n  \"c4\"",
            2,
            13,
            "this takes a whole number from 1 to 16",
        ),
        (
            "track l synth\nfrag r = l .every(4, cutoff)\n  \"c4\"",
            2,
            13,
            "a pattern method goes here, e.g. rev or fast(2)",
        ),
        (
            "track l synth\nfrag r = l .slow(16) .slow(4)\n  \"c4\"",
            3,
            3,
            "the pattern runs past 32 bars",
        ),
    ] {
        let err = Song::parse(text).expect_err(text);
        assert_eq!((err.line, err.col, err.msg), (line, col, msg), "{text}");
    }
    // No pattern method is also a parameter's name.
    for name in Pattern::NAMES {
        assert_eq!(Param::by_name(name), None, "{name}");
    }
}

/// A note edit from the grid can't bake a pattern into the line.
#[test]
fn a_patterned_frag_refuses_a_note_edit() {
    let mut s = Song::parse("track l synth\nfrag r = l .rev()\n  \"c4 d4\"\n").expect("parses");
    let before = s.clone();
    assert!(!s.edit_note(0, notes::Edit::Add { tick: 6, note: 64 }));
    assert_eq!(s, before);
}

/// #215: struct, sometimes and scale on a frag line print back as written.
#[test]
fn struct_sometimes_and_scale_print_back() {
    let text = "track lead synth\n\
        frag r = lead .struct(\"x ~ x x\") .sometimes(add(12)) .scale(eb minor)\n  \"c4 e4 g4 b4\"\n";
    let s = Song::parse(text).expect("parses");
    assert_eq!(s.frags[0].pattern.len(), 3);
    let notes = s.frags[0].notes.as_ref().expect("notes");
    assert!(
        notes
            .events
            .iter()
            .all(|e| [3, 5, 6, 8, 10, 11, 1].contains(&(e.note % 12)))
    );
    let printed = s.print();
    assert!(
        printed
            .contains("frag r = lead .struct(\"x ~ x x\") .sometimes(add(12)) .scale(d# minor)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

/// ADR-0024: a Modular setting holds its SynthDef on the indented lines
/// under it, as written, comments, `#` and blank lines too; a track plays it
/// by the setting's name, and the song prints it back the same.
#[test]
fn a_setting_holds_its_code() {
    let text = r#"setting buzz = Modular ModularBasic   # a buzz
  SynthDef(\buzz, { |freq = 440|
      // a saw, #1

      RLPF.ar(Saw.ar(freq), 1200, 0.5)
  }).add;

track lead synth buzz
track pad synth Modular ModularHoover
frag r = lead
  "c3 e3"
"#;
    let s = Song::parse(text).expect("parses");
    let code = "SynthDef(\\buzz, { |freq = 440|\n    // a saw, #1\n\n    RLPF.ar(Saw.ar(freq), 1200, 0.5)\n}).add;";
    assert_eq!(s.settings[0].code.as_deref(), Some(code));
    assert_eq!((s.code(0), s.code(1)), (Some(code), None));
    let printed = s.print();
    assert!(
        printed.starts_with(
            "tempo 120\nswing 50\nsetting buzz = Modular ModularBasic # a buzz\n  SynthDef(\\buzz, { |freq = 440|\n      // a saw, #1\n\n      RLPF.ar"
        ),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
    // A Modular setting may have no code, and plays its preset's.
    let plain =
        Song::parse("setting p = Modular ModularHoover\ntrack l synth p\n").expect("parses");
    assert_eq!(plain.settings[0].code, None);
}

/// ADR-0024: a mistake in a setting's code is the song's, at its line and
/// column; only a Modular setting has code.
#[test]
fn setting_code_errors_say_where() {
    for (text, line, col, msg) in [
        (
            "tempo 120\nsetting v = Modular ModularBasic\n  SynthDef(\\v, {\n    Saw.ar(440) +\n  }).add;\n",
            5,
            3,
            "an expression goes here",
        ),
        (
            "setting v = Modular ModularBasic\n  SynthDef(\\v, { Cosine.ar(440) }).add;\n",
            2,
            18,
            "this UGen is not part of a SynthDef here",
        ),
        (
            "setting v = Minimoog MiniLead\n  SynthDef(\\v, { Saw.ar(440) }).add;\n",
            2,
            3,
            "a lane goes under a frag",
        ),
        (
            "track l synth Modular nope",
            1,
            23,
            "a preset is a factory preset, as MiniBass",
        ),
    ] {
        let err = Song::parse(text).expect_err(text);
        assert_eq!((err.line, err.col, err.msg), (line, col, msg), "{text}");
    }
}

/// #353: a ghost note `o` parses, prints back and plays at its velocity.
#[test]
fn a_ghost_note_parses_and_prints_back() {
    let text = "tempo 120\ntrack kit drums\n\nfrag a = kit /16\n  sn o.x.X.o.\n";
    let song = Song::parse(text).expect("parses");
    let steps = &song.frags[0].lanes[0].steps;
    assert_eq!(steps[0], Step::Ghost);
    assert_eq!(Step::Ghost.velocity(), Some(crate::song::GHOST_VELOCITY));
    assert_eq!(Step::from_level(3), Some(Step::Ghost));
    assert_eq!(Song::parse(&song.print()).expect("prints back"), song);
}

/// #255: a `~` locks no value per voice, so a per-voice signal refuses it.
#[test]
fn a_rest_is_not_for_a_per_voice_signal() {
    let e = Song::parse("track lead synth\nmod lead.cutoff = env(perc) * \"~ 1000\"\n")
        .expect_err("per voice");
    assert_eq!(e.msg, "a ~ lets go of one value, not one per voice");
    let ok = Song::parse(
        "track lead synth\nfrag a = lead .cutoff(\"~ 800\")\n  \"c3 e3\"\nmod lead.resonance = \"<~ 0.7>\"\n",
    );
    let song = ok.expect("one value");
    assert_eq!(Song::parse(&song.print()), Ok(song));
}

/// #242: a digit after a hit, accent or ghost ratchets it; the lane counts
/// steps, not digits, and prints back as written. A ratchet goes only on
/// x, X or o, and is 2, 3 or 4.
#[test]
fn ratchets_parse_print_back_and_say_where_they_are_wrong() {
    let text = "tempo 120\ntrack kit drums\n\nfrag a = kit /16\n  sn x3.X2.o4.x...\n";
    let song = Song::parse(text).expect("parses");
    let lane = &song.frags[0].lanes[0];
    assert_eq!(lane.steps.len(), 10);
    let reps: Vec<u8> = (0..10).map(|n| lane.ratchet(n)).collect();
    assert_eq!(reps, [3, 1, 2, 1, 4, 1, 1, 1, 1, 1]);
    assert!(
        song.print().contains("  sn x3.X2.o4.x...\n"),
        "{}",
        song.print()
    );
    assert_eq!(Song::parse(&song.print()).expect("prints back"), song);
    let plain = Song::parse("track kit drums\nfrag a = kit\n  sn x.x.\n").expect("parses");
    assert!(
        plain.frags[0].lanes[0].ratchets.is_empty(),
        "no ratchet, nothing kept"
    );
    let head = "track kit drums\nfrag a = kit\n";
    for (lane, col, msg) in [
        ("  sn 3x", 6, "a ratchet follows a step: x3"),
        ("  sn f2", 7, "only x, X and o take a ratchet"),
        ("  sn .2", 7, "only x, X and o take a ratchet"),
        ("  sn x1", 7, "a ratchet is 2, 3 or 4"),
        ("  sn x5", 7, "a ratchet is 2, 3 or 4"),
        ("  sn x33", 8, "a ratchet is 2, 3 or 4"),
    ] {
        let err = Song::parse(&format!("{head}{lane}\n")).expect_err(lane);
        assert_eq!((err.line, err.col, err.msg), (3, col, msg), "{lane}");
    }
}

/// #355: `mute` and `solo` end a track line, print back and decide which
/// tracks are heard; anything else after a track's synth is an error.
#[test]
fn track_mute_and_solo_parse_print_and_decide_who_is_heard() {
    let text =
        "tempo 120\ntrack a drums Tr808 Kit808 mute\ntrack b drums solo\ntrack c drums mute solo\n";
    let song = Song::parse(text).expect("parses");
    let flags: Vec<(bool, bool)> = song.tracks.iter().map(|t| (t.mute, t.solo)).collect();
    assert_eq!(flags, vec![(true, false), (false, true), (true, true)]);
    assert_eq!(Song::parse(&song.print()).expect("prints back"), song);
    assert!(song.print().contains("track a drums Tr808 Kit808 mute\n"));
    // b and c are soloed: they play, c although muted; a does not.
    assert_eq!(
        (0..3).map(|t| song.heard(t)).collect::<Vec<_>>(),
        vec![false, true, true]
    );
    let mut quiet = song.clone();
    for t in 0..3 {
        quiet.set_track_flags(t, t == 0, false);
    }
    assert_eq!(
        (0..3).map(|t| quiet.heard(t)).collect::<Vec<_>>(),
        vec![false, true, true]
    );
    let err = Song::parse("track a drums Tr808 Kit808 loud\n").expect_err("loud");
    assert_eq!((err.line, err.col), (1, 28));
}
