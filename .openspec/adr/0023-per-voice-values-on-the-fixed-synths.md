# 0023: Per-voice values on the fixed synths

**Status:** Accepted · **Date:** 2026-10-06

## Context

ADR-0019's signals run at control rate and write one value per synth: every voice of a Mono or Poly synth reads one shared `MonoParams`. Two signals need a value per voice: `env(shape)`, which restarts with each note (`mod lead.cutoff = env(perc).exprange(200, 4000)`), and a list `[a, b]`, one number per voice (`lfo([1, 3, 5, 7])`). ADR-0020 and ADR-0021 gave both to the Modular voice, which runs its own graph per voice; on the fixed synths they were left for #273. A Mono or Poly voice already has two per-voice slots, `trim` (pitch) and `cutoff_trim`, which the pool sets each block for unison and analog drift; a whole `MonoParams` per voice would be kilobytes each for 64 voices.

## Decision

**A signal with `env` or a list is a per-voice signal. On a Mono or Poly track it is evaluated once per block for each voice of the pool and written into a small override table of that voice; the voice reads an overridden parameter instead of the synth's.**

- **Per-voice signals.** ADR-0019's signals gain `env(adsr)`, `env(perc)` and `env(a, d, s, r)` (seconds), and `[a, b, …]`, as ADR-0021 writes them: a list is a number per voice, `lfo([1, 3])` a rate per voice. A signal using either is per voice; every other signal stays per synth and is unchanged.
- **Envelopes at control rate.** The pool keeps an envelope per voice slot, stepped once per block with the times scaled to blocks, gated by the voice's gate and restarted when the slot takes a new note. `env(adsr)` follows the synth's amplifier ADSR; `perc` is ADR-0021's (2 ms, 0.3 s, 0, 0.3 s). It is unipolar, so `.exprange(200, 4000)` maps 0..1 (ADR-0021's polarity).
- **Lists by voice slot.** Voice slot *i* of the pool takes number *i* round the list, as in the Modular voice (ADR-0021), so a chord spreads across the list and a stolen voice keeps its slot's number.
- **An override table, absolute values.** Each voice slot holds a mask and a value for each of eight parameters: `Cutoff`, `Resonance`, `Vco1Level`, `Vco2Level`, `Vco3Level`, `NoiseLevel`, `RingLevel` and `SubLevel`, in the parameter's units, clamped and converted as `set_param` does (cutoff to a note). An overridden parameter is that voice's value under ADR-0019's write-over rule; the synth's value is never written, so when the modulation stops the mask clears and the voices read the synth's value again: nothing to put back.
- **Only those parameters, only Mono and Poly.** A per-voice signal on another parameter, on a strip, group or master, or on a track whose model is known and is not a Mono or Poly synth, is a parse error with its column. If the synth's model changes under the song, the voices of the new model don't read the table and the modulation has no effect. The Modular voice writes `env` and lists in its graph (ADR-0021).
- **No `.lag` in a per-voice signal.** A lag keeps one state for the song (ADR-0019); per voice it would need a state per voice. It is a parse error there.
- **Timing.** The values are evaluated once per block after the per-synth modulations, so a note that starts inside a block takes its own value from the next block, within 2.7 ms; the filter's cutoff follower (2 ms) and the level ramp of #271 smooth the step. The voice reads the table once per render call.

## Consequences

- `render` doesn't allocate: the envelopes and the table are fixed arrays in the pool, and a per-voice signal costs one evaluation per sounding voice per block (`make bench`, `per-voice`).
- The override replaces the synth's value, it does not add to it: a knob turned by hand on an overridden parameter is not heard on those voices until the modulation stops, as with any modulation under ADR-0019.
- FM, LA, the drum machines and the samplers can join later by reading the same table; nothing in the signal or the table is Mono-specific.

## Alternatives considered

- **A full `MonoParams` per voice.** Any parameter per voice, but kilobytes per voice for 64 voices, a copy per voice per block and coefficients recomputed per voice. Rejected.
- **Offsets on the synth's value, like `trim` and `cutoff_trim`.** Cheap and composable with a knob, but a modulation would mean something else per voice than per synth, against ADR-0019's write-over rule. Rejected.
- **Per-voice signals evaluated inside the voice, per sample.** The exact attack of `env(perc)`, but a signal's tree walk per sample per voice, against ADR-0019's control rate. Rejected; audio rate stays in the Modular voice.
