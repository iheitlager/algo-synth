# ADR index

| ADR | Title | Status |
|---|---|---|
| [0001](0001-everything-musical-is-wasm.md) | Everything musical is Rust in one wasm module on the audio thread; a C ABI, no wasm-bindgen, a JS shim with no logic | Accepted |
| [0002](0002-real-time-rules.md) | The render loop never allocates, locks or panics; tables and control-rate coefficients instead of per-sample transcendentals | Accepted |
| [0003](0003-vue-view.md) | A Vue + TypeScript view, separate from the engine; the worklet outside the bundler; the scope on an AnalyserNode | Accepted |
| [0004](0004-one-parameter-registry.md) | One parameter and source registry in Rust, mirrored in TypeScript, the mirror checked by a test | Accepted |
| [0005](0005-composition-model.md) | Composition: tracks own one source; fixed insert/send/master mixer; patterns in clips from hand, generator or score; the clock in the engine | Accepted, narrowed by 0008, song format superseded by 0012 |
| [0006](0006-static-serving.md) | Podman + Caddy serving static files; no backend | Accepted |
| [0007](0007-blep-table-oscillators.md) | Oscillators band-limit every step (wrap, pulse edge, sync reset) with a windowed-sinc BLEP table, not a 2-point polyBLEP | Accepted |
| [0008](0008-mono-only-to-the-ensemble.md) | Mono only, straight to the ensemble: Wave, Drums, the arrangement and algo loops removed for now | Accepted, deferrals lifted by 0012 |
| [0009](0009-synth-models.md) | Synth models: one shared voice with a `Model` per slot (enum dispatch) and one data-driven panel per model; six monosynths | Accepted |
| [0010](0010-mixer-topology.md) | Mixer topology: strips and eight group buses in one index space, `Out` routing that cannot cycle, three insert slots per strip and group with the type as a parameter | Accepted |
| [0011](0011-polyphony.md) | Polyphony: a voice pool per synth (the Mono voice per note), allocation and stealing, unison, analog variance, a shared LFO and a global voice budget | Accepted |
| [0012](0012-the-song-is-text.md) | The song is text: a Tidal/Strudel-style notation with classic note names and durations and drum lanes, parsed and printed by the engine; the drum grid and generators edit it; a language model writes it | Accepted |
| [0013](0013-sample-store.md) | The sample store: Rust parses and resamples WAV at load, in bounded memory; fixed slots, a hard cap, errors as codes | Accepted |
| [0014](0014-user-presets.md) | User presets: synth, insert, processor and strip kinds stored by name, applied on the synth defaults, kept in a browser library with export and import, apart from setups | Accepted |
| [0015](0015-the-arrangement.md) | The arrangement: sections and an `arrange` order in the song text, drum, synth, sampler and MIDI tracks, scenes and automation lanes for any parameter by name, MIDI files converted into notes, the arranger pane | Accepted |
| [0016](0016-note-events-on-a-tick-grid.md) | Note events on a tick grid: 48 ticks per bar, events compiled at load, fired at their tick with a fixed note-off queue, a seeded integer generator per cycle | Accepted |
| [0018](0018-the-song-is-the-session.md) | The song is the session: mixer, groups, master, processors and samples as lines in the song text, one namespace for tracks and strips, the setup file an import | Proposed |
| [0019](0019-patterns-signals-and-parameters.md) | One language for patterns, signals and parameters: pattern methods, control-rate signals with mathematics, parameters as methods, one precedence, compiled to a node pool | Accepted |
| [0020](0020-modular-voice-and-sound-screen.md) | A modular voice: unit-generator graphs in the song compiled per song and played per note, controls as generic parameters, a Sound screen; our own language, not scsynth | Accepted |
| [0021](0021-modular-voice-language.md) | The modular voice's language: bipolar audio-rate calls in hertz apart from the signals, `|>` as a postfix, one line, fixed per-voice limits, the program copied per note, no transcendental call per sample | Accepted |
| [0022](0022-supercollider-voices-on-the-synth.md) | SuperCollider voices on the Modular synth: SynthDefs in a subset of sclang evaluated at build time, a knob for every number a UGen takes, the code the synth's own; supersedes the syntax, storage and Sound screen of 0020 and 0021 | Proposed |
| [0022](0022-one-clock-one-transport.md) | One clock, one transport: the song's clock and the top bar's Play, Pause and Stop, no MIDI player; a MIDI file is imported when opened | Accepted |
