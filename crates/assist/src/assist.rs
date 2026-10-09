//! The assist loop (#385, ADR-0028): the model gets the language, the
//! engine's catalog, the song and a request; it checks and renders songs
//! with the tools and ends by proposing one, which the engine has parsed.
//! Every step goes out as an event.

use crate::provider::{Msg, Provider, ProviderError, Stop, ToolCall, ToolDef, ToolResult, Usage};
use crate::scope;
use crate::tools::{self, Limits};
use algo_dsp::song::Song;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The language, as the model reads it (#383).
const LANGUAGE: &str = include_str!("../../../.openspec/language.md");

/// What the browser asks for.
#[derive(Clone, Debug)]
pub struct Request {
    pub song: String,
    pub request: String,
    /// The track (instrument) the request may change, if one is in focus
    /// (#415): a proposal that changes anything else is refused.
    pub focus: Option<String>,
}

/// How far one request may go.
#[derive(Clone, Copy, Debug)]
pub struct LoopLimits {
    pub rounds: u32,
    pub time: Duration,
}

impl Default for LoopLimits {
    fn default() -> LoopLimits {
        LoopLimits {
            rounds: 8,
            time: Duration::from_secs(600),
        }
    }
}

/// A step, sent to the browser as a server-sent event of the same name.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum Event {
    Progress {
        round: u32,
        message: String,
    },
    Tool {
        round: u32,
        name: String,
        ok: bool,
        summary: String,
    },
    Text {
        text: String,
    },
    Song {
        song: String,
        summary: String,
    },
    Error {
        message: String,
        retryable: bool,
    },
    Done {
        rounds: u32,
        seconds: f32,
        usage: Usage,
    },
}

impl Event {
    /// The event's name, for the `event:` line.
    pub fn name(&self) -> &'static str {
        match self {
            Event::Progress { .. } => "progress",
            Event::Tool { .. } => "tool",
            Event::Text { .. } => "text",
            Event::Song { .. } => "song",
            Event::Error { .. } => "error",
            Event::Done { .. } => "done",
        }
    }

    /// The event's fields, without its name, for the `data:` line.
    pub fn data(&self) -> Value {
        let mut v = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Some(o) = v.as_object_mut() {
            o.remove("event");
        }
        v
    }
}

/// The tools the model may call.
pub fn tool_defs() -> Vec<ToolDef> {
    let song = json!({"type": "string", "description": "The whole song text."});
    vec![
        ToolDef {
            name: "check_song",
            description: "Parse a song with the engine's parser. Returns ok and its tracks, fragments, sections and bars, or the first error with its line and column. render_song parses too; use this for a quick look at the structure.",
            schema: json!({"type": "object", "properties": {"song": song}, "required": ["song"], "additionalProperties": false}),
        },
        ToolDef {
            name: "render_song",
            description: "Play a song offline and measure it: non-finite samples, peak (above 1.0 clips), RMS and stereo width, RMS per arrangement entry, each track's peak and level on its strip, and each Modular SynthDef's build. It parses the song first. A track in focus without fragments, in a song without an arrangement, plays an audition phrase (auditioned: true). Use it to check the mix and that every part sounds.",
            schema: json!({"type": "object", "properties": {
                "song": song,
                "bars": {"type": "integer", "description": "How many bars to render, at most 64; default the arrangement, or 4 without one."}
            }, "required": ["song"], "additionalProperties": false}),
        },
        ToolDef {
            name: "propose_song",
            description: "Propose the finished song to the user, who sees a diff and applies it. It must parse, stay within the track in focus and render cleanly: no non-finite samples, no clipping, the track in focus not silent; otherwise it is refused with the render's measures. Propose once, when it renders cleanly.",
            schema: json!({"type": "object", "properties": {
                "song": song,
                "summary": {"type": "string", "description": "What you changed and why, in one short paragraph."}
            }, "required": ["song", "summary"], "additionalProperties": false}),
        },
    ]
}

/// The system prompt: how to work, the language, the catalog. The same on
/// every request, so providers that cache keep it.
pub fn system_prompt() -> &'static str {
    static PROMPT: OnceLock<String> = OnceLock::new();
    PROMPT.get_or_init(|| {
        format!(
            "You write and change songs for algo-synth, a synthesizer in the browser. A song is a text in the language defined below; the user's engine parses it and plays it.\n\n\
How to work:\n\
- You get the current song and a request. Change what the request asks for and keep the rest: names, comments, settings, mixer lines and the arrangement, unless the request is about them.\n\
- Render every song you write with render_song; it parses it too and reports the first error with its line and column. Aim for no non-finite samples, the peak below 1.0, every new or changed part audible, and the parts balanced (drums and bass lead, pads and arps under them).\n\
- A track in focus without fragments is auditioned: the render plays a phrase on it (\"auditioned\": true). Don't add a fragment just to hear it.\n\
- When it renders cleanly, call propose_song once with the whole song and a short summary. It renders the song again and refuses one that clips, has non-finite samples or leaves the track in focus silent. The user reviews a diff and applies it.\n\
- If the request is a question, answer it in text and propose nothing.\n\
- Use only the models, presets, pads, parameters and scales of the catalog.\n\n\
<language>\n{LANGUAGE}\n</language>\n\n<catalog>\n{}</catalog>\n",
            catalog_text()
        )
    })
}

/// The catalog as compact text: models with their presets, pads, scales,
/// insert and processor types, and the parameters by scope with ranges.
fn catalog_text() -> String {
    let c = tools::catalog();
    let mut s = String::from("Models (engine, voices: presets):\n");
    for m in &c.models {
        s.push_str(&format!(
            "- {} ({}, {}): {}\n",
            m.name,
            m.engine,
            m.voices,
            m.presets.join(", ")
        ));
    }
    let modes: Vec<&str> = c.modes.iter().map(|m| m.name.as_str()).collect();
    s.push_str(&format!("Pads: {}\n", c.pads.join(" ")));
    s.push_str(&format!("Scales: {}\n", modes.join(", ")));
    s.push_str(&format!("Insert types: {}\n", c.inserts.join(", ")));
    s.push_str(&format!("Processor types: {}\n", c.processors.join(", ")));
    for scope in ["synth", "strip", "master"] {
        let line: Vec<String> = c
            .params
            .iter()
            .filter(|p| p.scope == scope)
            .map(|p| format!("{} {}..{}", p.name, p.lo, p.hi))
            .collect();
        s.push_str(&format!(
            "Parameters on a {scope} (name lo..hi):\n{}\n",
            line.join(", ")
        ));
    }
    s
}

/// The first message: the song, the focus and the request.
fn first_message(req: &Request) -> String {
    let focus = req
        .focus
        .as_deref()
        .map(|t| {
            format!(
                "The request is about the track `{t}`. Change only that track: its track line, its strip line, its frags, and autos, mods and scene values on it; leave everything else exactly as it is.\n"
            )
        })
        .unwrap_or_default();
    format!(
        "<song>\n{}\n</song>\n\n{focus}Request: {}",
        req.song.trim_end(),
        req.request.trim()
    )
}

/// The song in a tool's input, or why there is none.
fn song_of(call: &ToolCall) -> Result<&str, String> {
    match call.input.get("song").and_then(Value::as_str) {
        Some(s) if s.len() <= 1 << 20 => Ok(s),
        Some(_) => Err("the song is longer than 1 MB".into()),
        None => Err("`song` must be the song text, as a string".into()),
    }
}

/// What a tool did: its answer to the model, a line for the browser, and a
/// proposed song with its summary when `propose_song` succeeded.
struct Ran {
    result: ToolResult,
    ok: bool,
    summary: String,
    proposed: Option<(String, String)>,
}

async fn run_tool(call: &ToolCall, req: &Request) -> Ran {
    let answer = |content: Value, ok: bool, summary: String| Ran {
        result: ToolResult {
            id: call.id.clone(),
            name: call.name.clone(),
            content: content.to_string(),
            is_error: !ok,
        },
        ok,
        summary,
        proposed: None,
    };
    let song = match song_of(call) {
        Ok(s) => s.to_string(),
        Err(why) => return answer(json!({"error": why}), false, why),
    };
    match call.name.as_str() {
        "check_song" => {
            let c = tools::check(&song);
            let summary = match &c.error {
                None => format!(
                    "parses: {} tracks, {} frags, {} bars",
                    c.tracks.len(),
                    c.frags.len(),
                    c.bars
                ),
                Some(e) => format!("line {}, col {}: {}", e.line, e.col, e.msg),
            };
            let ok = c.ok;
            let mut v = serde_json::to_value(&c).unwrap_or(Value::Null);
            if let Some(o) = v.as_object_mut() {
                o.remove("canonical");
            }
            answer(v, ok, summary)
        }
        "render_song" => {
            let bars = call
                .input
                .get("bars")
                .and_then(Value::as_u64)
                .unwrap_or(64)
                .clamp(1, 64);
            match measure(&song, req.focus.as_deref(), bars).await {
                Ok(m) => {
                    let fault = m.fault();
                    let summary = format!(
                        "{} bars, peak {:.2}, {}",
                        m.rendered.bars,
                        m.rendered.peak,
                        fault.as_deref().unwrap_or("clean")
                    );
                    answer(m.report(), fault.is_none(), summary)
                }
                Err((v, summary)) => answer(v, false, summary),
            }
        }
        "propose_song" => {
            let summary = call
                .input
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let c = tools::check(&song);
            if let Some(e) = c.error {
                let line = format!("line {}, col {}: {}", e.line, e.col, e.msg);
                return answer(
                    json!({"error": e, "hint": "fix it and propose again"}),
                    false,
                    line,
                );
            }
            // With a track in focus (#415), a song that changes more is refused.
            if let Some(t) = &req.focus
                && let Err(why) = scope::check(&req.song, &song, t)
            {
                return answer(
                    json!({"error": why, "hint": "change only the track in focus and propose again"}),
                    false,
                    why,
                );
            }
            // Only a song that renders cleanly reaches the user (#430).
            match measure(&song, req.focus.as_deref(), 64).await {
                Ok(m) => match m.fault() {
                    Some(why) => {
                        let mut v = m.report();
                        if let Some(o) = v.as_object_mut() {
                            o.insert("error".into(), json!(why));
                            o.insert("hint".into(), json!("fix it and propose again"));
                        }
                        answer(v, false, why)
                    }
                    None => Ran {
                        proposed: Some((song, summary)),
                        ..answer(json!({"ok": true}), true, "proposed".into())
                    },
                },
                Err((v, why)) => answer(v, false, why),
            }
        }
        other => answer(
            json!({"error": format!("no tool {other}")}),
            false,
            format!("no tool {other}"),
        ),
    }
}

/// A track's peak on its strip at or below this is silence.
const SILENT: f32 = 1e-4;

/// A song as `render_song` and the gate play it.
struct Measured {
    rendered: tools::Rendered,
    /// The track in focus had no fragment and played an audition (#430).
    auditioned: bool,
    /// The track in focus, when it should sound: it was auditioned or one
    /// of its fragments plays.
    heard: Option<String>,
}

impl Measured {
    /// What keeps the song from the user: non-finite samples, clipping, or
    /// the track in focus silent.
    fn fault(&self) -> Option<String> {
        let r = &self.rendered;
        if r.nonfinite > 0 {
            return Some(format!("{} non-finite samples", r.nonfinite));
        }
        if r.peak > 1.0 {
            return Some(format!("the peak is {:.2}: it clips", r.peak));
        }
        let silent = |t: &str| {
            r.tracks
                .iter()
                .find(|x| x.name == t)
                .is_some_and(|x| x.peak <= SILENT)
        };
        match &self.heard {
            Some(t) if !r.cut_short && silent(t) => Some(format!("`{t}` is silent")),
            _ => None,
        }
    }

    /// The render's measures for the model, saying when it was an audition.
    fn report(&self) -> Value {
        let mut v = serde_json::to_value(&self.rendered).unwrap_or(Value::Null);
        if let Some(o) = v.as_object_mut() {
            o.insert("auditioned".into(), json!(self.auditioned));
        }
        v
    }
}

/// Render `song` for at most `bars` bars; with a track in focus that has no
/// fragment, and no arrangement, it plays an audition phrase on that track
/// (#430). A song that does not load gives its error and a line.
async fn measure(song: &str, focus: Option<&str>, bars: u64) -> Result<Measured, (Value, String)> {
    let audition = focus.and_then(|t| tools::audition(song, t));
    let auditioned = audition.is_some();
    let heard = focus.filter(|t| {
        auditioned
            || Song::parse(song).is_ok_and(|s| {
                s.tracks
                    .iter()
                    .position(|x| x.name == *t)
                    .is_some_and(|i| tools::plays(&s, i))
            })
    });
    let heard = heard.map(str::to_string);
    let text = audition.unwrap_or_else(|| song.to_string());
    let limits = Limits {
        max_bars: bars,
        ..Limits::default()
    };
    match tokio::task::spawn_blocking(move || tools::render(&text, limits)).await {
        Ok(Ok(rendered)) => Ok(Measured {
            rendered,
            auditioned,
            heard,
        }),
        Ok(Err(e)) => {
            let line = format!("line {}, col {}: {}", e.line, e.col, e.msg);
            Err((json!({"error": e}), line))
        }
        Err(_) => Err((
            json!({"error": "the render failed"}),
            "the render failed".into(),
        )),
    }
}

/// One provider call, retried twice on a transient failure (2 s, then 6 s).
async fn turn_with_retry<P: Provider>(
    p: &P,
    system: &str,
    tools: &[ToolDef],
    msgs: &[Msg],
) -> Result<crate::provider::Turn, ProviderError> {
    let mut wait = Duration::from_secs(2);
    for _ in 0..2 {
        match p.turn(system, tools, msgs).await {
            Err(ProviderError::Transient(_)) => {
                tokio::time::sleep(wait).await;
                wait *= 3;
            }
            other => return other,
        }
    }
    p.turn(system, tools, msgs).await
}

/// Run one request to the end, sending each step to `emit`. It always ends
/// with `Event::Done`.
pub async fn run<P: Provider>(
    p: &P,
    req: &Request,
    limits: LoopLimits,
    emit: &mut (impl FnMut(Event) + Send),
) {
    let started = Instant::now();
    let defs = tool_defs();
    let system = system_prompt();
    let mut msgs = vec![Msg::User(first_message(req))];
    let mut usage = Usage::default();
    let mut rounds = 0;
    let fail = |message: String, retryable: bool| Event::Error { message, retryable };
    loop {
        if rounds >= limits.rounds {
            emit(fail(format!("no song after {rounds} rounds"), false));
            break;
        }
        if started.elapsed() > limits.time {
            emit(fail("the request took too long".into(), false));
            break;
        }
        rounds += 1;
        let message = if rounds == 1 {
            "Writing the song"
        } else {
            "Revising"
        };
        emit(Event::Progress {
            round: rounds,
            message: message.into(),
        });
        let turn = match turn_with_retry(p, system, &defs, &msgs).await {
            Ok(t) => t,
            Err(e) => {
                emit(fail(
                    e.to_string(),
                    matches!(e, ProviderError::Transient(_)),
                ));
                break;
            }
        };
        usage += turn.usage;
        if !turn.text.trim().is_empty() {
            emit(Event::Text {
                text: turn.text.clone(),
            });
        }
        if turn.stop == Stop::Refusal {
            emit(fail("the model declined this request".into(), false));
            break;
        }
        if turn.calls.is_empty() {
            if turn.stop == Stop::MaxTokens {
                emit(fail(
                    "the model ran out of room for its answer".into(),
                    true,
                ));
            }
            break;
        }
        let mut results = Vec::new();
        let mut proposed = None;
        for call in &turn.calls {
            let ran = run_tool(call, req).await;
            emit(Event::Tool {
                round: rounds,
                name: call.name.clone(),
                ok: ran.ok,
                summary: ran.summary,
            });
            if proposed.is_none() {
                proposed = ran.proposed;
            }
            results.push(ran.result);
        }
        msgs.push(Msg::Assistant(turn));
        if let Some((song, summary)) = proposed {
            emit(Event::Song { song, summary });
            break;
        }
        msgs.push(Msg::Results(results));
    }
    emit(Event::Done {
        rounds,
        seconds: started.elapsed().as_secs_f32(),
        usage,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Turn;
    use std::sync::Mutex;

    /// A provider that plays back scripted turns and records what it was sent.
    struct Scripted {
        turns: Mutex<Vec<Result<Turn, ProviderError>>>,
        seen: Mutex<Vec<Vec<Msg>>>,
    }

    impl Scripted {
        fn new(mut turns: Vec<Result<Turn, ProviderError>>) -> Scripted {
            turns.reverse();
            Scripted {
                turns: Mutex::new(turns),
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    impl Provider for Scripted {
        async fn turn(
            &self,
            _system: &str,
            _tools: &[ToolDef],
            msgs: &[Msg],
        ) -> Result<Turn, ProviderError> {
            self.seen.lock().unwrap().push(msgs.to_vec());
            self.turns
                .lock()
                .unwrap()
                .pop()
                .unwrap_or_else(|| Err(ProviderError::Fatal("script ended".into())))
        }
    }

    fn calls(calls: Vec<(&str, Value)>) -> Turn {
        Turn {
            text: String::new(),
            calls: calls
                .into_iter()
                .enumerate()
                .map(|(i, (name, input))| ToolCall {
                    id: format!("c{i}"),
                    name: name.into(),
                    input,
                })
                .collect(),
            stop: Stop::Tools,
            usage: Usage {
                input: 1000,
                cached: 800,
                output: 100,
            },
            raw: json!([]),
        }
    }

    const GOOD: &str =
        "tempo 120\ntrack kit drums Tr909 Kit909\nfrag beat = kit /16\n  bd x...x...x...x...\n";
    const BAD: &str = "tempo 120\ntrack kit drums Tr909 Kit909\nfrag beat = kit /16\n  zz x...\n";

    async fn events(p: &Scripted, limits: LoopLimits) -> Vec<Event> {
        let req = Request {
            song: GOOD.into(),
            request: "add a snare".into(),
            focus: Some("kit".into()),
        };
        events_for(p, &req, limits).await
    }

    async fn events_for(p: &Scripted, req: &Request, limits: LoopLimits) -> Vec<Event> {
        let mut out = Vec::new();
        run(p, req, limits, &mut |e| out.push(e)).await;
        out
    }

    /// The tool lines the browser got, by name and whether they passed.
    fn tool_lines(ev: &[Event]) -> Vec<(String, bool, String)> {
        ev.iter()
            .filter_map(|e| match e {
                Event::Tool {
                    name, ok, summary, ..
                } => Some((name.clone(), *ok, summary.clone())),
                _ => None,
            })
            .collect()
    }

    /// An instrument without fragments (#430) is heard: the render plays an
    /// audition on it and says so, and the gate hears it too, so the model
    /// proposes the song as it is, with no test fragment.
    #[tokio::test]
    async fn an_instrument_without_fragments_is_auditioned() {
        let song = "tempo 120\ntrack lead synth\n";
        let p = Scripted::new(vec![
            Ok(calls(vec![(
                "render_song",
                json!({"song": song, "bars": 2}),
            )])),
            Ok(calls(vec![(
                "propose_song",
                json!({"song": song, "summary": "a lead"}),
            )])),
        ]);
        let req = Request {
            song: song.into(),
            request: "a brighter lead".into(),
            focus: Some("lead".into()),
        };
        let ev = events_for(&p, &req, LoopLimits::default()).await;
        let lines = tool_lines(&ev);
        assert!(lines[0].1 && lines[0].2.ends_with("clean"), "{lines:?}");
        assert!(ev.contains(&Event::Song {
            song: song.into(),
            summary: "a lead".into()
        }));
        let seen = p.seen.lock().unwrap();
        let Msg::Results(r) = &seen[1][2] else {
            panic!("the render's result: {:?}", seen[1])
        };
        assert!(
            r[0].content.contains("\"auditioned\":true"),
            "{}",
            r[0].content
        );
    }

    /// A song whose track in focus has a fragment but makes no sound is
    /// refused with why; the fix goes.
    #[tokio::test]
    async fn a_silent_track_in_focus_is_not_proposed() {
        let hush = GOOD.replace("bd x...x...x...x...", "bd ................");
        let p = Scripted::new(vec![
            Ok(calls(vec![(
                "propose_song",
                json!({"song": hush, "summary": "quiet"}),
            )])),
            Ok(calls(vec![(
                "propose_song",
                json!({"song": GOOD, "summary": "a kick"}),
            )])),
        ]);
        let ev = events(&p, LoopLimits::default()).await;
        assert_eq!(
            tool_lines(&ev),
            [
                ("propose_song".into(), false, "`kit` is silent".into()),
                ("propose_song".into(), true, "proposed".into())
            ]
        );
    }

    #[test]
    fn clipping_and_non_finite_samples_are_faults() {
        let r = tools::render(GOOD, Limits::default()).expect("renders");
        let m = |peak, nonfinite| Measured {
            rendered: tools::Rendered {
                peak,
                nonfinite,
                ..r.clone()
            },
            auditioned: false,
            heard: Some("kit".into()),
        };
        assert_eq!(m(0.5, 0).fault(), None);
        assert_eq!(
            m(1.3, 0).fault().as_deref(),
            Some("the peak is 1.30: it clips")
        );
        assert_eq!(m(0.5, 3).fault().as_deref(), Some("3 non-finite samples"));
    }

    /// The model's bad song is checked, the error goes back, the fix is
    /// rendered and proposed: the song event carries it, done counts it all.
    #[tokio::test]
    async fn a_parse_error_goes_back_and_the_fix_is_proposed() {
        let p = Scripted::new(vec![
            Ok(calls(vec![("check_song", json!({"song": BAD}))])),
            Ok(calls(vec![(
                "render_song",
                json!({"song": GOOD, "bars": 1}),
            )])),
            Ok(calls(vec![(
                "propose_song",
                json!({"song": GOOD, "summary": "a kick"}),
            )])),
        ]);
        let ev = events(&p, LoopLimits::default()).await;
        let tools: Vec<(String, bool)> = ev
            .iter()
            .filter_map(|e| match e {
                Event::Tool { name, ok, .. } => Some((name.clone(), *ok)),
                _ => None,
            })
            .collect();
        assert_eq!(
            tools,
            [
                ("check_song".into(), false),
                ("render_song".into(), true),
                ("propose_song".into(), true)
            ]
        );
        assert!(ev.contains(&Event::Song {
            song: GOOD.into(),
            summary: "a kick".into()
        }));
        let Some(Event::Done { rounds, usage, .. }) = ev.last() else {
            panic!("ends with done: {ev:?}")
        };
        assert_eq!(*rounds, 3);
        assert_eq!(
            *usage,
            Usage {
                input: 3000,
                cached: 2400,
                output: 300
            }
        );
        // The first message holds the song, the focus and the request; the
        // second call carries the check's error back.
        let seen = p.seen.lock().unwrap();
        let Msg::User(first) = &seen[0][0] else {
            panic!("a user message first")
        };
        assert!(
            first.contains("<song>") && first.contains("`kit`") && first.contains("add a snare")
        );
        let Msg::Results(r) = &seen[1][2] else {
            panic!("the check's result: {:?}", seen[1])
        };
        assert!(
            r[0].is_error && r[0].content.contains("a pad is"),
            "{:?}",
            r[0]
        );
    }

    #[tokio::test]
    async fn a_song_that_does_not_parse_is_not_proposed() {
        let p = Scripted::new(vec![
            Ok(calls(vec![(
                "propose_song",
                json!({"song": BAD, "summary": "x"}),
            )])),
            Ok(calls(vec![(
                "propose_song",
                json!({"song": GOOD, "summary": "fixed"}),
            )])),
        ]);
        let ev = events(&p, LoopLimits::default()).await;
        assert!(
            ev.iter().any(
                |e| matches!(e, Event::Tool { name, ok: false, .. } if name == "propose_song")
            )
        );
        assert!(ev.contains(&Event::Song {
            song: GOOD.into(),
            summary: "fixed".into()
        }));
    }

    /// With the track `kit` in focus (#415), a song that also changes the
    /// tempo is refused with what changed; one that changes only `kit` goes.
    #[tokio::test]
    async fn a_song_beyond_the_focused_track_is_not_proposed() {
        let faster = GOOD.replace("tempo 120", "tempo 128");
        let snare = format!("{GOOD}  sn ....x.......x...\n");
        let p = Scripted::new(vec![
            Ok(calls(vec![(
                "propose_song",
                json!({"song": faster, "summary": "x"}),
            )])),
            Ok(calls(vec![(
                "propose_song",
                json!({"song": snare, "summary": "a snare"}),
            )])),
        ]);
        let ev = events(&p, LoopLimits::default()).await;
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::Tool { name, ok: false, summary, .. }
                if name == "propose_song" && summary.contains("tempo"))),
            "{ev:?}"
        );
        assert!(ev.contains(&Event::Song {
            song: snare,
            summary: "a snare".into()
        }));
    }

    #[tokio::test]
    async fn the_loop_stops_at_its_rounds_and_on_errors() {
        let busy: Vec<_> = (0..5)
            .map(|_| Ok(calls(vec![("check_song", json!({"song": GOOD}))])))
            .collect();
        let ev = events(
            &Scripted::new(busy),
            LoopLimits {
                rounds: 2,
                ..LoopLimits::default()
            },
        )
        .await;
        assert!(ev.contains(&Event::Error {
            message: "no song after 2 rounds".into(),
            retryable: false
        }));
        assert!(matches!(ev.last(), Some(Event::Done { rounds: 2, .. })));

        let ev = events(
            &Scripted::new(vec![Err(ProviderError::Fatal("bad key".into()))]),
            LoopLimits::default(),
        )
        .await;
        assert!(ev.contains(&Event::Error {
            message: "bad key".into(),
            retryable: false
        }));

        let refused = Turn {
            stop: Stop::Refusal,
            ..calls(vec![])
        };
        let ev = events(&Scripted::new(vec![Ok(refused)]), LoopLimits::default()).await;
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::Error { message, .. } if message.contains("declined")))
        );

        let answer = Turn {
            text: "A kick on every beat.".into(),
            stop: Stop::End,
            ..calls(vec![])
        };
        let ev = events(&Scripted::new(vec![Ok(answer)]), LoopLimits::default()).await;
        assert!(ev.contains(&Event::Text {
            text: "A kick on every beat.".into()
        }));
        assert!(
            !ev.iter()
                .any(|e| matches!(e, Event::Song { .. } | Event::Error { .. })),
            "an answer, no song"
        );
    }

    #[tokio::test]
    async fn bad_tool_input_is_told_to_the_model() {
        let p = Scripted::new(vec![
            Ok(calls(vec![
                ("check_song", json!("{not json")),
                ("nope", json!({"song": GOOD})),
            ])),
            Ok(calls(vec![(
                "propose_song",
                json!({"song": GOOD, "summary": ""}),
            )])),
        ]);
        let ev = events(&p, LoopLimits::default()).await;
        let bad: Vec<_> = ev
            .iter()
            .filter(|e| matches!(e, Event::Tool { ok: false, .. }))
            .collect();
        assert_eq!(bad.len(), 2, "{ev:?}");
    }

    #[test]
    fn the_system_prompt_holds_the_language_and_the_catalog() {
        let s = system_prompt();
        assert!(
            s.contains("<language>") && s.contains("## frag"),
            "the language"
        );
        assert!(
            s.contains("Tr909") && s.contains("Kit909") && s.contains("Cutoff 20..20000"),
            "the catalog"
        );
        assert!(s.contains("phrygian-dominant"));
        assert_eq!(
            system_prompt().as_ptr(),
            s.as_ptr(),
            "built once, the same every request"
        );
    }

    #[test]
    fn events_serialise_as_the_contract_says() {
        let e = Event::Tool {
            round: 1,
            name: "check_song".into(),
            ok: true,
            summary: "parses".into(),
        };
        assert_eq!(e.name(), "tool");
        assert_eq!(
            e.data(),
            json!({"round": 1, "name": "check_song", "ok": true, "summary": "parses"})
        );
        let d = Event::Done {
            rounds: 2,
            seconds: 1.5,
            usage: Usage {
                input: 1,
                cached: 2,
                output: 3,
            },
        };
        assert_eq!(
            d.data(),
            json!({"rounds": 2, "seconds": 1.5, "usage": {"input": 1, "cached": 2, "output": 3}})
        );
    }
}
