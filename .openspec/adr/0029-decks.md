# 0029: Decks: several engines, one clock, extra decks in workers

**Status:** Accepted · **Date:** 2026-10-08 · **Amends:** 0001, 0006, 0022

## Context

Epic #391 asks for a DJ mode: several songs loaded at once, one on air, the next faded or morphed in on the beat. The engine plays one song (ADR-0018, ADR-0022), and each wasm instance holds one `Engine`, so N songs means N engines.

Spike #392 measured what N engines cost (M4 Pro; full numbers on the issue):

- One engine at the full 64-voice budget takes about 24% of a core; two take 48–54%, four 98% on average and 108% at the 99th percentile. An idle engine still costs 2–3%.
- Every AudioWorkletNode in one AudioContext renders on the same real-time thread. On that thread, four heavy engines gave 7.9 s of device underruns in 8 s, and two already glitched when every core was busy.
- Engines in Web Workers, each rendering 4 blocks ahead into a SharedArrayBuffer ring that the worklet only reads, gave no underruns with four heavy decks and all 12 cores busy, in Chromium and Firefox, and eight decks on a quiet machine. Rendering ahead adds 4 × 128 samples, 10.7 ms, to anything that changes a deck's sound. No deck ever fell out of step.
- SharedArrayBuffer needs cross-origin isolation (two response headers). The app loads nothing from another origin and runs unchanged with them.
- One `WebAssembly.Memory` shared between workers needs wasm threads, which need nightly Rust; the toolchain is pinned to stable. Each worker holds its own instance and memory: about 8.5 MB of engine plus its samples (at most 64 MiB a store).

ADR-0001 put everything musical in one wasm module on the audio thread. ADR-0022 made the song's clock the only clock. ADR-0006 serves static files with no special headers.

## Decision

**The engine that plays today stays on the audio thread. Up to three more decks each run their own engine in a Web Worker, rendering ahead into a ring the worklet reads. A deck mixer in the worklet's engine sums them, and that engine's clock leads.**

- **The main deck.** The worklet's engine is deck A: the song the composer, the synths and the mixer edit, as today, with no added latency for playing it live. Without other decks the app works exactly as it does now.
- **Worker decks.** Decks B, C and D are each a Web Worker holding its own instance of the same `dsp.wasm` and one `Engine`. A worker deck plays a loaded song; it is not edited live, so its 10.7 ms is not felt. Four slots in all, as epic #391 asks; more waits for numbers on slower machines.
- **Rendering ahead.** A worker deck renders 4 blocks ahead of the audio thread into a single-producer, single-consumer ring in a SharedArrayBuffer: atomics only, no locks. When the ring is full the worker waits on the read counter (`Atomics.wait` is allowed in a worker, never in the worklet). The ring, its block length and its depth are fixed when the deck is created; nothing allocates afterwards.
- **A late deck drops its block and stays on its timeline.** If a deck's block for block n is not ready when the worklet needs it, that deck is silent for block n, the read counter still advances, and its block n+1 plays at n+1. A deck never slips against the others; a late deck is a gap, never a drift.
- **The deck mixer is Rust, in the worklet's engine (ADR-0001).** The worklet shim copies each ready ring block into an input buffer of its engine, as it copies the output out today: bytes, no logic. The engine sums deck A and the input buffers through a deck mixer: a level per deck, an equal-power crossfade between two decks, and a three-band kill per deck. Its gains are computed when a parameter changes, not per sample (ADR-0002). Its parameters go in the registry and its mirror (ADR-0004). After the deck mixer, one limiter keeps the sum from clipping. Each deck keeps its own mixer and master chain: they are part of that song's sound.
- **One clock per instance (amends ADR-0022).** The worklet's engine owns the instance's clock and transport. A worker deck runs its own song clock, slaved: it takes its tempo from the master and starts on a block the master names. Commands to a worker deck carry the block they take effect at; a cue (start on the next bar or the next 8-bar phrase) is far enough ahead to land exactly, and a command without a block takes effect at the next block the worker renders, 4 blocks late. Epic #258 drives the master clock from the network leader, and the decks follow that.
- **Idle decks sleep.** A stopped deck renders nothing once its voices and effect tails have died out; it leaves its ring empty, and the mixer reads silence without counting an underrun.
- **Samples per deck.** Each worker deck loads only the samples its song names (ADR-0018 `samples` lines), into its own engine; the view fetches them once and sends them to every deck that needs them (ADR-0013). A store shared between decks waits for wasm threads on stable Rust.
- **Cross-origin isolation (amends ADR-0006).** Caddy and Vite send `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`. Anything later loaded from another origin has to allow it (CORP or CORS). The app checks `crossOriginIsolated` and, without it, offers deck A only and says why.
- **The real-time rules hold in the worker (ADR-0002).** A worker deck's render loop does not allocate, lock or panic, like the worklet's; only its wait on a full ring blocks, and that is the worker's own thread.
- **Messages.** The view talks to a worker deck through the worker's port, as it talks to the worklet through the node's port. A worker deck posts its playhead and meters to the view the same way.

## Consequences

- Up to four songs play at once without loading the audio thread with more than deck A and the summing; a heavy song on a worker deck cannot make deck A glitch.
- Deck A plays live as it does today. A worker deck answers its knobs 10.7 ms late, so editing a song means making it deck A; there is no moving a running engine between threads.
- A worker deck can be late under heavy load: the result is a dropped block on that deck, audible, but nothing else slips. The view shows each deck's dropped blocks.
- Memory grows by about 8.5 MB per deck plus its samples; four decks with full sample stores could take about 290 MB.
- Two headers are needed wherever the app is served, `make dev` included. A page served without them still works with deck A.
- Specs: spec 001 gains the deck mixer, the ring and the worker deck; spec 002 the slaved clock and the cue; spec 003 the deck view. `make bench` gains a scenario with the deck mixer and three input buffers.
- A second screen (epic #391, stage 7) is a view of the same decks; it sends through the main tab and never holds an engine.

## Alternatives considered

- **All decks on the audio thread.** Simplest, no headers, no latency. #392: two heavy decks already glitch on a busy machine, four do not play. Rejected beyond deck A.
- **Every deck in a worker, deck A too.** One model for all decks, an almost empty audio thread. But deck A, the one played and edited, would answer 10.7 ms late. Rejected; a live deck stays on the audio thread.
- **Two browser windows.** Two AudioContexts, two clocks, no shared limiter, a crossfader in JavaScript, never sample-accurate. That is epic #258's use, instances that play together, not transitions. Rejected for decks.
- **One AudioContext per deck.** Each gets its own thread, but audio crosses contexts only through a MediaStream: tens of milliseconds, drift, not sample-accurate. Rejected.
- **One shared wasm memory (wasm threads).** One sample store, no copies. Needs nightly Rust and `-Z build-std`. Deferred until it is on stable.
