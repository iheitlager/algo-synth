# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- The mixer owns every strip parameter (`Param::is_strip`) and has four post-fader sends: `EchoSend` and `ReverbSend` become `Send1` and `Send2`, `Send3` and `Send4` are new (P1–P4). The mixer pane replaces the Returns pane and the Channel section on each card; the drive insert stays on the synth (#45, epic #50).
- The send effects are four processors P1–P4, each an Off, Echo or Reverb with a return and five 0..1 knobs whose meaning depends on the type. `EchoTime`…`ReverbReturn` (ids 70–78) are replaced by `P1Type`…`P4E`, and later parameter ids move up by 19; setups store names, so only the old effect names need a migration (#46, epic #50).
- Master compressor (feed-forward, stereo-linked; `CompThreshold`, `CompRatio`, `CompAttack`, `CompRelease`, `CompMakeup`, ids 120–124; ratio 1 is off) with a gain-reduction meter, and a brick-wall limiter in place of the soft clip: master gain now comes after the compressor, and nothing is bent below full scale any more (#47, epic #50).
- Master equalizer before the compressor: a low shelf, two parametric bands and a high shelf (`EqLowFreq`…`EqHighGain`, ids 125–134, ±15 dB); bands at 0 dB are skipped, so flat is bit-exact (#48, epic #50).

## [0.7.0] - 2026-10-03

Synth setups next to the MIDI file (#41, PR #44).

### Added

- Synth setups (#41): **Save setup** downloads `<song>.synths.json` with the synths on screen, each one's kind, model and parameters by name, the channel routing and the global parameters (master gain, echo, reverb); **Open…** takes a MIDI file, a setup or both. Unknown entries are skipped with one notice, a bad file changes nothing, a part-count mismatch warns. The last setup per MIDI file is kept in `localStorage`; Demo ships `demo.synths.json` (Bass and three Bowed string violins a little apart). Values are written as the shortest decimal for the same f32. Spec 003 Req 7.
- `GlobalParam` in `params.ts`: the parameters `Param::is_global` marks, mirrored and checked by the mirror test, so a setup stores them once (#41).
- vitest for the view's pure functions (`make test-web`, part of `make test` and CI) (#41).

## [0.6.0] - 2026-10-03

The family of monosynths (epic #28, PR #42).

### Added

- **A family of six monosynths** (epic #28, ADR-0009, spec 005): every synth slot has a model, `Param::Model`: ARP 2600, Minimoog, Sequential Pro-One, Korg MS-20, Yamaha CS-15 or Roland SH-101. One shared voice with the model deciding the filter and its voicing, the high-pass stage, which envelope moves the cutoff, decay-as-release and the modulation source; two presets per model (14 in all). A new or reset synth is an ARP 2600, and its sound is pinned to what it was before models (#29, #30, #34).
- **Filter ADSR** (`FenvAttack`..`FenvRelease`, modulation source `Fenv`): the ARP 2600 and SH-101 keep one envelope for filter and loudness, the others have two (#31).
- **Filter flavours:** a 12 dB state-variable filter with saturating states and a one-pole high-pass, three ladder voicings (Moog, Pro-One, SH-101) and two 12 dB voicings (the MS-20 self-oscillates, the CS-15 does not); `HpCutoff`, `HpResonance`, `EnvHpCutoff` (#32).
- **Ring modulator, sub-oscillator and Osc 3 as a modulator:** `RingLevel` (VCO 1 × VCO 2), `SubLevel`/`SubOctave` (a band-limited square at an exact half or quarter of VCO 1's pitch), `Vco3KeyFollow`/`Vco3Low` (#33).
- **Poly-mod and LFO destinations:** `EnvFreq2`, `OscFreq2`, `EnvPw`, `OscPw`, `OscCutoff` (Pro-One style) and `LfoCutoff`, `LfoPw`; they add after the normals and the patch. On the Minimoog, which has no LFO, Osc 3 is the modulation source (#35, #36).
- **Per-model panels and palettes:** the view draws each synth from its model's description (`models.ts`, `SynthPanel.vue`) with that instrument's sections, control names and colours, a model picker and a preset list per model (#30, #34-#39).
- `make bench` has `six models` and `family worst` scenarios: 16 synths across the family at 3.9% and 5.0% of a core (#40).

## [0.5.0] - 2026-10-03

### Added

- Send effects: a stereo echo (up to 2 s in ms, feedback below 1, tone, ping-pong) and an 8-line Hadamard reverb (size, damping, pre-delay), fed by each synth's sends and returned into the master; nine global parameters (ids 70–78) and a Returns strip. `make bench` now runs the whole chain (#26).
- Drive insert on each synth's bus: Off, Overdrive, Distortion and Fuzz with first-order ADAA, plus `DriveMode`, `DriveAmount`, `DriveTone`, `DriveLevel` (ids 66–69, presets may set them) and a Drive row on each card. Spec 004 Req 11 (#25).
- Mixer: each synth has its own bus with `Level`, `Pan` (equal power), `EchoSend`, `ReverbSend` (post-fader), `Mute` and `Solo`, summed into a stereo master before the soft clip; a mixer row on each card. Parameter ids 60–65 (#24).
- Up to 16 Mono synths, each with its own parameters, values and patch, preallocated in `Engine::new`; `MasterGain` stays global. Each synth has its own live voice, and a voice keeps the synth its note started on. Loading a MIDI file puts its parts on synths 0, 1, 2…; each part picks a synth or mutes. New exports `synth_count` and `synth_reset`; `set_param`, `param_value`, `mono_preset`, `note_on`, `note_off` take the synth first, and `route(channel, synth)` mutes on an unknown synth. Spec 004 Req 10 (#19).
- The view: one card per synth, colour-coded, with **+ Synth** and × (removing one mutes its parts); the keyboard plays the selected synth, and a held key releases on the synth it started on. `make bench` sets up all 16 synths (3.3% and 5.8% of a core) (#19).

### Removed

- Wave and Drums (the 32-voice pool and first timbres in `voice.rs`, `Source` and its mirror, Wave's `Attack`/`Release` parameters, the Wave/Drums cards and pads), the algo pane, the arrangement pane and their demo model (`model/song.ts`). The demo MIDI file loses its drum part. ADR-0008: Mono only, straight to the ensemble (#18).

### Changed

- C ABI: `note_on(note, velocity)` and `note_off(note)` lose the source argument; `route(channel, target)` plays on Mono with target 0 and mutes with anything else; every channel starts playing. Parameter ids from `Vco1Wave` on move down by two (#18).
- Layout: transport, synths, MIDI player; plan.md goes from MVP 2 straight to MVP 5 (M2, M4, M5 deferred); spec 001 Req 3, 002 Req 9 and 003 Req 1, 3, 5 follow (#18).

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
