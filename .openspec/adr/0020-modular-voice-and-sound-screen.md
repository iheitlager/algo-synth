# 0020: A modular voice in the song, and a Sound screen

**Status:** Proposed · **Date:** 2026-10-05

## Context

Every model so far is fixed: a Mono or Poly voice with a `Model` that sets its character (ADR-0009, ADR-0011), FM, LA and the drum machines. The Spectral Lab (#192, ADR-0017) adds analysis and a Synclavier-style additive model in a second window. What SuperCollider adds, and the user wants, is designing the sound itself in code: a voice as a graph of unit generators, written in a few lines and played per note.

ADR-0019 gives the song text expressions and signals at control rate. A voice needs the same language at audio rate, which is where the cost is.

There was a choice between embedding SuperCollider's server (scsynth, in wasm as SuperSonic) and writing our own. SuperSonic is AGPL-3.0, which would put the whole served project under the AGPL, and it is a second engine with its own clock and mixer beside ours. Our own language stays Apache-2.0, std-only and inside ADR-0001, ADR-0002 and ADR-0012; a SuperCollider patch is rewritten in a few lines instead of pasted.

## Decision

**A new category of synth, Modular, whose voice is a graph of unit generators written in the song text, compiled once per song and played per note; and a Sound screen to design it.**

- **Voices in the song.** `voice <name> = { <expr> }` defines a voice in ADR-0019's expression language, with audio-rate unit generators: `sin saw pulse tri noise` (band-limited as ADR-0007 asks), `svf ladder` (the shared filters of `mono/`), `fm`, `mix pan`, `delay`, `env lfo`, and `freq gate vel` from the note. `|>` passes a signal on; `+ - *` and `range` work as for signals. A track plays it: `track lead synth Modular hoover`.
- **Compiled like a SynthDef.** The parser checks the graph and compiles it into a fixed array of nodes per voice, in evaluation order, with its state sized at load. A song has a stated limit of nodes per voice and voices per song. The existing voice pool (ADR-0011) allocates and steals the voices; `render` runs the arrays and never allocates (ADR-0002). A graph that changes takes over at the next note, the old one finishing its notes.
- **Controls.** Named controls in a voice (`ctl cutoff = 800 [100 8000 exp]`) are parameters of that synth: knobs on its faceplate, targets for ADR-0019 methods, scenes and lanes, and values in a `setting`. They map onto a fixed block of generic parameter ids in the registry (ADR-0004), so the registry does not grow per song.
- **The Sound screen.** A third main view beside Composer and Mixer: the voice's text with highlighting (#203), a scope and a spectrum, a keyboard, and the voice's controls. It edits the same song on the same engine, so a change sounds in the piece at once. It is a view in the app, not a window: the Spectral Lab (#192) stays its own window for analysis, and its additive model joins this language later as an `additive(...)` unit generator.
- **Our own language, not scsynth.** As in Context: one engine, one clock, Apache-2.0.

## Consequences

- One notation now spans notes, beats, arrangement, mix (ADR-0018), modulation (ADR-0019) and sound design; a song carries its own instruments.
- Spec 005 gains the Modular model and its unit generators; spec 003 gains the Sound screen.
- The CPU cost of a voice depends on its graph. A node limit bounds the worst case; `make bench` measures 16 voices of the largest allowed graph against plan.md's budget, and the limit is set from that measurement.
- A hoover and a gabber kick, written as voices, are the acceptance tests: they must sound right and render deterministically.
- Faceplates for Modular synths are generated from their controls, not drawn per model (ADR-0009's data-driven panel).

## Alternatives considered

- **Embed SuperSonic (scsynth in wasm).** Real SynthDefs, but AGPL-3.0 and a second engine, clock and mixer. Rejected.
- **A fixed modular model with patch cables** (the ARP 2600 style the Mono voice already has). Visual and bounded, but not code, and not writable by a language model. Rejected for this ADR; patching stays where it is.
- **The Sound screen as a second window,** like the Spectral Lab. It would need its own engine instance and messages to keep the song and the voice in step. Rejected: the voice belongs to the song.
