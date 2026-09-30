# 003: The view

The wide-screen browser view in `web/`. Decision: ADR-0003.

### Requirement 1: Four-pane layout [MUST]

The view SHALL fill the window with a transport bar across the top, the **algo pane** on the left, the **instruments pane** on the right (twice its width), and the **arrangement** (tracks × bars) across the bottom.

**Implementation:** `web/src/App.vue`

### Requirement 2: Power on by gesture [MUST]

Audio SHALL start only from a user gesture (the Power button), creating the AudioContext, compiling `dsp.wasm`, loading `worklet.js` and connecting worklet → analyser → destination. A failure SHALL be shown in the transport bar.

**Implementation:** `web/src/audio/engine.ts::power`, `web/src/components/TransportBar.vue`

### Requirement 3: Play the sources [MUST]

Each source card SHALL be playable: an on-screen keyboard for Mono and Wave, eight pads for Drums (GM drum notes), and the computer keyboard (`a`…`;`, C4 upward) on the selected source.

**Implementation:** `web/src/components/InstrumentsPane.vue`

### Requirement 4: Scope [SHOULD]

The transport bar SHALL draw the output waveform from the AnalyserNode.

**Implementation:** `web/src/components/TransportBar.vue::draw`

### Requirement 5: Algo loops and arrangement views [MUST]

The algo pane SHALL list loops with generator, parameters, scale, target, seed, mode and a step preview. The arrangement SHALL show one row per track (name, source, inserts) and clips placed by bar, coloured by origin.

**Implementation:** `web/src/components/AlgoPane.vue`, `web/src/components/ArrangePane.vue`, `web/src/model/song.ts`

### Requirement 6: No music logic in the view [MUST]

The view SHALL only send messages and draw; sequencing, generation and synthesis SHALL live in the engine. (ADR-0001)

**Tests:** review
