# 0037: A model says which parameters it reads

**Status:** Accepted · **Date:** 2026-10-16 · **Extends:** 0019, 0036

## Context

A parameter method, `mod`, `auto`, snapshot or setting change may name any of the registry's parameters, whatever the track's model. `Mono::set` stores every parameter for every model, so one the model's voice never reads parses and does nothing. `.lfospeed(4)` on a Minimoog is the DX7's LFO; `.lforate(4)` is the general LFO, which the Minimoog doesn't have either: its vibrato is VCO 3 in its low range (`modulates_with_osc3`). Nothing said so (#673).

What a model reads can't be derived from the engine code: whether a stored field matters depends on the render path and the model's flags. Only the web panels list a model's controls, and the engine can't depend on them (ADR-0001).

## Decision

**Each model says which parameters it reads, by engine and by what the model has, and a parameter it never reads is a parse error at its name.**

- **The sets** (`synth/reads.rs`, `Model::reads`). Every model reads the strip, the master, the arpeggiator and the voice pool (`Polyphony`, `Assign`, `UnisonDetune`, `Analog`, `Legato`, `Priority`, `Glide`, `ChorusMode`). The rest is by family: the DX7's operators and LFO, the D-50's second partial, the drum pads, the Modular's knobs, the SID's and the NES's own. The Mono voice's parameters go to every Mono and LA model, less what a model's flags say it lacks: the LFO on a model whose vibrato is VCO 3, the wavetables without `uses_tables`, the second LFO, ramp and patch slots 9 to 20 without `has_matrix`. The sampler reads its oscillator, filter and envelopes; the Pad Sampler its level. The mod wheel and pitch bend go to the engines that read them.
- **The check.** At a parameter method, `auto`, `snapshot` and `mod` on a track, and a setting's changes. Where the text names the model, at once; where the model is picked for the track's role after the clips, then, because the printed text names it and must parse back. A `strip1`–`strip16` target is not checked.
- **The message** names the model's own parameter for the job where it has one ("the Minimoog's vibrato is VCO 3: vco3coarse", "a DX7 parameter: this synth's LFO is lforate"), otherwise what the parameter is for or what the model reads. Messages stay `&'static str`.
- **Drift.** A test reads each model's panel in `models.ts` and requires every control's parameter to be read by its model; another requires every preset to set only what its model reads.

## Consequences

- Within the Mono family the sets are generous: a switch or knob a particular Mono model lacks still parses. Only a model's flags narrow it. Narrowing knob by knob is possible later, model by model, with the panel test as a floor.
- ADR-0036 left a track whose model is picked later unchecked; this check runs on it once picked. The two now differ: a lane is checked only where the model is named.
- The composer can still switch a track to a model that doesn't read what its clips name (`set_track_preset` checks only the kind); the printed text then fails to parse.
- Completion (#595) still offers every parameter; it doesn't know a track's model yet.

## Alternatives considered

- **Derive the sets from `Mono::set`.** It stores every field for every model on purpose; what matters is decided in the voices.
- **Take the sets from the panels.** The panels miss parameters models read (the voice pool on the NES and Modular, the mod wheel on the DX7), and the engine can't depend on the view (ADR-0001). They are a check instead.
- **A per-model list of all 636 parameters.** Precise, but 24 hand lists with no check beyond the panels; a gap breaks a song that works today.
- **Warn instead of erroring.** The song language has no warnings; an error at the name is what the other mistakes get.
