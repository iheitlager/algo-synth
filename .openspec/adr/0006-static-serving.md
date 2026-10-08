# 0006: Static serving with Podman and Caddy

**Status:** Accepted, amended by 0028 · **Date:** 2026-09-30

## Context

The server does no audio work (ADR-0001). It needs to serve `index.html`, the bundle, `worklet.js` and `dsp.wasm`, the latter as `application/wasm` so `WebAssembly.compileStreaming` accepts it. AudioWorklet needs a secure context: `localhost` qualifies, any other host needs HTTPS.

## Decision

**A three-stage `Containerfile`:** Rust builds the wasm, Node builds the Vue bundle with it, **Caddy** serves the result. `make serve` runs it on **localhost:6340**; `make dev` runs Vite on **6341** (the 63xx range stays clear of the usual 3000/5173/8080 dev servers). `worklet.js` and `dsp.wasm` are sent with `Cache-Control: no-cache`, because a cached worklet survives a rebuild. No backend, no accounts, no state on the server; songs are saved as files and in `localStorage`.

## Consequences

- Deploying elsewhere means TLS (Caddy does it automatically for a real hostname).
- Base images are pulled by tag for now; pin by digest before anything is public.
- **Amended by ADR-0028:** an assist server runs beside Caddy for the language model, and everything algo-synth serves binds to `127.0.0.1` only; serving another host is a later decision that brings authentication.
