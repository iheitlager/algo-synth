# 001: The engine

The wasm engine in `crates/dsp`: its C ABI, the render loop, voices and parameters. Decisions: ADR-0001, ADR-0002, ADR-0004.

### Requirement 1: C ABI without imports [MUST]

The engine SHALL compile to a `wasm32-unknown-unknown` module with no imports, exporting `memory`, `init`, `block_len`, `out_ptr`, `process`, `set_param`, `param_count`, `param_value`, `note_on`, `note_off`, `all_off` and `active_voices`, plus the MIDI import exports of spec 002 Req 9 and `mono_preset` (spec 004 Req 9), taking and returning numbers only. (ADR-0001)

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

**Tests:** `crates/dsp/src/engine/tests.rs::short_blocks_leave_the_tail_silent`

### Requirement 3: Voice allocation [MUST]

Every synth SHALL own a pool of voices, allocated in `Engine::new` (ADR-0011, spec 006 Req 1). On a monophonic model each owner (live input, each MIDI channel, each song track) SHALL have one voice of that pool (spec 004 Req 6); a polyphonic model SHALL press each note on a voice of its own; a drum kit or pad sampler SHALL hit pads on it instead of holding notes (spec 002 Req 1, spec 007). `note_off` SHALL release only that owner's key. A released voice SHALL end when its envelope does (spec 004 Req 4).

**Implementation:** `crates/dsp/src/engine.rs::Engine::note_on`, `crates/dsp/src/engine.rs::Engine::note_off`, `crates/dsp/src/poly.rs::Pool`

#### Scenario: note sounds then releases

- GIVEN a note on A3 with a 10 ms ADSR release
- WHEN it is held for 40 blocks and then released
- THEN the output peaks above 0.05 while held and reaches silence with no active voices after the release

#### Scenario: note off is per owner

- GIVEN notes held by live input and by MIDI channels 3 and 4
- WHEN channel 3's note is released
- THEN the live and channel 4 voices stay gated

**Tests:** `crates/dsp/src/engine/tests.rs::mono_follows_its_adsr`, `crates/dsp/src/engine/tests.rs::mono_owners_are_independent`

### Requirement 4: Bounded, finite output [MUST]

Output SHALL be finite and within ±1, whatever the input: the master limiter is the last stage (spec 002 Req 2) and turns NaN and infinity into silence. Parameters SHALL be clamped into range and NaN mapped to the lower bound; unknown parameter and source ids SHALL be ignored; an invalid sample rate SHALL fall back to 48 kHz. (ADR-0002)

**Implementation:** `crates/dsp/src/params.rs::Param::clamp`, `crates/dsp/src/engine.rs::Engine::new`, `crates/dsp/src/fx/limiter.rs::Limiter`

#### Scenario: sixteen loud voices

- GIVEN 16 voices at full master gain, resonance, drive and level
- WHEN 200 blocks are rendered
- THEN every sample is finite and within ±1, and the limiter is reached

#### Scenario: a full pool

- GIVEN 24 notes on one 16-voice pool
- WHEN it renders
- THEN all 16 voices sound and every sample of the pool's output is finite and within ±16, before the mixer

**Tests:** `crates/dsp/src/engine/tests.rs::loud_patches_are_limited_to_full_scale`, `crates/dsp/src/poly.rs::tests::every_sample_is_finite_and_bounded_at_full_polyphony`, `crates/dsp/src/fx/limiter.rs::tests::nothing_passes_full_scale_and_it_recovers`, `crates/dsp/src/params.rs::tests::clamp_rejects_nan_and_out_of_range`, `crates/dsp/src/engine/tests.rs::bad_sample_rate_falls_back`

### Requirement 5: No allocation or panic in render [MUST]

`render` SHALL NOT allocate, lock or panic, and SHALL NOT call transcendental functions per sample. Clippy SHALL deny `unwrap`, `expect`, `panic` and indexing in the engine. (ADR-0002)

**Implementation:** `Cargo.toml` (`[workspace.lints.clippy]`), `crates/dsp/src/voice.rs::lookup` (table lookups instead of per-sample transcendentals)

#### Scenario: a busy song allocates nothing

- GIVEN a song with drum lanes, chords, a live arp, automation and scenes, loaded and playing
- WHEN six bars are rendered under a counting allocator
- THEN `render` makes no allocation, reallocation or deallocation, and the song is heard

#### Scenario: the lints hold

- GIVEN the workspace
- WHEN `make lint` runs clippy natively and for `wasm32-unknown-unknown`
- THEN no `unwrap`, `expect`, `panic` or indexing is found

**Tests:** `crates/dsp/tests/render_no_alloc.rs::a_busy_song_renders_without_allocating` (#233), `make lint`

### Requirement 6: Parameter registry mirrored [MUST]

Every id list the engine exposes SHALL appear in `web/src/audio/params.ts` as a block of `Name: id,` lines with the same names and ids: `Param` (and its `GlobalParam` and `StripParam` subsets), the models, presets, waveforms, noise colours, note priorities, modulation sources and destinations, insert and processor types, the drum pads, the sampler's zone and pad fields, and the deck fields. (ADR-0004)

**Implementation:** `crates/dsp/src/params.rs::Param::ALL`, `crates/dsp/src/mono/patch.rs::ModSource::ALL`, `crates/dsp/src/mono/model.rs::Model::ALL`, `crates/dsp/src/sampler.rs::ZoneField::ALL`, `crates/dsp/src/padsampler.rs::PadField::ALL`, `crates/dsp/src/deck.rs::DeckField::ALL`

#### Scenario: a new parameter

- GIVEN a `Param` added in Rust but not in `params.ts`
- WHEN `cargo test` runs
- THEN `typescript_mirror_matches` fails and names the list

**Tests:** `crates/dsp/src/params.rs::tests::typescript_mirror_matches`, `crates/dsp/src/params.rs::tests::ids_round_trip`

### Requirement 7: Equal-tempered pitch [MUST]

MIDI note *n* SHALL sound at 440 · 2^((n − 69)/12) Hz.

**Implementation:** `crates/dsp/src/voice.rs::midi_to_hz`, `crates/dsp/src/mono/voice.rs::PitchTable` (per-sample pitch from a table, spec 004 Req 6)

#### Scenario: A4 and A5

- GIVEN notes 69 and 81
- WHEN they are turned into frequencies
- THEN they are 440 Hz and 880 Hz

**Tests:** `crates/dsp/src/voice.rs::tests::a4_is_440`, `crates/dsp/src/mono/voice.rs::tests::pitch_table_is_equal_tempered`

### Requirement 8: Decks [SHOULD]

The engine on the audio thread SHALL be deck A, and up to three more engines, decks B–D, SHALL each run in a Web Worker (ADR-0029, #391). A worker deck SHALL hold its own instance of the same `dsp.wasm` and render 4 blocks ahead of the worklet into a single-producer, single-consumer ring in a SharedArrayBuffer, counted in the worklet's block numbers, in order and without skipping a block unless it has fallen more than a second behind. The worklet SHALL copy a deck's block for the block it is about to render into the engine's input for that deck (`deck_in_ptr`, `deck_fed`) and count a block that came too late as dropped. The deck mixer SHALL sum deck A after its master with every fed deck, each through its level and its side of an equal-power crossfader (`deck_set`, `deck_crossfade`), ramping gains across the block, and limit the sum to ±1; an unfed deck SHALL be silent for that block, and with nothing fed and deck A at unity the output SHALL pass bit for bit. `render` SHALL not allocate with decks fed (Req 5).

**Implementation:** `crates/dsp/src/deck.rs::DeckMixer`, `crates/dsp/src/engine.rs::Engine::render`, `crates/dsp/src/ffi.rs` (`deck_in_ptr`, `deck_fed`, `deck_set`, `deck_crossfade`, `deck_peak`), `web/public/deck-worker.js`, `web/public/worklet.js` (`attachDeck`, `feedDecks`, `readDecks`), `web/src/audio/decks.ts`

#### Scenario: a deck fed for one block

- GIVEN deck C's input filled with 0.25 on the left and −0.25 on the right, and nothing else playing
- WHEN the engine renders a block, and then another without feeding it
- THEN the first block is 0.25 and −0.25 throughout, and the second is silent

#### Scenario: the crossfader in the middle

- GIVEN deck A on the left of the crossfader and deck B on the right
- WHEN the crossfader moves anywhere from 0 to 1
- THEN the squares of the two gains add up to 1

**Tests:** `crates/dsp/src/deck.rs::tests`, `crates/dsp/src/engine/tests.rs::a_fed_deck_joins_the_output`, `crates/dsp/tests/render_no_alloc.rs::a_busy_song_renders_without_allocating`
