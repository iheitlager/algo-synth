# 008: The assistant

A language model writes and changes the song (ADR-0012), through the assistant of algo-synth's one server (ADR-0028, ADR-0030). Epic #381. Decisions: ADR-0001, ADR-0012, ADR-0028.

Common to every requirement: the server serves `127.0.0.1` only, so there is no authentication (ADR-0028). Keys come from the environment and never appear in a response, an event or a log. Tests never call a provider: they use scripted providers, recorded reply shapes and a local mock.

### Requirement 1: Song tools [MUST]

The `algo-assist` crate SHALL offer, on the engine itself, `check` (the engine's parser: the canonical song with its tracks, fragments, sections and bars, or the first error with its line and column), `render` (the song played offline up to its arrangement's bars, four without one, within a bar and a wall-time limit, measured for non-finite samples, peak, RMS, stereo width, RMS per arrangement entry, each track's peak and level on its strip and each Modular SynthDef's build) and `catalog` (the models with their engines, voices and presets, the pads, every parameter with its range and scope, the scales, the insert and processor types), and an `assist check|render|catalog [file]` command that prints them as JSON (#384).

A character the SynthDef reader refuses SHALL be named in the error. `audition` SHALL add a fragment to a track that has none, in a song without an arrangement: a phrase over two octaves on a synth or sampler track, a beat on a drums track (#430). `plays` SHALL tell whether a track sounds: not muted or out-soloed, with a fragment that loops (no arrangement) or sits in an arranged section.

**Implementation:** `crates/assist/src/tools.rs::check`, `crates/assist/src/tools.rs::render`, `crates/assist/src/tools.rs::catalog`, `crates/assist/src/tools.rs::audition`, `crates/assist/src/tools.rs::plays`, `crates/assist/src/main.rs`

#### Scenario: a silent section reads silent

- GIVEN a loud bar, a silent bar and a loud bar in an arrangement
- WHEN it is rendered
- THEN the middle entry's RMS is below a fifth of the others', and a track no section plays reads zero

**Tests:** `crates/assist/src/tools.rs::tests::check_gives_the_canonical_song_or_where_it_failed`, `crates/assist/src/tools.rs::tests::render_measures_each_section_and_track`, `crates/assist/src/tools.rs::tests::render_keeps_to_its_limits_and_reports_a_bad_song`, `crates/assist/src/tools.rs::tests::render_reports_a_synthdef`, `crates/assist/src/tools.rs::tests::the_catalog_is_the_engines`, `crates/assist/src/tools.rs::tests::a_character_sclang_does_not_take_is_named`, `crates/assist/src/tools.rs::tests::a_track_without_fragments_is_auditioned`, `crates/assist/src/tools.rs::tests::a_track_plays_when_a_section_holds_its_fragment`

### Requirement 2: Providers behind one interface [MUST]

The server SHALL call a model through one `Provider` trait over three wire formats: Anthropic Messages, OpenAI-compatible chat completions and Gemini `generateContent`. These serve five providers: Anthropic, Mistral, Google, OpenRouter and a self-hosted model at a configured URL (#385). Each assistant turn SHALL go back to its provider as it came, thinking blocks and thought signatures included. The Anthropic request SHALL cache its system prompt, use adaptive thinking at an explicit effort, and leave `tool_choice` at `auto`. A provider SHALL be offered only when its key (self-hosted: its URL) is set, with the models of `providers.json` as its allowlist. A transient failure (a timeout, 429 or a 5xx) SHALL be retried twice; any other failure ends the request.

**Implementation:** `crates/assist/src/provider.rs::Provider`, `crates/assist/src/provider/anthropic.rs`, `crates/assist/src/provider/openai.rs`, `crates/assist/src/provider/gemini.rs`, `crates/assist/src/config.rs::Config`, `crates/assist/providers.json`

#### Scenario: a turn goes back as it came

- GIVEN an Anthropic turn with a thinking block and a tool call
- WHEN the next request is built
- THEN its content is the turn's, unchanged, and the system prompt carries `cache_control`

**Tests:** `crates/assist/src/provider/anthropic.rs::tests::the_body_caches_the_system_prompt_and_replays_turns_as_they_came`, `crates/assist/src/provider/anthropic.rs::tests::a_reply_parses_into_text_calls_and_usage`, `crates/assist/src/provider/openai.rs::tests::the_body_has_the_system_first_and_a_tool_message_per_result`, `crates/assist/src/provider/openai.rs::tests::a_completion_parses_its_calls_even_with_bad_arguments`, `crates/assist/src/provider/gemini.rs::tests::the_body_declares_functions_and_answers_them`, `crates/assist/src/provider/gemini.rs::tests::a_reply_parses_calls_and_skips_thoughts`, `crates/assist/src/config.rs::tests::only_providers_with_their_variables_are_offered`, `crates/assist/src/config.rs::tests::the_public_config_carries_no_key_or_base`

### Requirement 3: The loop [MUST]

For a request, the model SHALL get a system prompt holding how to work, the language (`.openspec/language.md`, #383) and the catalog, all the same on every request so it caches. Its first message SHALL hold the song, the track in focus and the request. Its tools SHALL be `check_song`, `render_song` and `propose_song`. `render_song` SHALL parse the song too, so the prompt does not ask for `check_song` first. With a track in focus that has no fragment, in a song without an arrangement, `render_song` SHALL play an audition on it and say so (`auditioned`) (#430). A proposed song that does not parse SHALL go back to the model as an error. With a track in focus (#415), a proposed song that changes anything but that track (its `track` line, its strip line, its frags, and autos, mods and scene values on it) SHALL go back to the model as an error naming what changed. A proposed song SHALL then be rendered as `render_song` does, and one with non-finite samples, a peak above 1.0, or a track in focus that should sound (auditioned, or one of its fragments plays) but is silent SHALL go back to the model as an error with the render's measures (#430). These checks, with the structure rules between scope and render, are the gate of Requirement 7. Any other song SHALL end the loop as a `song` event. A turn without tool calls SHALL end it as an answer. The loop SHALL stop at a refusal, a fatal provider error, its round limit (8) or its time limit (10 minutes), and SHALL always end with `done`, which carries the rounds, the seconds and the tokens (input, cached, output).

**Implementation:** `crates/assist/src/assist.rs::run`, `crates/assist/src/scope.rs::check`, `crates/assist/src/assist.rs::system_prompt`, `crates/assist/src/assist.rs::tool_defs`, `crates/assist/src/assist.rs::Event`

#### Scenario: a parse error is fixed

- GIVEN a model that first writes a song with an unknown pad, then renders and proposes the fixed one
- WHEN the loop runs
- THEN the check's error goes back to the model, the fixed song is the `song` event, and `done` counts three rounds and their tokens

**Tests:** `crates/assist/src/assist.rs::tests::a_parse_error_goes_back_and_the_fix_is_proposed`, `crates/assist/src/assist.rs::tests::a_song_that_does_not_parse_is_not_proposed`, `crates/assist/src/assist.rs::tests::the_loop_stops_at_its_rounds_and_on_errors`, `crates/assist/src/assist.rs::tests::bad_tool_input_is_told_to_the_model`, `crates/assist/src/assist.rs::tests::the_system_prompt_holds_the_language_and_the_catalog`, `crates/assist/src/assist.rs::tests::events_serialise_as_the_contract_says`

#### Scenario: the focused track is all that changes

- GIVEN the track `kit` in focus and a model that first proposes the song at a new tempo, then one with only a snare added to `kit`'s frag
- WHEN the loop runs
- THEN the first proposal goes back as an error naming the tempo, and the second is the `song` event

**Tests:** `crates/assist/src/assist.rs::tests::a_song_beyond_the_focused_track_is_not_proposed`, `crates/assist/src/scope.rs::tests::the_focused_track_may_change`, `crates/assist/src/scope.rs::tests::anything_else_is_named_and_refused`, `crates/assist/src/scope.rs::tests::no_such_track_holds_nothing`

#### Scenario: an instrument is heard and only a clean song goes

- GIVEN the track `lead` in focus, with no fragment, in a song without an arrangement
- WHEN the model renders and proposes the song as it is
- THEN the render plays an audition on `lead` and says so, and the song is proposed; a song whose track in focus is silent, or that clips, goes back as an error

**Tests:** `crates/assist/src/assist.rs::tests::an_instrument_without_fragments_is_auditioned`, `crates/assist/src/assist.rs::tests::a_silent_track_in_focus_is_not_proposed`, `crates/assist/src/assist.rs::tests::clipping_and_non_finite_samples_are_faults`

### Requirement 4: The server [MUST]

`algo-synth serve` SHALL be algo-synth's one server (ADR-0030): it listens on `ALGO_BIND` (default `127.0.0.1:6340`) and serves both the app and the assistant on that port.

The app comes from `ALGO_WEB` (default `web/dist`): `/` the app, `/assistant.html` the Assistant's window, an unknown path 404. Every response SHALL be cross-origin isolated (COOP `same-origin`, COEP `require-corp`, ADR-0029). The worklet, the deck worker and the engine SHALL be sent `no-cache`, and `.wasm` as `application/wasm`. Responses SHALL be compressed, except the event stream.

The assistant answers:
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

**Tests:** `crates/assist/src/server.rs::tests::health_and_providers`, `crates/assist/src/server.rs::tests::requests_are_refused_before_the_stream`, `crates/assist/src/server.rs::tests::a_request_streams_its_steps_through_the_real_adapter`, `crates/assist/src/server.rs::tests::a_closed_stream_stops_its_loop`, `crates/assist/src/server.rs::tests::one_server_serves_the_app_and_the_api`

### Requirement 5: Keys and serving [MUST]

The keys SHALL come from 1Password and never be stored in git:
- `op.env` holds `op://` references only, and `op run --env-file=op.env` fills them in for one process;
- `make env-check` says which resolve, ok or missing, never a value;
- `.env` files are ignored by git.

`make dev` SHALL run `algo-synth serve` on `127.0.0.1:6341` with the keys from 1Password, while `vite build --watch` keeps `web/dist` current. `make serve` SHALL run one image, the binary and the built app, published on the host's `127.0.0.1:6340` only. Keys SHALL pass in by name (`-e NAME`) from `op run`'s environment, never on a command line. Without 1Password both serve the app, and the assistant offers no provider.

**Implementation:** `op.env`, `Makefile` (`dev`, `env-check`, `image`, `serve`), `Containerfile`

#### Scenario: one container, one port

- GIVEN `make serve` without keys
- WHEN the container runs
- THEN it publishes `127.0.0.1:6340` only, and from inside it `/`, `/dsp.wasm` and `/api/providers` answer, the last with no providers

**Tests:** review: `make serve OP=`, then `podman ps` and requests from inside the container

### Requirement 6: An eval set [SHOULD]

The assistant SHALL have an eval set: about thirty requests on starting songs from `examples/songs/`, or on a song given in the case.
- **What they ask:** add a part, change a groove, re-harmonise, make a section build, write a song from scratch, fix a song that does not load, or answer a question without changing anything.
- **Grading, by code:** a song expected SHALL be proposed, parse and render clean (no non-finite samples, peak at most 1.0), and then pass the case's checks: fragments and tracks kept, tempo, swing, more lanes of a pad, more fragments, tracks or sections, a track on a model, the text holding something. A question SHALL be answered in words with no song.
- **The runner:** `assist eval --provider ID --model ID` SHALL run the cases through the real loop and report, per case and in total, the pass, the gate's rules that fired (#453), the rounds, the tokens (input, cached, output), the seconds and, where the prices are known, the cost.

Every run calls the provider and costs money, so it runs on request and never in CI (#388).

**Implementation:** `crates/assist/eval/cases.json`, `crates/assist/src/eval.rs::grade`, `crates/assist/src/eval.rs::run_case`, `crates/assist/src/eval.rs::markdown`, `crates/assist/src/main.rs`

#### Scenario: the checks grade what they say

- GIVEN a start with one drum fragment and a proposal at 128 BPM with a snare, a bass on an SH-101 and a section
- WHEN it is graded against every check
- THEN all pass, and graded against the start itself, every check that asks for a change fails

**Tests:** `crates/assist/src/eval.rs::tests::the_set_loads_and_its_start_songs_parse`, `crates/assist/src/eval.rs::tests::checks_grade_what_they_say`, `crates/assist/src/eval.rs::tests::a_question_passes_with_words_and_no_song`, `crates/assist/src/eval.rs::tests::cost_and_the_report`, `crates/assist/src/eval.rs::tests::a_case_runs_through_the_loop`

### Requirement 7: The acceptance gate [MUST]

`propose_song` SHALL run a proposed song through one ordered set of named rules (#453). A refusing rule SHALL send the song back to the model as an error carrying the rule ids (`rules`) and a message that says what to change; the browser's `tool` line SHALL carry the same ids. A warning SHALL ride along with the proposal as the `song` event's `warnings` (rule and message), and the review SHALL show them. The structure rules SHALL hold the proposal only to what it adds: a finding the current song already has SHALL not count. The rules, in order:

| Rule | Level | A song breaks it when |
|---|---|---|
| `parse` | refuse | it does not parse |
| `scope` | refuse | with a track in focus, anything else changes (#415) |
| `unused-frag` | refuse | it has an `arrange` and a frag sits in no arranged section |
| `unplayed-track` | refuse | a heard track (not muted, not out-soloed, not in focus) plays no frag |
| `unused-auto` | refuse | it has an `arrange` and an auto lane sits in no arranged section |
| `nonfinite` | refuse | the render has non-finite samples (#430) |
| `clip` | refuse | the render's peak is above 1.0 (#430) |
| `silent-focus` | refuse | the track in focus should sound but is silent (#430) |
| `silent-section` | warn | an arranged section plays no step and no note |
| `unarranged` | warn | it has sections but no `arrange` |
| `removed` | warn | a track or section of the current song is gone |

Without an `arrange`, every frag and auto lane loops, so `unused-frag` and `unused-auto` do not apply. A `mod` is not placed by sections; a frag's own methods go with the frag.

**Implementation:** `crates/assist/src/gate.rs::RULES`, `crates/assist/src/gate.rs::lint`, `crates/assist/src/assist.rs::run_tool`, `crates/assist/src/assist.rs::Event`, `web/src/components/AssistantPane.vue`

#### Scenario: a refusal names its rule and a warning reaches the review

- GIVEN a song with one arranged section and a model that first proposes it with a frag no section plays, then with an empty `gap` section arranged
- WHEN the loop runs
- THEN the first proposal goes back with `unused-frag` and the frag's name, and the second is the `song` event with a `silent-section` warning

**Tests:** `crates/assist/src/assist.rs::tests::the_gate_refuses_with_its_rule_and_passes_warnings_on`, `crates/assist/src/gate.rs::tests::the_song_passes_every_rule`, `crates/assist/src/gate.rs::tests::an_unused_frag_is_refused`, `crates/assist/src/gate.rs::tests::an_unplayed_track_is_refused`, `crates/assist/src/gate.rs::tests::an_unused_auto_is_refused`, `crates/assist/src/gate.rs::tests::a_silent_section_is_a_warning`, `crates/assist/src/gate.rs::tests::sections_without_an_arrangement_are_a_warning`, `crates/assist/src/gate.rs::tests::removed_tracks_and_sections_are_warnings`, `crates/assist/src/gate.rs::tests::what_the_current_song_already_breaks_is_not_held_against_it`, `crates/assist/src/gate.rs::tests::the_rules_are_ordered_and_named_in_the_spec`, `crates/assist/src/assist.rs::tests::clipping_and_non_finite_samples_are_faults`, `crates/assist/src/assist.rs::tests::a_silent_track_in_focus_is_not_proposed`, `web/src/audio/assist.test.ts`

#### Scenario: the eval records the rules that fired

- GIVEN a case whose proposal has sections but no `arrange`
- WHEN it runs through the loop
- THEN its outcome and its report row carry `unarranged`

**Tests:** `crates/assist/src/eval.rs::tests::a_case_runs_through_the_loop`, `crates/assist/src/eval.rs::tests::cost_and_the_report`
