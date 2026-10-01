# 001: The engine

The wasm engine in `crates/dsp`: its C ABI, the render loop, voices and parameters. Decisions: ADR-0001, ADR-0002, ADR-0004.

### Requirement 1: C ABI without imports [MUST]

The engine SHALL compile to a `wasm32-unknown-unknown` module with no imports, exporting `memory`, `init`, `block_len`, `out_ptr`, `process`, `set_param`, `param_count`, `param_value`, `note_on`, `note_off`, `all_off` and `active_voices`, plus the MIDI player exports of spec 002 Req 9 and `mono_preset` (spec 004 Req 9), taking and returning numbers only. (ADR-0001)

**Implementation:** `crates/dsp/src/ffi.rs`

#### Scenario: exports before init

- GIVEN a fresh instance
- WHEN `process`, `note_on` or `set_param` is called before `init`
- THEN nothing happens and `out_ptr` returns null

#### Scenario: exports drive the engine

- GIVEN `init(48000)`
- WHEN `note_on(0, 69, 1.0)` and `process(128)` are called
- THEN one voice is active and `out_ptr` is non-null

**Tests:** `crates/dsp/src/ffi.rs::tests::exports_are_no_ops_before_init`, `crates/dsp/src/ffi.rs::tests::exports_drive_the_engine`

### Requirement 2: Planar stereo blocks [MUST]

`process(frames)` SHALL render at most `block_len()` (128) frames into a planar buffer at `out_ptr()`: `block_len` left samples, then `block_len` right samples. Frames beyond `frames` SHALL be silent.

**Implementation:** `crates/dsp/src/engine.rs::Engine::render`

#### Scenario: short block

- GIVEN a sounding voice
- WHEN `render(64)` is called
- THEN samples 64..128 of each channel are zero

**Tests:** `crates/dsp/src/engine.rs::tests::short_blocks_leave_the_tail_silent`

### Requirement 3: Voice allocation [MUST]

`note_on` SHALL take a free voice, or steal the oldest when the pool is full. `note_off` SHALL release only gated voices of that source and note. A released voice SHALL free itself when its envelope falls below −80 dB, and the Release parameter SHALL be the time it takes to get there. Mono will be the exception from MVP 5: one monophonic voice per owner, outside this pool (spec 004 Req 6, planned). Until then Mono takes voices from this pool, and its own ADSR, not the Release parameter, ends them (spec 004 Req 4).

**Implementation:** `crates/dsp/src/engine.rs::Engine::note_on`, `crates/dsp/src/engine.rs::Engine::note_off`

#### Scenario: note sounds then releases

- GIVEN a note on A4
- WHEN it is held for 10 blocks and then released
- THEN the output peaks above 0.1 while held and reaches silence with no active voices after the release

#### Scenario: note off is per source

- GIVEN note 60 on Mono and on Drums
- WHEN note 60 is released on Drums
- THEN the Mono voice stays gated

**Tests:** `crates/dsp/src/engine.rs::tests::note_sounds_then_releases_to_silence`, `crates/dsp/src/engine.rs::tests::release_time_is_time_to_silence`, `crates/dsp/src/engine.rs::tests::note_off_only_touches_its_source`

### Requirement 4: Bounded, finite output [MUST]

Output SHALL be finite and bounded by the voice count times the master gain, whatever the input. Parameters SHALL be clamped into range and NaN mapped to the lower bound; unknown parameter and source ids SHALL be ignored; an invalid sample rate SHALL fall back to 48 kHz. (ADR-0002)

**Implementation:** `crates/dsp/src/params.rs::Param::clamp`, `crates/dsp/src/engine.rs::Engine::new`

#### Scenario: full pool

- GIVEN 40 notes on a 16-voice pool at full gain
- WHEN a block is rendered
- THEN every sample is finite and within ±16

**Tests:** `crates/dsp/src/engine.rs::tests::output_stays_finite_and_bounded_when_the_pool_is_full`, `crates/dsp/src/params.rs::tests::clamp_rejects_nan_and_out_of_range`, `crates/dsp/src/engine.rs::tests::bad_sample_rate_falls_back`

### Requirement 5: No allocation or panic in render [MUST]

`render` SHALL NOT allocate, lock or panic, and SHALL NOT call transcendental functions per sample. Clippy SHALL deny `unwrap`, `expect`, `panic` and indexing in the engine. (ADR-0002)

**Implementation:** `Cargo.toml` (`[workspace.lints.clippy]`), `crates/dsp/src/engine.rs::lookup`

**Tests:** `make lint`

### Requirement 6: Parameter registry mirrored [MUST]

Every `Param` and `Source` id SHALL appear in `web/src/audio/params.ts` as a `Name: id,` line. (ADR-0004)

**Implementation:** `crates/dsp/src/params.rs::Param::ALL`, `crates/dsp/src/source.rs::Source::ALL`

**Tests:** `crates/dsp/src/params.rs::tests::typescript_mirror_matches`, `crates/dsp/src/params.rs::tests::ids_round_trip`

### Requirement 7: Equal-tempered pitch [MUST]

MIDI note *n* SHALL sound at 440 · 2^((n − 69)/12) Hz.

**Implementation:** `crates/dsp/src/engine.rs::midi_to_hz`

**Tests:** `crates/dsp/src/engine.rs::tests::a4_is_440`
