# 003: The view

The wide-screen browser view in `web/`. Decision: ADR-0003.

### Requirement 1: Three-area layout [MUST]

The view SHALL fill the window with a transport bar across the top, the **synths** in the middle, and the **MIDI player** (one row per channel) across the bottom. The algo pane and the arrangement are removed for now (ADR-0008).

**Implementation:** `web/src/App.vue`

### Requirement 2: Power on by gesture [MUST]

Audio SHALL start only from a user gesture (the Power button), creating the AudioContext, compiling `dsp.wasm`, loading `worklet.js` and connecting worklet → analyser → destination. A failure SHALL be shown in the transport bar.

**Implementation:** `web/src/audio/engine.ts::power`, `web/src/components/TransportBar.vue`

### Requirement 3: Play the synths [MUST]

The view SHALL show one card per Mono synth, each with its own controls and on-screen keyboard; **+ Synth** SHALL add one (up to 16, reset to the default patch) and × SHALL remove one (never the last), muting the parts that played on it. The computer keyboard (`a`…`;`, C4 upward) SHALL play the selected synth, and a held key SHALL release on the synth it started on. Loading a MIDI file SHALL show a synth for each part, and each part SHALL pick its synth or mute.

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

### Requirement 7: Synth setups [SHOULD]

The view SHALL save the synths on screen, each one's kind, model and parameters, the channel routing and the global parameters (`GlobalParam`) as a versioned `.synths.json` file, with parameters keyed by name, not id. **Open…** SHALL take a MIDI file, a setup, or both in either order; with a MIDI file the setup SHALL apply once the parts arrive, alone it SHALL apply to the synths on screen. Applying SHALL reset each listed synth, set its model before its other parameters, then the routes and the globals. A file that isn't JSON or has an unknown version SHALL change nothing; unknown names, models and synths SHALL be skipped and listed in one notice, and a part-count mismatch SHALL warn. The last setup per MIDI file SHALL be kept in `localStorage` and restored when that file opens again, below a picked file and above a shipped one; Demo ships one. A parameter added to the registry SHALL be saved without changes to the setup code. The format is built in the view from values the engine reports, which clamps every value it receives (#41).

**Implementation:** `web/src/audio/setup.ts` (`buildSetup`, `parseSetup`, `applyPlan`, `shortF32`), `web/src/audio/engine.ts` (`saveSetup`, `openFiles`), `crates/dsp/src/params.rs::Param::is_global`, `web/public/demo.synths.json`

#### Scenario: round trip

- GIVEN a setup built from the view's values
- WHEN it is written as JSON, read back and applied
- THEN every per-synth parameter, global and route is the same 32-bit float or synth as before

#### Scenario: unknown entries

- GIVEN a setup with an unknown parameter, model, kind, a duplicate and an out-of-range synth
- WHEN it is read
- THEN the rest applies and each skip is listed once

**Tests:** `web/src/audio/setup.test.ts`, `crates/dsp/src/params.rs::tests::typescript_mirror_matches`, `crates/dsp/src/params.rs::tests::ids_round_trip`
