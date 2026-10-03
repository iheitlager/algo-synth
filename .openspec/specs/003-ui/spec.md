# 003: The view

The wide-screen browser view in `web/`. Decision: ADR-0003.

### Requirement 1: Three-area layout [MUST]

The view SHALL fill the window with a transport bar across the top, the **synths** in the middle, and the **MIDI player** (one row per channel) across the bottom. The algo pane and the arrangement are removed for now (ADR-0008).

**Implementation:** `web/src/App.vue`

### Requirement 2: Power on by gesture [MUST]

Audio SHALL start only from a user gesture (the Power button), creating the AudioContext, compiling `dsp.wasm`, loading `worklet.js` and connecting worklet → analyser → destination. A failure SHALL be shown in the transport bar.

**Implementation:** `web/src/audio/engine.ts::power`, `web/src/components/TransportBar.vue`

### Requirement 3: Play the synth [MUST]

The Mono card SHALL be playable from its on-screen keyboard and the computer keyboard (`a`…`;`, C4 upward).

**Implementation:** `web/src/components/InstrumentsPane.vue`

### Requirement 4: Scope [SHOULD]

The transport bar SHALL draw the output waveform from the AnalyserNode.

**Implementation:** `web/src/components/TransportBar.vue::draw`

### Requirement 5: Algo loops and arrangement views [MUST]

*Deferred (ADR-0008); the mock-up panes are removed.* The algo pane SHALL list loops with generator, parameters, scale, target, seed, mode and a step preview. The arrangement SHALL show one row per track (name, source, inserts) and clips placed by bar, coloured by origin.

**Implementation:** *(planned, plan.md MVP 4 and MVP 9)*

### Requirement 6: No music logic in the view [MUST]

The view SHALL only send messages and draw; sequencing, generation and synthesis SHALL live in the engine. (ADR-0001)

**Tests:** review
