# 0028: An LLM proxy beside Caddy, on localhost only

**Status:** Proposed · **Date:** 2026-10-08 · **Amends:** 0006, 0012

## Context

ADR-0012 decided that a language model writes the song text, with the engine's parser as the check, and held open *where the model call runs*. plan.md (MVP 11) lists three options: (a) the browser calls a provider with a key the user pastes in, (b) a small proxy next to Caddy holds the key, (c) no call from the app, the text pasted in from elsewhere. Option (c) works today and needs no code. Epic #381 asks for the assistant in the app, so (a) or (b).

A key in the browser (a) can be read by any script on the page and lands in `localStorage` or a prompt every session. The loop that makes the result good also wants the engine: check that a song parses, render it offline and look at the levels, fix and try again (#384). That loop belongs where the key is, and where the engine can run natively, not in the browser tab.

ADR-0006 says the server serves static files only: no backend, no accounts, no state. A proxy is the first server code, so it amends that.

So far algo-synth has only been run on the machine it plays on. ADR-0006 named `localhost:6340` and `:6341` but did not make it a rule, and `make serve` published the container on every interface of the host.

## Decision

**A second process, the assist server, beside Caddy, on localhost only.**

- **Localhost only.** Everything algo-synth serves binds to `127.0.0.1`: Caddy's static site (`make serve`, 6340), Vite in dev (6341), and the assist server (6342). A container's port is published as `127.0.0.1:<port>:<port>`, never on all interfaces. **Authentication is therefore out of scope for now:** the only client is the browser on the same machine. Serving to another host (a LAN, a public name, TLS) is a later decision with its own ADR, and it must bring authentication and rate limits per user before any key-holding endpoint is reachable from elsewhere.
- **Rust, axum, a crate in the workspace** (`crates/assist`), so `cargo test`, `cargo deny` and clippy cover it as they cover the engine. It links `algo-dsp` natively for its tools (#384): the parser, the printer, an offline render. ADR-0001 holds: the music logic stays in `crates/dsp`, and the server uses it rather than repeating it. The server is not in the wasm and the engine gains no dependency.
- **Routing.** Caddy proxies `/api/*` to `127.0.0.1:6342` with `flush_interval -1`, so streamed events are not buffered; Vite's `server.proxy` does the same in dev. The page and the API share an origin, so there is no CORS.
- **Endpoints:**
  - `GET /api/health`: the server is up.
  - `GET /api/providers`: the providers this server has keys for, each with its allowlisted models.
  - `POST /api/assist`: the current song text, the request, a provider and a model from that allowlist, and optionally a fragment to focus on. It answers with **server-sent events**: `progress`, `tool`, `text`, `song`, `error`, `done`.

  The browser never sends a prompt template or a free-form model name.
- **Providers behind one trait**, three wire formats:
  - Anthropic Messages, for Anthropic; the default model is `claude-opus-5-5`.
  - OpenAI-compatible chat completions, for Mistral, OpenRouter (GLM and others) and a self-hosted model (Ollama, llama.cpp server, vLLM, LM Studio) at a configured base URL.
  - Gemini `generateContent`, for Google.

  Each speaks raw HTTPS; there is no official Anthropic SDK for Rust. A provider without its key (or, self-hosted, its URL) is not offered.
- **The loop runs on the server.**
  1. The model gets the language (`.openspec/language.md`, #383) and the engine's catalog as a system prompt, cached where the provider allows.
  2. It gets the song and the request.
  3. It calls `check`, `render` and `catalog` as tools.
  4. It ends with a song that checks clean, or stops at a limit.

  The browser applies the result through the engine's parser again (ADR-0012): nothing reaches the engine that did not parse.
- **Keys** come from the environment only, filled by 1Password (`op run --env-file=op.env`, #386). They never appear in a response, an event, a log or the browser.
- **Limits on every request:** rounds of the loop, tokens, wall time, request size, the bars `render` may play, and a simple rate limit. They are there against runaway cost and loops, not against other users; on localhost there are none.
- **Without the server, the app is unchanged.** Caddy alone serves the static site as before; the Assistant (#387) hides when `/api/health` does not answer.

## Consequences

- algo-synth has a backend, but a small one: no accounts, no stored state, no song kept on the server. Songs stay files and `localStorage` (ADR-0006).
- `make serve` runs two processes, Caddy and the assist server (#386), and needs keys from 1Password for the assistant; without them the site still serves.
- The rule that everything binds to `127.0.0.1` closes what ADR-0006 left open: `make serve` no longer publishes 6340 on all interfaces.
- A server that serves another host is a breaking change to this ADR: authentication, per-user limits and TLS come first.
- Provider calls cost money, so tests never make one: a fake provider and recorded transcripts (#385), and the eval (#388) runs only on request.
- The engine stays the only judge of a song. A provider that writes poor songs fails `check` and `render`, not the app.

## Alternatives considered

- **(a) The browser calls the provider with a pasted key.** No server, but the key is exposed to every script on the page and stored in the browser, every provider needs CORS support, and the check-render-fix loop would run in the tab beside the audio. Rejected.
- **(c) Paste only, no call from the app.** Works today and stays possible, but no loop: nothing checks or renders the song before it is pasted. Kept as the fallback when the server is not running.
- **A Python (FastAPI) server**, as in lab271/demo-paragraphica, with the official Anthropic SDK and the song tools behind a native CLI. A second language and toolchain, and the tools a process away from the engine. Rejected for a Rust server linking `algo-dsp`.
- **Authentication now.** Nothing to protect from on localhost; it would only add setup. Deferred to the ADR that serves beyond localhost.
