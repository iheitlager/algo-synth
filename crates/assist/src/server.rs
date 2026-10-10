//! algo-synth's one server (ADR-0030, #406): axum on `127.0.0.1:6340`,
//! serving the built app and, under `/api`, the assistant (ADR-0028). The
//! assistant offers the providers it has keys for and runs the loop of
//! `crate::assist` for a request, streaming each step as a server-sent event.
//!
//! Every response is cross-origin isolated (ADR-0029); the worklet, the deck
//! worker and the engine are never cached, so a rebuild is picked up; the
//! rest is compressed, except the event stream.

use crate::assist::{self, Event, LoopLimits, Request};
use crate::config::{Config, Kind};
use crate::provider::anthropic::Anthropic;
use crate::provider::gemini::Gemini;
use crate::provider::openai::OpenAi;
use axum::Json;
use axum::Router;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::sse::{Event as Sse, KeepAlive};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::json;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower_http::set_header::SetResponseHeaderLayer;

/// The largest request body: a song of up to 1 MB and the request.
const BODY_LIMIT: usize = 2 << 20;
/// The longest request text.
const REQUEST_LIMIT: usize = 4000;

/// How much the server takes at once: against runaway loops and cost, not
/// other users (it serves localhost only).
#[derive(Clone, Copy, Debug)]
pub struct Rates {
    /// Requests running at once.
    pub concurrent: usize,
    /// Requests started in any minute.
    pub per_minute: usize,
}

impl Default for Rates {
    fn default() -> Rates {
        Rates {
            concurrent: 2,
            per_minute: 20,
        }
    }
}

pub struct AppState {
    pub config: Config,
    /// The built app (`web/dist`), served at `/`; none serves `/api` only.
    pub web: Option<std::path::PathBuf>,
    pub client: reqwest::Client,
    /// Anthropic's effort for the loop.
    pub effort: String,
    pub limits: LoopLimits,
    running: Arc<Semaphore>,
    started: Mutex<VecDeque<Instant>>,
    per_minute: usize,
}

impl AppState {
    pub fn new(config: Config, effort: String, limits: LoopLimits, rates: Rates) -> AppState {
        AppState {
            config,
            web: None,
            client: reqwest::Client::new(),
            effort,
            limits,
            running: Arc::new(Semaphore::new(rates.concurrent)),
            started: Mutex::new(VecDeque::new()),
            per_minute: rates.per_minute,
        }
    }

    /// Whether another request may start this minute; counts it if so.
    fn admit(&self) -> bool {
        let Ok(mut started) = self.started.lock() else {
            return false;
        };
        let now = Instant::now();
        while started
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60))
        {
            started.pop_front();
        }
        if started.len() >= self.per_minute {
            return false;
        }
        started.push_back(now);
        true
    }
}

/// The files a rebuild changes under the same name: never cached (ADR-0006).
const FRESH: [&str; 3] = ["/worklet.js", "/deck-worker.js", "/dsp.wasm"];

/// `Cache-Control: no-cache` on the files of `FRESH`.
async fn fresh(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let fresh = FRESH.contains(&req.uri().path());
    let mut resp = next.run(req).await;
    if fresh {
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    }
    resp
}

pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/api/health", get(|| async { Json(json!({"ok": true})) }))
        .route("/api/providers", get(providers))
        .route("/api/assist", post(assist))
        .layer(DefaultBodyLimit::max(BODY_LIMIT));
    let app = match &state.web {
        Some(dir) => api.fallback_service(tower_http::services::ServeDir::new(dir)),
        None => api,
    };
    // Cross-origin isolated, so decks can share SharedArrayBuffer rings (ADR-0029).
    let isolate = |name: &'static str, value: &'static str| {
        SetResponseHeaderLayer::overriding(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        )
    };
    app.layer(axum::middleware::from_fn(fresh))
        .layer(isolate("cross-origin-opener-policy", "same-origin"))
        .layer(isolate("cross-origin-embedder-policy", "require-corp"))
        // The default predicate leaves the event stream uncompressed, so it flows.
        .layer(tower_http::compression::CompressionLayer::new())
        .with_state(state)
}

async fn providers(State(s): State<Arc<AppState>>) -> Response {
    Json(s.config.public()).into_response()
}

#[derive(Deserialize)]
struct AssistBody {
    song: String,
    request: String,
    provider: String,
    model: String,
    #[serde(default)]
    focus: Option<String>,
}

fn refuse(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({"error": msg}))).into_response()
}

async fn assist(
    State(s): State<Arc<AppState>>,
    body: Result<Json<AssistBody>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Ok(Json(body)) = body else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "the request must be JSON with song, request, provider and model",
        );
    };
    if s.config.providers.is_empty() {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "no provider is configured: set a key, see README",
        );
    }
    let Some(p) = s.config.find(&body.provider, &body.model).cloned() else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "that provider or model is not offered",
        );
    };
    if body.request.trim().is_empty() || body.request.len() > REQUEST_LIMIT {
        return refuse(
            StatusCode::BAD_REQUEST,
            "the request is empty or longer than 4000 characters",
        );
    }
    let Ok(permit) = s.running.clone().try_acquire_owned() else {
        return refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "a request is already running; wait for it",
        );
    };
    if !s.admit() {
        return refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "too many requests this minute",
        );
    }
    let req = Request {
        song: body.song,
        request: body.request,
        focus: body.focus.filter(|f| !f.trim().is_empty()),
    };
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let state = s.clone();
    tokio::spawn(async move {
        let _permit = permit;
        // When the browser goes away (Stop, a closed tab) the stream closes:
        // the loop is dropped there, its provider call with it, so a request
        // nobody reads costs nothing more.
        let closed = tx.clone();
        let mut emit = |e: Event| {
            tx.send(e).ok();
        };
        let work = async {
            let client = state.client.clone();
            let (limits, model) = (state.limits, body.model);
            let key = p.key.clone().unwrap_or_default();
            match p.kind {
                Kind::Anthropic => {
                    let provider = Anthropic {
                        client,
                        url: p.base,
                        key,
                        model,
                        effort: state.effort.clone(),
                    };
                    assist::run(&provider, &req, limits, &mut emit).await;
                }
                Kind::Openai => {
                    let provider = OpenAi {
                        client,
                        base: p.base,
                        key: p.key,
                        model,
                    };
                    assist::run(&provider, &req, limits, &mut emit).await;
                }
                Kind::Gemini => {
                    let provider = Gemini {
                        client,
                        base: p.base,
                        key,
                        model,
                    };
                    assist::run(&provider, &req, limits, &mut emit).await;
                }
            }
        };
        tokio::select! {
            () = work => {}
            () = closed.closed() => {}
        }
    });
    let stream = UnboundedReceiverStream::new(rx)
        .map(|e| Ok::<_, Infallible>(Sse::default().event(e.name()).data(e.data().to_string())));
    axum::response::Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use http_body_util::BodyExt;
    use std::collections::HashMap;
    use tower::ServiceExt;

    const SONG: &str =
        "tempo 120\ntrack kit drums Tr909 Kit909\nclip beat = kit /16\n  bd x...x...x...x...\n";

    fn state(env: &[(&str, &str)], rates: Rates) -> Arc<AppState> {
        let env: HashMap<String, String> = env
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect();
        Arc::new(AppState::new(
            Config::from_env(&env),
            "high".into(),
            LoopLimits::default(),
            rates,
        ))
    }

    async fn call(
        app: Router,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, String) {
        let req = HttpRequest::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
            .expect("a request");
        let resp = app.oneshot(req).await.expect("an answer");
        let status = resp.status();
        let bytes = resp.into_body().collect().await.expect("a body").to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    fn ask(provider: &str, model: &str) -> serde_json::Value {
        json!({"song": SONG, "request": "add a snare", "provider": provider, "model": model, "focus": "kit"})
    }

    #[tokio::test]
    async fn health_and_providers() {
        let s = state(&[("ANTHROPIC_API_KEY", "sk-secret")], Rates::default());
        let (st, body) = call(router(s.clone()), "GET", "/api/health", None).await;
        assert_eq!((st, body.as_str()), (StatusCode::OK, r#"{"ok":true}"#));
        let (st, body) = call(router(s), "GET", "/api/providers", None).await;
        assert_eq!(st, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).expect("json");
        assert_eq!(v["providers"][0]["id"], "anthropic");
        assert_eq!(v["default"]["model"], "claude-opus-5-5");
        assert!(!body.contains("sk-secret"));
    }

    #[tokio::test]
    async fn requests_are_refused_before_the_stream() {
        let none = state(&[], Rates::default());
        let (st, _) = call(
            router(none),
            "POST",
            "/api/assist",
            Some(ask("anthropic", "claude-opus-5-5")),
        )
        .await;
        assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
        let s = state(&[("ANTHROPIC_API_KEY", "k")], Rates::default());
        let (st, _) = call(
            router(s.clone()),
            "POST",
            "/api/assist",
            Some(ask("anthropic", "gpt-9")),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "only the allowlist");
        let (st, _) = call(
            router(s.clone()),
            "POST",
            "/api/assist",
            Some(json!({"song": 1})),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        let long = json!({"song": SONG, "request": "x".repeat(5000), "provider": "anthropic", "model": "claude-opus-5-5"});
        let (st, _) = call(router(s), "POST", "/api/assist", Some(long)).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        let busy = state(
            &[("ANTHROPIC_API_KEY", "k")],
            Rates {
                concurrent: 0,
                per_minute: 20,
            },
        );
        let (st, _) = call(
            router(busy),
            "POST",
            "/api/assist",
            Some(ask("anthropic", "claude-opus-5-5")),
        )
        .await;
        assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    }

    /// A fake Anthropic answering in turn: a check, then a proposal. The
    /// server's real adapter talks to it, and the stream carries the steps.
    #[tokio::test]
    async fn a_request_streams_its_steps_through_the_real_adapter() {
        let turns = Arc::new(Mutex::new(vec![
            json!({"content": [{"type": "tool_use", "id": "t2", "name": "propose_song", "input": {"song": SONG, "summary": "a kick"}}],
                   "stop_reason": "tool_use", "usage": {"input_tokens": 10, "cache_read_input_tokens": 9000, "output_tokens": 20}}),
            json!({"content": [{"type": "text", "text": "Checking."}, {"type": "tool_use", "id": "t1", "name": "check_song", "input": {"song": SONG}}],
                   "stop_reason": "tool_use", "usage": {"input_tokens": 9010, "cache_creation_input_tokens": 0, "output_tokens": 20}}),
        ]));
        let seen = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let (t, s2) = (turns.clone(), seen.clone());
        let fake = Router::new().route(
            "/v1/messages",
            post(
                move |headers: axum::http::HeaderMap, Json(body): Json<serde_json::Value>| {
                    let (t, s2) = (t.clone(), s2.clone());
                    async move {
                        assert_eq!(
                            headers.get("x-api-key").and_then(|v| v.to_str().ok()),
                            Some("sk-test")
                        );
                        s2.lock().unwrap().push(body);
                        Json(t.lock().unwrap().pop().unwrap_or(json!({})))
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a port");
        let addr = listener.local_addr().expect("an address");
        tokio::spawn(async move { axum::serve(listener, fake).await });

        let mut config =
            Config::from_env(&[("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string())].into());
        config.providers[0].base = format!("http://{addr}/v1/messages");
        let s = Arc::new(AppState::new(
            config,
            "high".into(),
            LoopLimits::default(),
            Rates::default(),
        ));
        // Asked for gzip, as a browser does: the stream still comes plain.
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/api/assist")
            .header("content-type", "application/json")
            .header("accept-encoding", "gzip")
            .body(Body::from(ask("anthropic", "claude-opus-5-5").to_string()))
            .expect("a request");
        let resp = router(s).oneshot(req).await.expect("an answer");
        let st = resp.status();
        assert_eq!(
            resp.headers().get("content-encoding"),
            None,
            "the event stream is not compressed"
        );
        let body =
            String::from_utf8_lossy(&resp.into_body().collect().await.expect("a body").to_bytes())
                .into_owned();
        assert_eq!(st, StatusCode::OK);
        let names: Vec<&str> = body
            .lines()
            .filter_map(|l| l.strip_prefix("event: "))
            .collect();
        assert_eq!(
            names,
            [
                "progress", "text", "tool", "progress", "tool", "song", "done"
            ],
            "{body}"
        );
        assert!(body.contains(r#""summary":"a kick""#));
        assert!(body.contains(r#""cached":9000"#));
        assert!(!body.contains("sk-test"));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0]["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(
            seen[1]["messages"][1]["content"][1]["name"], "check_song",
            "the turn went back"
        );
    }

    /// Stop in the browser closes the stream: the loop is dropped and its
    /// place freed at once, so the next request runs, while the first model
    /// call was still waiting.
    #[tokio::test]
    async fn a_closed_stream_stops_its_loop() {
        let fake = Router::new().route(
            "/v1/messages",
            post(|| async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Json(json!({}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a port");
        let addr = listener.local_addr().expect("an address");
        tokio::spawn(async move { axum::serve(listener, fake).await });
        let mut config =
            Config::from_env(&[("ANTHROPIC_API_KEY".to_string(), "k".to_string())].into());
        config.providers[0].base = format!("http://{addr}/v1/messages");
        let one = Rates {
            concurrent: 1,
            per_minute: 20,
        };
        let s = Arc::new(AppState::new(
            config,
            "high".into(),
            LoopLimits::default(),
            one,
        ));
        let req = || {
            HttpRequest::builder()
                .method("POST")
                .uri("/api/assist")
                .header("content-type", "application/json")
                .body(Body::from(ask("anthropic", "claude-opus-5-5").to_string()))
                .expect("a request")
        };
        let first = router(s.clone()).oneshot(req()).await.expect("an answer");
        assert_eq!(first.status(), StatusCode::OK);
        drop(first);
        let mut admitted = false;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let again = router(s.clone()).oneshot(req()).await.expect("an answer");
            if again.status() == StatusCode::OK {
                admitted = true;
                break;
            }
        }
        assert!(admitted, "the first loop still holds its place");
    }

    /// The built app and `/api` from one server (#406): the engine and its
    /// workers never cached, every response cross-origin isolated, files
    /// compressed, the event stream not.
    #[tokio::test]
    async fn one_server_serves_the_app_and_the_api() {
        let dir = std::env::temp_dir().join(format!("algo-synth-web-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a dir");
        std::fs::write(
            dir.join("index.html"),
            "<!doctype html><title>algo-synth</title>",
        )
        .expect("index");
        std::fs::write(
            dir.join("assistant.html"),
            "<!doctype html><title>assistant</title>",
        )
        .expect("assistant");
        std::fs::write(dir.join("dsp.wasm"), b"\0asm\x01\0\0\0").expect("wasm");
        std::fs::write(
            dir.join("app.js"),
            "console.log('algo-synth');\n".repeat(400),
        )
        .expect("js");
        let mut state = AppState::new(
            Config::from_env(&HashMap::new()),
            "high".into(),
            LoopLimits::default(),
            Rates::default(),
        );
        state.web = Some(dir.clone());
        let app = router(Arc::new(state));
        let get = |uri: &str, gzip: bool| {
            let mut b = HttpRequest::builder().uri(uri);
            if gzip {
                b = b.header("accept-encoding", "gzip");
            }
            app.clone()
                .oneshot(b.body(Body::empty()).expect("a request"))
        };
        let h = |r: &Response, k: &str| {
            r.headers()
                .get(k)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string()
        };

        let wasm = get("/dsp.wasm", false).await.expect("an answer");
        assert_eq!(wasm.status(), StatusCode::OK);
        assert_eq!(h(&wasm, "content-type"), "application/wasm");
        assert_eq!(h(&wasm, "cache-control"), "no-cache");
        assert_eq!(h(&wasm, "cross-origin-opener-policy"), "same-origin");
        assert_eq!(h(&wasm, "cross-origin-embedder-policy"), "require-corp");

        let index = get("/", false).await.expect("an answer");
        assert_eq!(index.status(), StatusCode::OK);
        assert!(h(&index, "content-type").starts_with("text/html"));
        assert_eq!(
            h(&index, "cache-control"),
            "",
            "the page may be cached as usual"
        );
        let body = index
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("<title>algo-synth</title>"));

        assert_eq!(
            get("/assistant.html", false)
                .await
                .expect("an answer")
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            get("/nowhere.js", false).await.expect("an answer").status(),
            StatusCode::NOT_FOUND
        );
        let health = get("/api/health", false).await.expect("an answer");
        assert_eq!(health.status(), StatusCode::OK);
        assert_eq!(h(&health, "cross-origin-embedder-policy"), "require-corp");

        let js = get("/app.js", true).await.expect("an answer");
        assert_eq!(h(&js, "content-encoding"), "gzip", "files are compressed");
        std::fs::remove_dir_all(&dir).ok();
    }
}
