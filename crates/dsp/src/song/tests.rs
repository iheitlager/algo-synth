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
    assert_eq!(s.tracks, vec![Track { name: "kit".into() }]);
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
        ("track kit bass", 1, 11, "only drums tracks for now"),
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
            "loop a",
            1,
            1,
            "a line starts with tempo, swing, track or frag",
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
        });
    }
    for f in 0..r.below(5) {
        let mut pads: Vec<Pad> = Pad::ALL.iter().map(|(p, _)| *p).collect();
        let mut lanes = Vec::new();
        for _ in 0..1 + r.below(8) {
            let pad = pads.remove(r.below(pads.len()));
            let steps = (0..1 + r.below(MAX_STEPS))
                .map(|_| [Step::Off, Step::Hit, Step::Accent][r.below(3)])
                .collect();
            lanes.push(Lane { pad, steps });
        }
        song.frags.push(Fragment {
            name: format!("f{f}"),
            track: r.below(song.tracks.len()),
            lanes,
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
    let alphabet = b"tempo swing track frag drums = /16 bd sn ch xX.#\n\t 0123456789-+e\xc3\xa9";
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
