//! The assistant's eval (#388): requests on starting songs, run through the
//! real loop and graded by code, to tune the prompt and the effort and to
//! compare providers on songs that parse, play cleanly and do what was
//! asked. Every run calls a provider and costs money: run it on request.

use crate::assist::{self, Event, LoopLimits, Request};
use crate::provider::{Provider, Usage};
use crate::tools::{self, Limits};
use algo_dsp::mono::model::Model;
use algo_dsp::song::Song;
use serde::{Deserialize, Serialize};

/// The set, built in.
pub const CASES: &str = include_str!("../eval/cases.json");

/// A check on the result, beyond "it parses and renders clean".
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "check", rename_all = "snake_case")]
pub enum Expect {
    /// Every clip of the start is still there.
    KeepsClips,
    /// These clips are there.
    KeepsClipsNamed {
        names: Vec<String>,
    },
    /// Every track of the start is still there.
    KeepsTracks,
    Tempo {
        value: f32,
    },
    SwingAtLeast {
        value: f32,
    },
    /// More lanes of this pad than the start had.
    PadLanesMore {
        pad: String,
    },
    ClipsMore,
    TracksMore,
    ScenesMore,
    /// A track (or its setting) plays this model.
    TrackModel {
        model: String,
    },
    /// The song's text holds this.
    Contains {
        text: String,
    },
    /// An answer in words, no song proposed.
    NoSong,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Case {
    pub id: String,
    /// A song file, from the repository's root.
    pub start: Option<String>,
    /// Or the song itself.
    pub start_text: Option<String>,
    pub request: String,
    pub expect: Vec<Expect>,
}

#[derive(Deserialize)]
struct CaseFile {
    cases: Vec<Case>,
}

/// The cases of `json`.
pub fn cases(json: &str) -> Result<Vec<Case>, String> {
    serde_json::from_str::<CaseFile>(json)
        .map(|f| f.cases)
        .map_err(|e| e.to_string())
}

/// What the loop gave for a case.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub song: Option<String>,
    pub text: String,
    pub error: Option<String>,
    pub rounds: u32,
    pub seconds: f32,
    pub usage: Usage,
    /// The gate's rules that fired (#453): refusals, then the warnings.
    pub rules: Vec<String>,
}

/// Run `req` through the loop and keep what it gave.
pub async fn run_case<P: Provider>(p: &P, req: &Request, limits: LoopLimits) -> Outcome {
    let mut out = Outcome::default();
    assist::run(p, req, limits, &mut |e| match e {
        Event::Tool { rules, .. } => out.rules.extend(rules.iter().map(|r| r.to_string())),
        Event::Song { song, warnings, .. } => {
            out.song = Some(song);
            out.rules
                .extend(warnings.iter().map(|f| f.rule.to_string()));
        }
        Event::Text { text } => out.text.push_str(&text),
        Event::Error { message, .. } => out.error = Some(message),
        Event::Done {
            rounds,
            seconds,
            usage,
        } => {
            out.rounds = rounds;
            out.seconds = seconds;
            out.usage = usage;
        }
        _ => {}
    })
    .await;
    out
}

/// One check's name and whether it held.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Graded {
    pub check: String,
    pub ok: bool,
}

fn model_name(m: Model) -> &'static str {
    Model::ALL
        .iter()
        .find(|(x, _)| *x == m)
        .map_or("?", |(_, n)| n)
}

fn lanes_of(song: Option<&Song>, pad: &str) -> usize {
    song.map_or(0, |s| {
        s.clips
            .iter()
            .flat_map(|f| &f.lanes)
            .filter(|l| {
                algo_dsp::drums::Pad::ALL
                    .iter()
                    .any(|(p, n)| *p == l.pad && *n == pad)
            })
            .count()
    })
}

/// Grade an outcome against the case: a song expected must parse and render
/// clean, then each of the case's checks.
pub fn grade(start: &str, case: &Case, out: &Outcome) -> Vec<Graded> {
    let mut g = Vec::new();
    let mut put = |check: &str, ok: bool| {
        g.push(Graded {
            check: check.to_string(),
            ok,
        })
    };
    if case.expect.contains(&Expect::NoSong) {
        put(
            "no_song",
            out.song.is_none() && out.error.is_none() && !out.text.trim().is_empty(),
        );
        return g;
    }
    let Some(text) = out.song.as_deref() else {
        put("proposed", false);
        return g;
    };
    let before = Song::parse(start).ok();
    let after = Song::parse(text).ok();
    put("parses", after.is_some());
    let Some(after) = after else {
        return g;
    };
    let clean = tools::render(
        text,
        Limits {
            max_bars: 16,
            ..Limits::default()
        },
    )
    .is_ok_and(|r| r.nonfinite == 0 && r.peak <= 1.0);
    put("renders_clean", clean);
    let names = |s: &Song| s.clips.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    for e in &case.expect {
        let (name, ok) = match e {
            Expect::KeepsClips => (
                "keeps_clips".to_string(),
                before
                    .as_ref()
                    .is_none_or(|b| names(b).iter().all(|n| names(&after).contains(n))),
            ),
            Expect::KeepsClipsNamed { names: want } => (
                "keeps_clips_named".into(),
                want.iter().all(|n| names(&after).contains(n)),
            ),
            Expect::KeepsTracks => (
                "keeps_tracks".into(),
                before.as_ref().is_none_or(|b| {
                    b.tracks
                        .iter()
                        .all(|t| after.tracks.iter().any(|a| a.name == t.name))
                }),
            ),
            Expect::Tempo { value } => {
                (format!("tempo {value}"), (after.tempo - value).abs() < 0.5)
            }
            Expect::SwingAtLeast { value } => (format!("swing >= {value}"), after.swing >= *value),
            Expect::PadLanesMore { pad } => (
                format!("more {pad} lanes"),
                lanes_of(Some(&after), pad) > lanes_of(before.as_ref(), pad),
            ),
            Expect::ClipsMore => (
                "more clips".into(),
                after.clips.len() > before.as_ref().map_or(0, |b| b.clips.len()),
            ),
            Expect::TracksMore => (
                "more tracks".into(),
                after.tracks.len() > before.as_ref().map_or(0, |b| b.tracks.len()),
            ),
            Expect::ScenesMore => (
                "more scenes".into(),
                after.scenes.len() > before.as_ref().map_or(0, |b| b.scenes.len()),
            ),
            Expect::TrackModel { model } => (
                format!("a {model} track"),
                after.tracks.iter().any(|t| {
                    let preset = t.preset.or_else(|| {
                        t.setting
                            .and_then(|i| after.settings.get(i))
                            .map(|s| s.preset)
                    });
                    preset.is_some_and(|p| model_name(p.model()) == model)
                }),
            ),
            Expect::Contains { text: want } => {
                (format!("contains {want:?}"), text.contains(want.as_str()))
            }
            Expect::NoSong => ("no_song".into(), false),
        };
        put(&name, ok);
    }
    g
}

/// Dollars per million tokens: uncached input, cached input, output. Only
/// models whose prices are known; the rest report no cost.
fn prices(model: &str) -> Option<(f64, f64, f64)> {
    match model {
        "claude-opus-5-5" => Some((4.0, 0.20, 20.0)),
        "claude-sonnet-5-5" => Some((2.0, 0.20, 10.0)),
        // OpenRouter's list prices on 2026-10-10 (#452); no cache discount known.
        "qwen/qwen3.8-max-0902" => Some((2.0, 2.0, 6.0)),
        "moonshotai/kimi-k3" => Some((0.64, 0.64, 13.5)),
        "z-ai/glm-5.3" => Some((0.039, 0.039, 4.8)),
        _ => None,
    }
}

/// The cost of `u` on `model`, if its prices are known. Cache writes are
/// counted at the input price, so it is a little low on a first request.
pub fn cost(model: &str, u: Usage) -> Option<f64> {
    let (input, cached, output) = prices(model)?;
    let uncached = u.input.saturating_sub(u.cached) as f64;
    Some((uncached * input + u.cached as f64 * cached + u.output as f64 * output) / 1e6)
}

/// A case's line in the report.
#[derive(Clone, Debug, Serialize)]
pub struct Row {
    pub id: String,
    pub pass: bool,
    pub failed: Vec<String>,
    pub error: Option<String>,
    pub rules: Vec<String>,
    pub rounds: u32,
    pub seconds: f32,
    pub usage: Usage,
    pub cost: Option<f64>,
}

impl Row {
    pub fn new(case: &Case, out: &Outcome, graded: &[Graded], model: &str) -> Row {
        let failed: Vec<String> = graded
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.check.clone())
            .collect();
        Row {
            id: case.id.clone(),
            pass: failed.is_empty(),
            failed,
            error: out.error.clone(),
            rules: out.rules.clone(),
            rounds: out.rounds,
            seconds: out.seconds,
            usage: out.usage,
            cost: cost(model, out.usage),
        }
    }
}

/// The report as a Markdown table, with totals.
pub fn markdown(provider: &str, model: &str, rows: &[Row]) -> String {
    let mut s = format!(
        "## {provider} / {model}\n\n| case | pass | failed | rules | rounds | in / cached / out | s | $ |\n|---|---|---|---|---|---|---|---|\n"
    );
    let mut total = Usage::default();
    let mut dollars = Some(0.0);
    for r in rows {
        total += r.usage;
        dollars = dollars.zip(r.cost).map(|(a, b)| a + b);
        let failed = match (&r.error, r.failed.is_empty()) {
            (Some(e), _) => format!("{} ({e})", r.failed.join(", ")),
            (None, true) => String::new(),
            (None, false) => r.failed.join(", "),
        };
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} / {} / {} | {:.0} | {} |\n",
            r.id,
            if r.pass { "✓" } else { "✗" },
            failed,
            r.rules.join(", "),
            r.rounds,
            r.usage.input,
            r.usage.cached,
            r.usage.output,
            r.seconds,
            r.cost.map_or("-".into(), |c| format!("{c:.3}"))
        ));
    }
    let passed = rows.iter().filter(|r| r.pass).count();
    let mean = |f: fn(&Row) -> f64| rows.iter().map(f).sum::<f64>() / rows.len().max(1) as f64;
    s.push_str(&format!(
        "\n**{passed}/{} passed**, {:.1} rounds and {:.0} s a case on average; tokens {} in ({} cached), {} out{}\n",
        rows.len(),
        mean(|r| f64::from(r.rounds)),
        mean(|r| f64::from(r.seconds)),
        total.input,
        total.cached,
        total.output,
        dollars.map_or(String::new(), |d| format!("; about ${d:.2}")),
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Msg, ProviderError, Stop, ToolCall, ToolDef, Turn};
    use serde_json::json;
    use std::sync::Mutex;

    const START: &str =
        "tempo 120\ntrack kit drums Tr909 Kit909\nclip beat = kit /16\n  bd x...x...x...x...\n";
    const MORE: &str = "tempo 128\nswing 58\ntrack kit drums Tr909 Kit909\ntrack bass synth Sh101 Sh101Bass\n\
clip beat = kit /16\n  bd x...x...x...x...\n  sn ....x.......x...\nclip low = bass\n  \"a1 ~ a1 ~\"\n\
scene a 2: beat low\narrange a\n";

    fn case(expect: Vec<Expect>) -> Case {
        Case {
            id: "c".into(),
            start: None,
            start_text: Some(START.into()),
            request: "r".into(),
            expect,
        }
    }

    fn proposed(song: &str) -> Outcome {
        Outcome {
            song: Some(song.into()),
            rounds: 2,
            ..Outcome::default()
        }
    }

    #[test]
    fn the_set_loads_and_its_start_songs_parse() {
        let cs = cases(CASES).expect("loads");
        assert!(cs.len() >= 30, "{}", cs.len());
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut ids = std::collections::HashSet::new();
        for c in &cs {
            assert!(ids.insert(c.id.clone()), "{} twice", c.id);
            assert!(!c.expect.is_empty(), "{}", c.id);
            if let Some(path) = &c.start {
                let text = std::fs::read_to_string(root.join(path)).expect(path);
                assert!(Song::parse(&text).is_ok(), "{}: {path} parses", c.id);
            } else {
                assert!(c.start_text.is_some(), "{}: a start", c.id);
            }
        }
        // The two "fix" cases start from songs that do not parse.
        for id in ["fix-pad", "fix-order"] {
            let c = cs.iter().find(|c| c.id == id).expect(id);
            assert!(
                Song::parse(c.start_text.as_deref().expect("text")).is_err(),
                "{id} starts broken"
            );
        }
    }

    #[test]
    fn checks_grade_what_they_say() {
        let all = vec![
            Expect::KeepsClips,
            Expect::KeepsTracks,
            Expect::KeepsClipsNamed {
                names: vec!["beat".into()],
            },
            Expect::Tempo { value: 128.0 },
            Expect::SwingAtLeast { value: 56.0 },
            Expect::PadLanesMore { pad: "sn".into() },
            Expect::ClipsMore,
            Expect::TracksMore,
            Expect::ScenesMore,
            Expect::TrackModel {
                model: "Sh101".into(),
            },
            Expect::Contains {
                text: "a1 ~".into(),
            },
        ];
        let g = grade(START, &case(all.clone()), &proposed(MORE));
        assert!(g.iter().all(|c| c.ok), "{g:?}");
        assert_eq!(g.len(), all.len() + 2, "parses and renders_clean first");

        let g = grade(START, &case(all), &proposed(START));
        let failed: Vec<&str> = g
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.check.as_str())
            .collect();
        assert_eq!(
            failed,
            [
                "tempo 128",
                "swing >= 56",
                "more sn lanes",
                "more clips",
                "more tracks",
                "more scenes",
                "a Sh101 track",
                "contains \"a1 ~\""
            ]
        );

        let g = grade(
            START,
            &case(vec![Expect::ClipsMore]),
            &proposed("tempo 120\nclip x = nowhere\n  bd x\n"),
        );
        assert_eq!(
            g,
            [Graded {
                check: "parses".into(),
                ok: false
            }]
        );
        let g = grade(START, &case(vec![Expect::ClipsMore]), &Outcome::default());
        assert_eq!(
            g,
            [Graded {
                check: "proposed".into(),
                ok: false
            }]
        );
    }

    #[test]
    fn a_question_passes_with_words_and_no_song() {
        let c = case(vec![Expect::NoSong]);
        let answer = Outcome {
            text: "It plays a kick.".into(),
            ..Outcome::default()
        };
        assert_eq!(
            grade(START, &c, &answer),
            [Graded {
                check: "no_song".into(),
                ok: true
            }]
        );
        assert!(
            !grade(START, &c, &proposed(START))[0].ok,
            "it should not have changed the song"
        );
    }

    #[test]
    fn cost_and_the_report() {
        let u = Usage {
            input: 1_000_000,
            cached: 500_000,
            output: 100_000,
        };
        assert_eq!(
            cost("claude-opus-5-5", u),
            Some((500_000.0 * 4.0 + 500_000.0 * 0.2 + 100_000.0 * 20.0) / 1e6)
        );
        assert_eq!(cost("mistral-large-latest", u), None);
        let c = case(vec![Expect::Tempo { value: 128.0 }]);
        let out = Outcome {
            rules: vec!["unused-clip".into(), "removed".into()],
            ..proposed(MORE)
        };
        let row = Row::new(&c, &out, &grade(START, &c, &out), "claude-opus-5-5");
        assert!(row.pass);
        let md = markdown("anthropic", "claude-opus-5-5", &[row]);
        assert!(
            md.contains("| c | ✓ |  | unused-clip, removed | 2 |") && md.contains("**1/1 passed**"),
            "{md}"
        );
    }

    /// A provider that proposes the song it is told to: the runner gathers
    /// the outcome from the loop's events.
    struct Proposer(Mutex<Option<String>>);

    impl Provider for Proposer {
        async fn turn(&self, _s: &str, _t: &[ToolDef], _m: &[Msg]) -> Result<Turn, ProviderError> {
            let song = self
                .0
                .lock()
                .unwrap()
                .take()
                .ok_or(ProviderError::Fatal("done".into()))?;
            Ok(Turn {
                text: "Here it is.".into(),
                calls: vec![ToolCall {
                    id: "p".into(),
                    name: "propose_song".into(),
                    input: json!({"song": song, "summary": "s"}),
                }],
                stop: Stop::Tools,
                usage: Usage {
                    input: 100,
                    cached: 0,
                    output: 10,
                },
                raw: json!([]),
            })
        }
    }

    #[tokio::test]
    async fn a_case_runs_through_the_loop() {
        let p = Proposer(Mutex::new(Some(MORE.into())));
        let req = Request {
            song: START.into(),
            request: "more".into(),
            focus: None,
        };
        let out = run_case(&p, &req, LoopLimits::default()).await;
        assert_eq!(out.song.as_deref(), Some(MORE));
        assert_eq!(
            (out.rounds, out.usage.input, out.text.as_str()),
            (1, 100, "Here it is.")
        );
        assert!(out.rules.is_empty(), "{:?}", out.rules);
        // The gate's warnings are recorded with the case (#453).
        let p = Proposer(Mutex::new(Some(MORE.replace("arrange a\n", ""))));
        let out = run_case(&p, &req, LoopLimits::default()).await;
        assert_eq!(out.rules, ["unarranged"]);
    }
}
