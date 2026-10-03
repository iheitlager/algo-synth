# 006: Polyphony

Epic #78: each synth owns a voice pool, and seven polyphonic instruments join the models of spec 005: Prophet-5, Juno-106, Jupiter-8, Matrix-12, PPG Wave, Roland D-50 and Yamaha DX7. Decisions: ADR-0001, ADR-0002, ADR-0004, ADR-0009, ADR-0011. Draft: requirements marked *(planned)* are not built yet.

Common to every requirement: `render` follows ADR-0002 (no allocation, no panic, no per-sample transcendentals), every parameter and id is mirrored in `web/src/audio/params.ts` (ADR-0004), and a model's sound is an interpretation of the instrument: each requirement names the property it must have and is tested on that. Tests render offline at 48 kHz.

### Requirement 1: Voice pool [MUST]

Each synth SHALL own a pool of 16 voices, allocated in `Engine::new`, of which a model uses `Model::voices()`. A monosynth model SHALL keep one voice per owner with note priority, legato and glide on that voice (spec 004 Req 6), so every monosynth renders exactly as it did before the pool. A poly model SHALL press each note on a voice of its own. A voice SHALL remember its owner (live keys or a MIDI channel) and its note, so a note-off releases its own voice, and live keys and MIDI channels on one synth SHALL NOT release each other. Rerouting a channel or `all_off` SHALL release its voices.

**Implementation:** `crates/dsp/src/poly.rs::Pool`, `crates/dsp/src/engine.rs::Engine` (#80)

#### Scenario: a chord

- GIVEN a poly model with 6 voices and a chord of 4 notes
- WHEN the notes are pressed and one is released
- THEN 4 voices sound, and after the release only that voice falls silent

#### Scenario: owners do not mix

- GIVEN live keys and a MIDI channel both playing a poly synth
- WHEN the channel releases a note the live keys also hold
- THEN only the channel's voice is released

**Tests:** `crates/dsp/src/poly.rs::tests::a_chord_sounds_one_voice_per_note_and_releases_its_own`, `crates/dsp/src/poly.rs::tests::a_monophonic_synth_keeps_one_voice_per_owner`, `crates/dsp/src/poly.rs::tests::owners_do_not_release_each_other`, `crates/dsp/src/engine.rs::tests::a_poly_synth_plays_chords_from_live_keys_and_a_channel`, `crates/dsp/src/mono/preset.rs::tests::arp_presets_keep_their_sound`

### Requirement 2: Allocation and stealing [MUST]

A new note SHALL take a free voice, rotating from the last one used. A note already sounding on its owner SHALL re-use that voice. When no voice is free, or the synth's polyphony (`Polyphony`, at most the model's voices) or the global voice budget (64) is full, the voice in release that has sounded longest SHALL be stolen, else the oldest held note, and a stolen voice SHALL restart clean.

**Implementation:** `crates/dsp/src/poly.rs::Pool::allocate` (#80)

#### Scenario: stealing

- GIVEN 6 voices, all held, and a seventh note
- WHEN it is pressed
- THEN the oldest held note is stolen, and with one voice in release that voice is stolen first

**Tests:** `crates/dsp/src/poly.rs::tests::a_full_pool_steals_the_release_first_then_the_oldest_held`, `crates/dsp/src/poly.rs::tests::rotation_does_not_cut_a_released_tail_first`, `crates/dsp/src/poly.rs::tests::the_same_note_again_retriggers_its_voice`, `crates/dsp/src/poly.rs::tests::every_sample_is_finite_and_bounded_at_full_polyphony`

### Requirement 3: Unison and analog variance [MUST]

`Assign` SHALL be Poly or Unison. In Unison a note presses on every voice of the pool up to `Polyphony`, spread by `UnisonDetune` (0..50 cents either side, evenly), and stays bounded. `Analog` (0..1) SHALL give each voice a seeded static detune (up to ±6 cents), a slow drift (up to ±3 cents) and a small cutoff offset (up to ±0.5 semitone), worked out once per block; the same seed SHALL give the same sound, and `Analog` 0 SHALL be exact.

**Implementation:** `crates/dsp/src/poly.rs::Pool`, `crates/dsp/src/mono/voice.rs::MonoVoice` (#81)

#### Scenario: unison width

- GIVEN unison on 6 voices with 20 cents of detune
- WHEN a note is held
- THEN six distinct pitches sound within ±20 cents of the note and the sum stays within ±1

**Tests:** `crates/dsp/src/poly.rs::tests::unison_spreads_every_voice_around_the_note`, `crates/dsp/src/poly.rs::tests::unison_falls_back_to_the_last_key_held`, `crates/dsp/src/poly.rs::tests::analog_variance_is_bounded_repeatable_and_off_at_zero`

### Requirement 4: Shared LFO [MUST]

A poly model's pool SHALL run one LFO and one sample-and-hold, and every voice SHALL read them, so a vibrato is the same on every voice and a voice started later joins it in phase.

**Implementation:** `crates/dsp/src/poly.rs::Pool::render`, `crates/dsp/src/mono/voice.rs::SharedMod` (#80)

#### Scenario: coherent vibrato

- GIVEN vibrato on two voices started a quarter of an LFO cycle apart
- WHEN both render
- THEN their pitch modulation is identical

**Tests:** `crates/dsp/src/poly.rs::tests::the_lfo_is_shared_by_every_voice`

### Requirement 5: Voice budget [MUST]

At most 64 voices SHALL sound at once across all synths; a note past it SHALL steal the oldest voice in release anywhere, else the oldest held note of its own pool. `make bench` SHALL measure full polyphony on every poly model against the 25% budget of plan.md.

**Implementation:** `crates/dsp/src/engine.rs::Engine::start_voice`, `tools/bench.mjs` (#80, #91)

#### Scenario: the cap

- GIVEN 16 synths of 8 voices each, all pressed
- WHEN the chords are held
- THEN no more than 64 voices sound and every sample is finite and within ±1

**Tests:** `crates/dsp/src/engine.rs::tests::the_voice_budget_caps_the_voices_across_synths`, `crates/dsp/src/engine.rs::tests::a_note_at_the_budget_takes_a_released_voice_first`

### Requirement 6: Prophet-5 [MUST]

The Prophet-5 SHALL have 5 voices of two oscillators (A is VCO 2, B is VCO 1, A synced to B), noise, poly-mod, a 4-pole low-pass in the Pro-One voicing, a filter ADSR and a loudness ADSR, an LFO and unison; `Analog` SHALL default to a clearly audible drift.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Prophet5`, `web/src/audio/models.ts` (#82)

#### Scenario: five voices

- GIVEN a five-note chord, then a sixth note
- WHEN they are pressed
- THEN five voices sound and the sixth steals one

**Tests:** `crates/dsp/src/engine.rs::tests::the_prophet_5_has_five_voices_and_the_sixth_steals_one`, `crates/dsp/src/engine.rs::tests::the_prophet_bass_plays_one_note_on_all_five_voices`, `crates/dsp/src/mono/model.rs::tests::polyphonic_models_have_their_own_voice_count`, `crates/dsp/src/mono/preset.rs::tests::every_poly_preset_plays_a_full_chord`

### Requirement 7: Juno-106 [MUST]

The Juno-106 SHALL have 6 voices of one DCO (saw and pulse with pulse-width modulation from the LFO or by hand, a sub-oscillator), noise, a 4-pole low-pass, a high-pass in four steps, one ADSR driving filter and loudness, an LFO, and a stereo BBD-style chorus with modes I, II and I+II that decorrelates the left and right outputs and is off for mode off.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Juno106`, `crates/dsp/src/fx/chorus.rs::Chorus`, `crates/dsp/src/mixer.rs::Mixer::widen` (#83)

#### Scenario: the chorus widens

- GIVEN a held note with the chorus off, then in mode II
- WHEN left and right are compared
- THEN off is identical on both sides and the chorus makes them differ

**Tests:** `crates/dsp/src/fx/chorus.rs::tests::off_is_identical_on_both_sides_and_the_chorus_widens`, `crates/dsp/src/fx/chorus.rs::tests::every_mode_stays_bounded_and_keeps_the_level`, `crates/dsp/src/fx/chorus.rs::tests::the_modes_sweep_at_their_own_rates`, `crates/dsp/src/engine.rs::tests::the_juno_chorus_makes_the_two_sides_differ`, `crates/dsp/src/engine.rs::tests::the_juno_106_has_six_voices`, `crates/dsp/src/mono/voice.rs::tests::juno_high_pass_steps_thin_the_bass`, `crates/dsp/src/mono/voice.rs::tests::sh101_saw_and_pulse_add_up`

### Requirement 8: Jupiter-8 [MUST]

The Jupiter-8 SHALL have 8 voices of two VCOs (VCO 2 synced to VCO 1) with cross-modulation of VCO 1 by VCO 2, noise, a low-pass of 12 dB or 24 dB per octave by a switch, a high-pass, two envelopes (filter and loudness), an LFO, and poly or unison assignment.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Jupiter8`, `crates/dsp/src/mono.rs::MonoParams::filter` (#84)

#### Scenario: the slope switch

- GIVEN the filter in 12 dB and in 24 dB
- WHEN the response is measured two and three octaves above the cutoff
- THEN it falls by about 12 and 24 dB per octave

**Tests:** `crates/dsp/src/mono/voice.rs::tests::jupiter_slope_switch_is_12_or_24_db_per_octave`, `crates/dsp/src/mono/voice.rs::tests::cross_mod_moves_vco1_from_vco2`, `crates/dsp/src/engine.rs::tests::the_jupiter_8_has_eight_voices`, `crates/dsp/src/mono/model.rs::tests::only_the_jupiter_has_a_slope_switch`

### Requirement 9: Matrix-12 [MUST]

The Matrix-12 SHALL have 12 voices of two oscillators with sync, a filter with a switchable slope and a high-pass, three envelopes and two LFOs, and a modulation matrix of 20 slots reading every source (including the second LFO and a ramp) into every destination, adding in the voice.

**Implementation:** `crates/dsp/src/mono/patch.rs::Patch`, `crates/dsp/src/mono/model.rs::Model::Matrix12` *(planned, #85)*

#### Scenario: the matrix

- GIVEN 20 slots joining the second LFO, the ramp and the envelopes to destinations
- WHEN a note is held
- THEN each destination receives the sum of its slots

**Tests:** *(planned)*

### Requirement 10: Table oscillator [MUST]

A table oscillator SHALL play single-cycle wavetables generated by the engine at start (no ROM data) with a position between waves, optional stepped positions, and an attack-plus-loop mode for sampled transients, reading its table by linear interpolation, with no allocation in `render`.

**Implementation:** `crates/dsp/src/table.rs::{Tables, TableOsc}` *(planned, #86)*

#### Scenario: pitch and position

- GIVEN a table oscillator on any wave at notes 24 to 108
- WHEN it renders
- THEN its pitch is within 1 cent, and moving the position moves between distinct spectra

**Tests:** *(planned)*

### Requirement 11: PPG Wave [MUST]

The PPG Wave SHALL have 8 voices of two wavetable oscillators whose wave position is moved by the filter envelope and the LFO, optional stepped positions, a 4-pole analog-style low-pass, two envelopes and an LFO.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::PpgWave` *(planned, #87)*

#### Scenario: a sweep

- GIVEN the filter envelope into the wave position
- WHEN a note is held
- THEN the spectrum changes over the envelope

**Tests:** *(planned)*

### Requirement 12: Roland D-50 [MUST]

The D-50 SHALL have 16 voices of two partials, each a synthesised oscillator (square or saw with pulse-width modulation) or a PCM attack-plus-loop from the table oscillator, each with its own resonant filter and envelopes, the pair added, synced or ring-modulated, and dual (layered) tones. Its PCM samples SHALL be generated by the engine, not taken from any ROM.

**Implementation:** `crates/dsp/src/la.rs::LaVoice`, `crates/dsp/src/mono/model.rs::Model::D50` *(planned, #88)*

#### Scenario: attack over a body

- GIVEN a PCM attack partial over a synthesised body
- WHEN a note is held
- THEN the transient sounds in the first tens of milliseconds and the body carries on

**Tests:** *(planned)*

### Requirement 13: FM voice and DX7 [MUST]

The DX7 SHALL have 16 voices of 6 sine operators, each with a frequency ratio or fixed frequency, detune (−7..+7), output level, velocity sensitivity and a 4-rate, 4-level envelope; 32 algorithms; feedback on operator 6; an LFO and a pitch envelope. An operator's carrier or modulator role SHALL follow the algorithm, and every output SHALL stay finite and bounded.

**Implementation:** `crates/dsp/src/fm.rs::FmVoice`, `crates/dsp/src/fm/algorithms.rs`, `crates/dsp/src/mono/model.rs::Model::Dx7` *(planned, #89)*

#### Scenario: algorithms

- GIVEN each of the 32 algorithms with a modulator at a high level
- WHEN a note renders
- THEN the carriers sound at the played pitch, and the modulators add partials only through their routes

**Tests:** *(planned)*

### Requirement 14: DX7 SysEx import [SHOULD]

The engine SHALL parse a DX7 single-voice SysEx message and a 32-voice bank (packed format) into the DX7 parameters, total over any input (it never panics, and reports the voices it could not read), and JavaScript SHALL only forward the bytes.

**Implementation:** `crates/dsp/src/fm/sysex.rs`, `crates/dsp/src/ffi.rs` *(planned, #90)*

#### Scenario: a bank

- GIVEN a packed bank of 32 voices and a malformed one
- WHEN they are parsed
- THEN the first yields 32 voices and the second an error without panic

**Tests:** *(planned)*

### Requirement 15: Polyphonic budget [MUST]

Chord pads on every poly model SHALL render within the performance budget, and every preset of every poly model SHALL be finite, bounded, audible and silent after release at full polyphony.

**Implementation:** `tools/bench.mjs`, `crates/dsp/src/mono/preset.rs` *(planned, #91)*

#### Scenario: all of them at once

- GIVEN a chord on each poly model
- WHEN they are held
- THEN the load is within 25% of a core and every sample is within ±1

**Tests:** *(planned)*
