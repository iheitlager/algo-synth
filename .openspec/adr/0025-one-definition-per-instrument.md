# 0025: One definition per instrument

**Status:** Accepted · **Date:** 2026-10-07 · supersedes the part of ADR-0009 that has `mono/model.rs` hold what each model decides

## Context

ADR-0009 put "what each model decides" in `mono/model.rs`, as answers on `enum Model`. With 20 models that became 20 `match self` methods, one per question (`voices`, `filter`, `low_pass`, `hp`, six `uses_*` engine flags, eight behaviour flags), and the presets lived apart in a 3,500-line `mono/preset.rs` with its own per-preset matches for the model, the code and the changes. To read one instrument meant reading about 20 match arms and a slice of another file; adding one meant editing every match (#330).

## Decision

**Each instrument is one definition, in one file: `crates/dsp/src/synth/<model>.rs`, holding `pub const DEF: ModelDef` and its presets.**

- `ModelDef` is plain `Copy` data: voice count, `engine: Engine` (Mono, La, Fm, Drums(machine), Sampler, Pads, Graph), the low-pass, its 12 dB setting and revisions (`slope12`, `revs`, read by `low_pass`, #321), the high-pass, the behaviour flags, and `presets: &[PresetDef]`. A definition states what differs from `ModelDef::MONO`.
- `Model::def()` in `synth.rs` is the one `match` from a model to its definition. The `Model` methods stay as one-line reads of it, so call sites do not change; the voice dispatch (`PolyVoice::new`, `fits`) matches on `engine`.
- A `PresetDef` holds what the preset changes from `DEFAULTS` and, for a Modular preset, its SuperCollider code. `Preset::model`, `changes` and `code` find the preset in the definitions, when a preset loads, never in `render`.
- `mono/model.rs` keeps the model ids and the filter voicings the definitions share (`MOOG`, `D50`, …). `mono/preset.rs` keeps the `Preset` ids and names (ADR-0004) and `DEFAULTS`.
- Not by mono or poly: the polysynths play the same Mono voice, and the voice count is a field.
- The view's faceplates (`web/src/audio/models.ts`) are unchanged: they are view data (ADR-0001, ADR-0009).

## Consequences

- One file reads as one instrument; a new model is a new file, one arm in `Model::def` and its id. A test holds every preset to exactly one definition.
- Nothing is boxed, allocated or dispatched dynamically; `render` reads the same answers as before (ADR-0002). The move was checked by a fingerprint of every preset's parameters and sound, identical before and after.
- A preset's model is found by a search over 20 short lists. That is cheap at load time and never done per sample.
