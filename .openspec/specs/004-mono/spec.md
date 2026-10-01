# 004: The Mono voice

The ARP 2600-style semi-modular monophonic voice in `crates/dsp/src/mono/`: oscillators, noise, filter, modulation, note handling, normalled routing, MIDI input and presets. Decisions: ADR-0001, ADR-0002, ADR-0004, ADR-0007. Draft: requirements marked *(planned)* are not built yet (plan.md MVP 2, epic #2); paths name where the code will land.

Common to every requirement: `render` follows ADR-0002 (no allocation, no panic, no per-sample transcendentals), every new parameter and id is mirrored in `web/src/audio/params.ts` (ADR-0004), and parameters are Mono-wide until tracks address them as (track, parameter) in MVP 4 (spec 002 Req 1). Tests render offline at 48 kHz.

### Requirement 1: Oscillators [MUST]

The voice SHALL have three VCOs, each with saw, pulse, triangle and sine waveforms, a coarse tune in semitones, a fine tune in cents and a level into the mixer. Saw and pulse SHALL be band-limited by a band-limited step (BLEP) table (ADR-0007). Pulse width SHALL range from 5% to 95% and MAY be modulated (PWM); a changing width SHALL NOT click. VCO 2 and VCO 3 MAY hard-sync to VCO 1, and a sync reset SHALL be band-limited too.

**Implementation:** `crates/dsp/src/mono/osc.rs::Osc`, `crates/dsp/src/mono.rs::MonoParams` (#4)

#### Scenario: pitch

- GIVEN one VCO at any note from 24 to 108 and fine tune 0
- WHEN one second is rendered
- THEN the measured frequency is within 1 cent of spec 001 Req 7

#### Scenario: aliasing

- GIVEN a 5 kHz saw
- WHEN its spectrum is measured
- THEN every component that is not a harmonic is below −60 dB relative to the fundamental

#### Scenario: sync stays bounded

- GIVEN VCO 2 synced to VCO 1 at any ratio from 1 to 8
- WHEN a sweep of VCO 2's pitch is rendered
- THEN every sample is finite and within ±1.5 (a band-limited step rings by about 9% of its height, and a sync reset can land within a sample of another edge)

#### Scenario: pulse-width sweep

- GIVEN a 220 Hz pulse
- WHEN its width is swept from 5% to 95% over one second
- THEN no sample-to-sample step is larger than a fixed-width pulse already has

**Tests:** `crates/dsp/src/mono/osc.rs::tests::pitch_within_a_cent`, `crates/dsp/src/mono/osc.rs::tests::saw_aliasing_below_60_db`, `crates/dsp/src/mono/osc.rs::tests::sync_is_bounded`, `crates/dsp/src/mono/osc.rs::tests::pwm_sweep_has_no_clicks`, `crates/dsp/src/mono/osc.rs::tests::pwm_sweep_keeps_one_pair_of_edges_per_cycle`, `crates/dsp/src/mono/osc.rs::tests::sync_locks_slave_to_master_period`, `crates/dsp/src/mono/osc.rs::tests::narrow_pulses_are_bounded`, `crates/dsp/src/mono.rs::tests::coarse_and_fine_set_the_ratio`

### Requirement 2: Noise [MUST]

The voice SHALL have a white and a pink noise source from a seeded generator, with a colour switch and a level into the mixer; the same seed SHALL give the same samples. Neither SHALL have a DC offset: pink, whose filter passes DC, goes through a DC blocker.

**Implementation:** `crates/dsp/src/mono/noise.rs::Noise` (#5)

#### Scenario: spectral slope

- GIVEN ten seconds of each noise colour
- WHEN the power per octave is measured from 100 Hz to 10 kHz
- THEN white is flat and pink falls 3 dB per octave, both within ±1 dB

**Tests:** `crates/dsp/src/mono/noise.rs::tests::white_is_flat`, `crates/dsp/src/mono/noise.rs::tests::pink_falls_3_db_per_octave`, `crates/dsp/src/mono/noise.rs::tests::same_seed_same_noise`, `crates/dsp/src/mono/noise.rs::tests::no_dc_and_bounded`

### Requirement 3: Ladder filter [MUST]

The voice SHALL filter the mixed oscillators and noise through a 4-pole zero-delay-feedback ladder low-pass with cutoff, resonance and drive. Cutoff SHALL be smoothed at control rate and the filter coefficient recomputed only when the smoothed cutoff changes. Drive SHALL saturate without a per-sample `tanh` (a rational approximation or a table). The filter coefficient SHALL come from a table built in `Engine::new`, so modulating the cutoff costs no transcendental math either. Resonance SHALL self-oscillate from 0.8 of its range.

**Implementation:** `crates/dsp/src/mono/ladder.rs::Ladder`, `crates/dsp/src/mono/ladder.rs::LadderTables` (#6)

#### Scenario: slope

- GIVEN a cutoff of 1 kHz and no resonance
- WHEN the response is measured at 4 kHz and 8 kHz
- THEN it falls 24 dB per octave, within ±2 dB

#### Scenario: self-oscillation is bounded

- GIVEN maximum resonance and drive, with no input
- WHEN a cutoff sweep from 20 Hz to 20 kHz is rendered
- THEN the filter oscillates near the cutoff and every sample is finite and within ±2

**Tests:** `crates/dsp/src/mono/ladder.rs::tests::falls_24_db_per_octave`, `crates/dsp/src/mono/ladder.rs::tests::self_oscillation_is_bounded`, `crates/dsp/src/mono/ladder.rs::tests::any_parameters_stay_finite`, `crates/dsp/src/mono/ladder.rs::tests::cutoff_is_smoothed`, `crates/dsp/src/mono/ladder.rs::tests::table_matches_tan`

### Requirement 4: Envelopes [MUST]

The voice SHALL have an ADSR and an AR envelope; the ADSR SHALL drive the VCA. Each segment SHALL take its set time from wherever it starts, curved as an RC envelope is. Their rates SHALL be computed when a time parameter or the gate changes, not per sample. A gate that opens during release SHALL continue from the current level, without a jump; across notes this needs one voice per owner (Req 6, MVP 5), since until then each note starts a fresh voice. A note shorter than one block SHALL still sound, held for that block. The sustain level SHALL follow its parameter while a note is held.

**Implementation:** `crates/dsp/src/mono/env.rs::Env`, `crates/dsp/src/voice.rs::Voice` (#7)

#### Scenario: segment times

- GIVEN attack, decay and release times between 1 ms and 10 s
- WHEN the ADSR is gated, held and released offline
- THEN each segment reaches its target within ±1 ms of its set time

**Tests:** `crates/dsp/src/mono/env.rs::tests::segment_times`, `crates/dsp/src/mono/env.rs::tests::retrigger_does_not_jump`, `crates/dsp/src/mono/env.rs::tests::ar_holds_at_full_level`, `crates/dsp/src/engine.rs::tests::mono_follows_its_adsr`, `crates/dsp/src/engine.rs::tests::a_mono_tap_shorter_than_a_block_sounds`, `crates/dsp/src/engine.rs::tests::mono_sustain_moves_a_held_note`, `crates/dsp/src/mono/env.rs::tests::sustain_follows_while_held`

### Requirement 5: LFO and sample-and-hold [MUST]

The voice SHALL have an LFO (sine, triangle, saw, square) from 0.01 Hz to 50 Hz, and a sample-and-hold that samples the noise once per LFO period and holds the value until the next. Their destinations come with routing (Req 7).

**Implementation:** `crates/dsp/src/mono/lfo.rs::Lfo` (#7)

#### Scenario: sample-and-hold period

- GIVEN the LFO at 4 Hz
- WHEN two seconds are rendered
- THEN the S&H output changes exactly 8 times, once per LFO period, and is constant in between

**Tests:** `crates/dsp/src/mono/lfo.rs::tests::lfo_frequency`, `crates/dsp/src/mono/lfo.rs::tests::sample_and_hold_once_per_period`

### Requirement 6: Mono note handling and glide [MUST]

Each owner (live input, and each MIDI channel of the player) SHALL have one monophonic Mono voice, allocated in `Engine::new`; Mono SHALL NOT take voices from the shared pool. Each voice SHALL keep a stack of held keys and sound one of them by a priority (last, low or high). With legato on, a new key while another is held SHALL change pitch without retriggering the envelopes; releasing a key SHALL fall back to the next held key by priority. Glide SHALL move the pitch to a new key in a set time, from 0 (off) to 5 s.

**Implementation:** `crates/dsp/src/mono/voice.rs::MonoVoice`, `crates/dsp/src/engine.rs::Engine::note_on` *(planned, #8; moved to plan.md MVP 5)*

#### Scenario: fall back on release

- GIVEN priority last, keys 60 then 64 then 67 held
- WHEN 67 is released, then 60
- THEN the voice sounds 64 both times

#### Scenario: legato

- GIVEN legato on and key 60 held
- WHEN key 62 is pressed
- THEN the pitch changes to 62 and the ADSR keeps its level

#### Scenario: glide time

- GIVEN glide 100 ms
- WHEN key 48 is followed by key 60
- THEN the pitch reaches 60 within 100 ms ± 1 ms

#### Scenario: owners are independent

- GIVEN MIDI channels 2 and 3 both routed to Mono, and live input on Mono
- WHEN each plays a note
- THEN three Mono voices sound, and a note off on one leaves the others gated

**Tests:** `crates/dsp/src/mono/voice.rs::tests::priority_falls_back_on_release`, `crates/dsp/src/mono/voice.rs::tests::legato_keeps_the_envelope`, `crates/dsp/src/mono/voice.rs::tests::glide_time`, `crates/dsp/src/engine.rs::tests::mono_owners_are_independent` *(planned)*

### Requirement 7: Normalled routing and patches [MUST]

Every module SHALL have a default (normalled) connection, as on the 2600: VCO 1-3 and noise into the mixer, the mixer into the filter, the filter into the VCA; the ADSR to the filter cutoff and the VCA; the key to VCO pitch and to the filter cutoff (key tracking); the LFO to VCO pitch through the mod wheel. The AR envelope and the S&H SHALL have no default destination.

A patch SHALL be a fixed table of 8 overrides, each (source, destination, amount), allocated in `Engine::new`. Sources: VCO 1-3, noise, ADSR, AR, LFO, S&H, mod wheel, velocity, key. Destinations: VCO 1-3 pitch, pulse width, filter cutoff, resonance, VCA, LFO rate. An override SHALL replace the normalled connection to its destination. Amounts SHALL be clamped to −1..=1; unknown source or destination ids SHALL be ignored. The view SHALL show the normalled path and the patch, and only send edits (spec 003 Req 6).

**Implementation:** `crates/dsp/src/mono/patch.rs::Patch` *(planned, #9; moved to plan.md MVP 5)*

#### Scenario: empty patch is the normalled voice

- GIVEN an empty patch
- WHEN a note is played
- THEN the output equals the normalled path rendered directly

#### Scenario: an override replaces its destination

- GIVEN an override (S&H → filter cutoff, 0.5)
- WHEN a note is held for one second with the LFO at 4 Hz
- THEN the cutoff changes 4 times and the ADSR no longer moves it

**Tests:** `crates/dsp/src/mono/patch.rs::tests::empty_patch_is_normalled`, `crates/dsp/src/mono/patch.rs::tests::override_replaces_destination`, `crates/dsp/src/mono/patch.rs::tests::bad_ids_are_ignored` *(planned)*

### Requirement 8: MIDI input [MUST]

The engine SHALL take raw MIDI channel messages through one export, `midi_in(status, d1, d2)`, and interpret them in Rust; JavaScript SHALL only forward the bytes it gets from Web MIDI. Note on and off SHALL carry velocity, and a note on with velocity 0 SHALL be a note off. Pitch bend SHALL be read as 14 bits, with its range a parameter (default ±2 semitones). The mod wheel (CC 1) SHALL be a modulation source (Req 7). Other messages SHALL be ignored.

**Implementation:** `crates/dsp/src/midi.rs`, `crates/dsp/src/ffi.rs::midi_in` *(planned, #10; moved to plan.md MVP 11)*

#### Scenario: bend

- GIVEN a held A4 and a bend range of 2 semitones
- WHEN pitch bend 0x3FFF arrives
- THEN the voice sounds B4 within 1 cent

**Tests:** `crates/dsp/src/midi.rs::tests::zero_velocity_is_note_off`, `crates/dsp/src/midi.rs::tests::bend_is_14_bit`, `crates/dsp/src/midi.rs::tests::unknown_messages_are_ignored` *(planned)*

### Requirement 9: Presets [SHOULD]

The engine SHALL ship four Mono presets as Rust data, selected by id: bass, lead, sync lead and bowed string (the starting patch for the MVP 5 ensemble). Selecting a preset SHALL set the Mono parameters, and the patch once routing exists (Req 7, MVP 5); the preset ids SHALL be mirrored in `params.ts`. A preset SHALL set every Mono parameter, starting from the defaults, so none is left over from the last one. The engine SHALL report each parameter's current value (`param_value`), so the view shows what a preset set.

**Implementation:** `crates/dsp/src/mono/preset.rs::Preset`, `crates/dsp/src/engine.rs::Engine::preset`, `crates/dsp/src/ffi.rs::mono_preset`, `crates/dsp/src/ffi.rs::param_value` (#11)

#### Scenario: every preset is safe

- GIVEN each preset
- WHEN a note across the keyboard is held and released
- THEN every sample is finite and bounded, and the voice falls silent after its release

**Tests:** `crates/dsp/src/mono/preset.rs::tests::every_preset_is_bounded`, `crates/dsp/src/mono/preset.rs::tests::typescript_mirror_matches`, `crates/dsp/src/mono/preset.rs::tests::a_preset_sets_every_mono_parameter`, `crates/dsp/src/mono/preset.rs::tests::defaults_cover_every_mono_parameter_once`
