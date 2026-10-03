# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.4.0] - 2026-10-03

A playable Mono (plan.md MVP 2, epic #2, PR #17): note handling and routing.

### Added

- `mono::voice`: one monophonic Mono voice per owner (live input and each MIDI channel), allocated in `Engine::new` instead of the shared pool. A 16-key stack with note priority (last, low, high), legato (a new key while one is held keeps the envelope; release falls back to the next held key) and glide (0-5 s, linear in semitones, exact in time). `Priority`, `Legato` and `Glide` parameters with a mirrored `NotePriority` id list, and a Keys row on the Mono card (#8).
- `mono::patch`: normalled routing and an 8-slot patch (spec 004 Req 7). Normals with amounts: ADSR → cutoff (`EnvCutoff`), key tracking (`KeyTrack`), LFO × mod wheel → pitch (`Vibrato`, `ModWheel`), ADSR → VCA. Sources VCO 1-3, noise, ADSR, AR, LFO, S&H, mod wheel, velocity, key; destinations VCO 1-3 pitch, pulse width, cutoff, resonance, VCA, LFO rate. An override replaces the normals to its destination. 28 parameters (24 patch fields, 4 normals) and `ModSource`/`ModDest` ids, mirrored; LFO, AR, normal and patch rows on the Mono card. The AR, LFO and S&H are audible now (#9).
- Presets carry normals and patches: Sync lead's ADSR → VCO 2 sweep, Bowed string's vibrato and velocity → cutoff, filter envelopes and key tracking on Bass and Lead (#9).
- `PitchTable`: note to increment in 1/16-semitone steps, so Mono's pitch moves every sample with no `exp2` in `render` (about 0.003 cents of error) (#8).

### Changed

- plan.md: note handling and routing are back in MVP 2 (instruments first); MVP 5 is the ensemble and the score again.
- `make bench` plays its 16 Mono voices from a 16-channel MIDI file, since live Mono is monophonic now (2.2% and 3.9% of a core).

## [0.3.0] - 2026-10-01

The Mono voice (plan.md MVP 2, epic #2, PR #15). Note handling and routing move to MVP 5, Web MIDI input to MVP 11.

### Added

- Spec 004: the Mono voice (plan.md MVP 2, epic #2), Req 1-9 with measurable scenarios; spec 001 Req 3 notes Mono's own voices (#3).
- `mono::osc`: three VCOs for Mono (saw, pulse, triangle, sine) with coarse and fine tune, level, pulse width and hard sync of VCO 2/3 to VCO 1, band-limited by a BLEP table; 15 new parameters and a `Waveform` id list, mirrored in `params.ts`; VCO controls on the Mono card (#4).
- `mono::noise`: white (seeded xorshift32) and pink (Paul Kellet's filter plus a DC blocker) noise into the Mono mixer, with level and colour parameters and a mirrored `NoiseColour` id list; the drums share the generator (#5).
- `mono::ladder`: a 4-pole zero-delay-feedback ladder low-pass with cutoff, resonance (self-oscillating from 0.8) and drive, replacing Mono's one-pole; `g` from a table built in `Engine::new`, cutoff smoothed in pitch over 2 ms, input saturated by a rational `tanh`; three parameters and a Ladder row on the Mono card (#6).
- `mono::env`: ADSR and AR envelopes with RC-style curves, each segment taking its set time from where it starts (within 1 ms from 1 ms to 10 s); the ADSR is Mono's VCA, with sliders on the Mono card. `mono::lfo`: an LFO (sine, triangle, saw, square, 0.01-50 Hz) and a sample-and-hold on noise; eight parameters mirrored in `params.ts`. The AR, LFO and S&H get destinations with routing (#9) (#7).
- `mono::preset`: Bass, Lead, Sync lead and Bowed string, as Rust data over one `DEFAULTS` table that is also the voice's starting state; a preset picker on the Mono card. Exports `mono_preset`, `param_count` and `param_value`: the worklet reports every parameter's value at start and after a preset, and the Mono controls show the engine's values instead of TypeScript defaults. Presets set parameters only until routing (#9) (#11).
- `make bench` (`tools/bench.mjs`): 16 Mono voices on the built `dsp.wasm` in Node's V8, a realistic and a worst-case patch, against the 25% budget; parameter ids read from `params.ts`. A performance counter in the transport bar: DSP load (average and peak share of real time per audio callback) and sounding voices, from the worklet about twice a second; new export `active_voices` (#12).
- ADR-0007: BLEP-table oscillators instead of polyBLEP, which leaves aliases at −22 dB where spec 004 asks for −60 dB.

### Changed

- Scope of MVP 2 (plan.md): Mono note handling and normalled routing (#8, #9) move to MVP 5, Web MIDI input (#10) to MVP 11; MVP 2 ends with presets. Until routing, the LFO, AR and sample-and-hold have no destination. Profiling now names Chrome DevTools' WebAudio panel, since `chrome://webaudio-internals` is gone (ADR-0002).
- The master output soft-clips: unchanged below 0.5, never past ±1, NaN and infinity silenced (PR #15 review).
- Spec 004 Req 1: BLEP table instead of polyBLEP; the sync bound is ±1.5 (band-limited steps ring), plus a pulse-width sweep scenario.
- Mono output lags the other sources by 8 samples (0.17 ms), the BLEP kernel's half-width.
- Mono's amplitude follows its own ADSR (`Adsr*` parameters), not the test voice's `Attack`/`Release`, which now shape only Wave.
- Mono's filter sits at a fixed cutoff (4 kHz by default) instead of tracking the key; key tracking returns as a normalled connection with routing (#9).

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
