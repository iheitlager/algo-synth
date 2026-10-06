# 0021: The modular voice's language and how it runs

**Status:** Proposed · **Date:** 2026-10-06

## Context

ADR-0020 decides that a Modular synth's voice is a graph of unit generators written in the song text, compiled once and played per note. It leaves open how the language relates to ADR-0019's signals, how a graph is written on a line-based song, how a graph reaches a voice, and how audio-rate evaluation stays inside ADR-0002. #216 builds it; these are the choices made on the way.

## Decision

**A voice is its own small language in the expression style of ADR-0019, compiled into a fixed `Copy` program that each note copies, and evaluated once a sample without allocation or transcendental calls.**

- **Its own vocabulary.** Inside `voice <name> = { … }` unit generators are calls, bipolar (−1..1) and in hertz, as in SuperCollider: `sin(f) saw(f) tri(f) pulse(f, width) noise() lfo(rate, shape) svf(mode, cutoff, res) env(…)`, with `freq gate vel` from the note. `.range(a, b)` and `.exprange(a, b)` map −1..1, and their bounds may be signals (`saw(freq).range(freq, freq * 3)`). ADR-0019's signals (bare, unipolar, on song time) keep their meaning; the two share the lexer and the parser's shape, not their nodes.
- **`|>` is a postfix.** `x |> svf(lp, 800)` passes `x` as the filter's input and binds like a method, so `a |> svf(…) * env(adsr)` multiplies the filtered signal.
- **One line.** A voice is written on one line between braces, as every other item of the line-based song is; a block form spanning lines can come later.
- **Fixed limits per voice.** At most 32 nodes, 8 band-limited oscillators (saw, tri, pulse), 8 phases (sin, lfo), 4 filters and 4 envelopes. The program and the voice's state are fixed arrays; a voice is a variant of the pool's `PolyVoice` like the LA and FM voices (ADR-0011).
- **The program travels by copy.** The compiled program lives in the synth's `MonoParams` (a preset's voice, or the song's at load) and each note copies it when it starts, so a changed graph takes the next note while sounding notes finish theirs, with no reference into a song that may be swapped at the bar line.
- **No transcendental call per sample.** `sin` reads the shared sine table; `exprange` and the filter's cutoff in hertz use a fast `exp2`/`log2` (within 1e-5); a filter converts its cutoff to a note only when it changes.
- **Loudness.** A voice that uses `env` is shaped by it and ends when its envelopes do (a percussive one ends with the key held); a voice without `env` sounds through the synth's ADSR, so every voice ends.
- **Presets.** The Modular model has presets like every model (spec 005): their voices are text in `Preset::voice_text`, compiled outside `render` when the preset is chosen.

## Consequences

- PR 1 of #216 measured 64 voices of the hoover preset (three band-limited oscillators, two LFOs, a filter) at 22% of one core against plan.md's 25% budget (`make bench`, `modular`). The limits above are an upper bound on a single voice, not a promise that 64 voices of the largest graph fit; the bench of the largest graph sets the limit before the language grows (#216, PR 3).
- A graph's state is reset when a silent voice starts a note, not when the program changes under a sounding one.
- Controls (`ctl`), the ladder, `fm`, `delay`, `mix`, `pan`, per-voice `env(shape)` and multichannel `[a, b]` come in later steps of #216, within the same rules.

## Alternatives considered

- **One language with ADR-0019's signals.** One vocabulary, but either the merged signals change meaning (bipolar, in hertz) or audio-rate calls inherit song time; and a recursive tree walk per sample. Rejected.
- **A program shared by reference from the song.** No copy per note, but a lifetime tie to a song swapped inside `render`, and a changed graph would change sounding notes. Rejected.
- **Hz-to-note and `exp2` with `std` per sample.** Exact, but `log2` and `powf` per sample per voice against ADR-0002. Rejected for the fast approximations, tested against `std`.
