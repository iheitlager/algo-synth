use super::spans;
use super::*;

const BEAT: &str = "\
# a comment
tempo 124
swing 56
track kit drums

clip beat = kit /16
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
    let f = &s.clips[0];
    assert_eq!((f.name.as_str(), f.track, f.lanes.len()), ("beat", 0, 3));
    assert_eq!(f.lanes[0].pad, LanePad::Named(Pad::Bd));
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
        "# a comment\ntempo 124\nswing 56\ntrack kit drums Tr808 Kit808\n\nclip beat = kit /16\n  bd x...x...x...x... # four on the floor\n  sn ....X.......X..x\n  ch x.x.x.x.x.x.x.x.\n"
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
        ("clip a = kit", 1, 10, "no track has this name"),
        ("track kit drums\nclip a kit", 2, 8, "= and a track go here"),
        (
            "track kit drums\nclip a = kit /8\n  bd x",
            2,
            14,
            "a drum grid is /12, /16, /24, /32 or /48",
        ),
        (
            "track kit drums\nclip a = kit\n  zz x...",
            3,
            3,
            "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr rd, or p0 to p15",
        ),
        (
            "track kit drums\nclip a = kit\n  bd x..z",
            3,
            9,
            "a step is x, X, o, f, d or .",
        ),
        (
            "track kit drums\nclip a = kit\n  bd",
            3,
            3,
            "a lane needs its steps: x, X, o, f, d or .",
        ),
        (
            "track kit drums\nclip a = kit\n  bd x\n  bd x",
            4,
            3,
            "this pad already has a lane",
        ),
        (
            "track kit drums\nclip a = kit\ntempo 120",
            2,
            1,
            "a clip needs at least one lane",
        ),
        ("  bd x...", 1, 3, "a lane goes under a clip"),
        (
            "play a",
            1,
            1,
            "a line starts with tempo, swing, scale, setting, track, samples, zone, pad, table, attack, strip, group, master, clip, auto, snapshot, mod, scene, arrange or loop",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text),
            Err(SongError {
                line,
                col,
                msg: msg.into()
            }),
            "{text:?}"
        );
    }
}

#[test]
fn limits_hold() {
    let long = format!("track kit drums\nclip a = kit\n  bd {}", "x".repeat(65));
    assert_eq!(
        Song::parse(&long).map_err(|e| e.msg),
        Err("a lane has at most 64 steps".into())
    );
    let tracks: String = (0..17).map(|i| format!("track t{i} drums\n")).collect();
    assert_eq!(
        Song::parse(&tracks).map_err(|e| (e.line, e.msg)),
        Err((17, "a song has at most 16 tracks".into()))
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
    assert!(!s.set_step(1, 0, 0, Step::Hit), "no second clip");
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
            song.clips.push(Clip {
                name: format!("f{f}"),
                track: 1,
                lanes: Vec::new(),
                notes: Some(notes::parse(seq, 1).expect("valid")),
                live: false,
                voicing: None,
                pattern: Vec::new(),
                grid: 16,
                from: None,
                beat: None,
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
                pad: LanePad::Named(pad),
                steps,
                call: None,
                ratchets: Vec::new(),
            });
        }
        song.clips.push(Clip {
            name: format!("f{f}"),
            track: 0,
            lanes,
            notes: None,
            live: false,
            voicing: None,
            pattern: Vec::new(),
            grid: GRIDS[r.below(GRIDS.len())],
            from: None,
            beat: None,
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
    let alphabet = b"tempo swing scale c minor track clip drums synth euclid(3,8,1) = /16 bd sn ch c4 xX.#\n\t 0123456789-+e\xc3\xa9";
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

clip riff = bass
  \"c4 [e4 g4] ~ <c5 d5>?\"   # a sharp is not a comment: c#4
clip line = bass
  c#4:4 e4:8. [c4,e4,g4]:2 r:8
clip beat = kit
  bd x...
";

#[test]
fn note_clips_parse_and_print_back() {
    let s = Song::parse(RIFF).expect("parses");
    assert_eq!(s.tracks[0].kind, Kind::Synth);
    assert_eq!(s.clips[0].notes.as_ref().map(|n| n.bars), Some(8));
    assert!(s.clips[0].lanes.is_empty());
    assert_eq!(s.clips[2].notes, None);
    let text = s.print();
    assert!(text.contains("track bass synth Minimoog MiniBass\ntrack kit drums Tr808 Kit808\n"));
    assert!(text.contains(
        "clip riff = bass\n  \"c4 [e4 g4] ~ <c5 d5>?\" # a sharp is not a comment: c#4\n"
    ));
    assert!(text.contains("clip line = bass\n  c#4:4 e4:8. [c4,e4,g4]:2 r:8\n"));
    assert_eq!(Song::parse(&text), Ok(s));
}

#[test]
fn note_errors_say_line_and_column() {
    let cases: [(&str, usize, usize, &str); 8] = [
        (
            "track b synth\nclip a = b\n  \"c4 x4\"",
            3,
            7,
            "a note is a letter a to g, maybe # or b, and an octave 0 to 9, as c4",
        ),
        (
            "track b synth\nclip a = b\n  c4:4 \"e4\"",
            3,
            8,
            "mini-notation goes inside quotes, classic notes outside",
        ),
        (
            "track b synth\nclip a = b\n  \"c4:4\"",
            3,
            6,
            "durations go outside the quotes, as c4:4",
        ),
        (
            "track b synth\nclip a = b\n  c4:4 e4",
            3,
            10,
            "a classic note has a duration, as c4:4",
        ),
        (
            "track b synth\nclip a = b\n  c4:4\n  e4:4",
            4,
            3,
            "a note clip is one line of notes",
        ),
        (
            "track b synth\nclip a = b\nclip c = b\n  c4:4",
            2,
            1,
            "a clip needs a line of notes",
        ),
        (
            "track b synth\nclip a = b /16\n  c4:4",
            2,
            12,
            "a note clip has no step grid",
        ),
        (
            "track b drums\nclip a = b\n  c4:4",
            3,
            3,
            "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr rd, or p0 to p15",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text),
            Err(SongError {
                line,
                col,
                msg: msg.into()
            }),
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

clip hits = pads
  bd x...x...
  sn ....X...
clip tune = keys
  c4:4 e4:8 g4:8
clip mini = keys
  \"c4 [e4 g4]\"
";

#[test]
fn sampler_tracks_hold_lanes_or_notes() {
    let s = Song::parse(SAMPLED).expect("parses");
    assert_eq!(s.tracks[0].kind, Kind::Sampler);
    assert_eq!(s.clips[0].lanes.len(), 2);
    assert!(s.clips[0].notes.is_none());
    assert!(s.clips[1].lanes.is_empty() && s.clips[1].notes.is_some());
    let text = s.print();
    assert!(text.contains("track pads sampler\ntrack keys sampler\n"));
    assert!(text.contains("clip hits = pads /16\n  bd x...x...\n  sn ....X...\n"));
    assert_eq!(Song::parse(&text), Ok(s));
}

#[test]
fn sampler_errors_say_where() {
    let cases: [(&str, usize, usize, &str); 5] = [
        (
            "track p sampler\nclip a = p\n  zz x...",
            3,
            3,
            "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr rd, or p0 to p15",
        ),
        (
            "track p sampler\nclip a = p\n  bd x...\n  c4:4",
            4,
            3,
            "a clip holds lanes or notes, not both",
        ),
        (
            "track p sampler\nclip a = p\n  c4:4\n  bd x...",
            4,
            3,
            "a clip holds lanes or notes, not both",
        ),
        (
            "track p sampler\nclip a = p\ntempo 120",
            2,
            1,
            "a clip needs lanes or a line of notes",
        ),
        // #395: a grid is for lanes; a clip of notes has none, as on a synth.
        (
            "track s sampler Sampler\nclip f = s /24\n  \"c4 e4 g4\"",
            2,
            12,
            "a note clip has no step grid",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text),
            Err(SongError {
                line,
                col,
                msg: msg.into()
            }),
            "{text:?}"
        );
    }
}

#[test]
fn a_pad_sampler_takes_pads_by_number() {
    // #662: every pad is reachable, `p7` as `p0`; the percussion names reach theirs.
    let text = "track d drums PadSampler\nclip a = d\n  p7 x...\n  cb ..x.\n  ma .x..\n";
    let s = Song::parse(text).expect("parses");
    let pads: Vec<_> = s.clips[0].lanes.iter().map(|l| l.pad).collect();
    assert_eq!(
        pads,
        [
            LanePad::Number(7),
            LanePad::Named(Pad::Cb),
            LanePad::Named(Pad::Ma)
        ]
    );
    assert_eq!(s.clips[0].lanes[0].pad.note(), 43);
    assert_eq!(Song::parse(&s.print()), Ok(s));
    let mut reached: Vec<usize> = (0..16u8)
        .filter_map(|n| crate::padsampler::pad_of(LanePad::Number(n).note()))
        .collect();
    reached.dedup();
    assert_eq!(reached, (0..16).collect::<Vec<_>>());
    for (pad, name) in Pad::ALL {
        assert_eq!(LanePad::from_name(name), Some(LanePad::Named(pad)));
    }
    assert_eq!(LanePad::from_name("p15"), Some(LanePad::Number(15)));
    assert_eq!(LanePad::from_name("p16"), None);
    assert_eq!(LanePad::from_name("p05"), None);
    assert_eq!(LanePad::Number(5).id(), PAD_NUMBER + 5);
    let ts = include_str!("../../../../web/src/audio/params.ts");
    assert!(
        ts.contains(&format!("export const PAD_NUMBER = {PAD_NUMBER}\n")),
        "params.ts mirrors PAD_NUMBER"
    );
    // A model that does not name its pads keeps the lanes as they are.
    assert!(Song::parse("track p sampler\nclip a = p\n  p3 x...\n  cl x...").is_ok());
}

/// #673: a parameter its track's model never reads is an error at its
/// name, where the model is named or picked, not a method that does nothing.
#[test]
fn a_parameter_the_model_does_not_read_says_where() {
    let moog = "the Minimoog's vibrato is VCO 3: vco3coarse";
    let cases: [(&str, usize, usize, &str); 8] = [
        (
            "track solo synth Minimoog\nclip a = solo .lfospeed(4) .vibrato(1)\n  \"a3 e3\"",
            2,
            16,
            moog,
        ),
        (
            "track solo synth Minimoog MiniLead\nclip a = solo .lforate(4)\n  \"a3\"",
            2,
            16,
            moog,
        ),
        (
            "track solo synth Minimoog\nauto a = solo.LfoWave 1 /1",
            2,
            15,
            moog,
        ),
        (
            "track solo synth Minimoog\nsnapshot s: solo.Level 1, solo.LfoRate 2",
            2,
            32,
            moog,
        ),
        (
            "track b synth Juno106\nmod b.op1level = sine",
            2,
            7,
            "a DX7 parameter: this track's synth is not a DX7",
        ),
        (
            "track e synth Dx7\nclip a = e .attack(0.1)\n  \"c4\"",
            2,
            13,
            "the DX7's envelopes are its operators' rates and levels: op1r1",
        ),
        (
            "setting lead = Juno106 JunoBrass: LfoSpeed 4\ntrack l synth lead",
            1,
            35,
            "a DX7 parameter: this synth's LFO is lforate",
        ),
        // A kit picked by its role is a Tr808, and the text then names it.
        (
            "track kit drums\nclip b = kit\n  bd x\nmod kit.cutoff = sine",
            4,
            9,
            "a drum machine reads its pads' parameters: bdtune",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text).map(|_| ()),
            Err(SongError {
                line,
                col,
                msg: msg.into()
            }),
            "{text}"
        );
    }
    // The LFO is the Juno's vibrato; a model left to the role is checked
    // only once it is picked.
    assert!(Song::parse("track b synth Juno106\nclip a = b .lforate(4)\n  \"c3\"").is_ok());
    assert!(Song::parse("track solo synth\nclip a = solo .cutoff(900)\n  \"a3\"").is_ok());
}

/// #673: the strip, the arpeggiator and the voice pool are every model's.
#[test]
fn every_model_takes_strip_and_arp_parameters() {
    for (m, name) in Model::ALL {
        let kind = ["synth", "drums", "sampler"]
            .into_iter()
            .find(|k| Song::parse(&format!("track t {k} {name}")).is_ok())
            .unwrap_or_else(|| panic!("no kind plays {name}"));
        let text = format!(
            "track t {kind} {name}\nmod t.level = 0.5\nmod t.pan = 0\nmod t.send1 = 0.2\nmod t.room = 0.3\n\
            snapshot s: t.ArpOn 1, t.Polyphony 1"
        );
        assert!(
            Song::parse(&text).is_ok(),
            "{m:?}: {:?}",
            Song::parse(&text)
        );
    }
}

/// #680: a track switches only to a model that takes what the song names on
/// it, so the printed text still parses.
#[test]
fn a_track_switches_only_to_a_model_that_takes_its_parameters() {
    let dx7 = Model::Dx7.def().presets[0].preset;
    let mut s =
        Song::parse("track e synth Dx7\nclip a = e .op1level(0.5)\n  \"c4\"").expect("parses");
    assert!(
        !s.set_track_preset(0, Preset::JunoBrass),
        "a Juno has no op1level"
    );
    assert_eq!(s.tracks[0].preset, Some(dx7), "unchanged");
    assert!(s.set_track_preset(0, Model::Dx7.def().presets[1].preset));
    assert_eq!(Song::parse(&s.print()).map(|_| ()), Ok(()));
    // A mod line, an auto and a snapshot on the track count; on a strip not.
    for line in [
        "mod b.lforate = sine",
        "auto x = b.LfoRate 1 /1",
        "snapshot y: b.LfoRate 2",
    ] {
        let mut s = Song::parse(&format!("track b synth Juno106\n{line}\n")).expect(line);
        assert!(!s.set_track_preset(0, Preset::MiniLead), "{line}");
        assert!(s.set_track_preset(0, Preset::JunoPad), "{line}");
    }
    let mut s = Song::parse("track b synth Juno106\nmod strip1.lforate = sine\n").expect("parses");
    assert!(s.set_track_preset(0, Preset::MiniLead));
    // A setting, and a lane the model has no pad for (#662).
    let mut s = Song::parse(
        "setting m = Minimoog MiniLead\nsetting j = Juno106 JunoPad\n\
        track b synth j\nclip a = b .lforate(4)\n  \"c3\"",
    )
    .expect("parses");
    assert!(!s.set_track_setting(0, 0));
    assert!(s.set_track_setting(0, 1));
    let mut s = Song::parse("track d drums Tr808\nclip a = d\n  cy x...").expect("parses");
    assert!(!s.set_track_preset(0, Preset::PadsLoud));
    assert!(s.set_track_preset(0, Preset::Kit909));
}

#[test]
fn a_lane_without_a_pad_on_its_model_says_where() {
    let cases: [(&str, usize, usize, &str); 5] = [
        (
            "track d drums PadSampler\nclip a = d\n  cl x...",
            3,
            3,
            "a Pad Sampler has no such pad: name it p0 to p15",
        ),
        (
            "track p sampler PadSampler\nclip a = p\n  bd x...\n  cy x...",
            4,
            3,
            "a Pad Sampler has no such pad: name it p0 to p15",
        ),
        (
            "track d drums Tr808\nclip a = d\n  p5 x...",
            3,
            3,
            "a pad by number is for a Pad Sampler",
        ),
        (
            "track d drums Tr909 Hard909\nclip a = d\n  bd x...\n  p0 x...",
            4,
            3,
            "this pad already has a lane",
        ),
        (
            "track d drums Tr909 Hard909\nclip a = d\n  p1 x...",
            3,
            3,
            "a pad by number is for a Pad Sampler",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text).map(|_| ()),
            Err(SongError {
                line,
                col,
                msg: msg.into()
            }),
            "{text}"
        );
    }
}

#[test]
fn sampler_tracks_hold_generators() {
    // #251: a call is notes on a sampler track too, never a lane.
    let text = "scale c minor\ntrack keys sampler\n\nclip w = keys live\n  walk(c4,8,1)\nclip a = keys\n  arp([c4,e4,g4],up,16)\nclip e = keys\n  euclid(3,8) c4\n";
    let s = Song::parse(text).expect("parses");
    assert!(
        s.clips
            .iter()
            .all(|f| f.notes.is_some() && f.lanes.is_empty())
    );
    assert!(s.clips[0].live);
    let printed = s.print();
    assert!(printed.contains("clip w = keys live\n  walk(c4,8,1)\n"));
    assert!(printed.contains("  euclid(3,8) c4\n"));
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn bars_is_not_for_lanes() {
    // #251: `bars N` before lanes was dropped; it is an error at the `bars`.
    assert_eq!(
        Song::parse("track p sampler\nclip a = p bars 2\n  bd x..."),
        Err(SongError {
            line: 2,
            col: 12,
            msg: crate::notes::BARS_MSG.into()
        })
    );
}

/// #612, #613: an arp's modes, octaves, triplet rates and `bars N` parse
/// and print back; `bars` stays off a call of a clip and stops at 64.
#[test]
fn an_arp_takes_bars_octaves_and_tidals_modes() {
    let text = "track lead synth\nclip rise = lead bars 3\n  arp([c4,e4,g4],up&down:2,12)\nclip p = lead\n  arp(rise,pinkyupdown:3,24)\n";
    let s = Song::parse(text).expect("parses");
    let rise = s.clips[0].notes.as_ref().expect("notes");
    assert_eq!((rise.bars, rise.events.len()), (3, 36));
    let printed = s.print();
    assert!(
        printed.contains("clip rise = lead bars 3\n  arp([c4,e4,g4],up&down:2,12)\n"),
        "{printed}"
    );
    assert!(
        printed.contains("  arp(rise,pinkyupdown:3,24)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
    // One bar is the default, and prints without `bars`.
    let one = Song::parse("track lead synth\nclip a = lead bars 1\n  arp([c4,e4],up,16)").unwrap();
    assert!(one.print().contains("clip a = lead\n"));
    assert_eq!(
        Song::parse(
            "track lead synth\nclip a = lead\n  \"c4\"\nclip b = lead bars 2\n  arp(a,up,16)"
        ),
        Err(SongError {
            line: 4,
            col: 15,
            msg: crate::notes::BARS_MSG.into()
        })
    );
    assert_eq!(
        Song::parse("track lead synth\nclip a = lead bars 65\n  arp([c4,e4],up,16)").map(|_| ()),
        Err(SongError {
            line: 2,
            col: 20,
            msg: "a line is 1 to 64 bars".into()
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
            Err(SongError {
                line,
                col: 1,
                msg: msg.into()
            }),
            "{text:?}"
        );
    }
}

const GENERATED: &str = "\
tempo 120
scale c minor
track kit drums
track lead synth

clip beat = kit
  bd euclid(3,8)
  sn euclid(5,16,2)
  ch x.x.
clip line = lead
  euclid(5,8) scale c4
clip pulse = lead
  euclid(3,8,1) g3!
";

#[test]
fn generators_and_scales_parse_and_print_back() {
    let s = Song::parse(GENERATED).expect("parses");
    assert_eq!(s.scale.map(|k| (k.root, k.mode.name())), Some((0, "minor")));
    let lanes = &s.clips[0].lanes;
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
            "a root is a letter a to g, maybe # or b, maybe an octave",
        ),
        ("scale c funky", 1, 9, "no scale has this mode"),
        ("scale c", 1, 8, "a mode goes here: scale c minor"),
        ("scale c minor\nscale d major", 2, 1, "a song has one scale"),
        (
            "track t drums\nclip a = t\n  bd x\nscale c minor",
            4,
            1,
            "the scale goes before the clips",
        ),
        (
            "track t drums\nclip a = t\n  bd euclid(9,8)",
            3,
            6,
            "euclid cannot have more hits than steps",
        ),
        (
            "track t synth\nclip a = t\n  euclid(3,8) scale c4",
            3,
            15,
            "a scale walk needs a scale line before it",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text),
            Err(SongError {
                line,
                col,
                msg: msg.into()
            }),
            "{text:?}"
        );
    }
}

#[test]
fn generator_calls_read_earlier_clips_and_print_back() {
    let text = "\
scale d dorian
track lead synth

clip riff = lead
  d4:4 f4:8 a4:8 c5:2
clip arp = lead
  arp([d4,f4,a4],updown,16)
clip wander = lead
  walk(d4,8,3)
clip learned = lead
  markov(1,riff,9)
clip changed = lead
  mutate(riff,40,2)
";
    let s = Song::parse(text).expect("parses");
    assert_eq!(s.clips[3].notes.as_ref().map(|n| n.events.len()), Some(4));
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
fn a_call_cannot_read_a_later_or_missing_clip() {
    let text = "track t synth\nclip a = t\n  mutate(b,10,1)\nclip b = t\n  c4:4\n";
    assert_eq!(
        Song::parse(text),
        Err(SongError {
            line: 3,
            col: 10,
            msg: "no note clip with this name comes before this one".into()
        })
    );
    let own = "track t synth\nclip a = t\n  markov(1,a,1)\n";
    assert!(Song::parse(own).is_err());
}

#[test]
fn a_live_clip_prints_and_parses_back() {
    let text = "scale c minor\ntrack lead synth\n\nclip w = lead live\n  walk(c4,8,1)\nclip s = lead\n  arp([c4,e4],up,8)\n";
    let s = Song::parse(text).expect("parses");
    assert!(s.clips[0].live && !s.clips[1].live);
    let printed = s.print();
    assert!(printed.contains("clip w = lead live\n  walk(c4,8,1)\n"));
    assert!(printed.contains("clip s = lead\n"));
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn live_errors_say_where() {
    let cases: [(&str, usize, usize, &str); 3] = [
        (
            "track k drums\nclip a = k live\n  bd x",
            2,
            12,
            "only a note clip can be live",
        ),
        (
            "track t synth\nclip a = t live\n  c4:4",
            3,
            3,
            "a live clip is a call: arp, walk, markov, mutate, root, prog or bass",
        ),
        (
            "track t synth\nclip a = t live\n  euclid(3,8) c4",
            3,
            3,
            "a live clip is a call: arp, walk, markov, mutate, root, prog or bass",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text),
            Err(SongError {
                line,
                col,
                msg: msg.into()
            }),
            "{text:?}"
        );
    }
}

#[test]
fn freezing_replaces_the_call_with_notes() {
    let mut s = Song::parse(
        "scale c minor\ntrack t synth\nclip w = t live\n  walk(c4,4,1)\nclip p = t\n  c4:4\n",
    )
    .unwrap();
    assert!(!s.freeze(1, None), "a written clip has no call");
    assert!(!s.freeze(9, None), "no such clip");
    let before = s.clips[0].notes.as_ref().unwrap().events.clone();
    assert!(s.freeze(0, None));
    assert!(!s.clips[0].live);
    let printed = s.print();
    assert!(!printed.contains("walk(") && !printed.contains("live"));
    assert_eq!(
        Song::parse(&printed).unwrap().clips[0]
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

clip beat = kit
  bd x..x..x...x..x..
  cp ....x.......x...
  ch x.x.x.x.x.x.x.x.
  oh ..x...x...x...x.
  cb euclid(5,16,2)

clip acid = bass
  \"a1! a1 [a2 a1] ~ a1! ~ [c2 e2] a1\"

clip stabs = lead
  \"~ [a3!,c4!,e4!] ~ ~ ~ [a3,c4,e4] ~ ~\"

clip arp = lead
  arp([a3,c4,e4,g4],updown,16)

clip drift = lead live
  walk(a3,16,7)
";

#[test]
fn an_electro_example_parses_and_prints_back() {
    let s = Song::parse(ELECTRO).expect("parses");
    assert_eq!((s.tracks.len(), s.clips.len()), (3, 5));
    assert_eq!(Song::parse(&s.print()), Ok(s));
}

/// #486, ADR-0031: a song in the old words (`frag`, `section`, a `scene`
/// that is a mixer snapshot) reads as the same song in Ableton's words, and
/// prints in them, its comments kept.
#[test]
fn a_song_in_the_old_words_reads_and_prints_in_the_new() {
    let old = "tempo 120
track kit drums
# the beat
frag beat = kit /16
  bd x...x...x...x...
scene dub: kit.Level 0.5
scene drop : kit.Level 1
section a 2: beat [dub] # quiet
section b 2: beat [drop]
arrange a b
";
    let new = "tempo 120
track kit drums
# the beat
clip beat = kit /16
  bd x...x...x...x...
snapshot dub: kit.Level 0.5
snapshot drop: kit.Level 1
scene a 2: beat [dub] # quiet
scene b 2: beat [drop]
arrange a b
";
    let (o, n) = (
        Song::parse(old).expect("old"),
        Song::parse(new).expect("new"),
    );
    assert_eq!(o, n);
    assert_eq!(o.snapshots.len(), 2);
    assert_eq!(o.scenes.len(), 2);
    assert_eq!(o.print(), n.print());
    let printed = o.print();
    for gone in ["frag ", "section ", "scene dub", "scene drop"] {
        assert!(!printed.contains(gone), "{gone} in\n{printed}");
    }
    assert!(printed.contains("# the beat") && printed.contains("# quiet"));
}

/// The example songs and sounds (#562) parse, print back equal and are arranged.
#[test]
fn the_example_songs_parse_and_print_back() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for (dir, want) in [("songs", 29), ("sounds", 8)] {
        let mut seen = 0;
        for entry in std::fs::read_dir(root.join(dir)).expect("examples dir") {
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
        }
        assert_eq!(seen, want, "examples/{dir}");
    }
}

const ARRANGED: &str = "\
track kit drums
clip beat = kit /16
  bd x...x...x...x...
clip fill = kit /16
  sn ..x.
scene intro 2: beat
scene main 4 : beat fill
scene gap 1:
arrange intro main main gap
loop 3 6
";

/// Spec 002 Req 4: scenes, the arrangement and a loop region parse and
/// print canonically.
#[test]
fn scenes_and_the_arrangement_parse_and_print() {
    let s = Song::parse(ARRANGED).expect("parses");
    assert_eq!(s.scenes.len(), 3);
    assert_eq!(
        s.scenes[1],
        Scene {
            name: "main".into(),
            bars: 4,
            clips: vec![0, 1],
            autos: vec![],
            snapshots: vec![],
        }
    );
    assert_eq!(
        s.scenes[2].clips,
        Vec::<usize>::new(),
        "an empty scene is silent bars"
    );
    assert_eq!(s.arrange, vec![0, 1, 1, 2]);
    assert_eq!(s.loop_bars, Some((3, 6)));
    assert_eq!(s.bars(), 11);
    let text = s.print();
    assert!(text.ends_with("scene intro 2: beat\nscene main 4: beat fill\nscene gap 1:\narrange intro main main gap\nloop 3 6\n"), "{text}");
    assert_eq!(Song::parse(&text), Ok(s));
}

#[test]
fn a_step_falls_in_its_scene_and_the_loop_wraps() {
    let s = Song::parse(ARRANGED).expect("parses");
    assert_eq!(
        s.at(0),
        At::In {
            entry: 0,
            scene: 0,
            local: 0
        }
    );
    assert_eq!(
        s.at(31),
        At::In {
            entry: 0,
            scene: 0,
            local: 31
        }
    );
    assert_eq!(
        s.at(32),
        At::In {
            entry: 1,
            scene: 1,
            local: 0
        }
    );
    // Bars 3 to 6 are steps 32..96; step 96 wraps back to 32.
    assert_eq!(
        s.at(96),
        At::In {
            entry: 1,
            scene: 1,
            local: 0
        }
    );
    assert_eq!(
        s.at(96 + 63),
        At::In {
            entry: 1,
            scene: 1,
            local: 63
        }
    );
    let mut no_loop = s.clone();
    no_loop.loop_bars = None;
    assert_eq!(
        no_loop.at(96),
        At::In {
            entry: 2,
            scene: 1,
            local: 0
        },
        "main again, from its start"
    );
    assert_eq!(
        no_loop.at(160),
        At::In {
            entry: 3,
            scene: 2,
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
    let head = "track kit drums\nclip b = kit\n  bd x\n";
    let cases: [(&str, usize, usize, &str); 12] = [
        (
            "scene 1a 2: b",
            4,
            7,
            "a name is a letter, then letters, digits or _",
        ),
        ("scene a", 4, 8, "a number of bars and : go here"),
        ("scene a 2 b", 4, 11, ": goes here, after the bars"),
        ("scene a 0: b", 4, 9, "a scene is 1 to 256 bars"),
        ("scene a 300: b", 4, 9, "a scene is 1 to 256 bars"),
        (
            "scene a 2: c",
            4,
            12,
            "no clip, auto or [snapshot] has this name",
        ),
        ("scene a 2: b b", 4, 14, "this clip is already in the scene"),
        (
            "scene a 2: b\nscene a 1: b",
            5,
            7,
            "there is already a scene with this name",
        ),
        ("arrange x", 4, 9, "no scene has this name"),
        (
            "scene a 2: b\narrange",
            5,
            8,
            "scenes go here, in the order they play",
        ),
        (
            "scene a 2: b\narrange a\nloop 2 3",
            6,
            1,
            "the loop ends after the arrangement",
        ),
        ("loop 1 1", 4, 1, "a loop needs an arrange line"),
    ];
    for (tail, line, col, msg) in cases {
        let err = Song::parse(&format!("{head}{tail}")).expect_err(tail);
        assert_eq!((err.line, err.col, &*err.msg), (line, col, msg), "{tail}");
    }
    let err =
        Song::parse(&format!("{head}scene a 1:\narrange a\nloop 3 2")).expect_err("backwards");
    assert_eq!(err.msg, "the last bar comes after the first");
    let err = Song::parse(&format!("{head}scene a 1:\narrange a\narrange a")).expect_err("two");
    assert_eq!(err.msg, "a song has one arrange line");
}

const AUTOMATED: &str = "\
track kit drums
clip beat = kit /16
  bd x...
auto sweep = kit.BdTune ramp -12 12 /2
auto duck = strip3.Level 1 0.5 0.25 1 /1
auto wet = master.P2Return 0 0.4 /4
snapshot drop: strip1.Mute 1, group2.Send1 0.5, master.P2Return 0.4
scene a 2: beat sweep [drop]
scene b 1: duck wet
arrange a b
";

/// ADR-0015: automation lanes and snapshots, by parameter name on a target,
/// parse and print canonically.
#[test]
fn automation_and_snapshots_parse_and_print() {
    let s = Song::parse(AUTOMATED).expect("parses");
    assert_eq!(s.autos.len(), 3);
    assert_eq!(s.autos[0].target, Target::Track(0));
    assert_eq!(s.autos[0].param, Param::BdTune);
    assert_eq!(s.autos[0].shape, Shape::Ramp(-12.0, 12.0));
    assert_eq!(s.autos[1].target, Target::Strip(2));
    assert_eq!(s.autos[1].shape, Shape::Steps(vec![1.0, 0.5, 0.25, 1.0]));
    assert_eq!(s.autos[2].target, Target::Master);
    assert_eq!(
        s.snapshots[0].sets,
        vec![
            (Target::Strip(0), Param::Mute, 1.0),
            (Target::Strip(17), Param::Send1, 0.5),
            (Target::Master, Param::P2Return, 0.4)
        ]
    );
    assert_eq!(
        (s.scenes[0].autos.clone(), s.scenes[0].snapshots.clone()),
        (vec![0], vec![0])
    );
    assert_eq!(s.scenes[1].autos, vec![1, 2]);
    let text = s.print();
    assert!(
        text.contains("auto sweep = kit.BdTune ramp -12 12 /2\n"),
        "{text}"
    );
    assert!(
        text.contains("snapshot drop: strip1.Mute 1, group2.Send1 0.5, master.P2Return 0.4\n"),
        "{text}"
    );
    assert!(text.contains("scene a 2: beat sweep [drop]\n"), "{text}");
    assert_eq!(Song::parse(&text), Ok(s));
}

/// ADR-0019: a parameter is named in any case, as `kit.bdtune` or
/// `kit.BdTune`, and prints by its registry name.
/// #683: an auto's values may be one quoted sequence, nesting as a mod's,
/// one cycle a bar from the lane's start; a `~` holds the last value.
#[test]
fn an_auto_takes_a_nested_sequence() {
    let head = "track kit drums\nclip b = kit\n  bd x\n";
    let s =
        Song::parse(&format!("{head}auto a = kit.BdTune \"<-3 [5  ~]>\" /2\n")).expect("parses");
    let at = |steps: f64| s.autos[0].value_at(steps);
    assert_eq!([at(4.0), at(20.0)], [-3.0, 5.0]);
    assert!(at(28.0).is_nan(), "a ~ has no value");
    assert_eq!(at(36.0), -3.0, "round again after two bars");
    assert!(
        s.print()
            .contains("auto a = kit.BdTune \"<-3 [5 ~]>\" /2\n"),
        "{}",
        s.print()
    );
    assert_eq!(Song::parse(&s.print()), Ok(s.clone()));
    // Plain values as before.
    let plain = Song::parse(&format!("{head}auto a = kit.BdTune 1 2 /1\n")).expect("parses");
    assert_eq!(plain.autos[0].shape, Shape::Steps(vec![1.0, 2.0]));
    for (line, col, msg) in [
        (
            "auto a = kit.BdTune \"1 x\" /1",
            24,
            "a sequence holds numbers, ~, [ ] and < >, e.g. \"<300 [800 1200]>\"",
        ),
        (
            "auto a = kit.BdTune \"<1 [2\" /1",
            25,
            "this bracket is not closed",
        ),
        (
            "auto a = kit.BdTune \"1\" 2 /1",
            21,
            "a sequence is one quoted text: \"<300 [800 1200]>\"",
        ),
    ] {
        let err = Song::parse(&format!("{head}{line}")).expect_err(line);
        assert_eq!(
            (err.line, err.col, err.msg.as_ref()),
            (4, col, msg),
            "{line}"
        );
    }
}

#[test]
fn a_parameter_name_is_read_in_any_case() {
    let lower = AUTOMATED
        .replace("kit.BdTune", "kit.bdtune")
        .replace("master.P2Return 0.4", "master.p2return 0.4");
    assert_eq!(Song::parse(&lower), Song::parse(AUTOMATED));
}

#[test]
fn a_lane_steps_or_ramps_over_its_length_and_loops() {
    let s = Song::parse(AUTOMATED).expect("parses");
    let (ramp, steps) = (&s.autos[0], &s.autos[1]);
    assert_eq!(ramp.value_at(0.0), -12.0);
    assert_eq!(ramp.value_at(16.0), 0.0, "half way over two bars");
    assert!((ramp.value_at(31.999) - 12.0).abs() < 0.01);
    assert_eq!(ramp.value_at(32.0), -12.0, "and round again");
    assert_eq!(
        [0.0, 3.9, 4.0, 8.0, 12.0, 15.9].map(|t| steps.value_at(t)),
        [1.0, 1.0, 0.5, 0.25, 1.0, 1.0]
    );
}

#[test]
fn automation_errors_say_where() {
    let head = "track kit drums\nclip b = kit\n  bd x\n";
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
            "auto a = kit.BdTune ramp 1 /1",
            4,
            21,
            "a ramp goes from one value to another",
        ),
        (
            "auto b = kit.BdTune 1 /1",
            4,
            6,
            "there is already a clip or auto with this name",
        ),
        (
            "snapshot s: strip1.Mute 1 strip2.Mute 1",
            4,
            27,
            "a comma goes between settings",
        ),
        ("scene a 1: [none]", 4, 12, "no snapshot has this name"),
    ];
    for (tail, line, col, msg) in cases {
        let err = Song::parse(&format!("{head}{tail}")).expect_err(tail);
        assert_eq!((err.line, err.col, &*err.msg), (line, col, msg), "{tail}");
    }
}

/// #171: arranger edits keep the song valid and print back as text.
#[test]
fn arranger_edits_change_the_song_and_its_text() {
    let mut s = Song::parse(AUTOMATED).expect("parses");
    // Toggle: take the beat out of scene a, put the lane duck in, the snapshot out.
    assert!(s.toggle(0, 0, 0));
    assert!(s.toggle(0, 1, 1));
    assert!(s.toggle(0, 2, 0));
    assert_eq!(
        (
            s.scenes[0].clips.clone(),
            s.scenes[0].autos.clone(),
            s.scenes[0].snapshots.clone()
        ),
        (vec![], vec![0, 1], vec![])
    );
    assert!(
        !s.toggle(0, 0, 9) && !s.toggle(9, 0, 0) && !s.toggle(0, 3, 0),
        "no such item, scene or kind"
    );
    // Add a scene: named partN, appended to the arrangement.
    assert_eq!(s.add_scene(4), Some(2));
    assert_eq!(
        (s.scenes[2].name.as_str(), s.arrange.clone()),
        ("part1", vec![0, 1, 2])
    );
    assert_eq!(s.add_scene(0), None);
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
        text.contains("scene a 2: sweep duck\n")
            && text.contains("scene part1 8:\n")
            && text.contains("arrange a b b\n"),
        "{text}"
    );
    assert_eq!(Song::parse(&text), Ok(s));
}

#[test]
fn editing_a_note_rewrites_the_text() {
    use crate::notes::Edit;
    let mut s = Song::parse("track t synth\nclip a = t\n  c4:4 e4:4 g4:2\n").unwrap();
    assert!(s.edit_note(0, Edit::Add { tick: 36, note: 72 }));
    let text = s.print();
    assert!(
        text.contains("clip a = t\n  \"c4@12 e4@12 g4@12 c5@3 ~@9\"\n"),
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
        "no such clip"
    );
}

#[test]
fn a_generated_clip_is_frozen_before_it_is_edited() {
    use crate::notes::Edit;
    let mut s = Song::parse(
        "scale c minor\ntrack t synth\nclip a = t\n  euclid(3,8) c4\nclip b = t\n  walk(c4,4,1)\n",
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
    let text = "track v synth\nclip a = v bars 2\n  d5@0:48:100 f#5@6:6:64\n";
    let mut s = Song::parse(text).expect("parses");
    assert!(s.edit_note(0, notes::Edit::Add { tick: 24, note: 69 }));
    let printed = s.print();
    assert!(
        printed.contains("clip a = v bars 2\n  d5@0:48:100 f#5@6:6:64 a4@24:3\n"),
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

clip beat = kit /16
  bd x..x..x...x..x..
clip sub = bass
  \"e2 ~ ~ e2\"
clip nile = lead
  \"e4 f4 g#4 ~\"
clip sand = lead live
  arp([e4,g#4,b4],updown,16)
clip hold = pad
  \"[e3,g#3,b3]\"
clip deep = low
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
        "track a synth\ntrack b synth\nclip x = a\n  \"[c4,e4]\"\nclip y = b live\n  arp([c4,e4],up,16)\n",
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
            "a model (as Minimoog or Tr808) or a setting goes here".into()
        ))
    );
    assert_eq!(
        at("track a synth Minimoog ProLead\n"),
        Err((1, 24, "this preset is for another model".into()))
    );
    assert_eq!(
        at("track a synth Tr808\n"),
        Err((1, 15, "this model does not play this kind of track".into()))
    );
    assert_eq!(
        at("track a synth Minimoog MiniLead x\n"),
        Err((1, 33, "unexpected text".into()))
    );
    assert_eq!(
        at("setting s = Minimoog MiniLead: Level 1\n"),
        Err((1, 32, "a setting changes the synth's own parameters".into()))
    );
    assert_eq!(
        at("setting s = Minimoog MiniLead: Cutoff 1 Resonance 1\n"),
        Err((1, 41, "a comma goes between changes".into()))
    );
    assert_eq!(
        at("track a synth\nsetting s = Minimoog MiniLead\n"),
        Err((2, 1, "a setting goes before the tracks".into()))
    );
    assert_eq!(
        at("setting s = Tr808 Kit808\ntrack a synth s\n"),
        Err((2, 15, "this model does not play this kind of track".into()))
    );
    assert_eq!(
        at("setting Minimoog = Minimoog MiniLead\n"),
        Err((
            1,
            9,
            "a name is a letter, then letters, digits or _, and not a model's".into()
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
clip beat = kit /16
  # four on the floor
  bd x...x...x...x...
  cp ....x.......x...   # clap on 2 and 4

clip line = bass
  # a sharp stays a sharp: c#4
  \"a2 ~ a2 [a2 c3]\"   # root and fifth

# the arrangement
scene intro 4: beat line   # four bars
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
        "# the beat\nclip beat = kit /16\n",
        "  # four on the floor\n  bd x...x...x...x...\n",
        "  cp ....x.......x... # clap on 2 and 4\n",
        "clip line = bass\n  # a sharp stays a sharp: c#4\n  \"a2 ~ a2 [a2 c3]\" # root and fifth\n",
        "# the arrangement\nscene intro 4: beat line # four bars\n",
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
        Song::parse("track k drums\nclip a = k\n  # the kick\n  bd x...\n  sn ..x. # snare\n")
            .expect("parses");
    let without = Song::parse("track k drums\nclip a = k\n  sn ..x.\n").expect("parses");
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
    let s = Song::parse("tempo 100\ntrack k drums\nclip a = k\n  bd x...\n").expect("parses");
    assert_eq!(
        s.print(),
        "tempo 100\nswing 50\ntrack k drums Tr808 Kit808\n\nclip a = k /16\n  bd x...\n"
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

/// The normative definition of the language (#383): the composer's part and
/// the sound designer's (#514).
const LANGUAGE: &str = include_str!("../../../../.openspec/language.md");
const SOUND_DESIGN: &str = include_str!("../../../../.openspec/sound-design.md");

/// #383: every ```song block of the language's definition parses and prints
/// back equal.
#[test]
fn the_language_examples_parse() {
    let n = song_blocks_parse(".openspec/language.md", LANGUAGE)
        + song_blocks_parse(".openspec/sound-design.md", SOUND_DESIGN);
    assert!(n >= 20, "{n} song blocks");
}

/// #383: the definition grows with the language. Every line keyword has a
/// heading; every word the parser reads from a fixed list (track kinds and
/// clip words, generator calls, pattern and signal methods, signal sources,
/// pads, steps, grids, models, modes, chord qualities, arp modes, insert and
/// processor types, filter voicings) is written in its code or grammar.
#[test]
fn the_language_definition_covers_the_parser() {
    // Words in backticks and in fenced blocks, split at what can't be in one.
    let (mut code, mut prose) = (String::new(), String::new());
    let mut fenced = false;
    let mut headings = Vec::new();
    for line in LANGUAGE.lines().chain(SOUND_DESIGN.lines()) {
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
        // `&` too, for arp's `up&down` (#612).
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '&')))
        .collect();
    let in_a_heading = |k: &str| headings.iter().any(|h| h.split([' ', ',']).any(|w| w == k));
    let mut missing: Vec<String> = KEYWORDS
        .iter()
        .filter(|k| !in_a_heading(k))
        .map(|k| format!("heading {k}"))
        .collect();
    let grids: Vec<String> = GRIDS.iter().map(|g| g.to_string()).collect();
    let lists: [Vec<&str>; 16] = [
        lex::WORDS.to_vec(),
        notes::CALLS.to_vec(),
        Pattern::NAMES.to_vec(),
        signal::SOURCES.to_vec(),
        signal::METHODS.to_vec(),
        ALIASES.iter().map(|(a, _)| *a).collect(),
        UNITS.iter().map(|u| u.name).collect(),
        Pad::ALL.iter().map(|(_, n)| *n).collect(),
        Model::ALL.iter().map(|(_, n)| *n).collect(),
        crate::algo::Mode::all()
            .flat_map(|m| m.names().iter().copied())
            .collect(),
        notes::QUALITIES
            .iter()
            .map(|q| q.0)
            .filter(|q| !q.is_empty())
            .collect(),
        notes::ArpMode::WORDS.iter().map(|m| m.1).collect(),
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
        ".openspec/language.md and sound-design.md do not define: {missing:?}"
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
            e.as_ref()
                .is_none_or(|e| !e.msg.starts_with("a line starts with")),
            "{k}: {e:?}"
        );
    }
}

/// #103: a chord clip in the song's key, voiced, prints back as written.
#[test]
fn a_voiced_progression_in_the_key_prints_back() {
    let text =
        "scale a minor\ntrack chords synth\n\nclip prog = chords voicing\n  \"<i VI III VII>\"\n";
    let s = Song::parse(text).expect("parses");
    let f = &s.clips[0];
    assert!(f.voicing.is_some());
    assert!(s.print().contains("clip prog = chords"));
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
            "track kit drums\nclip b = kit /16 voicing\n  bd x...\n",
            2,
            18,
            "voicing is for a clip of notes",
        ),
        (
            "scale c minor\ntrack l synth\nclip w = l live voicing\n  walk(c4,8,1)\n",
            3,
            17,
            "a live clip is not voiced",
        ),
    ] {
        assert_eq!(
            Song::parse(text),
            Err(SongError {
                line,
                col,
                msg: msg.into()
            }),
            "{text}"
        );
    }
}

/// #606, #607: a voicing style and anchor on the clip line, spaced or not,
/// over slash chords; it prints back and parses to the same song.
#[test]
fn a_voicing_style_over_slash_chords_prints_back() {
    let text = "scale c major\ntrack keys synth\n\nclip pad = keys voicing(drop2, c5) .slow(2)\n  \"<ii7 V7/1 Imaj7 a:m7/g>\"\n";
    let s = Song::parse(text).expect("parses");
    let v = s.clips[0].voicing.expect("voiced");
    assert_eq!((v.style, v.anchor), (Some(notes::Style::Drop2), Some(72)));
    let printed = s.print();
    assert!(
        printed
            .contains("clip pad = keys voicing(drop2,c5) .slow(2)\n  \"<ii7 V7/1 Imaj7 a:m7/g>\""),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed).expect("parses back"), s);
    let events = &s.clips[0].notes.as_ref().unwrap().events;
    assert!(events.iter().all(|e| e.note <= 72), "{events:?}");
    // a:m7/g: the g under the chord.
    let last = events.iter().map(|e| e.start).max().unwrap();
    let low = events
        .iter()
        .filter(|e| e.start == last)
        .map(|e| e.note)
        .min();
    assert_eq!(low.map(|n| n % 12), Some(7));
}

#[test]
fn a_bad_voicing_says_where() {
    let styles = "a voicing is close, open, drop2, drop3, shell or quartal, then maybe an anchor note, as voicing(drop2,c5)";
    for (text, col, msg) in [
        (
            "track k synth\nclip a = k voicing(wide)\n  \"c:m7\"\n",
            12,
            styles,
        ),
        (
            "track k synth\nclip a = k voicing(drop2, h5)\n  \"c:m7\"\n",
            12,
            "an anchor is a note with its octave, as c5",
        ),
        (
            "track k synth\nclip a = k\n  \"c:m7/5\"\n",
            9,
            "after / comes an inversion 1 to 3 or a bass note a to g, as c:m7/1 or c/g",
        ),
    ] {
        assert_eq!(
            Song::parse(text),
            Err(SongError {
                line: if col == 9 { 3 } else { 2 },
                col,
                msg: msg.into()
            }),
            "{text}"
        );
    }
}

/// #103: one progression feeds the pad, the bass and the arp, and each track
/// picks a synth for its role.
#[test]
fn a_progression_feeds_pad_bass_and_arp() {
    let text = "scale c minor\ntrack pad synth\ntrack bass synth\ntrack arp synth\n\n\
                clip chords = pad voicing\n  prog(4,7)\n\
                clip low = bass\n  root(chords)\n\
                clip ripple = arp\n  arp(chords,updown,16)\n";
    let s = Song::parse(text).expect("parses");
    let printed = s.print();
    for line in [
        "  prog(4,7)",
        "  root(chords)",
        "  arp(chords,updown,16)",
        "clip chords = pad",
    ] {
        assert!(printed.contains(line), "{line} in\n{printed}");
    }
    assert_eq!(Song::parse(&printed).expect("parses back"), s);
    let bars: Vec<u32> = s
        .clips
        .iter()
        .map(|f| f.notes.as_ref().unwrap().bars)
        .collect();
    assert_eq!(bars, [4, 4, 4]);
    // The bass plays the roots of the voiced chords: the same pitch classes.
    let chords = &s.clips[0].notes.as_ref().unwrap().events;
    for e in &s.clips[1].notes.as_ref().unwrap().events {
        assert!(
            chords
                .iter()
                .any(|c| c.start == e.start && c.note % 12 == e.note % 12)
        );
        assert!(e.note < 48, "below C3");
    }
}

/// #608, #609, #610: a progression with sevenths, a rhythm and a style
/// feeds the bass and the arp, which follow its chords where they start.
#[test]
fn a_prog_with_a_rhythm_feeds_root_and_arp() {
    let text = "scale c minor\ntrack pad synth\ntrack bass synth\ntrack arp synth\n\n\
                clip chords = pad voicing\n  prog(4,7,jazz,\"4 2 2\",7th)\n\
                clip low = bass\n  root(chords)\n\
                clip ripple = arp\n  arp(chords,up,8)\n";
    let s = Song::parse(text).expect("parses");
    let printed = s.print();
    assert!(
        printed.contains("  prog(4,7,7th,\"4 2 2\",jazz)"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed).expect("parses back"), s);
    let events = |k: usize| s.clips[k].notes.as_ref().unwrap().events.clone();
    let mut starts: Vec<u32> = events(0).iter().map(|e| e.start).collect();
    starts.dedup();
    assert_eq!(starts, [0, 48, 72, 96, 144, 168]);
    assert!(events(0).iter().all(|e| starts.contains(&e.start)));
    assert_eq!(events(0).len(), 6 * 4, "a seventh on each");
    // A root where each chord starts, and an arp that plays each chord's
    // notes from its start.
    let roots: Vec<u32> = events(1).iter().map(|e| e.start).collect();
    assert_eq!(roots, starts);
    let chords = events(0);
    for e in events(2) {
        let at = starts.iter().rev().find(|s| **s <= e.start).unwrap();
        assert!(
            chords
                .iter()
                .any(|c| c.start == *at && c.note % 12 == e.note % 12),
            "{e:?}"
        );
    }
}

/// #611: a bass line follows a progression with a rhythm, and a live
/// clip's first bar, as root does.
#[test]
fn a_bass_follows_the_chords_and_a_live_source() {
    let text = "scale c minor\ntrack pad synth\ntrack bass synth\n\n\
                clip chords = pad voicing\n  prog(4,7,\"4 2 2\")\n\
                clip low = bass\n  bass(chords,walk,2,5)\n\
                clip moving = pad live\n  prog(2,3)\n\
                clip under = bass\n  bass(moving,fifths)\n";
    let s = Song::parse(text).expect("parses");
    let printed = s.print();
    assert!(printed.contains("  bass(chords,walk,5)"), "{printed}");
    assert_eq!(Song::parse(&printed).expect("parses back"), s);
    let events = |k: usize| s.clips[k].notes.as_ref().unwrap().events.clone();
    let chords = events(0);
    for start in [0, 48, 72, 96, 144, 168] {
        let low = chords
            .iter()
            .filter(|e| e.start == start)
            .map(|e| e.note)
            .min()
            .unwrap();
        let bass = events(1).into_iter().find(|e| e.start == start).unwrap();
        assert_eq!(bass.note % 12, low % 12, "the root at {start}");
    }
    // The live clip's first cycle is its source, as root reads it: a root
    // and its fifth on each chord, the tonic first and the dominant last.
    let under = events(3);
    assert_eq!(under.len(), 4);
    assert_eq!((under[0].note % 12, under[2].note % 12), (0, 7));
}

// --- Mixer lines (ADR-0018) -----------------------------------------------------

const MIXED: &str = "track kit drums\ntrack bass synth\n\
    strip bass: Level 0.8, Pan -0.2, I1Type Overdrive, I1A 0.6, Out group1\n\
    strip strip5: Mute 1\n\
    group 1 drums: Level 0.9, Out group3\n\
    group 3: Out none\n\
    master: MasterGain 0.6, P1Type Echo, P1Return 0.25, P3Type Chorus\n";

/// #568: the groups a song uses are those with a `group` line and those an
/// `Out` or a pad's `out` goes to; `Out none` and the master are none.
#[test]
fn a_song_uses_its_group_lines_and_the_groups_it_routes_to() {
    // group 1 and 3 have lines, group 1 is routed to, `Out none` is no group.
    assert_eq!(Song::parse(MIXED).expect("parses").groups_used(), 0b101);
    let routed = "track kit drums PadSampler\ntrack bass synth\n\
        strip bass: Out group2\n\
        pad kit 1 = upright-piano-kw/C4vH : out 5\n";
    assert_eq!(Song::parse(routed).expect("parses").groups_used(), 0b1_0010);
    assert_eq!(
        Song::parse("track bass synth\nstrip bass: Out master\n")
            .expect("parses")
            .groups_used(),
        0
    );
}

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
            "an insert is Off, Overdrive, Distortion, Fuzz, Eq, Comp, Vocoder or Bitcrush",
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
            Err(SongError {
                line: 2,
                col,
                msg: msg.into()
            }),
            "{line}"
        );
    }
    let twice = format!("{head}strip bass: Level 1\nstrip bass: Pan 0\n");
    assert_eq!(
        Song::parse(&twice),
        Err(SongError {
            line: 3,
            col: 1,
            msg: "this strip already has a line".into()
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
    let text = "track kit drums\nclip b = kit\n  bd x\n\
        mod kit.BdTune = sine.range(-12,12).slow(4)   # sweep\n\
        mod master.p2return = lfo(0.2, saw).range(0, 0.5) + perlin * 0.1\n";
    let s = Song::parse(text).expect("parses");
    assert_eq!(s.mods.len(), 2);
    assert_eq!(
        (s.mods[0].target, s.mods[0].param),
        (Target::Track(0), Param::BdTune)
    );
    assert_eq!(
        (s.mods[1].target, s.mods[1].param),
        (Target::Master, Param::P2Return)
    );
    let printed = s.print();
    assert!(
        printed.contains("\nmod kit.bdtune = sine.range(-12, 12).slow(4) # sweep\n"),
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
    let head = "track kit drums\nclip b = kit\n  bd x\n";
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
        ("mod kit.bdtune 1", 16, "= and a signal go here"),
        (
            "mod kit.bdtune =",
            17,
            "a signal goes here, e.g. sine.range(300, 3000)",
        ),
        (
            "mod kit.bdtune =  sine.wobble(2)",
            24,
            "no such method: range exprange slow fast segment lag onNote",
        ),
        (
            "mod kit.bdtune = lfo(1, pink)",
            25,
            "a shape is sine, cosine, saw, tri or square",
        ),
    ] {
        let err = Song::parse(&format!("{head}{line}")).expect_err(line);
        assert_eq!((err.line, err.col, &*err.msg), (4, col, msg), "{line}");
    }
    let twice = format!("{head}mod kit.bdtune = 1\nmod kit.BdTune = 2");
    let err = Song::parse(&twice).expect_err("twice");
    assert_eq!(
        (err.line, err.col, &*err.msg),
        (5, 5, "this parameter already has a mod")
    );
    let params = ["level", "pan", "send1"];
    let many: String = (0..33)
        .map(|n| format!("mod strip{}.{} = 1\n", n % 16 + 1, params[n / 16]))
        .collect();
    let err = Song::parse(&format!("{head}{many}")).expect_err("many");
    assert_eq!((err.line, &*err.msg), (36, "a song has at most 32 mods"));
    let big = format!("{head}mod kit.bdtune = {}1", "1 + ".repeat(130));
    let err = Song::parse(&big).expect_err("big");
    assert_eq!(err.msg, "a song's signals have at most 256 nodes");
}

/// ADR-0023: a signal with `env` or a list goes on a Mono or Poly track's
/// voice parameters, without `.lag`, and says where when it does not.
#[test]
fn per_voice_mod_errors_say_where() {
    let head = "track kit drums\ntrack lead synth Juno106 JunoPad\n\
        track buzzer synth Modular ModularBasic\nclip b = kit\n  bd x\n";
    for (line, col, msg) in [
        (
            "mod strip1.level = [0.5, 1]",
            20,
            "env and lists give each voice its own value: they modulate a track",
        ),
        (
            "mod lead.pan = env(perc)",
            16,
            "per voice: cutoff, resonance, vco1level, vco2level, vco3level, noiselevel, ringlevel, sublevel, wt1pos or wt2pos",
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
            // The Modular doesn't read cutoff at all (#673).
            "mod buzzer.cutoff = [300, 600]",
            12,
            "the Modular reads its knobs, ctl1 to ctl32, and its envelope",
        ),
    ] {
        let err = Song::parse(&format!("{head}{line}")).expect_err(line);
        assert_eq!((err.line, err.col, &*err.msg), (6, col, msg), "{line}");
    }
    // On the track's voice parameters it parses, and so does a method.
    let ok = format!(
        "{head}clip r = lead .resonance(lfo([1, 3]).range(0, 0.6))\n  \"c3\"\n\
        mod lead.cutoff = env(perc).exprange(200, 4000)\nmod lead.vco1level = [1, 0.5]\n"
    );
    let song = Song::parse(&ok).expect("parses");
    assert!(song.mods.iter().all(|m| m.signal.per_voice()));
    assert_eq!(Song::parse(&song.print()), Ok(song));
}

/// #204: parameter methods on a clip's line parse with it, belong to its
/// track, and print after the rest of the line.
#[test]
fn clip_methods_parse_and_print() {
    let text = "track kit drums\ntrack lead synth\ntrack pad synth\n\
        clip b = kit /16 .Level(0.5)\n  bd x...\n\
        clip r = lead live  .cutoff(sine.slow(4).range(300,3000)) .resonance( 0.7 )   # acid\n  arp([c4,e4,g4],up,8)\n\
        clip p = pad voicing .pan(lfo(0.25).range(-1, 1)) .cutoff( \"<300  800>\" )\n  \"[c3,e3,g3] [f3,a3,c4]\"\n\
        mod lead.cutoff = 900\n";
    let s = Song::parse(text).expect("parses");
    let scoped: Vec<_> = s.mods.iter().map(|m| (m.target, m.param, m.clip)).collect();
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
    assert!(s.clips[1].live && s.clips[2].voicing.is_some());
    let printed = s.print();
    for line in [
        "clip b = kit /16 .level(0.5)\n",
        "clip r = lead live .cutoff(sine.slow(4).range(300, 3000)) .resonance(0.7) # acid\n",
        "clip p = pad voicing .pan(lfo(0.25).range(-1, 1)) .cutoff(\"<300 800>\")\n",
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
        clip a = bass .lpf(sine.range(300, 3000)) .lpq(0.6) .attack(0.01) .room(0.3)\n  \"c2 ~ c2 ~\"\n\
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
    let e = Song::parse("track bass synth\nclip a = bass .distort(0.5)\n  \"c2\"\n")
        .expect_err("no alias");
    assert_eq!(e.msg, "no parameter has this name");
}

/// #660: the echo and the reverb in real units, Strudel's names and ours,
/// as clip methods writing the master's knobs and on the master line.
#[test]
fn effects_are_named_in_real_units() {
    let text = "master: EchoTime 0.25, ReverbSize 2, ReverbCutoff 4000\n\
        track kit drums\n\
        clip a = kit .delay(0.3) .delaytime(0.125) .delayfeedback(0.5) .roomsize(3)\n  bd x...\n\
        mod master.roomdim = 2000\n";
    let s = Song::parse(text).expect("parses");
    let master = s
        .mix
        .iter()
        .find(|m| m.at == Mix::Master)
        .expect("a master line");
    let knob = |p: Param| master.sets.iter().find(|(q, _)| *q == p).map(|(_, v)| *v);
    let close = |a: Option<f32>, b: f32| a.is_some_and(|a| (a - b).abs() < 1e-4);
    assert!(close(knob(Param::P1A), 250.0_f32.ln() / 2000.0_f32.ln()));
    assert!(close(knob(Param::P2A), 20.0_f32.ln() / 100.0_f32.ln()));
    assert!(close(knob(Param::P2B), (0.2_f32).ln() / 0.05_f32.ln()));
    let mods: Vec<_> = s
        .mods
        .iter()
        .map(|m| (m.target, m.param, m.alias))
        .collect();
    assert_eq!(
        mods,
        vec![
            (Target::Track(0), Param::Send1, Some("delay")),
            (Target::Master, Param::P1A, Some("delaytime")),
            (Target::Master, Param::P1B, Some("delayfeedback")),
            (Target::Master, Param::P2A, Some("roomsize")),
            (Target::Master, Param::P2B, Some("roomdim")),
        ]
    );
    let printed = s.print();
    assert!(
        printed.contains("master: EchoTime 0.25, ReverbSize 2, ReverbCutoff 4000"),
        "{printed}"
    );
    assert!(
        printed.contains(".delay(0.3) .delaytime(0.125) .delayfeedback(0.5) .roomsize(3)"),
        "{printed}"
    );
    assert!(printed.contains("mod master.roomdim = 2000"), "{printed}");
    assert_eq!(Song::parse(&printed), Ok(s));
    for u in UNITS {
        assert!(((u.knob)((u.value)(0.4)) - 0.4).abs() < 1e-5, "{}", u.name);
        assert!(Param::by_name(u.name).is_none(), "{}", u.name);
    }
}

/// #660: `room` is the reverb on P2 and `delay` the echo on P1; on a slot
/// holding another effect, or as a lane's knob values, they are refused.
#[test]
fn room_and_delay_need_their_effect() {
    for (text, line, col, msg) in [
        (
            "master: P2Type Chorus\ntrack kit drums\nclip a = kit .room(0.4)\n  bd x\n",
            3,
            15,
            "this is P2's reverb, and P2Type is not Reverb",
        ),
        (
            "track kit drums\nclip a = kit .delaytime(0.2)\n  bd x\nmaster: P1Type Reverb\n",
            2,
            15,
            "this is P1's echo, and P1Type is not Echo",
        ),
        (
            "master: P1Type Chorus, EchoTime 0.2\n",
            1,
            24,
            "this is P1's echo, and P1Type is not Echo",
        ),
        (
            "master: P2Type Echo\nmod master.roomsize = 2\n",
            2,
            12,
            "this is P2's reverb, and P2Type is not Reverb",
        ),
        (
            "snapshot s: master.EchoTime 0.2\n",
            1,
            20,
            "a value in seconds or Hz goes on the master line, a mod or a clip method",
        ),
    ] {
        let e = Song::parse(text).expect_err(text);
        assert_eq!((e.line, e.col, e.msg.as_ref()), (line, col, msg), "{text}");
    }
    // A slot set to the effect it needs is fine.
    assert!(
        Song::parse(
            "master: P2Type Reverb, ReverbSize 1\ntrack kit drums\nclip a = kit .room(1)\n  bd x\n"
        )
        .is_ok()
    );
}

#[test]
fn clip_method_errors_say_where() {
    let head = "track kit drums\n";
    for (line, col, msg) in [
        ("clip b = kit .nope(1)", 15, "no parameter has this name"),
        (
            "clip b = kit .mastergain(1)",
            15,
            "this parameter does not belong to this target",
        ),
        (
            "clip b = kit .model(1)",
            15,
            "the model and the routing can't be automated",
        ),
        (
            "clip b = kit .cutoff",
            21,
            "( and a value go here, e.g. .cutoff(800)",
        ),
        (
            "clip b = kit .(1)",
            15,
            "a parameter name goes here, e.g. .cutoff(800)",
        ),
        ("clip b = kit .cutoff(sine", 21, "this ( is not closed"),
        (
            "clip b = kit .cutoff(sine.wobble(1))",
            27,
            "no such method: range exprange slow fast segment lag onNote",
        ),
        (
            "clip b = kit .cutoff()",
            22,
            "a signal goes here, e.g. sine.range(300, 3000)",
        ),
        (
            "clip b = kit .cutoff(1) x",
            25,
            "a parameter method goes here, e.g. .cutoff(800)",
        ),
        (
            "clip b = kit .cutoff(1) .Cutoff(2)",
            26,
            "this parameter already has a method",
        ),
    ] {
        let err = Song::parse(&format!("{head}{line}\n  bd x")).expect_err(line);
        assert_eq!((err.line, err.col, &*err.msg), (2, col, msg), "{line}");
    }
}

/// ADR-0019, #215: pattern methods on a clip's line transform its notes when
/// the song loads, and print before its parameter methods.
#[test]
fn pattern_methods_transform_a_clips_notes() {
    let text = "track lead synth\n\
        clip r = lead .cutoff(900) .fast(2) .every(2, rev) .off( 0.125 ,add(12))\n  \"c4 d4 e4 f4\"\n";
    let s = Song::parse(text).expect("parses");
    let f = &s.clips[0];
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
        printed.contains("clip r = lead .fast(2) .every(2, rev) .off(1/8, add(12)) .cutoff(900)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

/// #629: a value a bar, parsed from a song, kept as typed and printed back.
#[test]
fn a_value_a_bar_parses_and_prints_back() {
    let text = "scale c minor\ntrack lead synth\n\
        clip r = lead .fast(\"<1 2>\") .add(\"<0 <1 -1>>\")\n  \"c4 [eb4,g4]\"\n\
        clip w = lead live .add(\"<0 12>\")\n  walk(c4,8,1)\n";
    let s = Song::parse(text).expect("parses");
    let n = s.clips[0].notes.as_ref().expect("notes");
    assert_eq!(n.bars, 4);
    let lows: Vec<u8> = (0..4)
        .map(|b| {
            n.events
                .iter()
                .filter(|e| e.start / 48 == b)
                .map(|e| e.note)
                .min()
                .unwrap_or(0)
        })
        .collect();
    assert_eq!(lows, vec![60, 61, 60, 59]);
    let printed = s.print();
    assert!(
        printed.contains("clip r = lead .fast(\"<1 2>\") .add(\"<0 <1 -1>>\")\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
    let err = Song::parse(
        "scale c minor\ntrack lead synth\nclip w = lead live .fast(\"<1 2>\")\n  walk(c4,8,1)\n",
    );
    assert_eq!(
        err.map_err(|e| e.msg),
        Err("a live clip takes only pitch methods: scale, chord, oct, add(n) and sub(n)".into())
    );
}

#[test]
fn add_of_a_list_makes_each_note_a_chord() {
    // #650: one copy of each note per offset, and it prints back.
    let text =
        "track lead synth\nclip r = lead .add([0,4,7]) .every(2, add([0,12]))\n  \"c4 d4\"\n";
    let s = Song::parse(text).expect("parses");
    let n = s.clips[0].notes.as_ref().expect("notes");
    let at = |t: u32| {
        n.events
            .iter()
            .filter(|e| e.start == t)
            .map(|e| e.note)
            .collect::<Vec<_>>()
    };
    assert_eq!(at(0), [60, 64, 67, 72, 76, 79]);
    assert_eq!(at(24), [62, 66, 69, 74, 78, 81]);
    assert_eq!(at(48), [60, 64, 67]);
    let printed = s.print();
    assert!(
        printed.contains("clip r = lead .add([0,4,7]) .every(2, add([0,12]))\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn pattern_method_errors_say_where() {
    for (text, line, col, msg) in [
        (
            "track kit drums\nclip b = kit .fast(2)\n  bd x",
            2,
            15,
            "pattern methods are for a clip of notes",
        ),
        (
            "track l synth\nclip w = l live .rev()\n  walk(c4,8,1)",
            2,
            18,
            "a live clip takes only pitch methods: scale, chord, oct, add(n) and sub(n)",
        ),
        (
            "track l synth\nclip w = l live .add([0,7])\n  walk(c4,8,1)",
            2,
            18,
            "a live clip takes only pitch methods: scale, chord, oct, add(n) and sub(n)",
        ),
        (
            "track l synth\nclip r = l .fast(0)\n  \"c4\"",
            2,
            13,
            "this takes a whole number from 1 to 16",
        ),
        (
            "track l synth\nclip r = l .every(4, cutoff)\n  \"c4\"",
            2,
            13,
            "a pattern method goes here, e.g. rev or fast(2)",
        ),
        (
            "track l synth\nclip r = l .slow(16) .slow(8)\n  \"c4\"",
            3,
            3,
            "the pattern runs past 64 bars",
        ),
    ] {
        let err = Song::parse(text).expect_err(text);
        assert_eq!((err.line, err.col, &*err.msg), (line, col, msg), "{text}");
    }
    // No pattern method is also a parameter's name.
    for name in Pattern::NAMES {
        assert_eq!(Param::by_name(name), None, "{name}");
    }
}

/// A note edit from the grid can't bake a pattern into the line.
#[test]
fn a_patterned_clip_refuses_a_note_edit() {
    let mut s = Song::parse("track l synth\nclip r = l .rev()\n  \"c4 d4\"\n").expect("parses");
    let before = s.clone();
    assert!(!s.edit_note(0, notes::Edit::Add { tick: 6, note: 64 }));
    assert_eq!(s, before);
}

/// #215: struct, sometimes and scale on a clip line print back as written.
#[test]
fn struct_sometimes_and_scale_print_back() {
    let text = "track lead synth\n\
        clip r = lead .struct(\"x ~ x x\") .sometimes(add(12)) .scale(eb minor)\n  \"c4 e4 g4 b4\"\n";
    let s = Song::parse(text).expect("parses");
    assert_eq!(s.clips[0].pattern.len(), 3);
    let notes = s.clips[0].notes.as_ref().expect("notes");
    assert!(
        notes
            .events
            .iter()
            .all(|e| [3, 5, 6, 8, 10, 11, 1].contains(&(e.note % 12)))
    );
    let printed = s.print();
    assert!(
        printed
            .contains("clip r = lead .struct(\"x ~ x x\") .sometimes(add(12)) .scale(d# minor)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn euclid_on_a_clip_is_its_struct_and_prints_back() {
    let euclid = Song::parse("track bass synth\nclip p = bass .euclid(5,8)\n  \"c2 a2 eb2\"\n")
        .expect("parses");
    let spelled = Song::parse(
        "track bass synth\nclip p = bass .struct(\"x ~ x x ~ x x ~\")\n  \"c2 a2 eb2\"\n",
    )
    .expect("parses");
    assert_eq!(euclid.clips[0].notes, spelled.clips[0].notes);
    let printed = euclid.print();
    assert!(
        printed.contains("clip p = bass .euclid(5,8)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(euclid));
}

/// The notes of the clip named `name`, with their starts.
fn notes_of(s: &Song, name: &str) -> Vec<(u32, u8)> {
    s.clips
        .iter()
        .find(|c| c.name == name)
        .and_then(|c| c.notes.as_ref())
        .map(|n| n.events.iter().map(|e| (e.start, e.note)).collect())
        .unwrap_or_default()
}

#[test]
fn a_clip_takes_its_roots_from_an_earlier_clip() {
    // The pedal plays a1 four bars, then d1 four: as <a1 d1>/4 (ADR-0035).
    let s = Song::parse(
        "track bass synth\ntrack lead synth\n\
         clip pedal = bass .slow(4)\n  \"<a1 d1>\"\n\
         clip arp = lead .fast(8) .scale(pedal minor-pentatonic)\n  \"<0 2 1 3 2>\"\n\
         clip same = lead .fast(8) .scale(<a1 d1>/4 minor-pentatonic)\n  \"<0 2 1 3 2>\"\n",
    )
    .expect("parses");
    assert_eq!(notes_of(&s, "arp"), notes_of(&s, "same"));
    let printed = s.print();
    assert!(
        printed.contains("clip arp = lead .fast(8) .scale(pedal minor-pentatonic)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn a_follower_stays_in_phase_with_a_source_of_another_length() {
    // Three bars of roots under two bars of degrees: six bars, each note on
    // the root of the bar it falls in, counted from the song's first bar.
    let s = Song::parse(
        "track bass synth\ntrack lead synth\n\
         clip roots = bass\n  \"<a1 c2 e2>\"\n\
         clip f = lead .scale(roots major)\n  \"<0 0>\"\n",
    )
    .expect("parses");
    let bars: Vec<u8> = notes_of(&s, "f").iter().map(|(_, n)| *n).collect();
    assert_eq!(bars, vec![33, 36, 40, 33, 36, 40]);
    // Before the source's first note in its loop, its last chord sounds.
    let s = Song::parse(
        "track bass synth\ntrack lead synth\n\
         clip late = bass\n  \"~ e2\"\n\
         clip f = lead .scale(late minor)\n  \"0 2\"\n",
    )
    .expect("parses");
    assert_eq!(notes_of(&s, "f"), vec![(0, 40), (24, 43)]);
}

#[test]
fn oct_moves_whole_octaves_after_the_degrees_are_notes() {
    let s = Song::parse(
        "track bass synth\ntrack lead synth\n\
         clip pedal = bass\n  \"a1\"\n\
         clip up = lead .scale(pedal minor) .oct(2)\n  \"0 2\"\n",
    )
    .expect("parses");
    assert_eq!(notes_of(&s, "up"), vec![(0, 57), (24, 60)]);
    assert_eq!(
        Song::parse("track lead synth\nclip x = lead .oct(1) .scale(a1 minor)\n  \"0\"\n")
            .map(|_| ())
            .map_err(|e| e.msg),
        Err("oct moves notes: write it after scale".into())
    );
}

#[test]
fn a_clip_read_for_roots_comes_before_and_has_notes() {
    for (text, line, msg) in [
        (
            "track lead synth\nclip f = lead .scale(later minor)\n  \"0\"\nclip later = lead\n  \"a1\"\n",
            2,
            "a root is a letter a to g, maybe # or b, maybe an octave, or an earlier clip",
        ),
        (
            "track lead synth\nclip p = lead\n  \"a1\"\nclip f = lead .scale(p)\n  \"0\"\n",
            4,
            crate::usage::SCALE,
        ),
        (
            "track lead synth\nclip p = lead\n  \"~\"\nclip f = lead .scale(p minor)\n  \"0\"\n",
            4,
            "this clip has no notes to take roots from",
        ),
    ] {
        let e = Song::parse(text).expect_err(msg);
        assert_eq!((&*e.msg, e.line), (msg, line), "{text}");
    }
}

#[test]
fn degrees_count_the_notes_of_an_earlier_clips_chord() {
    let harm = "track pad synth\ntrack lead synth\n\
                clip harm = pad\n  \"<[a2,c3,e3,g3] [d3,f3,a3,c4]>\"\n";
    let s = Song::parse(&format!(
        "{harm}clip f = lead .chord(harm)\n  \"0 1 2 3\"\n"
    ))
    .expect("parses");
    let notes: Vec<u8> = notes_of(&s, "f").iter().map(|(_, n)| *n).collect();
    assert_eq!(
        notes,
        vec![45, 48, 52, 55, 50, 53, 57, 60],
        "Am7 then Dm7, a bar each"
    );
    // Past the top an octave up, below the root an octave down; [0,2] both.
    let s = Song::parse(&format!(
        "{harm}clip f = lead .chord(harm)\n  \"4 -1 [0,2]\"\n"
    ))
    .expect("parses");
    assert_eq!(
        notes_of(&s, "f")[..4],
        [(0, 57), (16, 43), (32, 45), (32, 52)]
    );
    // A note moves to the nearest chord note: b2 to c3 over Am7.
    let s = Song::parse(&format!("{harm}clip f = lead .chord(harm)\n  \"b2\"\n")).expect("parses");
    assert_eq!(notes_of(&s, "f")[0], (0, 48));
    // With a rhythm, and printed back.
    let s = Song::parse(&format!(
        "{harm}clip f = lead .euclid(5,8) .chord(harm)\n  \"0 2 1\"\n"
    ))
    .expect("parses");
    let printed = s.print();
    assert!(
        printed.contains("clip f = lead .euclid(5,8) .chord(harm)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
    let e =
        Song::parse(&format!("{harm}clip f = lead .chord(nope)\n  \"0\"\n")).expect_err("no clip");
    assert_eq!(e.msg, crate::usage::CHORD);
}

#[test]
fn a_live_clip_takes_pitch_methods_and_prints_back() {
    let text = "scale c minor\ntrack pad synth\ntrack lead synth\n\
                clip harm = pad\n  \"<[a2,c3,e3] [d3,f3,a3]>\"\n\
                clip sand = lead live .scale(harm minor) .oct(1)\n  walk(c4,8,1)\n";
    let s = Song::parse(text).expect("parses");
    assert_eq!(s.clips[1].pattern.len(), 2);
    // The bar kept for a source is on harm's first root, an octave up.
    let a_minor = [9, 11, 0, 2, 4, 5, 7];
    assert!(
        notes_of(&s, "sand")
            .iter()
            .all(|(_, n)| a_minor.contains(&(n % 12)))
    );
    let printed = s.print();
    assert!(
        printed.contains("clip sand = lead live .scale(harm minor) .oct(1)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

/// The notes of clip `k`, in order.
fn clip_notes(s: &Song, k: usize) -> Vec<u8> {
    let n = s
        .clips
        .get(k)
        .and_then(|c| c.notes.as_ref())
        .expect("notes");
    n.events.iter().map(|e| e.note).collect()
}

/// #570: Strudel's `n("<0 2 1 3 2>*8").scale("A1:minor:pentatonic")`.
#[test]
fn degrees_play_the_clips_scale_from_its_root() {
    let text = "track bass synth
\
        clip arp = bass .fast(8) .scale(a1 minor-pentatonic)\n  \"<0 2 1 3 2>\"\n";
    let s = Song::parse(text).expect("parses");
    let n = s.clips[0].notes.as_ref().expect("notes");
    assert_eq!(n.bars, 5);
    assert_eq!(n.text, "\"<0 2 1 3 2>\"", "the line stays as written");
    let notes = clip_notes(&s, 0);
    // a1 d2 c2 e2 d2, round and round, eight to a bar
    assert_eq!(
        notes.get(..10),
        Some(&[33, 38, 36, 40, 38, 33, 38, 36, 40, 38][..])
    );
    assert!(n.events.iter().all(|e| !e.degree));
    let printed = s.print();
    assert!(
        printed.contains("clip arp = bass .fast(8) .scale(a1 minor-pentatonic)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

/// #576: Strudel's scale names, its colon form, and a near miss named.
#[test]
fn strudels_scales_by_any_name_and_with_colons() {
    let s = Song::parse("scale c:minor:pentatonic\ntrack l synth\nclip r = l\n  \"0 1 2\"\n")
        .expect("parses");
    assert_eq!(clip_notes(&s, 0), [48, 51, 53]);
    assert!(s.print().contains("scale c minor-pentatonic\n"));
    // an alias prints by the mode's first name
    let s = Song::parse("scale d aeolian\n").expect("parses");
    assert!(s.print().contains("scale d minor\n"));
    let s =
        Song::parse("track l synth\nclip r = l .scale(<a1:whole-tone d2:hirajoshi>)\n  \"0 1\"\n")
            .expect("parses");
    // a1 b1 on a whole-tone scale, then d2 e2 on hirajoshi (0 2 3 7 8)
    assert_eq!(clip_notes(&s, 0), [33, 35, 38, 40]);
    for (text, col, msg) in [
        (
            "scale c minor-pentatonik\n",
            9,
            "no scale has this mode; did you mean minor-pentatonic?",
        ),
        (
            "scale c:hirajosi\n",
            9,
            "no scale has this mode; did you mean hirajoshi?",
        ),
        ("scale c funky\n", 9, "no scale has this mode"),
    ] {
        let err = Song::parse(text).expect_err(text);
        assert_eq!((err.line, err.col, &*err.msg), (1, col, msg), "{text}");
    }
}

/// #571: Strudel's `n("<0 2 1 3 2>*8").scale("<A1 D2>/4:minor:pentatonic")`,
/// the degrees running on unbroken across the change of root.
#[test]
fn a_scale_progression_plays_the_degrees_on_each_root() {
    let text = "track bass synth\n\
        clip arp = bass .fast(8) .scale(<a1 d2>/4 minor-pentatonic)\n  \"<0 2 1 3 2>\"\n";
    let s = Song::parse(text).expect("parses");
    let n = s.clips[0].notes.as_ref().expect("notes");
    assert_eq!((n.bars, n.events.len()), (40, 320));
    let notes = clip_notes(&s, 0);
    // bar 1 on a1: degrees 0 2 1 3 2 0 2 1
    assert_eq!(notes.get(..8), Some(&[33, 38, 36, 40, 38, 33, 38, 36][..]));
    // bar 5 on d2 (d f g a c), the cycle 32 steps in: degrees 1 3 2 0 2 1 3 2
    assert_eq!(
        notes.get(32..40),
        Some(&[41, 45, 43, 38, 43, 41, 45, 43][..])
    );
    let printed = s.print();
    assert!(
        printed.contains("clip arp = bass .fast(8) .scale(<a1 d2>/4 minor-pentatonic)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn degrees_follow_the_song_scale_and_add_moves_them_before_scale() {
    let at = |text: &str| clip_notes(&Song::parse(text).expect(text), 0);
    // no .scale: the song's, from its root in octave 3 (a3 is 57)
    assert_eq!(
        at("scale a minor-pentatonic\ntrack l synth\nclip r = l\n  \"0 1 -1\"\n"),
        [57, 60, 55]
    );
    // a root without an octave is in octave 3 too
    assert_eq!(
        at("track l synth\nclip r = l .scale(a minor)\n  \"0 2\"\n"),
        [57, 60]
    );
    // before .scale, add moves degrees; after it, semitones
    assert_eq!(
        at("track l synth\nclip r = l .add(1) .scale(a1 minor-pentatonic)\n  \"0\"\n"),
        [36]
    );
    assert_eq!(
        at("track l synth\nclip r = l .scale(a1 minor-pentatonic) .add(1)\n  \"0\"\n"),
        [34]
    );
    // .scale still snaps notes, and a line may mix them
    assert_eq!(
        at("track l synth\nclip r = l .scale(c2 major)\n  \"c#4 0\"\n"),
        [60, 36]
    );
}

#[test]
fn a_degree_without_a_scale_is_an_error() {
    for (text, line, col, msg) in [
        (
            "track l synth\nclip r = l\n  \"0 2\"\n",
            3,
            3,
            "a degree needs a scale: a scale line, or .scale(a1 minor) on the clip",
        ),
        (
            "track l synth\nclip r = l .every(2, scale(a minor))\n  \"0 2\"\n",
            3,
            3,
            "a degree needs a scale: a scale line, or .scale(a1 minor) on the clip",
        ),
        (
            "track l synth\nclip r = l voicing .scale(a minor)\n  \"[0,2,4]\"\n",
            3,
            3,
            "voicing takes notes, not degrees",
        ),
    ] {
        let err = Song::parse(text).expect_err(text);
        assert_eq!((err.line, err.col, &*err.msg), (line, col, msg), "{text}");
    }
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
clip r = lead
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
            "a lane goes under a clip",
        ),
        (
            "track l synth Modular nope",
            1,
            23,
            "a preset is a factory preset, as MiniBass",
        ),
    ] {
        let err = Song::parse(text).expect_err(text);
        assert_eq!((err.line, err.col, &*err.msg), (line, col, msg), "{text}");
    }
}

/// #674: the setting a Modular track plays names its knobs, as its SynthDef's
/// arguments; on the track a name reaches its knob wherever a parameter goes.
const VIBRA: &str = r#"setting vibra = Modular ModularBasic
  SynthDef(\vibra, { |freq = 220, gate = 1, vib = 4, vibmod = 6, cutoff = 800|
      var f = freq * (SinOsc.kr(vib) * vibmod).midiratio;
      RLPF.ar(LFTri.ar(f), cutoff, 0.5) * EnvGen.kr(Env.adsr(0.01, 0.1, 0.8, 0.1), gate) * 0.4
  }).add;
track lead synth vibra
track bass synth Minimoog
"#;

#[test]
fn a_modular_knob_goes_by_its_name() {
    let s = Song::parse(VIBRA).expect("parses");
    assert_eq!(s.settings[0].knobs[..3], ["vib", "vibmod", "cutoff"]);
    let text = format!(
        "{VIBRA}clip a = lead .vib(4) .VibMod(\"<.25 .5 1 2 12>\") .cutoff(1200)\n  \"a3 e3\"\n\
         clip b = bass .cutoff(900)\n  \"a1\"\n\
         auto wob = lead.vib ramp 2 8 /4\n\
         snapshot calm: lead.vibmod 0.5\n\
         mod lead.cutoff = lfo(1).range(400, 2000)\n"
    );
    let s = Song::parse(&text).expect("parses");
    let methods: Vec<_> = s.mods.iter().map(|m| (m.target, m.param)).collect();
    assert_eq!(
        methods,
        [
            (Target::Track(0), Param::Ctl1),
            (Target::Track(0), Param::Ctl2),
            // A knob's name wins over the synth's own parameter.
            (Target::Track(0), Param::Ctl3),
            // Not on a track playing another synth.
            (Target::Track(1), Param::Cutoff),
            (Target::Track(0), Param::Ctl3),
        ]
    );
    assert_eq!(
        (s.autos[0].target, s.autos[0].param),
        (Target::Track(0), Param::Ctl1)
    );
    assert_eq!(s.snapshots[0].sets, [(Target::Track(0), Param::Ctl2, 0.5)]);
    let printed = s.print();
    for want in [
        "clip a = lead .vib(4) .vibmod(\"<0.25 0.5 1 2 12>\") .cutoff(1200)",
        "clip b = bass .cutoff(900)",
        "auto wob = lead.vib ramp 2 8 /4",
        "snapshot calm: lead.vibmod 0.5",
        "mod lead.cutoff = ",
    ] {
        assert!(printed.contains(want), "{want} in\n{printed}");
    }
    assert_eq!(Song::parse(&printed), Ok(s));
    // CtlN still reaches a knob, and prints by the knob's name.
    let by_number = Song::parse(&format!(
        "{VIBRA}clip a = lead .ctl1(4) .ctl2(6)\n  \"a3\"\n"
    ))
    .expect("parses");
    assert_eq!(
        by_number.mods.iter().map(|m| m.param).collect::<Vec<_>>(),
        [Param::Ctl1, Param::Ctl2]
    );
    assert!(by_number.print().contains(".vib(4) .vibmod(6)"));
}

/// #674: a name the setting has no knob for says which it has; a UGen's
/// number has no name.
#[test]
fn a_modular_knob_name_it_lacks_says_its_knobs() {
    let knobs = "vibra's knobs are vib, vibmod, cutoff";
    for (tail, col, msg) in [
        ("clip a = lead .vibe(4)\n  \"a3\"", 16, knobs),
        ("mod lead.depth = lfo(1)", 5, knobs),
        ("auto w = lead.rq ramp 0 1 /4", 10, knobs),
        ("snapshot s: lead.wobble 1", 13, knobs),
        // Another synth's track keeps the old message.
        (
            "clip b = bass .vibe(4)\n  \"a1\"",
            16,
            "no parameter has this name",
        ),
    ] {
        let err = Song::parse(&format!("{VIBRA}{tail}\n")).expect_err(tail);
        assert_eq!((err.line, err.col, &*err.msg), (8, col, msg), "{tail}");
    }
}

/// #353: a ghost note `o` parses, prints back and plays at its velocity.
#[test]
fn a_ghost_note_parses_and_prints_back() {
    let text = "tempo 120\ntrack kit drums\n\nclip a = kit /16\n  sn o.x.X.o.\n";
    let song = Song::parse(text).expect("parses");
    let steps = &song.clips[0].lanes[0].steps;
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
        "track lead synth\nclip a = lead .cutoff(\"~ 800\")\n  \"c3 e3\"\nmod lead.resonance = \"<~ 0.7>\"\n",
    );
    let song = ok.expect("one value");
    assert_eq!(Song::parse(&song.print()), Ok(song));
}

/// #242: a digit after a hit, accent or ghost ratchets it; the lane counts
/// steps, not digits, and prints back as written. A ratchet goes only on
/// x, X or o, and is 2, 3 or 4.
#[test]
fn ratchets_parse_print_back_and_say_where_they_are_wrong() {
    let text = "tempo 120\ntrack kit drums\n\nclip a = kit /16\n  sn x3.X2.o4.x...\n";
    let song = Song::parse(text).expect("parses");
    let lane = &song.clips[0].lanes[0];
    assert_eq!(lane.steps.len(), 10);
    let reps: Vec<u8> = (0..10).map(|n| lane.ratchet(n)).collect();
    assert_eq!(reps, [3, 1, 2, 1, 4, 1, 1, 1, 1, 1]);
    assert!(
        song.print().contains("  sn x3.X2.o4.x...\n"),
        "{}",
        song.print()
    );
    assert_eq!(Song::parse(&song.print()).expect("prints back"), song);
    let plain = Song::parse("track kit drums\nclip a = kit\n  sn x.x.\n").expect("parses");
    assert!(
        plain.clips[0].lanes[0].ratchets.is_empty(),
        "no ratchet, nothing kept"
    );
    let head = "track kit drums\nclip a = kit\n";
    for (lane, col, msg) in [
        ("  sn 3x", 6, "a ratchet follows a step: x3"),
        ("  sn f2", 7, "only x, X and o take a ratchet"),
        ("  sn .2", 7, "only x, X and o take a ratchet"),
        ("  sn x1", 7, "a ratchet is 2, 3 or 4"),
        ("  sn x5", 7, "a ratchet is 2, 3 or 4"),
        ("  sn x33", 8, "a ratchet is 2, 3 or 4"),
    ] {
        let err = Song::parse(&format!("{head}{lane}\n")).expect_err(lane);
        assert_eq!((err.line, err.col, &*err.msg), (3, col, msg), "{lane}");
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

/// The text `span` (UTF-16 units) points at in `text`.
fn at16(text: &str, (start, len): (u32, u32)) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    String::from_utf16_lossy(
        units
            .get(start as usize..(start + len) as usize)
            .unwrap_or(&[]),
    )
}

/// #205: each clip's spans point at its words in the printed text: a
/// lane's steps (a ratchet with its digit, a call whole), and the words of a
/// note line, whatever its form, past comments and non-ASCII text.
#[test]
fn spans_point_at_the_words_of_the_printed_text() {
    let text = "# naïve — a beat\ntempo 120\ntrack kit drums\ntrack lead synth\n\
        clip b = kit /16\n  # the kick\n  bd x3..X...\n  cl euclid(3,8)\n\
        clip m = lead\n  \"c4 [e4 g4] ~ <c5 [d5,f5]>\"\n\
        clip c = lead\n  c4:4 r:8 [e4,g4]:8.\n\
        clip t = lead\n  d5@0:6:90 a4@12:24\n\
        clip g = lead\n  arp([c4,e4,g4],up,16)\n";
    let song = Song::parse(text).expect("parses");
    let printed = song.print();
    let s = spans::spans(&song, &printed).clips;
    let words =
        |f: usize| -> Vec<String> { s[f].notes.iter().map(|sp| at16(&printed, *sp)).collect() };
    let bd: Vec<String> = s[0].lanes[0].iter().map(|sp| at16(&printed, *sp)).collect();
    assert_eq!(bd, ["x3", ".", ".", "X", ".", ".", "."]);
    assert!(
        s[0].lanes[1]
            .iter()
            .all(|sp| at16(&printed, *sp) == "euclid(3,8)")
    );
    assert_eq!(words(1), ["c4", "e4", "g4", "~", "c5", "[d5,f5]"]);
    assert_eq!(words(2), ["c4", "r", "[e4,g4]"]);
    assert_eq!(words(3), ["d5@0:6:90", "a4@12:24"]);
    assert_eq!(words(4), ["arp([c4,e4,g4],up,16)"]);
    // Every note event names the word it came from, through a pattern method.
    let rev =
        Song::parse("track lead synth\nclip m = lead .rev() .fast(2)\n  \"c4 [e4 g4] <c5 d5>\"\n")
            .expect("parses");
    let p = rev.print();
    let sp = spans::spans(&rev, &p).clips;
    let n = rev.clips[0].notes.as_ref().expect("notes");
    for e in &n.events {
        let word = at16(&p, sp[0].notes[usize::from(e.word)]);
        assert_eq!(word, crate::notes::note_name(e.note), "{e:?}");
    }
}

/// #214: a `samples` line names the pack or kit a sampler or drums track
/// wants; it prints after the tracks, keeps its comment, follows a track
/// taken out, and says where it is wrong.
#[test]
fn samples_lines_parse_print_and_say_where_they_are_wrong() {
    let text = "track keys sampler Sampler SamplerKeys\ntrack kit drums PadSampler\n# the piano\nsamples keys upright-piano-kw\nsamples kit audiophob\n";
    let song = Song::parse(text).expect("parses");
    assert_eq!(
        song.samples,
        [
            (0, "upright-piano-kw".to_string()),
            (1, "audiophob".to_string())
        ]
    );
    let printed = song.print();
    assert!(
        printed.contains("# the piano\nsamples keys upright-piano-kw\nsamples kit audiophob\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(song.clone()));
    // A track taken out takes its line; the next one moves down.
    let mut gone = song.clone();
    assert!(gone.remove_track(0));
    assert_eq!(gone.samples, [(0, "audiophob".to_string())]);
    // Set from the panel: replaced, or added; never on a synth track.
    let mut set = song;
    assert!(set.set_samples(0, "sweep-pad"));
    assert_eq!(set.samples[0], (0, "sweep-pad".to_string()));
    assert!(!set.set_samples(0, "Bad Id"));
    let head = "track keys sampler\ntrack lead synth\n";
    for (line, col, msg) in [
        ("samples", 8, "a track goes here, then a pack or kit id"),
        ("samples nobody x", 9, "no track has this name"),
        (
            "samples lead x",
            9,
            "samples go on a sampler or drums track",
        ),
        (
            "samples keys",
            13,
            "a pack or kit id goes here, as upright-piano-kw",
        ),
        (
            "samples keys Piano",
            14,
            "an id is lower-case letters, digits and -, as upright-piano-kw",
        ),
        ("samples keys a b", 16, "unexpected text"),
        (
            "samples keys a\nsamples keys b",
            9,
            "this track already has its samples",
        ),
    ] {
        let err = Song::parse(&format!("{head}{line}\n")).expect_err(line);
        assert_eq!((err.col, &*err.msg), (col, msg), "{line}");
    }
}

/// #522: a sampler track's zones are lines of their own, by a dropped
/// file's hash or a pack's sample; they print back with their comments, take
/// the place of a `samples` line, follow a track taken out and say where they
/// are wrong.
#[test]
fn zone_lines_parse_print_and_say_where_they_are_wrong() {
    let hash = "@0123456789abcdef0123456789abcdef";
    let text = format!(
        "track keys sampler Sampler SamplerKeys\ntrack pno sampler Sampler SamplerKeys\n# low\nzone keys 1 = {hash} : keyhi b2, root c2, loop sustain\nzone keys 3 = upright-piano-kw/C4vH : keylo c3, vello 40, tune -5, level 0.8, seqlen 2, seqpos 2, release 1\nzone pno 1 = {hash}\n"
    );
    let song = Song::parse(&text).expect("parses");
    assert_eq!(song.zones.len(), 3);
    let z = &song.zones[0];
    assert_eq!((z.track, z.zone, z.source.as_str()), (0, 0, hash));
    assert_eq!((z.fields.key_hi, z.fields.root), (47, Some(36)));
    let printed = song.print();
    assert!(
        printed.contains(&format!(
            "# low\nzone keys 1 = {hash} : keyhi b2, root c2, loop sustain\n"
        )),
        "{printed}"
    );
    assert!(printed.contains("zone pno 1 = @"), "{printed}");
    assert_eq!(Song::parse(&printed), Ok(song.clone()));
    // A track taken out takes its zones; the next one moves down.
    let mut gone = song.clone();
    assert!(gone.remove_track(0));
    assert_eq!(gone.zones.len(), 1);
    assert_eq!(gone.zones[0].track, 0);
    // A pack or its zones, not both: each setter takes the other away.
    let mut set = song.clone();
    assert!(set.set_samples(0, "sweep-pad"));
    assert_eq!(set.zones.len(), 1);
    let zones = song.zones[..2].to_vec();
    assert!(set.set_zones(0, zones));
    assert!(set.samples.is_empty() && set.zones.len() == 3);
    assert_eq!(Song::parse(&set.print()), Ok(set.clone()));
    assert!(set.set_zones(0, Vec::new()));
    assert_eq!(set.zones.len(), 1);
    let head = "track keys sampler\ntrack lead synth\ntrack kit drums\n";
    for (line, col, msg) in [
        ("zone", 5, "a track goes here, then a zone number, 1 to 64"),
        ("zone nobody 1 = a/b", 6, "no track has this name"),
        ("zone lead 1 = a/b", 6, "zones go on a sampler track"),
        ("zone kit 1 = a/b", 6, "zones go on a sampler track"),
        ("zone keys 65 = a/b", 11, "a zone number is 1 to 64"),
        ("zone keys 1 a/b", 13, "= and a source go here"),
        (
            "zone keys 1 = synth:LaChoir:c4",
            15,
            "a zone's source is a pack's sample, as upright-piano-kw/C4vH, or @ and a file's hash",
        ),
        ("zone keys 1 = a/b x", 19, ": and the zone's fields go here"),
        (
            "zone keys 1 = a/b : wobble 1",
            21,
            "a zone field: keylo keyhi vello velhi root tune level loop loopstart loopend seqlen seqpos release",
        ),
        (
            "zone keys 1 = a/b : keylo h9",
            27,
            "a key is a note, as c4, or 0 to 127",
        ),
        ("zone keys 1 = a/b : keylo", 26, "a value goes here"),
        (
            "zone keys 1 = a/b\nzone keys 1 = a/c",
            11,
            "this track already has this zone",
        ),
        (
            "samples keys piano\nzone keys 1 = a/b",
            6,
            "this track has a samples line: a track has a pack or its zones, not both",
        ),
        (
            "zone keys 1 = a/b\nsamples keys piano",
            9,
            "this track has zone lines: a track has a pack or its zones, not both",
        ),
    ] {
        let err = Song::parse(&format!("{head}{line}\n")).expect_err(line);
        assert_eq!((err.col, &*err.msg), (col, msg), "{line}");
    }
}

/// #527: a pad sampler's pads are lines of their own, as zones are: on a
/// drums or sampler track, by a hash or a pack's sample, in place of a
/// `samples` line, following a track taken out.
#[test]
fn pad_lines_parse_print_and_say_where_they_are_wrong() {
    let hash = "@0123456789abcdef0123456789abcdef";
    let text = format!(
        "track kit drums PadSampler\ntrack keys sampler PadSampler\n# kick\npad kit 1 = {hash} : tune -2, choke 1\npad kit 16 = audiophob/snare : level 1.2, pan -0.5, decay 0.3, vellevel 0.5, velstart 0.25, oneshot 0, out 3\npad keys 2 = {hash}\n"
    );
    let song = Song::parse(&text).expect("parses");
    assert_eq!(song.pads.len(), 3);
    let p = &song.pads[1];
    assert_eq!(
        (p.track, p.pad, p.source.as_str()),
        (0, 15, "audiophob/snare")
    );
    assert_eq!((p.fields.out, p.fields.one_shot), (3, false));
    let printed = song.print();
    assert!(
        printed.contains(&format!("# kick\npad kit 1 = {hash} : tune -2, choke 1\n")),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(song.clone()));
    let mut gone = song.clone();
    assert!(gone.remove_track(0));
    assert_eq!((gone.pads.len(), gone.pads[0].track), (1, 0));
    // A kit or its pads, not both: each setter takes the other away.
    let mut set = song.clone();
    assert!(set.set_samples(0, "audiophob"));
    assert_eq!(set.pads.len(), 1);
    assert!(set.set_pads(0, song.pads[..2].to_vec()));
    assert!(set.samples.is_empty() && set.pads.len() == 3);
    assert_eq!(Song::parse(&set.print()), Ok(set.clone()));
    assert!(set.set_pads(0, Vec::new()));
    assert_eq!(set.pads.len(), 1);
    let head = "track kit drums\ntrack lead synth\n";
    for (line, col, msg) in [
        ("pad", 4, "a track goes here, then a pad number, 1 to 16"),
        (
            "pad lead 1 = a/b",
            5,
            "pads go on a drums or sampler track, or a Nes synth",
        ),
        ("pad kit 17 = a/b", 9, "a pad number is 1 to 16"),
        (
            "pad kit 1 = plaits:0.1:0.2:0.3",
            13,
            "a pad's source is a pack's sample, as audiophob/kick, or @ and a file's hash",
        ),
        (
            "pad kit 1 = a/b : wah 1",
            19,
            "a pad field: tune level pan decay choke vellevel velstart oneshot out",
        ),
        ("pad kit 1 = a/b : choke 9", 25, "a choke group is 0 to 8"),
        (
            "pad kit 1 = a/b\npad kit 1 = a/c",
            9,
            "this track already has this pad",
        ),
        (
            "samples kit audiophob\npad kit 1 = a/b",
            5,
            "this track has a samples line: a track has a kit or its pads, not both",
        ),
        (
            "pad kit 1 = a/b\nsamples kit audiophob",
            9,
            "this track has pad lines: a track has a kit or its pads, not both",
        ),
    ] {
        let err = Song::parse(&format!("{head}{line}\n")).expect_err(line);
        assert_eq!((err.col, &*err.msg), (col, msg), "{line}");
    }
}

/// #214: a group bus is named in the song's `group` line, set or taken away
/// from the view; a line with neither name nor values goes.
#[test]
fn a_group_is_named_in_its_line() {
    let mut song = Song::parse("track kit drums\ngroup 2: Level 0.8\n").expect("parses");
    assert!(song.set_group_name(0, Some("drum_bus")));
    assert!(song.set_group_name(1, Some("keys")));
    let printed = song.print();
    assert!(
        printed.contains("group 1 drum_bus\n"),
        "a name alone: {printed}"
    );
    assert!(printed.contains("group 2 keys: Level 0.8"), "{printed}");
    assert_eq!(Song::parse(&printed), Ok(song.clone()));
    assert!(song.set_group_name(0, None));
    assert!(
        !song.mix.iter().any(|m| m.at == Mix::Group(0)),
        "no name, no values: gone"
    );
    assert!(song.set_group_name(1, None));
    assert!(
        song.mix.iter().any(|m| m.at == Mix::Group(1)),
        "its values stay"
    );
    assert!(!song.set_group_name(8, Some("x")) && !song.set_group_name(0, Some("drum bus")));
}

/// ADR-0033: recipe lines parse, print back as they read, and keep one per slot.
#[test]
fn recipes_parse_and_print_back() {
    let text = "table 1 = upright-piano-kw/C4vH : inharmonic 1.6, noise 1.5\nattack 1 = @0123456789abcdef0123456789abcdef\ntable 2 = strings/A3 : window 2048\n";
    let s = Song::parse(text).expect("parses");
    assert_eq!(s.recipes.len(), 3);
    assert_eq!(s.recipes[0].edits[crate::analysis::lab::INHARMONIC], 1.6);
    let printed = s.print();
    assert!(
        printed.contains("table 1 = upright-piano-kw/C4vH : inharmonic 1.6, noise 1.5"),
        "{printed}"
    );
    assert!(printed.contains("attack 1 = @0123456789abcdef0123456789abcdef"));
    assert!(printed.contains("table 2 = strings/A3 : window 2048"));
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn bad_recipes_say_where() {
    for (text, col) in [
        ("table 9 = a/b", 7),
        ("table 1 = piano", 11),
        ("table 1 = a/b : wobble 2", 17),
        ("table 1 = a/b : window 3000", 17),
        ("table 1 = a/b : inharmonic x", 28),
        ("table 1 a/b", 9),
    ] {
        let e = Song::parse(text).expect_err(text);
        assert_eq!((e.line, e.col), (1, col), "{text}: {}", e.msg);
    }
    let twice = Song::parse("table 1 = a/b\ntable 1 = a/c").expect_err("twice");
    assert_eq!(twice.line, 2);
    // A table and an attack may share a number.
    assert!(Song::parse("table 1 = a/b\nattack 1 = a/b").is_ok());
}

/// #575: Strudel's `n("0 .. 12").scale("C:chromatic")`, the scale on top.
#[test]
fn a_chromatic_run_from_the_scale_line_and_its_octave() {
    let s = Song::parse("scale c chromatic\ntrack lead synth\nclip run = lead\n  \"0 .. 12\"\n")
        .expect("parses");
    assert_eq!(
        clip_notes(&s, 0),
        (48..=60).collect::<Vec<u8>>(),
        "c3 to c4"
    );
    let s = Song::parse("scale c1 chromatic\ntrack lead synth\nclip run = lead\n  \"12 .. 0\"\n")
        .expect("parses");
    assert_eq!(
        clip_notes(&s, 0),
        (24..=36).rev().collect::<Vec<u8>>(),
        "c2 down to c1"
    );
    let printed = s.print();
    assert!(printed.contains("scale c1 chromatic\n"), "{printed}");
    assert!(printed.contains("  \"12 .. 0\"\n"), "{printed}");
    assert_eq!(Song::parse(&printed), Ok(s));
    // the octave is for degrees only: a walk keeps its own start
    let s =
        Song::parse("scale a1 minor\ntrack l synth\nclip w = l\n  walk(a4,4,1)\n").expect("parses");
    assert!(clip_notes(&s, 0).iter().all(|n| *n >= 69));
    // .scale on the clip wins over the line's octave
    let s = Song::parse("scale c1 major\ntrack l synth\nclip r = l .scale(c4 major)\n  \"0\"\n")
        .expect("parses");
    assert_eq!(clip_notes(&s, 0), [60]);
    assert_eq!(
        Song::parse("scale h1 major\n").map(|_| ()),
        Err(SongError {
            line: 1,
            col: 7,
            msg: "a root is a letter a to g, maybe # or b, maybe an octave".into(),
        })
    );
}

/// A clip's events as (start, len, note).
fn timing_of(s: &Song, name: &str) -> Vec<(u32, u32, u8)> {
    s.clips
        .iter()
        .find(|c| c.name == name)
        .and_then(|c| c.notes.as_ref())
        .map(|n| n.events.iter().map(|e| (e.start, e.len, e.note)).collect())
        .unwrap_or_default()
}

#[test]
fn struct_takes_the_rhythm_of_a_drum_clip_or_a_note_clip() {
    let s = Song::parse(
        "track kit drums\ntrack bass synth\n\
         clip beat = kit /16\n  bd x..x..x...x.x...\n  ch ..........x.....\n\
         clip riff = bass\n  \"c4 ~ e4 g4\"\n\
         clip a = bass .struct(beat)\n  \"c2 e2\"\n\
         clip b = bass .struct(\"x ~ ~ x ~ ~ x ~ ~ ~ x ~ x ~ ~ ~\")\n  \"c2 e2\"\n\
         clip c = bass .struct(riff)\n  \"<a1 d2>\"\n\
         clip d = bass .struct(\"x ~ x x\")\n  \"<a1 d2>\"\n",
    )
    .expect("parses");
    assert_eq!(timing_of(&s, "a"), timing_of(&s, "b"));
    assert_eq!(timing_of(&s, "a").len(), 5, "two pads on one step hit once");
    assert_eq!(timing_of(&s, "c"), timing_of(&s, "d"));
    let printed = s.print();
    assert!(
        printed.contains("clip a = bass .struct(beat)\n"),
        "{printed}"
    );
    assert!(
        printed.contains("clip c = bass .struct(riff)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

#[test]
fn struct_stays_in_phase_with_a_source_of_another_length() {
    // A lane of 12 sixteenths comes round every 3/4 bar: three bars to a
    // loop; under two bars of notes, six bars, each hit at its song tick.
    let s = Song::parse(
        "track kit drums\ntrack bass synth\n\
         clip beat = kit /16\n  bd x.......x...\n\
         clip f = bass .struct(beat)\n  \"<c2 d2>\"\n",
    )
    .expect("parses");
    let starts: Vec<(u32, u8)> = notes_of(&s, "f");
    let ticks = [
        0, 24, 36, 60, 72, 96, 108, 132, 144, 168, 180, 204, 216, 240, 252, 276,
    ];
    let want: Vec<(u32, u8)> = ticks
        .iter()
        .map(|t| (*t, if (t / 48) % 2 == 0 { 36 } else { 38 }))
        .collect();
    assert_eq!(starts, want);
    assert_eq!(
        s.clips
            .iter()
            .find(|c| c.name == "f")
            .and_then(|c| c.notes.as_ref())
            .map(|n| n.bars),
        Some(6)
    );
}

#[test]
fn a_clip_read_for_a_rhythm_comes_before_and_has_hits() {
    let bad = crate::usage::STRUCT;
    for (text, line, col, msg) in [
        (
            "track lead synth\nclip f = lead .struct(nope)\n  \"c4\"\n",
            2,
            16,
            bad,
        ),
        (
            "track lead synth\nclip f = lead .struct(later)\n  \"c4\"\nclip later = lead\n  \"a1\"\n",
            2,
            16,
            bad,
        ),
        (
            "track lead synth\nclip p = lead\n  \"~\"\nclip f = lead .struct(p)\n  \"c4\"\n",
            4,
            16,
            "this clip has no hits to take a rhythm from",
        ),
        (
            "track kit drums\ntrack lead synth\nclip p = kit\n  bd x...\nclip f = lead .scale(p minor)\n  \"0\"\n",
            5,
            16,
            "a drum clip has no notes to take roots from",
        ),
        (
            "track kit drums\ntrack lead synth\nclip p = kit\n  bd x...\nclip f = lead .chord(p)\n  \"0\"\n",
            5,
            16,
            "a drum clip has no notes to take chords from",
        ),
    ] {
        let e = Song::parse(text).expect_err(msg);
        assert_eq!((&*e.msg, e.line, e.col), (msg, line, col), "{text}");
    }
}

/// #649: `.jux(rev)` as two tracks panned apart, the right one made from
/// the left through `.rev()`.
#[test]
fn a_clip_is_made_from_an_earlier_clip_and_methods() {
    let text = "track left synth\ntrack right synth\n\
        clip fwd = left\n  \"c4 d4 e4 f4\"\nclip back = right fwd .rev() .add(12)\n";
    let s = Song::parse(text).expect("parses");
    let back = &s.clips[1];
    assert_eq!((back.from.as_deref(), back.track), (Some("fwd"), 1));
    let n = back.notes.as_ref().expect("notes");
    let notes: Vec<(u32, u8)> = n.events.iter().map(|e| (e.start, e.note)).collect();
    assert_eq!(notes, [(0, 77), (12, 76), (24, 74), (36, 72)]);
    let printed = s.print();
    assert!(
        printed.contains("clip back = right fwd .rev() .add(12)\n\n")
            || printed.ends_with("clip back = right fwd .rev() .add(12)\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

/// #649: the source is the earlier clip after its own methods; a
/// clip made from a clip refuses edits, and follows its source's.
#[test]
fn a_clip_made_from_a_clip_follows_its_source() {
    let text = "track l synth\nclip a = l .fast(2)\n  \"c4 d4\"\nclip b = l a\n";
    let mut s = Song::parse(text).expect("parses");
    let events = |s: &Song, f: usize| s.clips[f].notes.as_ref().expect("notes").events.len();
    assert_eq!(events(&s, 1), 4);
    let before = s.clone();
    assert!(!s.edit_note(1, notes::Edit::Add { tick: 6, note: 64 }));
    assert_eq!(s, before);
    let mut s =
        Song::parse("track l synth\nclip a = l\n  \"c4 d4\"\nclip b = l a\n").expect("parses");
    assert!(s.edit_note(0, notes::Edit::Add { tick: 6, note: 64 }));
    assert_eq!(events(&s, 1), 3);
    assert_eq!(Song::parse(&s.print()), Ok(s));
}

#[test]
fn a_clip_made_from_a_clip_names_one_of_notes_before_it() {
    for (text, line, col, msg) in [
        (
            "track l synth\nclip b = l nope\n",
            2,
            12,
            "no clip with this name comes before this one",
        ),
        (
            "track l synth\nclip b = l b\n",
            2,
            12,
            "no clip with this name comes before this one",
        ),
        (
            "track l synth\nclip b = l a\nclip a = l\n  \"c4\"\n",
            2,
            12,
            "no clip with this name comes before this one",
        ),
        (
            "track kit drums\ntrack l synth\nclip a = kit\n  bd x...\nclip b = l a\n",
            5,
            12,
            "a clip of notes is made from a clip of notes",
        ),
        (
            "track kit drums\ntrack l synth\nclip a = l\n  \"c4\"\nclip b = kit a\n",
            5,
            14,
            "a drum clip is made from a drum clip",
        ),
        (
            "track kit drums\nclip a = kit\n  bd x...\nclip b = kit a .add(1)\n",
            4,
            17,
            "a drum clip takes methods of time, not pitch",
        ),
        (
            "track kit drums\nclip a = kit\n  bd x...\nclip b = kit a .every(2, strum(10))\n",
            4,
            17,
            "a drum clip takes methods of time, not pitch",
        ),
        (
            "track kit drums\nclip a = kit\n  bd x...\nclip b = kit a .fast(3)\n",
            4,
            14,
            "a method moved a hit off the clip's step grid",
        ),
        (
            "track kit drums\nclip a = kit\n  bd x...\nclip b = kit a .slow(8)\n",
            4,
            14,
            "a lane has at most 64 steps",
        ),
        (
            "track kit drums\nclip a = kit\n  bd x...\nclip b = kit a\n  sn x...\n",
            5,
            3,
            "a clip made from a clip has no line of its own",
        ),
        (
            "track kit drums\nclip b = kit .rev()\n  bd x...\n",
            2,
            15,
            "pattern methods are for a clip of notes",
        ),
        (
            "track l synth\nclip a = l\n  \"c4\"\nclip b = l a\n  \"d4\"\n",
            5,
            3,
            "a clip made from a clip has no line of its own",
        ),
        (
            "track l synth\nclip a = l\n  \"c4\"\nclip b = l a voicing\n",
            4,
            14,
            "a clip made from a clip is voiced as its source",
        ),
        (
            "track l synth\nclip a = l\n  \"c4\"\nclip b = l a x\n",
            4,
            14,
            "unexpected text",
        ),
    ] {
        let e = Song::parse(text).expect_err(msg);
        assert_eq!((&*e.msg, e.line, e.col), (msg, line, col), "{text}");
    }
}

/// #653: a drum clip made from an earlier one, its steps through methods
/// of time, back on its grid with each step's kind and ratchet.
#[test]
fn a_drum_clip_is_made_from_an_earlier_drum_clip() {
    let text = "track kit drums\ntrack kat drums\nclip beat = kit /16\n  \
        bd x...o...X...x3...\n  sn ....f.......d...\nclip back = kat beat .rev()\n";
    let s = Song::parse(text).expect("parses");
    let back = &s.clips[1];
    assert_eq!((back.track, back.grid, back.lanes.len()), (1, 16, 2));
    let steps = |l: &Lane| {
        (0..l.steps.len())
            .map(|n| l.step_text(n))
            .collect::<String>()
    };
    // Each step's span reversed: the step at 12 plays at 3, 15 - 12.
    assert_eq!(steps(&back.lanes[0]), "...x3...X...o...x");
    assert_eq!(steps(&back.lanes[1]), "...d.......f....");
    let printed = s.print();
    assert!(
        printed.contains("clip back = kat beat .rev()\n"),
        "{printed}"
    );
    assert_eq!(Song::parse(&printed), Ok(s));
}

/// #653: on the grid, methods of time; two hits on one step: the
/// stronger plays. A sampler clip of lanes too.
#[test]
fn a_drum_clip_from_a_clip_keeps_to_its_grid() {
    let s = Song::parse(
        "track kit drums\nclip a = kit /16\n  bd x.......X.......\n\
         clip b = kit a .fast(2) .every(2, rev)\nclip c = kit a .off(1/2, ply(1))\n",
    )
    .expect("parses");
    let steps = |f: usize| {
        let l = &s.clips[f].lanes[0];
        (0..l.steps.len())
            .map(|n| l.step_text(n))
            .collect::<String>()
    };
    // Twice as fast, then the first of each two bars backwards.
    assert_eq!(steps(1), "...X...x...X...xx...X...x...X...");
    // x at 0 and X at 8, each copied half a bar on: X over x at 0.
    assert_eq!(steps(2), "X.......X.......");
    let sampler = Song::parse("track s sampler\nclip a = s\n  bd x...\nclip b = s a .rev()\n")
        .expect("parses");
    assert_eq!(sampler.clips[1].lanes.len(), 1);
}

/// #653: a step edit of the source moves the clip made from it; the clip
/// takes none of its own.
#[test]
fn a_drum_clip_from_a_clip_follows_its_source() {
    let mut s = Song::parse("track kit drums\nclip a = kit\n  bd x...\nclip b = kit a .rev()\n")
        .expect("parses");
    let before = s.clone();
    assert!(!s.set_step(1, 0, 1, Step::Hit));
    assert!(!s.set_ratchet(1, 0, 3, 2));
    assert_eq!(s, before);
    assert!(s.set_step(0, 0, 1, Step::Accent));
    let l = &s.clips[1].lanes[0];
    assert_eq!(
        &l.steps[..4],
        [Step::Off, Step::Off, Step::Accent, Step::Hit]
    );
    assert!(s.set_ratchet(0, 0, 0, 2));
    assert_eq!(s.clips[1].lanes[0].ratchet(3), 2);
    assert_eq!(Song::parse(&s.print()), Ok(s));
}

/// #652: a General MIDI preset loads its pack with no `samples` line; a
/// written line or zone lines take its place, and a new preset its pack.
#[test]
fn a_general_midi_preset_names_its_pack() {
    let ids = |s: &Song| {
        s.sample_ids()
            .map(|(t, id)| (t, id.to_string()))
            .collect::<Vec<_>>()
    };
    let text = "track keys sampler GeneralMidi GmMarimba\ntrack bass sampler GeneralMidi\n";
    let mut s = Song::parse(text).expect("parses");
    assert_eq!(
        s.tracks[1].preset.and_then(Preset::pack),
        Some("gm-piano"),
        "the first program"
    );
    assert_eq!(ids(&s), [(0, "gm-marimba".into()), (1, "gm-piano".into())]);
    assert!(s.samples.is_empty());
    assert!(!s.print().contains("samples"), "{}", s.print());
    assert!(s.set_track_preset(0, Preset::GmFlute));
    assert_eq!(ids(&s)[0], (0, "gm-flute".into()));
    let written = Song::parse(&format!("{text}samples keys upright-piano-kw\n")).expect("parses");
    assert_eq!(
        ids(&written),
        [(0, "upright-piano-kw".into()), (1, "gm-piano".into())]
    );
    let zoned =
        Song::parse(&format!("{text}zone keys 1 = gm-marimba/gm-marimba-01\n")).expect("parses");
    assert_eq!(ids(&zoned), [(1, "gm-piano".into())]);
    let e = Song::parse("track lead synth GeneralMidi GmPiano\n").expect_err("not a synth");
    assert_eq!((e.line, e.col), (1, 18), "{e:?}");
}

/// The steps of each lane of clip `f`, as the grid writes them.
fn lane_text(s: &Song, f: usize) -> Vec<String> {
    s.clips[f]
        .lanes
        .iter()
        .map(|l| {
            let steps: String = (0..l.steps.len()).map(|n| l.step_text(n)).collect();
            format!("{} {steps}", l.pad.name())
        })
        .collect()
}

#[test]
fn a_beat_line_makes_the_lanes_a_grid_would() {
    // #659: the issue's example, against its grid.
    let beat =
        Song::parse("track left drums\nclip fwd = left\n  \"bd sn [ch oh] ch\"\n").expect("parses");
    let grid = Song::parse(
        "track left drums\nclip fwd = left /16\n  bd x...............\n  sn ....x...........\n  \
         ch ........x...x...\n  oh ..........x.....\n",
    )
    .expect("parses");
    assert_eq!(beat.clips[0].grid, 16);
    assert_eq!(beat.clips[0].lanes, grid.clips[0].lanes);
    // Prints as written, without the grid it picked.
    let text = beat.print();
    assert!(
        text.contains("clip fwd = left\n  \"bd sn [ch oh] ch\"\n"),
        "{text}"
    );
    assert_eq!(Song::parse(&text), Ok(beat));
}

#[test]
fn a_beat_line_picks_its_grid() {
    let s = Song::parse("track d drums\nclip a = d\n  \"bd*3 sn\"\n").expect("parses");
    assert_eq!(s.clips[0].grid, 12);
    assert_eq!(lane_text(&s, 0), ["bd x.x.x.......", "sn ......x....."]);
    // A straight beat stays on /16, a written grid is kept.
    let s = Song::parse("track d drums\nclip a = d\n  \"bd sn\"\n").expect("parses");
    assert_eq!(s.clips[0].grid, 16);
    let s = Song::parse("track d drums\nclip a = d /32\n  \"bd sn\"\n").expect("parses");
    assert_eq!(s.clips[0].grid, 32);
    assert_eq!(s.clips[0].lanes[0].steps.len(), 32);
    assert!(s.print().contains("clip a = d /32\n  \"bd sn\"\n"));
    // Alternation runs over bars; an accent is an accent step.
    let s = Song::parse("track d drums\nclip a = d\n  \"<bd sn> ch!\"\n").expect("parses");
    assert_eq!(
        lane_text(&s, 0),
        [
            "bd x...............................",
            "ch ........X...............X.......",
            "sn ................x...............",
        ]
    );
}

#[test]
fn a_beat_line_reads_strudels_names_and_pad_numbers() {
    let s = Song::parse("track d drums\nclip a = d\n  \"sd hh rim [bd,cb]\"\n").expect("parses");
    let pads: Vec<&str> = s.clips[0].lanes.iter().map(|l| l.pad.name()).collect();
    assert_eq!(pads, ["sn", "ch", "rs", "bd", "cb"]);
    // Numbers in the pad range, on a Pad Sampler, as `n("0 1 [2 3] 2")`.
    let s =
        Song::parse("track d drums PadSampler\nclip a = d\n  \"0 1 [2 5] p7\"\n").expect("parses");
    let pads: Vec<LanePad> = s.clips[0].lanes.iter().map(|l| l.pad).collect();
    assert_eq!(
        pads,
        [
            LanePad::Named(Pad::Bd),
            LanePad::Named(Pad::Rs),
            LanePad::Named(Pad::Sn),
            LanePad::Number(5),
            LanePad::Number(7),
        ]
    );
}

#[test]
fn a_beat_line_says_where_it_is_wrong() {
    let cases: [(&str, usize, usize, &str); 9] = [
        (
            "track d drums\nclip a = d\n  \"bd zz\"",
            3,
            7,
            "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr rd, or p0 to p15",
        ),
        (
            "track d drums PadSampler\nclip a = d\n  \"bd 16\"",
            3,
            7,
            "a pad's number is 0 to 15",
        ),
        (
            "track d drums Tr808\nclip a = d\n  \"bd 5\"",
            3,
            7,
            "a pad by number is for a Pad Sampler",
        ),
        (
            "track d drums\nclip a = d\n  \"bd sn*5\"",
            3,
            7,
            "this hit falls on no step: a grid is /12, /16, /24, /32 or /48",
        ),
        (
            "track d drums\nclip a = d /16\n  \"bd*3 sn\"",
            3,
            4,
            "this hit falls off the clip's step grid",
        ),
        (
            "track d drums\nclip a = d\n  \"bd? sn\"",
            3,
            6,
            "a beat line takes no ?, & or ^: write those as lanes",
        ),
        (
            "track d drums\nclip a = d\n  bd x...\n  \"sn\"",
            4,
            3,
            "a drum clip is lanes or one beat line",
        ),
        (
            "track d drums\nclip a = d\n  \"sn\"\n  bd x...",
            4,
            3,
            "a drum clip is lanes or one beat line",
        ),
        (
            "track d drums\nclip a = d\n  \"~ ~\"",
            3,
            3,
            "a beat line needs a hit",
        ),
    ];
    for (text, line, col, msg) in cases {
        assert_eq!(
            Song::parse(text).map(|_| ()),
            Err(SongError {
                line,
                col,
                msg: msg.into()
            }),
            "{text}"
        );
    }
}

#[test]
fn a_step_edit_turns_a_beat_line_into_lanes() {
    let mut s = Song::parse("track d drums\nclip a = d\n  \"bd sn\"\nclip b = d a .rev()\n")
        .expect("parses");
    // A clip made from it follows it (#653).
    assert_eq!(s.clips[1].lanes.len(), 2);
    assert_eq!(
        s.clips[1].lanes[0].steps[15],
        Step::Hit,
        "the kick, turned round"
    );
    assert!(s.set_step(0, 1, 2, Step::Hit));
    assert!(s.clips[0].beat.is_none());
    let text = s.print();
    assert!(
        text.contains("clip a = d /16\n  bd x...............\n  sn ..x.....x......."),
        "{text}"
    );
    assert_eq!(Song::parse(&text), Ok(s));
}

/// ADR-0038: on a clip, the four envelope times are held per note; a `mod`
/// line's are not. Written as numbers, a time is 0 to 10 seconds and a
/// sustain 0 to 1, checked where the signal is written.
#[test]
fn envelope_times_on_a_clip_are_held_per_note() {
    let text = "track pad synth Juno106\n\
        clip r = pad .attack(\"0.1 0\") .dec(0.2) .sustain(\"<1 0.5>\") .rel(rand.range(0.1, 2)) .cutoff(\"400 900\")\n  \"c2 [d#3,g3]\"\n\
        mod pad.attack = 0.3\n";
    let s = Song::parse(text).expect("parses");
    let held: Vec<(Param, bool)> = s.mods.iter().map(|m| (m.param, m.on_note())).collect();
    assert_eq!(
        held,
        [
            (Param::AdsrAttack, true),
            (Param::AdsrDecay, true),
            (Param::AdsrSustain, true),
            (Param::AdsrRelease, true),
            (Param::Cutoff, false),
            (Param::AdsrAttack, false),
        ]
    );
    assert_eq!(Song::parse(&s.print()), Ok(s));
    for (methods, col, msg) in [
        (".attack(12)", 22, "an envelope time is 0 to 10 seconds"),
        (
            ".release(\"0.1 -1\")",
            23,
            "an envelope time is 0 to 10 seconds",
        ),
        (".sus(\"1 1.5\")", 19, "a sustain is 0 to 1"),
    ] {
        let text = format!("track pad synth Juno106\nclip r = pad {methods}\n  \"c2\"\n");
        let err = Song::parse(&text).expect_err(methods);
        assert_eq!((err.line, err.col, &*err.msg), (2, col, msg), "{methods}");
    }
}

/// #677: `.adsr("a:d:s:r")` is Strudel's envelope in one method, a parameter
/// method per part; fewer parts or an empty one leave the rest alone, and it
/// prints back as one method.
#[test]
fn adsr_is_the_envelope_in_one_method() {
    let parts = |methods: &str| {
        let text = format!("track pad synth Juno106\nclip r = pad {methods}\n  \"c2 [d#3,g3]\"\n");
        let s = Song::parse(&text).unwrap_or_else(|e| panic!("{methods}: {e:?}"));
        assert_eq!(Song::parse(&s.print()), Ok(s.clone()), "{methods}");
        let printed = s.print();
        let line = printed
            .lines()
            .find(|l| l.starts_with("clip r"))
            .unwrap_or("")
            .to_string();
        let held: Vec<(Param, Option<&str>)> = s.mods.iter().map(|m| (m.param, m.alias)).collect();
        (held, line)
    };
    let a = Some("adsr");
    let (held, line) = parts(".adsr(\"[.1 0]:.2:[1 0]\") .room(0.5)");
    assert_eq!(
        held,
        [
            (Param::AdsrAttack, a),
            (Param::AdsrDecay, a),
            (Param::AdsrSustain, a),
            (Param::Send2, Some("room"))
        ]
    );
    assert_eq!(line.matches(".adsr(").count(), 1, "{line}");
    assert!(line.ends_with(" .room(0.5)"), "{line}");
    let (held, _) = parts(".adsr(\"<.01 .2>:.1:.5:.3\")");
    assert_eq!(held.iter().map(|h| h.0).collect::<Vec<_>>(), NOTE_TIMES);
    let (held, line) = parts(".adsr(\".1::.5\")");
    assert_eq!(
        held.iter().map(|h| h.0).collect::<Vec<_>>(),
        [Param::AdsrAttack, Param::AdsrSustain]
    );
    assert!(line.contains(".adsr(\"0.1::0.5\")"), "{line}");
    for (methods, col, msg) in [
        (
            ".adsr(\".1:.2:.5:.3:1\")",
            33,
            "adsr has four parts: attack:decay:sustain:release",
        ),
        (
            ".adsr(\".1:x\")",
            24,
            "a sequence holds numbers, ~, [ ] and < >, e.g. \"<300 [800 1200]>\"",
        ),
        (
            ".adsr(0.1)",
            20,
            "adsr takes \"attack:decay:sustain:release\"",
        ),
        (
            ".adsr(\"::\")",
            21,
            "an attack, a decay, a sustain or a release goes here",
        ),
        (
            ".adsr(\".1\") .attack(.2)",
            27,
            "this parameter already has a method",
        ),
        (".adsr(\".1:.2:2\")", 27, "a sustain is 0 to 1"),
    ] {
        let text = format!("track pad synth Juno106\nclip r = pad {methods}\n  \"c2\"\n");
        let err = Song::parse(&text).expect_err(methods);
        assert_eq!((err.line, err.col, &*err.msg), (2, col, msg), "{methods}");
    }
    // A model that doesn't read the ADSR (#673).
    let err =
        Song::parse("track e synth Dx7\nclip r = e .adsr(\".1:.2\")\n  \"c2\"\n").expect_err("dx7");
    assert_eq!((err.line, err.col), (2, 13));
}
