# 0002: Real-time rules for the render loop

**Status:** Accepted · **Date:** 2026-09-30

## Context

`process` has 2.7 ms per block at 48 kHz, shared with the browser. A panic in wasm traps the module and silences the worklet for good. Allocation can call into the allocator at unpredictable cost; `sin`, `exp`, `pow` and `tan` per sample per voice are the most expensive math in a naive synth.

## Decision

1. **`render` never allocates.** Everything is allocated in `Engine::new` (voice pools, tables, buffers, the song buffer). Pools are fixed-size; when full, the oldest voice is stolen.
2. **`render` never panics.** Clippy denies `unwrap`, `expect`, `panic!` and indexing; lookups use `get` with a fallback. The release profile is `panic = "abort"`, so a panic that slips through at least doesn't grow the module with unwinding tables.
3. **No transcendentals per sample.** Oscillators read tables; coefficients (envelope rates, filter `g`, pitch increments) are computed when a parameter or note changes, and smoothed at control rate.
4. **Inputs are clamped at the edge.** Every parameter has a range; NaN becomes the lower bound. Unknown ids are ignored.
5. **Output is bounded and finite.** Tests assert it for every source and effect.

## Consequences

- Some code is more verbose (`iter`, `get`, `zip` instead of indexing).
- Adding a source means sizing its pool up front.
- Profiling happens at the end of each milestone (plan.md): render capacity in Chrome DevTools' WebAudio panel (`chrome://webaudio-internals` was retired), with `make bench` as an offline check.
