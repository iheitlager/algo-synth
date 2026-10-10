//! The song tools (#384): `check` parses a song with the engine's parser,
//! `render` plays it offline and measures it, `catalog` lists what the engine
//! offers. Each returns a plain value the server sends to a model as JSON.

use algo_dsp::algo::Mode;
use algo_dsp::drums::Pad;
use algo_dsp::engine::{BLOCK, Engine, SYNTHS};
use algo_dsp::fx::insert::InsertType;
use algo_dsp::fx::processor::ProcType;
use algo_dsp::modular::sc;
use algo_dsp::mono::model::Model;
use algo_dsp::mono::preset::Preset;
use algo_dsp::params::Param;
use algo_dsp::song::{Kind, Song, SongError};
use serde::Serialize;
use std::time::{Duration, Instant};

/// The sample rate the tools render at.
const SAMPLE_RATE: f32 = 48_000.0;
/// Bars rendered of a song without an arrangement, where every clip loops.
const FREE_BARS: u64 = 4;

/// Where a text stopped parsing.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ErrorAt {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

impl From<SongError> for ErrorAt {
    fn from(e: SongError) -> ErrorAt {
        ErrorAt {
            line: e.line,
            col: e.col,
            msg: e.msg.to_string(),
        }
    }
}

/// What `check` found: a song, in its canonical form, or the first error.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Checked {
    pub ok: bool,
    pub error: Option<ErrorAt>,
    /// The song as the engine prints it, when it parses.
    pub canonical: Option<String>,
    pub tracks: Vec<String>,
    pub clips: Vec<String>,
    pub scenes: Vec<String>,
    /// Bars in the arrangement; 0 without one.
    pub bars: u64,
}

/// The error the sclang reader gives for a character it does not take.
const NOT_SCLANG: &str = "this character is not part of sclang here";

/// `e`, naming the character when the sclang reader refused one (#430): the
/// message alone sends a model guessing.
fn named(text: &str, mut e: ErrorAt) -> ErrorAt {
    if e.msg == NOT_SCLANG
        && let Some(c) = text
            .lines()
            .nth(e.line.saturating_sub(1))
            .and_then(|l| l.chars().nth(e.col.saturating_sub(1)))
    {
        e.msg = format!("{NOT_SCLANG}: `{c}`");
    }
    e
}

/// Parse `text` with the engine's parser (ADR-0012: the parser is the check).
pub fn check(text: &str) -> Checked {
    match Song::parse(text) {
        Ok(s) => Checked {
            ok: true,
            error: None,
            canonical: Some(s.print()),
            tracks: s.tracks.iter().map(|t| t.name.clone()).collect(),
            clips: s.clips.iter().map(|f| f.name.clone()).collect(),
            scenes: s.scenes.iter().map(|x| x.name.clone()).collect(),
            bars: s.bars(),
        },
        Err(e) => Checked {
            ok: false,
            error: Some(named(text, e.into())),
            canonical: None,
            tracks: Vec::new(),
            clips: Vec::new(),
            scenes: Vec::new(),
            bars: 0,
        },
    }
}

/// How much `render` may play.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The most bars rendered.
    pub max_bars: u64,
    /// The most wall time spent rendering.
    pub max_time: Duration,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_bars: 64,
            max_time: Duration::from_secs(20),
        }
    }
}

/// The level of one arrangement entry, on the master output.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SceneLevel {
    pub name: String,
    /// The bar it starts on, from 1.
    pub bar: u64,
    pub bars: u64,
    pub rms: f32,
}

/// A track's level on its synth's strip, after the fader: the peak, and the
/// mean of the blocks' peaks while it sounds. Tracks on one synth share it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TrackLevel {
    pub name: String,
    pub synth: Option<usize>,
    pub peak: f32,
    pub level: f32,
}

/// A Modular setting's SynthDef as the engine builds it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SynthDefInfo {
    pub setting: String,
    pub ok: bool,
    pub error: Option<ErrorAt>,
    /// Voices its cost allows, and whether it plays in stereo.
    pub voices: usize,
    pub stereo: bool,
    pub knobs: usize,
}

/// What `render` measured.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Rendered {
    pub bars: u64,
    pub seconds: f32,
    /// Stopped at the wall-time limit before the bars were done.
    pub cut_short: bool,
    /// Samples that are NaN or infinite; any is a fault.
    pub nonfinite: usize,
    /// The highest sample, either side; above 1 clips.
    pub peak: f32,
    pub rms: f32,
    /// The side over the mid, in RMS: 0 is mono.
    pub width: f32,
    pub scenes: Vec<SceneLevel>,
    pub tracks: Vec<TrackLevel>,
    pub synthdefs: Vec<SynthDefInfo>,
}

/// Load `text` in an engine and play it offline: up to its arrangement's
/// bars (or `FREE_BARS` without one), within `limits`. A text that does not
/// load gives its error.
pub fn render(text: &str, limits: Limits) -> Result<Rendered, ErrorAt> {
    let song = Song::parse(text).map_err(|e| named(text, e.into()))?;
    let mut e = Engine::new(SAMPLE_RATE);
    let Some(buf) = e.song_buffer(text.len()) else {
        return Err(ErrorAt {
            line: 1,
            col: 1,
            msg: "the song is too long".into(),
        });
    };
    buf.copy_from_slice(text.as_bytes());
    e.load_song()?;
    e.song_play();

    let tempo = f64::from(song.tempo.max(1.0));
    let bar_frames = 4.0 * 60.0 / tempo * f64::from(SAMPLE_RATE);
    let full = if song.bars() > 0 {
        song.bars()
    } else {
        FREE_BARS
    };
    let bars = full.min(limits.max_bars);
    let blocks = (bars as f64 * bar_frames / BLOCK as f64).ceil() as usize;

    let routes: Vec<Option<usize>> = (0..song.tracks.len()).map(|t| e.song_routed(t)).collect();
    // Per synth: the peak, the sum of its blocks' peaks and how many sounded.
    let mut strips = vec![(0.0_f32, 0.0_f64, 0_usize); SYNTHS];
    // Per bar: the sum of squares on the master, and the samples in it.
    let mut per_bar = vec![(0.0_f64, 0_usize); bars as usize + 1];
    let (mut nonfinite, mut peak) = (0_usize, 0.0_f32);
    let (mut mid, mut side, mut frames) = (0.0_f64, 0.0_f64, 0_usize);
    let started = Instant::now();
    let mut cut_short = false;
    e.clear_meters();
    for block in 0..blocks {
        if started.elapsed() > limits.max_time {
            cut_short = true;
            break;
        }
        e.render(BLOCK);
        let out = e.output();
        let (left, right) = out.split_at(BLOCK);
        for (i, (l, r)) in left.iter().zip(right).enumerate() {
            if !l.is_finite() || !r.is_finite() {
                nonfinite += 1;
                continue;
            }
            peak = peak.max(l.abs()).max(r.abs());
            let (l, r) = (f64::from(*l), f64::from(*r));
            mid += ((l + r) * 0.5).powi(2);
            side += ((l - r) * 0.5).powi(2);
            let bar = ((block * BLOCK + i) as f64 / bar_frames) as usize;
            if let Some(b) = per_bar.get_mut(bar) {
                b.0 += l * l + r * r;
                b.1 += 2;
            }
        }
        frames += BLOCK;
        let meters = *e.meters();
        e.clear_meters();
        for (s, m) in strips.iter_mut().enumerate() {
            let p = meters.get(s).copied().unwrap_or(0.0);
            if p > 0.0 {
                m.0 = m.0.max(p);
                m.1 += f64::from(p);
                m.2 += 1;
            }
        }
    }

    let rms_of = |from: u64, n: u64| {
        let (sum, count) = per_bar
            .iter()
            .skip(from as usize)
            .take(n as usize)
            .fold((0.0, 0), |(s, c), b| (s + b.0, c + b.1));
        if count == 0 {
            0.0
        } else {
            (sum / count as f64).sqrt() as f32
        }
    };
    let mut scenes = Vec::new();
    let mut at = 0_u64;
    for &i in &song.arrange {
        let Some(sec) = song.scenes.get(i) else {
            continue;
        };
        let n = u64::from(sec.bars);
        if at >= bars {
            break;
        }
        scenes.push(SceneLevel {
            name: sec.name.clone(),
            bar: at + 1,
            bars: n.min(bars - at),
            rms: rms_of(at, n),
        });
        at += n;
    }
    let tracks = song
        .tracks
        .iter()
        .zip(&routes)
        .map(|(t, route)| {
            let m = route.and_then(|s| strips.get(s)).copied();
            let (peak, sum, n) = m.unwrap_or((0.0, 0.0, 0));
            TrackLevel {
                name: t.name.clone(),
                synth: *route,
                peak,
                level: if n == 0 { 0.0 } else { (sum / n as f64) as f32 },
            }
        })
        .collect();
    let synthdefs = song
        .settings
        .iter()
        .filter_map(|st| st.code.as_ref().map(|code| (st, code)))
        .map(|(st, code)| match sc::compile(code) {
            Ok(p) => SynthDefInfo {
                setting: st.name.clone(),
                ok: true,
                error: None,
                voices: p.program.voice_cap(),
                stereo: p.program.is_stereo(),
                knobs: p.knobs.len(),
            },
            Err(err) => SynthDefInfo {
                setting: st.name.clone(),
                ok: false,
                error: Some(ErrorAt {
                    line: err.line,
                    col: err.col,
                    msg: err.msg.to_string(),
                }),
                voices: 0,
                stereo: false,
                knobs: 0,
            },
        })
        .collect();
    let rendered_bars = ((frames as f64 / bar_frames).floor() as u64).min(bars);
    Ok(Rendered {
        bars: rendered_bars,
        seconds: frames as f32 / SAMPLE_RATE,
        cut_short,
        nonfinite,
        peak,
        rms: rms_of(0, bars),
        width: if mid > 0.0 {
            (side / mid).sqrt() as f32
        } else {
            0.0
        },
        scenes,
        tracks,
        synthdefs,
    })
}

/// Whether track `t` of `song` plays: not muted or out-soloed, and one of
/// its clips loops (no arrangement) or sits in an arranged scene.
pub fn plays(song: &Song, t: usize) -> bool {
    let Some(track) = song.tracks.get(t) else {
        return false;
    };
    let soloed = song.tracks.iter().any(|x| x.solo);
    if track.mute || (soloed && !track.solo) {
        return false;
    }
    let mine = |f: &usize| song.clips.get(*f).is_some_and(|x| x.track == t);
    if song.arrange.is_empty() {
        (0..song.clips.len()).any(|f| mine(&f))
    } else {
        song.arrange
            .iter()
            .filter_map(|s| song.scenes.get(*s))
            .any(|s| s.clips.iter().any(mine))
    }
}

/// `text` with a clip that plays `track`, when it has none and the song
/// has no arrangement (#430): an instrument is heard without the model
/// writing a test clip and taking it out again. Notes over two octaves
/// (a run, a held note, low repeats, a chord) or a beat on a drums track.
pub fn audition(text: &str, track: &str) -> Option<String> {
    let song = Song::parse(text).ok()?;
    let (t, kind) = song
        .tracks
        .iter()
        .enumerate()
        .find(|(_, x)| x.name == track)
        .map(|(t, x)| (t, x.kind))?;
    if !song.arrange.is_empty() || song.clips.iter().any(|f| f.track == t) {
        return None;
    }
    let name = (1..)
        .map(|i| {
            if i == 1 {
                "audition".to_string()
            } else {
                format!("audition{i}")
            }
        })
        .find(|n| song.clips.iter().all(|f| &f.name != n))?;
    let body = match kind {
        Kind::Drums => "  bd x...x...x...x...\n  sn ....x.......x...\n  ch x.x.x.x.x.x.x.x.\n",
        Kind::Synth | Kind::Sampler => "  \"<[c3 eb3 g3 c4] [c4@3 ~] c2*4 [g3,c4,eb4]>\"\n",
    };
    Some(format!(
        "{}\nclip {name} = {track}\n{body}",
        text.trim_end()
    ))
}

/// A model, how it plays and its presets.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModelInfo {
    pub name: String,
    /// The voice it plays with: Mono, Fm, La, Drums, Sampler, Pads or Graph.
    pub engine: String,
    pub voices: usize,
    pub presets: Vec<String>,
}

/// A parameter by its registry name (ADR-0004): its range, and whether it
/// belongs to a synth, a mixer strip or the whole engine.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ParamInfo {
    pub name: String,
    pub lo: f32,
    pub hi: f32,
    pub scope: &'static str,
}

/// A scale mode and its steps from the root.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModeInfo {
    pub name: String,
    pub steps: Vec<u8>,
}

/// What the engine offers a song, read from the engine.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Catalog {
    pub models: Vec<ModelInfo>,
    pub pads: Vec<String>,
    pub params: Vec<ParamInfo>,
    pub modes: Vec<ModeInfo>,
    pub inserts: Vec<String>,
    pub processors: Vec<String>,
}

/// Everything a song can name: generated from the engine, so it never
/// drifts from it.
pub fn catalog() -> Catalog {
    let preset_name = |p: Preset| {
        Preset::ALL
            .iter()
            .find(|(q, _)| *q == p)
            .map_or_else(|| format!("{p:?}"), |(_, n)| (*n).to_string())
    };
    Catalog {
        models: Model::ALL
            .iter()
            .map(|(m, name)| ModelInfo {
                name: (*name).to_string(),
                engine: format!("{:?}", m.def().engine),
                voices: m.voices(),
                presets: m
                    .def()
                    .presets
                    .iter()
                    .map(|d| preset_name(d.preset))
                    .collect(),
            })
            .collect(),
        pads: Pad::ALL.iter().map(|(_, n)| (*n).to_string()).collect(),
        params: Param::ALL
            .iter()
            .map(|(p, name)| {
                let (lo, hi) = p.range();
                ParamInfo {
                    name: (*name).to_string(),
                    lo,
                    hi,
                    scope: if p.is_global() {
                        "master"
                    } else if p.is_strip() {
                        "strip"
                    } else {
                        "synth"
                    },
                }
            })
            .collect(),
        modes: Mode::ALL
            .iter()
            .map(|(_, name, steps)| ModeInfo {
                name: (*name).to_string(),
                steps: steps.to_vec(),
            })
            .collect(),
        inserts: InsertType::ALL
            .iter()
            .map(|(_, n)| (*n).to_string())
            .collect(),
        processors: ProcType::ALL
            .iter()
            .map(|(_, n)| (*n).to_string())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEAT: &str = "tempo 120\ntrack kit drums Tr909 Kit909\ntrack bass synth Sh101 Sh101Bass\n\
clip beat = kit /16\n  bd x...x...x...x...\nclip low = bass\n  \"a1 a1 a1 a1\"\n\
scene loud 1: beat\nscene gap 1:\nscene again 1: beat\narrange loud gap again\n";

    #[test]
    fn check_gives_the_canonical_song_or_where_it_failed() {
        let c = check(BEAT);
        assert!(c.ok && c.error.is_none());
        assert_eq!(c.tracks, ["kit", "bass"]);
        assert_eq!(c.clips, ["beat", "low"]);
        assert_eq!(c.scenes, ["loud", "gap", "again"]);
        assert_eq!(c.bars, 3);
        let canonical = c.canonical.expect("printed");
        assert_eq!(
            check(&canonical).canonical.as_deref(),
            Some(canonical.as_str())
        );

        let bad = check("tempo 120\ntrack kit drums\nclip b = kit\n  bd x\nclip b = kit\n  sn x\n");
        assert!(!bad.ok && bad.canonical.is_none());
        let e = bad.error.expect("an error");
        assert_eq!((e.line, e.col), (5, 6), "{}", e.msg);
    }

    /// Each arrangement entry is measured on its own bars: a silent one
    /// between two loud ones reads near silence.
    #[test]
    fn render_measures_each_scene_and_track() {
        let r = render(BEAT, Limits::default()).expect("renders");
        assert_eq!((r.bars, r.cut_short, r.nonfinite), (3, false, 0));
        assert!(r.peak > 0.05 && r.peak <= 1.0, "peak {}", r.peak);
        let s: Vec<(&str, u64, f32)> = r
            .scenes
            .iter()
            .map(|x| (x.name.as_str(), x.bar, x.rms))
            .collect();
        assert_eq!(
            s.iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(),
            [("loud", 1), ("gap", 2), ("again", 3)]
        );
        let (loud, gap, again) = (s[0].2, s[1].2, s[2].2);
        assert!(loud > 0.01 && again > 0.01, "{loud} {again}");
        assert!(
            gap < 0.2 * loud,
            "the gap is near silence: {gap} against {loud}"
        );
        let [kit, bass] = [&r.tracks[0], &r.tracks[1]];
        assert!(kit.peak > 0.05 && kit.level > 0.0, "{kit:?}");
        assert_eq!(
            (bass.peak, bass.level),
            (0.0, 0.0),
            "no scene plays the bass"
        );
        assert_ne!(kit.synth, bass.synth);
        assert!(r.synthdefs.is_empty());
    }

    #[test]
    fn render_keeps_to_its_limits_and_reports_a_bad_song() {
        let one = render(
            BEAT,
            Limits {
                max_bars: 1,
                ..Limits::default()
            },
        )
        .expect("renders");
        assert_eq!(one.bars, 1);
        assert_eq!(one.scenes.len(), 1);
        let none = render(
            BEAT,
            Limits {
                max_time: Duration::ZERO,
                ..Limits::default()
            },
        )
        .expect("renders");
        assert!(none.cut_short && none.bars == 0);
        let e = render("tempo 120\nclip b = nowhere\n  bd x\n", Limits::default())
            .expect_err("no track");
        assert_eq!(e.line, 2);
    }

    #[test]
    fn render_reports_a_synthdef() {
        let song = concat!(
            "tempo 120\n",
            "setting beep = Modular ModularBasic\n",
            "  SynthDef(\\beep, { |freq = 440, gate = 1|\n",
            "      Pan2.ar(SinOsc.ar(freq) * EnvGen.kr(Env.perc(0.01, 0.2), gate), 0.3)\n",
            "  }).add;\n",
            "track lead synth beep\n",
            "clip r = lead\n",
            "  \"a4 ~ c5 ~\"\n",
        );
        let r = render(song, Limits::default()).expect("renders");
        let d = &r.synthdefs[0];
        assert!(d.ok && d.voices > 0 && d.stereo && d.knobs > 0, "{d:?}");
        assert_eq!(r.bars, FREE_BARS, "no arrangement: a few bars of the loops");
        assert!(r.peak > 0.01 && r.width > 0.0, "{} {}", r.peak, r.width);
    }

    /// A refused character is named, so the model knows what to take out.
    #[test]
    fn a_character_sclang_does_not_take_is_named() {
        let song = concat!(
            "tempo 120\n",
            "setting beep = Modular ModularBasic\n",
            "  SynthDef(\\beep, { |freq = 440, gate = 1|\n",
            "      SinOsc.ar(freq) $ 2\n",
            "  }).add;\n",
            "track lead synth beep\n",
        );
        let e = check(song).error.expect("refused");
        assert_eq!(
            (e.line, e.msg.as_str()),
            (4, "this character is not part of sclang here: `$`")
        );
        let e = render(song, Limits::default()).expect_err("refused");
        assert!(e.msg.ends_with("`$`"), "{e:?}");
    }

    /// A track without clips is auditioned: a synth plays the phrase, a
    /// drums track a beat, and the track sounds on its strip.
    #[test]
    fn a_track_without_clips_is_auditioned() {
        for (song, track) in [
            ("tempo 120\ntrack lead synth\n", "lead"),
            ("tempo 120\ntrack kit drums Tr909 Kit909\n", "kit"),
        ] {
            assert!(!plays(&Song::parse(song).unwrap(), 0));
            let heard = audition(song, track).expect("auditioned");
            let parsed = Song::parse(&heard).expect("parses");
            assert!(plays(&parsed, 0), "{heard}");
            let r = render(&heard, Limits::default()).expect("renders");
            assert!(r.tracks[0].peak > 0.01, "{track}: {:?}", r.tracks[0]);
        }
        assert_eq!(audition("tempo 120\ntrack lead synth\n", "bass"), None);
        assert_eq!(audition(BEAT, "kit"), None, "it has clips");
        let pad = BEAT.replace("arrange", "track pad synth\narrange");
        assert_eq!(audition(&pad, "pad"), None, "an arrangement");
    }

    /// The bass of `BEAT` has a clip but no scene plays it.
    #[test]
    fn a_track_plays_when_a_scene_holds_its_clip() {
        let song = Song::parse(BEAT).unwrap();
        assert!(plays(&song, 0) && !plays(&song, 1));
    }

    #[test]
    fn the_catalog_is_the_engines() {
        let c = catalog();
        assert_eq!(c.models.len(), Model::ALL.len());
        let presets: usize = c.models.iter().map(|m| m.presets.len()).sum();
        assert_eq!(
            presets,
            Preset::ALL.len(),
            "every preset in exactly one model"
        );
        assert!(
            c.models
                .iter()
                .any(|m| m.name == "Tr909" && m.presets.iter().any(|p| p == "Kit909"))
        );
        assert_eq!(c.params.len(), Param::ALL.len());
        let scope = |n: &str| c.params.iter().find(|p| p.name == n).map(|p| p.scope);
        assert_eq!(
            (scope("Cutoff"), scope("Level"), scope("MasterGain")),
            (Some("synth"), Some("strip"), Some("master"))
        );
        assert_eq!(c.modes.len(), 11);
        assert!(
            c.modes
                .iter()
                .any(|m| m.name == "phrygian-dominant" && m.steps == [0, 1, 4, 5, 7, 8, 10])
        );
        assert_eq!(c.pads.len(), Pad::ALL.len());
        assert!(
            c.inserts.iter().any(|i| i == "Vocoder") && c.processors.iter().any(|p| p == "Reverb")
        );
        serde_json::to_string(&c).expect("serialises");
    }
}
