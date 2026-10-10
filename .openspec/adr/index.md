# ADR index

| ADR | Title | Status |
|---|---|---|
| [0001](0001-everything-musical-is-wasm.md) | Everything musical is Rust in one wasm module on the audio thread; a C ABI, no wasm-bindgen, a JS shim with no logic | Accepted, amended by 0029 and 0017 |
| [0002](0002-real-time-rules.md) | The render loop never allocates, locks or panics; tables and control-rate coefficients instead of per-sample transcendentals | Accepted, clarified (load-time buffers) |
| [0003](0003-vue-view.md) | A Vue + TypeScript view, separate from the engine; the worklet outside the bundler; the scope on an AnalyserNode | Accepted |
| [0004](0004-one-parameter-registry.md) | One parameter and source registry in Rust, mirrored in TypeScript, the mirror checked by a test | Accepted, clarified (no `source.rs`) |
| [0005](0005-composition-model.md) | Composition: tracks own one source; fixed insert/send/master mixer; patterns in clips from hand, generator or score; the clock in the engine | Accepted, narrowed by 0008, superseded in part by 0010, 0012, 0015, 0016 |
| [0006](0006-static-serving.md) | Podman + Caddy serving static files; no backend | Accepted, amended by 0028 and 0029, Caddy superseded by 0030 |
| [0007](0007-blep-table-oscillators.md) | Oscillators band-limit every step (wrap, pulse edge, sync reset) with a windowed-sinc BLEP table, not a 2-point polyBLEP | Accepted |
| [0008](0008-mono-only-to-the-ensemble.md) | Mono only, straight to the ensemble: Wave, Drums, the arrangement and algo loops removed for now | Accepted, deferrals lifted by 0012 |
| [0009](0009-synth-models.md) | Synth models: one shared voice with a `Model` per slot (enum dispatch) and one data-driven panel per model; six monosynths, now twenty models | Accepted, superseded in part by 0025 |
| [0010](0010-mixer-topology.md) | Mixer topology: strips and eight group buses in one index space, `Out` routing that cannot cycle, three insert slots per strip and group with the type as a parameter | Accepted |
| [0011](0011-polyphony.md) | Polyphony: a voice pool per synth (the Mono voice per note), allocation and stealing, unison, analog variance, a shared LFO and a global voice budget | Accepted, extended (PolyMoog) |
| [0012](0012-the-song-is-text.md) | The song is text: a Tidal/Strudel-style notation with classic note names and durations and drum lanes, parsed and printed by the engine; the drum grid and generators edit it; a language model writes it | Accepted, amended by 0015, 0022, 0028 |
| [0013](0013-sample-store.md) | The sample store: Rust parses and resamples WAV at load, in bounded memory; fixed slots, a hard cap, errors as codes | Accepted |
| [0014](0014-user-presets.md) | User presets: four kinds (synth, insert, processor, strip) stored by name, applied on the synth defaults, kept in a browser library with export and import, apart from setups | Accepted |
| [0015](0015-the-arrangement.md) | The arrangement: sections and an `arrange` order in the song text, drum, synth, sampler and MIDI tracks, scenes and automation lanes for any parameter by name, MIDI files converted into notes, the arranger pane | Accepted, amended by 0018, 0022, 0027, 0031 (its words) |
| [0016](0016-note-events-on-a-tick-grid.md) | Note events on a tick grid: 48 ticks per bar, events compiled at load, fired at their tick with a fixed note-off queue, a seeded integer generator per cycle | Accepted |
| [0017](0017-spectral-lab.md) | The Spectral Lab: a third window (`spectral-lab.html`) with its own engine; STFT analysis, partial tracking and additive resynthesis in Rust, called offline from a Web Worker that only relays; results as arrays in wasm memory; send to the main window over a BroadcastChannel | Proposed |
| [0018](0018-the-song-is-the-session.md) | The song is the session: mixer, groups, master, processors and samples as lines in the song text, one namespace for tracks and strips, the setup file an import | Accepted, amended by 0027, 0031 and #214 (setup import stays in TS) |
| [0019](0019-patterns-signals-and-parameters.md) | One language for patterns, signals and parameters: pattern methods, control-rate signals with mathematics, parameters as methods, one precedence, compiled to a node pool | Accepted, extended (#255, #298) |
| [0020](0020-modular-voice-and-sound-screen.md) | A modular voice: unit-generator graphs in the song compiled per song and played per note, controls as generic parameters, a Sound screen; our own language, not scsynth | Accepted, superseded in part by 0024 |
| [0021](0021-modular-voice-language.md) | The modular voice's language: bipolar audio-rate calls in hertz apart from the signals, `|>` as a postfix, one line, fixed per-voice limits, the program copied per note, no transcendental call per sample | Accepted, superseded in part by 0024 |
| [0022](0022-one-clock-one-transport.md) | One clock, one transport: the song's clock and the top bar's Play, Pause and Stop, no MIDI player; a MIDI file is imported when opened | Accepted, amended by 0029 |
| [0023](0023-per-voice-values-on-the-fixed-synths.md) | Per-voice values on the fixed synths: `env` and lists make a signal per voice, evaluated per block into an override table of eight continuous parameters on Mono and Poly voices, absolute values, lists by voice slot | Accepted |
| [0024](0024-supercollider-voices-on-the-synth.md) | SuperCollider voices on the Modular synth: SynthDefs in a subset of sclang evaluated at build time, a knob for every number a UGen takes, the code the synth's own; supersedes the syntax, storage and Sound screen of 0020 and 0021 | Accepted |
| [0025](0025-one-definition-per-instrument.md) | One definition per instrument: a `ModelDef` and its presets per file in `synth/`, `Model::def` the one match; supersedes the per-model answers in `model.rs` of 0009 | Accepted |
| [0026](0026-drum-hits-on-their-lanes-grid.md) | Drum hits on their lane's grid (/12 to /48), placed between the clock's steps and queued; flams' and drags' graces queued a step ahead; the 48-tick note grid unchanged | Accepted, extended (ratchets, #242) |
| [0027](0027-autocommit.md) | Autocommit: the engine folds live synth and mixer changes into the song and the text applies as it is typed; the commit buttons and Save setup go; supersedes the write-back rule of 0018 and the two files of 0015 | Accepted |
| [0028](0028-an-llm-proxy-beside-caddy.md) | An LLM proxy beside Caddy, on localhost only: a Rust (axum) assist server linking algo-dsp for its tools, one loop over five providers behind one trait, keys from 1Password, SSE to the Assistant; everything binds to 127.0.0.1, so no authentication for now | Accepted, amended by 0030 |
| [0029](0029-decks.md) | Decks: the main engine stays on the audio thread, up to three more in Web Workers rendering 4 blocks ahead into SharedArrayBuffer rings; a deck mixer in Rust, one clock per instance, a late deck drops its block; cross-origin isolation headers | Accepted |
| [0030](0030-one-server.md) | One binary, one server, one port: `algo-synth serve` on 127.0.0.1:6340 serves the app and `/api`, with isolation, no-cache and compression; one image; Caddy and the second process go | Accepted |
| [0031](0031-abletons-words.md) | Ableton's words: `frag` → `clip`, `section` → `scene`, `scene` → `snapshot` in the song, the engine, the app, the assistant and the docs; the parser still reads the old words, the printer writes the new | Accepted |

## Status and later decisions

- **Proposed:** decided in outline, not built yet. **Accepted:** built, or being built as decided.
- A later decision never rewrites an earlier ADR's Decision. The earlier ADR's status line names it (*amended by*, *superseded in part by*, *extended*, *clarified*), and a closing **Later decisions** section says in a line each what changed.
- A new ADR has the sections Context, Decision, Consequences and Alternatives considered.
