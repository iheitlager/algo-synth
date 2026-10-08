# 0030: One binary, one server, one port

**Status:** Accepted · **Date:** 2026-10-08 · **Supersedes:** the Caddy serving of 0006 · **Amends:** 0028

## Context

ADR-0006 served the static app with Caddy. ADR-0028 added the assist server beside it, so serving took two processes:
- **In development:** Vite on 6341 and the assist server on 6342, in two terminals, with Vite proxying `/api`.
- **In containers:** Caddy on 6340 and the assist server, joined by a podman network and a reverse proxy, built as two images with the Rust toolchain twice.

On localhost Caddy does nothing the Rust server cannot: static files, a few headers (cross-origin isolation for the decks, ADR-0029; no-cache on the files a rebuild changes; the wasm type), compression. Its strength, automatic TLS for a public host, is out of scope while everything serves `127.0.0.1` (ADR-0028).

## Decision

**One binary, `algo-synth`, and one server, `algo-synth serve`, on one port, 6340, with everything at its own path.**

- **Paths:**
  - `/` is the app and `/assistant.html` the Assistant's window, served from the built app (`web/dist`, `ALGO_WEB`);
  - `/api/*` is the assistant (ADR-0028);
  - an unknown path is a 404.
- **What every response gets:** cross-origin isolation (`Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp`), and compression, except the event stream of `/api/assist`, which must flow. The worklet, the deck worker and the engine are sent `Cache-Control: no-cache`; `.wasm` is `application/wasm`.
- **It binds `127.0.0.1:6340`** (`ALGO_BIND`). In the container it listens on all its interfaces, and the one port is published on the host's `127.0.0.1` only.
- **The binary** keeps the song tools and the eval as subcommands: `check`, `render`, `catalog`, `eval`.
- **One image:** one Rust stage builds the wasm and the server, Node builds the app, a slim runtime holds both. Caddy, the second image, the network and the proxies go.
- **`make dev`** runs the same server on 6341 while `vite build --watch` keeps `web/dist` current: one terminal, one port, a reload shows a change. Vite's own server (6343) stays only for UI work against a fake assistant (`ASSIST_URL`).
- **Keys** still come from 1Password (`op run`, #386). Without them the app serves and the assistant offers no provider.

## Consequences

- One process to start, one port to remember, one image to build. The event stream passes no proxy, so nothing can buffer it.
- Static serving is ours to test: the routes, headers and compression are checked in `crates/assist`'s tests.
- Development loses Vite's hot module reload for the app; a reload shows a change after the watching build.
- Serving beyond localhost brings back a proxy in front for TLS. That decision also brings authentication (ADR-0028).
- `make serve` is 6340 and `make dev` 6341, as before; `make assist` and 6342 are gone.

## Alternatives considered

- **Keep Caddy and start both from one make target.** Fewer terminals, but still two processes, two images and a proxy to keep streaming. Rejected.
- **Caddy and the server in one container under a supervisor.** One image, still two servers. Rejected.
- **Vite's dev server in front, proxying to the Rust server.** Keeps hot reload, but it is two servers again. Kept only for UI work against the fake assistant.
