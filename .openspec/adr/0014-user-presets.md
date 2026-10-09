# 0014: User presets: four kinds, stored by name, a library beside the setups

**Status:** Accepted · **Date:** 2026-10-04

## Context

Factory presets are Rust data (`mono::preset`), selected by id; a setup (`.synths.json`) saves a whole session. Nothing lets a user keep one synth sound, one effect or one channel's processing and use it again in another song (epic #153).

## Decision

**A user preset is a named set of parameter values stored by name, of one of four kinds, each touching one scope; the presets live in a library in the view, apart from setups.**

- **Kinds and scopes.**
  - *synth*: the model (by name) and every synth parameter, which is every parameter that is neither global nor a strip's. It is applied on the synth defaults (`synth_defaults`, new in the ABI), so a parameter added after the preset was saved takes its default, not the synth's previous value. The strip is not touched.
  - *insert*: one insert slot's type (by name) and knobs A–E; it loads into any slot of any strip or group.
  - *processor*: one processor's type (by name), knobs A–E and return; it loads into any of P1–P4. The chain toggles (`P2In`…) are routing and stay.
  - *strip*: pan, the four sends and the three insert slots of a strip or group. Never the fader, mute, solo or `Out`: those belong to the song.
- **By name.** Parameters, models and effect types are stored by name, as setups are (ADR-0004); reading a library drops what it cannot place, with one warning per kind of problem, and rejects a file that is not a library or has another version.
- **Factory and user.** Factory presets stay in Rust, where tests render each one bounded and audible. User presets are data the view sends as parameter changes, as a setup is applied; the engine gains only `synth_defaults`.
- **The library.** One list per browser, kept in IndexedDB and exported or imported as `algo-synth.presets.json`. Saving a preset with the name of one of its kind (and model or type) replaces it; importing numbers a name that is taken. Without storage the library lasts the session and says so.
- **Setups stay self-contained.** A setup stores values, never a reference to a preset, so a song opens the same on any machine whatever its library holds.
- **Copy and paste** (#152) is an unnamed preset of a kind on a clipboard, applied with the same plan.

## Consequences

- One pure module (`web/src/audio/presets.ts`: capture, plan, modified, parse, merge) serves all four kinds and the clipboard; the view adds pickers.
- A preset made before a parameter existed still loads; one made after a parameter is removed loads with a warning.
- Sharing a sound means exporting the library or a setup; there is no cloud store.
- A sampler synth's preset holds its parameters, not its samples, which live in the sample store (ADR-0013).

## Alternatives considered

- **User presets in Rust.** The engine would store and serialise strings and files, against ADR-0001's line that the view holds files and the engine plays. Rejected.
- **Presets inside the setup.** Songs would carry presets they don't use and the library would split per song. Rejected; values are enough.
- **localStorage.** About 5 MB per origin and synchronous; a library of hundreds of synth presets nears it. IndexedDB is used instead.
