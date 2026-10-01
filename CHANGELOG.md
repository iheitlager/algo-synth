# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Spec 004: the Mono voice (plan.md MVP 2, epic #2), Req 1-9 with measurable scenarios; spec 001 Req 3 notes Mono's own voices (#3).
- `mono::osc`: three VCOs for Mono (saw, pulse, triangle, sine) with coarse and fine tune, level, pulse width and hard sync of VCO 2/3 to VCO 1, band-limited by a BLEP table; 15 new parameters and a `Waveform` id list, mirrored in `params.ts`; VCO controls on the Mono card (#4).
- `mono::noise`: white (seeded xorshift32) and pink (Paul Kellet's filter plus a DC blocker) noise into the Mono mixer, with level and colour parameters and a mirrored `NoiseColour` id list; the drums share the generator (#5).
- ADR-0007: BLEP-table oscillators instead of polyBLEP, which leaves aliases at −22 dB where spec 004 asks for −60 dB.

### Changed

- Spec 004 Req 1: BLEP table instead of polyBLEP; the sync bound is ±1.5 (band-limited steps ring), plus a pulse-width sweep scenario.
- Mono output lags the other sources by 8 samples (0.17 ms), the BLEP kernel's half-width.

## [0.2.0] - 2026-10-01

A MIDI file player in the engine, ahead of the plan's order (plan.md MVP 1b, spec 002 Req 9).

### Added

- `smf`: a total Standard MIDI File parser (types 0 and 1, running status, tempo and track names); malformed input returns an error, never a panic.
- `player`: a file compiled once to sample-timed events through the tempo map; a transport (play, stop, seek, position) run inside `render`.
- Per-channel routing to Mono, Wave, Drums or mute, channel 10 defaulting to Drums; a bad file keeps the loaded song.
- `voice`: a first timbre per source (Mono: polyBLEP saw through a key-tracked low-pass; Wave: sine plus second harmonic; Drums: kick, toms, snare and hats chosen by GM note).
- C ABI exports for loading, parts, events, transport and routing; the worklet and `web/src/audio/engine.ts` carry them.
- `web/public/demo.mid`, Pachelbel's Canon, written by `tools/make_demo_mid.py` (public domain, no third-party licence).
- `clippy.toml`: the panic lints are relaxed inside tests.
- The view: Demo, Open MIDI…, Play/Stop and the position in the transport; a parts pane with a source picker, piano roll, playhead and click to seek per part.

### Changed

- Spec 001 Req 1 names the player exports; spec 002 Req 8 records the parser, Req 9 the playback.

## [0.1.0] - 2026-09-30

The base: the pipeline from Rust to the speakers, and the plan from one mono voice to a true algo synth (plan.md MVP 1).

### Added

- `crates/dsp`: the engine as a `wasm32-unknown-unknown` cdylib with a plain C ABI and no imports (ADR-0001). One engine per wasm instance, planar stereo 128-frame blocks, a 16-voice test voice (table sine, AR envelope) with oldest-voice stealing, master gain.
- Parameter and source registries (`Param`, `Source`: Mono, Wave, Drums) mirrored in `web/src/audio/params.ts`, the mirror checked by `cargo test` (ADR-0004).
- Real-time rules as lints: no `unwrap`, `expect`, `panic` or indexing; `unsafe` only in `ffi.rs` (ADR-0002).
- `web/`: Vue 3 + TypeScript view with transport (power, master, scope), algo pane, instruments pane (keyboards, drum pads, computer keyboard) and bar arrangement, over a static demo song (ADR-0003, ADR-0005).
- `web/public/worklet.js`: the AudioWorklet shim.
- Podman + Caddy image serving static files on port 6340; Vite dev on 6341 (ADR-0006).
- Grouped `make help`, `make check`, CI with actions pinned by commit, `cargo deny`.
- `.openspec/`: vision, plan (five milestones, eleven MVPs, the Vivaldi ensemble as second base), ADRs 0001-0006, specs 001-003.
