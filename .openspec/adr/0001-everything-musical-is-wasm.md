# 0001: Everything musical is wasm

**Status:** Accepted, amended by 0029 · **Date:** 2026-09-30

## Context

algo-synth must run entirely in the browser. Audio in a browser is produced on a dedicated audio thread by an `AudioWorkletProcessor`, which must be a JavaScript class. That scope has no `fetch`, no `TextDecoder`, and a hard deadline every 128 frames (2.7 ms at 48 kHz). Garbage-collected JavaScript in that loop glitches under load; Rust compiled to wasm doesn't.

## Decision

**All music logic is Rust, in one crate (`crates/dsp`), compiled to one `wasm32-unknown-unknown` module:** sound sources, mixer, effects, the sequencer and its clock, generators, the song model, and later file parsers (MIDI import). Nothing musical is decided in JavaScript.

**A plain C ABI, no wasm-bindgen.** The exports take and return numbers only. wasm-bindgen's glue expects `TextDecoder` and friends, which the worklet scope lacks. The module must have no imports, so `new WebAssembly.Instance(module, {})` works inside the worklet.

**One engine per wasm instance**, held in a thread-local (wasm32 without atomics has one thread). The exports take no pointers; the only pointer that crosses is `out_ptr()`, the engine's own output buffer, read by JavaScript. The one `unsafe` is the `#[unsafe(no_mangle)]` attribute, confined to `ffi.rs`; the workspace denies `unsafe_code` elsewhere.

**The JS shim (`web/public/worklet.js`) holds no logic.** It instantiates the module the main thread compiled, forwards port messages to exports, calls `process`, and copies the block into the output channels. Bulk data (songs, patterns, imported scores) will be written into a buffer the engine allocates at init (ADR-0005, spec 002).

**No dependencies in the engine**, std only. `cargo deny` keeps it deliberate.

**Amended by ADR-0029:** the module also runs in Web Workers, one engine per worker, for decks beyond the main one; still one module, no imports, no logic in the worker shim. The main engine stays on the audio thread.

## Consequences

- The engine is testable natively: `cargo test` renders blocks and checks them without a browser.
- A second wasm module for the UI (Leptos, egui) stays possible without touching this boundary (ADR-0003).
- Strings don't cross the ABI; errors are reported as numbers or ignored (unknown ids are no-ops).
- The main thread must compile the module and pass it to the worklet in `processorOptions`.
