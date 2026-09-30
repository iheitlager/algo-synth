# 0003: A Vue view, separate from the engine

**Status:** Accepted · **Date:** 2026-09-30

## Context

The UI is panes, grids, knobs and a scope: DOM work, not DSP. The repository was started as "rust/wasm/vue.js". The alternative, a Rust UI (Leptos, Yew, egui) as a second wasm module, keeps one language but needs wasm-bindgen, which the engine module must not use (ADR-0001).

## Decision

**The view is Vue 3 + TypeScript, built with Vite, in `web/`.** It renders state and sends messages; it makes no musical decisions (ADR-0001).

**The worklet shim and `dsp.wasm` live in `web/public/`, outside the bundle.** A worklet module is loaded by URL and can't import the app's chunks. `make wasm` copies the built module there.

**The scope and meters use Web Audio's `AnalyserNode`** on the main thread; the engine doesn't compute display data.

**TypeScript 6.x**, pinned: vue-tsc does not yet run on TypeScript 7 (the native port has no JS API).

## Consequences

- Two toolchains (cargo, npm); `make` hides both.
- If the UI later moves to Rust/wasm, it's a new module and a new ADR; the engine ABI stays.
