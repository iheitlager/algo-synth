# 0004: One parameter registry

**Status:** Accepted, clarified 2026-10-09 (#227) · **Date:** 2026-09-30

## Context

Every knob sends `set_param(id, value)`, every note names a source id. Two hand-kept lists of ids drift silently: a knob ends up turning the wrong thing.

## Decision

**Rust is the source of truth** (`crates/dsp/src/params.rs`, `source.rs`): each enum has an `ALL` table of `(variant, name)`, a `from_id`, and (for parameters) a range with `clamp`. **`web/src/audio/params.ts` mirrors them** as `Name: id,` lines, and a Rust unit test reads the TypeScript file with `include_str!` and fails if any line is missing or wrong.

## Consequences

- No code generator to maintain; the test is the guard.
- Parameters per source instance (MVP 2 onwards) will need an addressing scheme (track, parameter); spec 002 settles it before MVP 3.

## Later decisions

- **Clarified (#227):** `source.rs` is gone; a track plays a synth with a model (ADR-0009, ADR-0025), not a source. The registry is `crates/dsp/src/params.rs` and the enums named in the header of `web/src/audio/params.ts` (models, presets, oscillators, noise, drive, processors and the rest), all held to the mirror by the same test.
