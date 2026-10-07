# 0009: Synth models: one voice, six instruments

**Status:** Accepted · **Date:** 2026-10-03 · A seventh model, the ARP Odyssey, joined in #64 on the same terms. · What each model decides moved to one definition per instrument in ADR-0025.

## Context

ADR-0008 narrowed the product to the Mono voice, and spec 004 Req 10 holds 16 synths that are all ARP 2600-style. The next step is a family of monosynths (epic #28): ARP 2600, Minimoog, Sequential Pro-One, Korg MS-20, Yamaha CS-15 and Roland SH-101, each recognisable as its own instrument: its own controls, its own sound, its own panel and colours.

These machines share most of their parts: VCOs, noise, a low-pass filter, envelopes, an LFO. They differ in how many of each, which filter, which envelope drives what, and what is on the panel.

## Decision

**One shared voice with a `Model` per synth slot, switched by enum dispatch, and one data-driven panel per model.**

- `mono/model.rs` holds `enum Model` (`Arp2600`, `Minimoog`, `ProOne`, `Ms20`, `Cs15`, `Sh101`) and what each model decides: the filter and its voicing, whether a high-pass stage exists and where, which envelope drives the cutoff, whether decay doubles as release. `Model` is a parameter (`Param::Model`, ADR-0004), so a preset and `param_value` carry it.
- The modules stay shared in `crates/dsp/src/mono/`: VCOs, noise, ladder, a 12 dB state-variable filter (`svf.rs`), the ADSR, AR and filter ADSR, LFO, patch. A new module serves every model that wants it.
- Dispatch is a `match` on a `Copy` enum inside `MonoVoice::render`, so nothing is boxed or allocated (ADR-0002) and the 16-synth budget stays one measurement.
- A model is not a lock: every parameter exists on every synth. The *panel* shows what the real instrument has, and the *presets* set the rest to neutral values. A model's character comes from the DSP choices above, not from hiding knobs.
- The view describes each model as data (`web/src/audio/models.ts`: sections, controls, labels, palette) and draws it with one component. The view sends messages and draws (ADR-0001); which control belongs on which panel is layout, not music logic.
- Selecting a model on a synth loads that model's first preset, so a switch is always a complete, consistent sound.
- Spec 005 holds one requirement per model; spec 004 holds the shared modules.

## Consequences

- Every model plays in the same 16-slot ensemble and in the MIDI player; six different synths can play one file.
- A model-specific behaviour is one arm of a `match`, so adding a seventh instrument is a `Model` variant, a panel description and presets.
- The parameter registry grows (about 20 parameters); ADR-0004's mirror test keeps `params.ts` honest.
- The sound of a model is an interpretation of the instrument, not a circuit emulation. Each requirement names the property it must have (slope, self-oscillation, which envelope moves what) and is tested on that.
- ARP 2600 rendering must not change: a test pins its presets' output.

## Alternatives considered

- **A voice type per model (a trait or six structs).** Duplicates the oscillators, envelopes and note handling six times and needs trait objects or generics through the engine's fixed arrays. Rejected.
- **Models as presets only.** Cannot give the MS-20 or CS-15 their high-pass stage and 12 dB filter, or the Minimoog its decay-as-release. Rejected.
