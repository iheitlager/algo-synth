# 008: The assistant

A language model writes and changes the song (ADR-0012), through the assist server beside Caddy (ADR-0028). Epic #381. Decisions: ADR-0001, ADR-0012, ADR-0028.

Common to every requirement: the server serves `127.0.0.1` only, so there is no authentication (ADR-0028). Keys come from the environment and never appear in a response, an event or a log. Tests never call a provider: they use scripted providers, recorded reply shapes and a local mock.

### Requirement 1: Song tools [MUST]

The `algo-assist` crate SHALL offer, on the engine itself, `check` (the engine's parser: the canonical song with its tracks, fragments, sections and bars, or the first error with its line and column), `render` (the song played offline up to its arrangement's bars, four without one, within a bar and a wall-time limit, measured for non-finite samples, peak, RMS, stereo width, RMS per arrangement entry, each track's peak and level on its strip and each Modular SynthDef's build) and `catalog` (the models with their engines, voices and presets, the pads, every parameter with its range and scope, the scales, the insert and processor types), and an `assist check|render|catalog [file]` command that prints them as JSON (#384).

**Implementation:** `crates/assist/src/tools.rs::check`, `crates/assist/src/tools.rs::render`, `crates/assist/src/tools.rs::catalog`, `crates/assist/src/main.rs`

#### Scenario: a silent section reads silent

- GIVEN a loud bar, a silent bar and a loud bar in an arrangement
- WHEN it is rendered
- THEN the middle entry's RMS is below a fifth of the others', and a track no section plays reads zero

**Tests:** `crates/assist/src/tools.rs::tests::check_gives_the_canonical_song_or_where_it_failed`, `crates/assist/src/tools.rs::tests::render_measures_each_section_and_track`, `crates/assist/src/tools.rs::tests::render_keeps_to_its_limits_and_reports_a_bad_song`, `crates/assist/src/tools.rs::tests::render_reports_a_synthdef`, `crates/assist/src/tools.rs::tests::the_catalog_is_the_engines`

### Requirement 2: Providers behind one interface [MUST]

The server SHALL call a model through one `Provider` trait over three wire formats: Anthropic Messages, OpenAI-compatible chat completions and Gemini `generateContent`. These serve five providers: Anthropic, Mistral, Google, OpenRouter and a self-hosted model at a configured URL (#385). Each assistant turn SHALL go back to its provider as it came, thinking blocks and thought signatures included. The Anthropic request SHALL cache its system prompt, use adaptive thinking at an explicit effort, and leave `tool_choice` at `auto`. A provider SHALL be offered only when its key (self-hosted: its URL) is set, with the models of `providers.json` as its allowlist. A transient failure (a timeout, 429 or a 5xx) SHALL be retried twice; any other failure ends the request.

**Implementation:** `crates/assist/src/provider.rs::Provider`, `crates/assist/src/provider/anthropic.rs`, `crates/assist/src/provider/openai.rs`, `crates/assist/src/provider/gemini.rs`, `crates/assist/src/config.rs::Config`, `crates/assist/providers.json`

#### Scenario: a turn goes back as it came

- GIVEN an Anthropic turn with a thinking block and a tool call
- WHEN the next request is built
- THEN its content is the turn's, unchanged, and the system prompt carries `cache_control`

**Tests:** `crates/assist/src/provider/anthropic.rs::tests::the_body_caches_the_system_prompt_and_replays_turns_as_they_came`, `crates/assist/src/provider/anthropic.rs::tests::a_reply_parses_into_text_calls_and_usage`, `crates/assist/src/provider/openai.rs::tests::the_body_has_the_system_first_and_a_tool_message_per_result`, `crates/assist/src/provider/openai.rs::tests::a_completion_parses_its_calls_even_with_bad_arguments`, `crates/assist/src/provider/gemini.rs::tests::the_body_declares_functions_and_answers_them`, `crates/assist/src/provider/gemini.rs::tests::a_reply_parses_calls_and_skips_thoughts`, `crates/assist/src/config.rs::tests::only_providers_with_their_variables_are_offered`, `crates/assist/src/config.rs::tests::the_public_config_carries_no_key_or_base`

### Requirement 3: The loop [MUST]

For a request, the model SHALL get a system prompt holding how to work, the language (`.openspec/language.md`, #383) and the catalog, all the same on every request so it caches. Its first message SHALL hold the song, the fragment in focus and the request. Its tools SHALL be `check_song`, `render_song` and `propose_song`. A proposed song that does not parse SHALL go back to the model as an error; one that parses SHALL end the loop as a `song` event. A turn without tool calls SHALL end it as an answer. The loop SHALL stop at a refusal, a fatal provider error, its round limit (8) or its time limit (10 minutes), and SHALL always end with `done`, which carries the rounds, the seconds and the tokens (input, cached, output).

**Implementation:** `crates/assist/src/assist.rs::run`, `crates/assist/src/assist.rs::system_prompt`, `crates/assist/src/assist.rs::tool_defs`, `crates/assist/src/assist.rs::Event`

#### Scenario: a parse error is fixed

- GIVEN a model that first writes a song with an unknown pad, then renders and proposes the fixed one
- WHEN the loop runs
- THEN the check's error goes back to the model, the fixed song is the `song` event, and `done` counts three rounds and their tokens

**Tests:** `crates/assist/src/assist.rs::tests::a_parse_error_goes_back_and_the_fix_is_proposed`, `crates/assist/src/assist.rs::tests::a_song_that_does_not_parse_is_not_proposed`, `crates/assist/src/assist.rs::tests::the_loop_stops_at_its_rounds_and_on_errors`, `crates/assist/src/assist.rs::tests::bad_tool_input_is_told_to_the_model`, `crates/assist/src/assist.rs::tests::the_system_prompt_holds_the_language_and_the_catalog`, `crates/assist/src/assist.rs::tests::events_serialise_as_the_contract_says`

### Requirement 4: The server [MUST]

`assist serve` SHALL listen on `ASSIST_BIND` (default `127.0.0.1:6342`) and answer the following:
- `GET /api/health`;
- `GET /api/providers`: the offered providers and models and the default, with no key or base;
- `POST /api/assist` with the song, the request, a provider, a model and an optional focus: the loop's steps as server-sent events named `progress`, `tool`, `text`, `song`, `error` and `done`.

Before the stream it SHALL refuse a request:
- `503` without providers;
- `400` for a provider or model not offered, a body that is not the request, or an empty request or one over 4000 characters;
- `429` when a request is running already or twenty started this minute.

Bodies are limited to 2 MB. When the browser closes the stream (Stop, a closed tab), the request's loop SHALL be dropped at once, its provider call with it, so a request nobody reads costs nothing more.

**Implementation:** `crates/assist/src/server.rs::router`, `crates/assist/src/server.rs::AppState`, `crates/assist/src/main.rs`

#### Scenario: a request through the real adapter

- GIVEN the server pointed at a local mock of the Messages API that answers a check, then a proposal
- WHEN a request is posted
- THEN the stream is `progress`, `text`, `tool`, `progress`, `tool`, `song`, `done`, the mock saw the cached system prompt and the first turn sent back, and no key is in the stream

**Tests:** `crates/assist/src/server.rs::tests::health_and_providers`, `crates/assist/src/server.rs::tests::requests_are_refused_before_the_stream`, `crates/assist/src/server.rs::tests::a_request_streams_its_steps_through_the_real_adapter`, `crates/assist/src/server.rs::tests::a_closed_stream_stops_its_loop`

### Requirement 5: Keys and serving [MUST]

The keys SHALL come from 1Password and never be stored in git. `op.env` holds `op://` references only; `op run --env-file=op.env` fills them in for one process; `make env-check` says which resolve, ok or missing, never a value. `.env` files are ignored by git. `make assist` SHALL run the server on `127.0.0.1:6342` with the keys from 1Password; `make dev` proxies `/api` to it.

For `make serve`, the assist server SHALL run as its own image beside Caddy, on a private podman network, with no port published on the host. Caddy SHALL proxy `/api/*` to it without buffering, so events stream. Keys SHALL pass into the container by name (`-e NAME`) from the `op run` environment, never on a command line. Without 1Password the server does not start, and the site serves as before.

**Implementation:** `op.env`, `Makefile` (`assist`, `env-check`, `image`, `serve`), `Containerfile` (`assist`), `Caddyfile`, `web/vite.config.ts`

#### Scenario: only Caddy reaches the server

- GIVEN `make serve` without keys
- WHEN the containers run
- THEN Caddy publishes `127.0.0.1:6340` and the assist server publishes nothing; from Caddy, `/api/health` and `/api/providers` answer through the proxy, the latter with no providers

**Tests:** review: `make serve OP=`, then `podman ps` and `podman exec algo-synth wget -qO- http://127.0.0.1:80/api/providers`

