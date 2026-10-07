# 004: The Mono voice

The ARP 2600-style semi-modular monophonic voice in `crates/dsp/src/mono/`: oscillators, noise, filter, modulation, note handling, normalled routing, MIDI input and presets. Decisions: ADR-0001, ADR-0002, ADR-0004, ADR-0007. Every requirement is built except Req 8 (MIDI input), which is planned (plan.md MVP 11) and names no code yet. Since ADR-0011 the voice is also the per-note voice of the polyphonic models (spec 006).

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

**Tests:** `crates/dsp/src/mono/osc.rs::tests::pitch_within_a_cent`, `crates/dsp/src/mono/osc.rs::tests::saw_aliasing_below_60_db`, `crates/dsp/src/mono/osc.rs::tests::sync_is_bounded`, `crates/dsp/src/mono/osc.rs::tests::pwm_sweep_has_no_clicks`, `crates/dsp/src/mono/osc.rs::tests::pwm_sweep_keeps_one_pair_of_edges_per_cycle`, `crates/dsp/src/mono/osc.rs::tests::sync_locks_slave_to_master_period`, `crates/dsp/src/mono/osc.rs::tests::narrow_pulses_are_bounded`, `crates/dsp/src/mono.rs::tests::coarse_and_fine_set_the_tune`

### Requirement 2: Noise [MUST]

The voice SHALL have a white and a pink noise source from a seeded generator, with a colour switch and a level into the mixer; the same seed SHALL give the same samples. Neither SHALL have a DC offset: pink, whose filter passes DC, goes through a DC blocker.

**Implementation:** `crates/dsp/src/mono/noise.rs::Noise` (#5)

#### Scenario: spectral slope

- GIVEN ten seconds of each noise colour
- WHEN the power per octave is measured from 100 Hz to 10 kHz
- THEN white is flat and pink falls 3 dB per octave, both within ±1 dB

**Tests:** `crates/dsp/src/mono/noise.rs::tests::white_is_flat`, `crates/dsp/src/mono/noise.rs::tests::pink_falls_3_db_per_octave`, `crates/dsp/src/mono/noise.rs::tests::same_seed_same_noise`, `crates/dsp/src/mono/noise.rs::tests::no_dc_and_bounded`

### Requirement 3: Ladder filter [MUST]

The voice SHALL filter the mixed oscillators and noise through a 4-pole zero-delay-feedback ladder low-pass with cutoff, resonance and drive. Cutoff SHALL be smoothed at control rate and the filter coefficient recomputed only when the smoothed cutoff changes. Drive SHALL saturate without a per-sample `tanh` (a rational approximation or a table). The filter coefficient SHALL come from a table built in `Engine::new`, so modulating the cutoff costs no transcendental math either. Resonance SHALL self-oscillate from 0.8 of its range on the plain ladder; a voicing (Req 13) SHALL set where on its knob it starts (#342). A voicing MAY also saturate each stage (Req 13); each stage's saturator SHALL then be taken at the previous sample's level, so the loop is still solved exactly with no iteration and no per-sample transcendental, a small signal SHALL pass as through the linear ladder, and every scenario here SHALL hold for it too.

**Implementation:** `crates/dsp/src/mono/ladder.rs::Ladder`, `crates/dsp/src/mono/ladder.rs::LadderTables`, `crates/dsp/src/mono/model.rs::Stages` (#6, #306)

#### Scenario: slope

- GIVEN a cutoff of 1 kHz and no resonance
- WHEN the response is measured at 4 kHz and 8 kHz
- THEN it falls 24 dB per octave, within ±2 dB

#### Scenario: self-oscillation is bounded

- GIVEN maximum resonance and drive, with no input
- WHEN a cutoff sweep from 20 Hz to 20 kHz is rendered
- THEN the filter oscillates near the cutoff and every sample is finite and within ±2

**Tests:** `crates/dsp/src/mono/ladder.rs::tests::falls_24_db_per_octave`, `crates/dsp/src/mono/ladder.rs::tests::self_oscillation_is_bounded`, `crates/dsp/src/mono/ladder.rs::tests::any_parameters_stay_finite`, `crates/dsp/src/mono/ladder.rs::tests::cutoff_is_smoothed`, `crates/dsp/src/mono/ladder.rs::tests::table_matches_tan`, `crates/dsp/src/mono/ladder.rs::tests::soft_input_matches_the_linear_ladder`, `crates/dsp/src/mono/ladder.rs::tests::an_impulse_decays_below_the_threshold_and_rings_above`, `crates/dsp/src/mono/ladder.rs::tests::sat_gain_is_saturate_over_x`

### Requirement 4: Envelopes [MUST]

The voice SHALL have an ADSR and an AR envelope; the ADSR SHALL drive the VCA. Each segment SHALL take its set time from wherever it starts, curved as an RC envelope is. Their rates SHALL be computed when a time parameter or the gate changes, not per sample. A gate that opens during release SHALL continue from the current level, without a jump; across notes this holds within an owner's voice (Req 6). A note shorter than one block SHALL still sound, held for that block. The sustain level SHALL follow its parameter while a note is held.

**Implementation:** `crates/dsp/src/mono/env.rs::Env`, `crates/dsp/src/mono/voice.rs::MonoVoice` (#7)

#### Scenario: segment times

- GIVEN attack, decay and release times between 1 ms and 10 s
- WHEN the ADSR is gated, held and released offline
- THEN each segment reaches its target within ±1 ms of its set time

**Tests:** `crates/dsp/src/mono/env.rs::tests::segment_times`, `crates/dsp/src/mono/env.rs::tests::retrigger_does_not_jump`, `crates/dsp/src/mono/env.rs::tests::ar_holds_at_full_level`, `crates/dsp/src/engine/tests.rs::mono_follows_its_adsr`, `crates/dsp/src/engine/tests.rs::a_mono_tap_shorter_than_a_block_sounds`, `crates/dsp/src/engine/tests.rs::mono_sustain_moves_a_held_note`, `crates/dsp/src/mono/env.rs::tests::sustain_follows_while_held`

### Requirement 5: LFO and sample-and-hold [MUST]

The voice SHALL have an LFO (sine, triangle, saw, square) from 0.01 Hz to 50 Hz, and a sample-and-hold that samples the noise once per LFO period and holds the value until the next. Their destinations come with routing (Req 7).

**Implementation:** `crates/dsp/src/mono/lfo.rs::Lfo` (#7)

#### Scenario: sample-and-hold period

- GIVEN the LFO at 4 Hz
- WHEN two seconds are rendered
- THEN the S&H output changes exactly 8 times, once per LFO period, and is constant in between

**Tests:** `crates/dsp/src/mono/lfo.rs::tests::lfo_frequency`, `crates/dsp/src/mono/lfo.rs::tests::sample_and_hold_once_per_period`

### Requirement 6: Mono note handling and glide [MUST]

On a monophonic model, each owner (live input, each song track) SHALL have one voice of its synth's pool, allocated in `Engine::new` (ADR-0011, spec 006 Req 1). Each voice SHALL keep a stack of held keys and sound one of them by a priority (last, low or high). With legato on, a new key while another is held SHALL change pitch without retriggering the envelopes; releasing a key SHALL fall back to the next held key by priority. Glide SHALL move the pitch to a new key in a set time, from 0 (off) to 5 s, in a straight line of semitones; it applies when a key arrives while another is held. A voice SHALL remember up to 16 held keys, forgetting the oldest. Pitch SHALL come from a table, so it can move every sample without per-sample `exp2` (ADR-0002).

**Implementation:** `crates/dsp/src/mono/voice.rs::MonoVoice`, `crates/dsp/src/mono/voice.rs::PitchTable`, `crates/dsp/src/engine.rs::Engine::note_on`, `crates/dsp/src/poly.rs::Pool` (#8)

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

- GIVEN MIDI channels 3 and 4 playing on synth 0, and live input on synth 0
- WHEN each plays a note
- THEN three voices of synth 0's pool sound, and a note off on one leaves the others gated

**Tests:** `crates/dsp/src/mono/voice.rs::tests::priority_falls_back_on_release`, `crates/dsp/src/mono/voice.rs::tests::legato_keeps_the_envelope`, `crates/dsp/src/mono/voice.rs::tests::glide_time`, `crates/dsp/src/engine/tests.rs::mono_owners_are_independent`, `crates/dsp/src/mono/voice.rs::tests::pitch_table_is_equal_tempered`, `crates/dsp/src/mono/voice.rs::tests::tune_is_heard_within_a_cent`, `crates/dsp/src/mono/voice.rs::tests::a_full_key_stack_forgets_the_oldest`

### Requirement 7: Normalled routing and patches [MUST]

Every module SHALL have a default (normalled) connection, as on the 2600: VCO 1-3 and noise into the mixer, the mixer into the filter, the filter into the VCA; the ADSR to the filter cutoff and the VCA; the key to VCO pitch and to the filter cutoff (key tracking); the LFO to VCO pitch through the mod wheel. The AR envelope and the S&H SHALL have no default destination. The audio path and key → VCO pitch are wired; the modulation normals (ADSR → cutoff, key → cutoff, LFO × mod wheel → pitch, ADSR → VCA) each have an amount (`EnvCutoff`, `KeyTrack`, `Vibrato`). The mod wheel is a parameter until MIDI input sends CC 1 (Req 8).

A patch SHALL be a fixed table of 8 overrides, each (source, destination, amount), allocated in `Engine::new`. Sources: VCO 1-3, noise, ADSR, AR, LFO, S&H, mod wheel, velocity, key. Destinations: VCO 1-3 pitch, pulse width, filter cutoff, resonance, VCA, LFO rate. An override SHALL replace every normalled connection to its destination, and overrides to one destination SHALL add up. Amount 1 at full source SHALL be 24 semitones of VCO pitch, 0.45 of pulse width, 48 semitones of cutoff, full resonance, full VCA gain, or 4 octaves of LFO rate (applied once per block). Amounts SHALL be clamped to −1..=1 (NaN is 0); unknown source or destination ids SHALL be ignored. The patch slots SHALL be parameters, so presets carry them. When a patch drives the VCA, the AR envelope too SHALL keep the voice sounding. The view SHALL show the normalled path and the patch, and only send edits (spec 003 Req 6).

**Implementation:** `crates/dsp/src/mono/patch.rs::Patch`, `crates/dsp/src/mono/patch.rs::modulate`, `crates/dsp/src/mono/voice.rs::MonoVoice::render` (#9)

#### Scenario: empty patch is the normalled voice

- GIVEN an empty patch
- WHEN a note is played
- THEN the output equals the normalled path rendered directly

#### Scenario: an override replaces its destination

- GIVEN an override (S&H → filter cutoff, 0.5)
- WHEN a note is held for one second with the LFO at 4 Hz
- THEN the cutoff changes 4 times and the ADSR no longer moves it

**Tests:** `crates/dsp/src/mono/patch.rs::tests::empty_patch_is_normalled`, `crates/dsp/src/mono/patch.rs::tests::override_replaces_destination`, `crates/dsp/src/mono/patch.rs::tests::bad_ids_are_ignored`, `crates/dsp/src/mono/voice.rs::tests::sample_and_hold_takes_over_the_cutoff`, `crates/dsp/src/mono/voice.rs::tests::normalled_connections_move_their_destinations`, `crates/dsp/src/mono/voice.rs::tests::ar_on_the_vca_shapes_the_note`

### Requirement 8: MIDI input [MUST]

The engine SHALL take raw MIDI channel messages through one export, `midi_in(status, d1, d2)`, and interpret them in Rust; JavaScript SHALL only forward the bytes it gets from Web MIDI. Note on and off SHALL carry velocity, and a note on with velocity 0 SHALL be a note off. Pitch bend SHALL be read as 14 bits, with its range a parameter (default ±2 semitones). The mod wheel (CC 1) SHALL be a modulation source (Req 7). Other messages SHALL be ignored.

**Implementation:** (planned, #10, plan.md MVP 11) a MIDI message parser in Rust behind one `midi_in` export; not built. The song already plays notes through `Engine::note_on` (Req 10).

#### Scenario: bend

- GIVEN a held A4 and a bend range of 2 semitones
- WHEN pitch bend 0x3FFF arrives
- THEN the voice sounds B4 within 1 cent

**Tests:** (planned) a velocity-0 note on is a note off, bend is read as 14 bits, unknown messages are ignored.

### Requirement 9: Presets [SHOULD]

The engine SHALL ship four Mono presets as Rust data, selected by id: bass, lead, sync lead and bowed string (the starting patch for the MVP 5 ensemble). Selecting a preset SHALL set the Mono parameters, the normalled amounts and the patch (Req 7); the preset ids SHALL be mirrored in `params.ts`. A preset SHALL set every Mono parameter, starting from the defaults, so none is left over from the last one. The engine SHALL report each parameter's current value (`param_value`), so the view shows what a preset set.

**Implementation:** `crates/dsp/src/mono/preset.rs::Preset`, `crates/dsp/src/engine.rs::Engine::preset`, `crates/dsp/src/ffi.rs::mono_preset`, `crates/dsp/src/ffi.rs::param_value` (#11)

#### Scenario: every preset is safe

- GIVEN each preset
- WHEN a note across the keyboard is held and released
- THEN every sample is finite and bounded, and the voice falls silent after its release

**Tests:** `crates/dsp/src/mono/preset.rs::tests::every_preset_is_bounded`, `crates/dsp/src/params.rs::tests::typescript_mirror_matches`, `crates/dsp/src/mono/preset.rs::tests::a_preset_sets_every_mono_parameter`, `crates/dsp/src/mono/preset.rs::tests::defaults_cover_every_mono_parameter_once`

### Requirement 10: Independent synths [MUST]

The engine SHALL hold 16 synths, allocated in `Engine::new`, each with its own parameters, values, patch and model (spec 005; plan.md MVP 5); `set_param`, `param_value`, `mono_preset`, `note_on` and `note_off` SHALL name the synth, and `synth_reset` SHALL put one back to the defaults. `MasterGain` SHALL stay global. Each synth SHALL have its own live keys; each song track SHALL play on one synth or be muted, and importing a MIDI file SHALL put its tracks on synths 0, 1, 2… in channel order (ADR-0022). A voice SHALL keep the synth its note started on, and SHALL follow that synth's parameters while it sounds. An unknown synth SHALL be ignored (or mute, as a route), and a live note SHALL never reach a track's voice.

**Implementation:** `crates/dsp/src/engine.rs::Engine` (`SYNTHS`, `set_param`, `preset`, `reset`, `song_route`), `crates/dsp/src/ffi.rs` (`synth_count`, `synth_reset`), `web/src/audio/engine.ts::addSynth` (#19)

#### Scenario: own patch

- GIVEN synth 1 with every VCO and the noise at level 0
- WHEN synth 0 and synth 1 each play a note
- THEN synth 1 stays below −80 dB and synth 0 sounds

#### Scenario: one synth per part

- GIVEN the four-part demo file
- WHEN it is imported
- THEN its tracks, channels 1-4, play on synths 0-3

#### Scenario: sixteen at once

- GIVEN 16 synths on different presets
- WHEN each plays a note at full master gain
- THEN 16 voices sound and every sample is finite and within ±1

**Tests:** `crates/dsp/src/engine/tests.rs::synths_have_their_own_parameters`, `crates/dsp/src/engine/tests.rs::a_preset_on_one_synth_leaves_the_others`, `crates/dsp/src/engine/tests.rs::master_gain_is_global`, `crates/dsp/src/engine/tests.rs::unknown_synths_are_ignored`, `crates/dsp/src/engine/tests.rs::an_imported_track_plays_on_its_routed_synth`, `crates/dsp/src/engine/tests.rs::a_track_voice_follows_its_synths_parameters`, `crates/dsp/src/engine/tests.rs::sixteen_differently_patched_synths_play_together`, `crates/dsp/src/engine/tests.rs::demo_file_imports`, `crates/dsp/src/ffi.rs::tests::exports_drive_the_engine`

### Requirement 11: Drive insert [SHOULD]

Drive SHALL be an insert type of a strip's insert slots (spec 002 Req 2, ADR-0010): Overdrive (soft, asymmetric), Distortion (hard) and Fuzz, with amount, tone (a low-pass after the shaper) and level as knobs A, B and C. The shapers SHALL use first-order antiderivative anti-aliasing and no per-sample transcendental functions (ADR-0002). An Off slot SHALL pass the bus through bit for bit, and every drive type SHALL stay finite and bounded for any input.

**Implementation:** `crates/dsp/src/fx/drive.rs::Drive`, `crates/dsp/src/fx/insert.rs::Insert` (#25, #58)

#### Scenario: harmonics and aliases

- GIVEN a sine through a mode other than Off
- WHEN the drive goes up
- THEN its harmonics grow, and a 4.7 kHz sine at full drive keeps the alias at 19.8 kHz below 1% of the fundamental

**Tests:** `crates/dsp/src/fx/drive.rs::tests::off_is_bit_exact`, `crates/dsp/src/fx/drive.rs::tests::every_mode_is_finite_and_bounded_for_any_input`, `crates/dsp/src/fx/drive.rs::tests::more_drive_means_more_harmonics`, `crates/dsp/src/fx/drive.rs::tests::a_high_sine_at_full_drive_keeps_its_aliases_low`, `crates/dsp/src/engine/tests.rs::drive_shapes_the_synth_bus_only`

### Requirement 12: Filter envelope [MUST]

The voice SHALL have a second ADSR for the filter (`FenvAttack`, `FenvDecay`, `FenvSustain`, `FenvRelease`), independent of the loudness ADSR and available as the modulation source `Fenv`. Which envelope the normalled cutoff follows (`EnvCutoff`) SHALL be the model's choice (spec 005 Req 1): the ARP 2600 follows the ADSR, so its sound is unchanged. The filter ADSR SHALL obey Req 4 (times, retrigger, sustain following).

**Implementation:** `crates/dsp/src/mono/voice.rs::MonoVoice::render`, `crates/dsp/src/mono/patch.rs::Normals` (#31)

#### Scenario: independent envelopes

- GIVEN a filter attack of 1 s and a loudness attack of 1 ms on a model that uses the filter ADSR
- WHEN a note is held
- THEN the loudness is full within 5 ms and the cutoff modulation reaches its peak after 1 s ± 1 ms

**Tests:** `crates/dsp/src/mono/voice.rs::tests::filter_envelope_is_independent`, `crates/dsp/src/mono/voice.rs::tests::arp_cutoff_still_follows_the_adsr`, `crates/dsp/src/mono/patch.rs::tests::normalled_cutoff_follows_the_chosen_envelope`, `crates/dsp/src/mono/model.rs::tests::single_envelope_models_follow_the_adsr`

### Requirement 13: Filter flavours [MUST]

The voice SHALL have, besides the Req 3 ladder, a 12 dB state-variable filter with a saturating state, giving a low-pass and a high-pass output, and a one-pole high-pass. The ladder SHALL have three voicings (Moog, Pro-One, SH-101) differing in drive, resonance and bass compensation, and in what saturates inside it: the Moog voicing (Minimoog, ARP 2600) SHALL saturate each stage's differential pair, `g·(tanh(in) − tanh(out))`, as the transistor ladder does (#306), and the Roland voicings (SH-101, Juno-106 with its own trim, Jupiter-8) SHALL saturate each OTA's input, `g·tanh(in − out)`, as the IR3109 cascade does (#305). The Sequential voicings SHALL be their chips (#321): the CEM3320 (Pro-One, Prophet-5 Rev 3) SHALL take OTA stages and clip its input and its feedback apart, the feedback at its gain at the previous sample's output, as its resonance VCA does, so resonance takes the bass of a hot input as of a soft one; the SSM2040 (Prophet-5 Rev 1/2) SHALL take OTA stages with a wider linear range, and keep more bass under resonance than the CEM3320. A model with a revision switch (`FilterRev`, 1..=3, default 3) SHALL take that revision's filter: the Prophet-5 the SSM2040 at Rev 1 and 2, the Odyssey the 4023's two poles at Rev 1 and the 4035's transistor ladder at Rev 2; the 12 dB filter two (MS-20, CS-15) differing in the resonance at which it self-oscillates and in its saturation ceiling. The high-pass stage SHALL have its own cutoff (`HpCutoff`), resonance (`HpResonance`) and envelope amount (`EnvHpCutoff`). Coefficients SHALL come from the table of Req 3, so nothing costs a transcendental per sample.

Resonance SHALL follow each voicing's knob taper and feedback path (#342): the feedback SHALL reach the self-oscillation threshold (`ONSET_K`) at the voicing's `onset` on the knob and its full feedback (`k_scale` of the range) at the top, a straight line at an onset of 0.8; a voicing MAY high-pass its feedback path (`loop_hp`), solved inside the loop, which stops the whistle at the lowest cutoffs. At full resonance with no input every voicing SHALL whistle at a peak from 0.1 to 0.3 within 3% of the cutoff (220 Hz to 3.5 kHz). The onsets past 0.8 and the loop high-pass of the IR3109 voicings are estimates, marked as such in `model.rs`. SuperCollider's `MoogFF` gain 0..4 SHALL span the knob, so it whistles at 4.

**Implementation:** `crates/dsp/src/mono/svf.rs::Svf`, `crates/dsp/src/mono/svf.rs::OnePole`, `crates/dsp/src/mono/model.rs::LadderVoicing`, `crates/dsp/src/mono/model.rs::SvfVoicing`, `crates/dsp/src/mono/model.rs::Model::low_pass` (#32, #321)

#### Scenario: slope

- GIVEN the 12 dB filter at 1 kHz with no resonance
- WHEN the low-pass response is measured at 4 kHz and 8 kHz
- THEN it falls 12 dB per octave, within ±2 dB; the high-pass mirrors it below the cutoff

#### Scenario: self-oscillation is bounded

- GIVEN the MS-20 voicing at maximum resonance with no input
- WHEN a cutoff sweep from 20 Hz to 20 kHz is rendered
- THEN every sample is finite and within ±2, and the CS-15 voicing at maximum resonance does not oscillate without input

#### Scenario: the transistor ladder saturates per stage

- GIVEN a hot 200 Hz sine into a 1 kHz ladder at full drive and resonance short of oscillation
- WHEN the transistor ladder and the single-saturator ladder are measured
- THEN the transistor ladder's 3rd and 5th harmonics stand more than three times higher against the fundamental

#### Scenario: the OTA cascade rounds a hot input's edges

- GIVEN a hot 100 Hz square into a 1 kHz OTA ladder at full drive with some resonance
- WHEN it is measured against the single-saturator ladder
- THEN the 3rd harmonic is the same within 3%, and the 9th and 15th come out lower, the 15th by more than a fifth

#### Scenario: the CEM3320's resonance takes the bass of a hot input

- GIVEN a hot 200 Hz sine into a 1 kHz ladder at full drive
- WHEN resonance is turned from none to just short of oscillation
- THEN the CEM3320's fundamental keeps less than half of what the transistor ladder's keeps

#### Scenario: the SSM2040 keeps more bass than the CEM3320

- GIVEN the Prophet-5's Rev 1/2 and Rev 3 voicings at 1 kHz with strong resonance
- WHEN a soft 100 Hz sine is measured through each
- THEN the SSM2040 passes more than 1.3 times as much of it

#### Scenario: the revision switch

- GIVEN the Odyssey at 1 kHz with no resonance
- WHEN `FilterRev` is 1, then 3
- THEN it falls 12 dB per octave at Rev 1 and much faster at Rev 3, and turning the switch while a resonant note sounds stays finite and within ±2

**Tests:** `crates/dsp/src/mono/ladder.rs::tests::the_resonance_taper`, `crates/dsp/src/mono/ladder.rs::tests::each_voicing_whistles_from_its_onset`, `crates/dsp/src/mono/ladder.rs::tests::the_loop_high_pass_stops_the_lowest_whistle`, `crates/dsp/src/mono/svf.rs::tests::falls_12_db_per_octave`, `crates/dsp/src/mono/svf.rs::tests::high_pass_rises_12_db_per_octave`, `crates/dsp/src/mono/svf.rs::tests::self_oscillation_is_bounded`, `crates/dsp/src/mono/svf.rs::tests::any_parameters_stay_finite`, `crates/dsp/src/mono/svf.rs::tests::one_pole_rises_6_db_per_octave`, `crates/dsp/src/mono/voice.rs::tests::ladder_voicings_differ_and_stay_bounded`, `crates/dsp/src/mono/patch.rs::tests::high_pass_follows_the_chosen_envelope`, `crates/dsp/src/mono/model.rs::tests::models_pair_their_filters_and_stages`, `crates/dsp/src/mono/ladder.rs::tests::hot_transistor_stages_saturate_differently`, `crates/dsp/src/mono/model.rs::tests::moog_ladders_saturate_per_stage`, `crates/dsp/src/mono/ladder.rs::tests::hot_ota_stages_round_the_edges`, `crates/dsp/src/mono/model.rs::tests::roland_ladders_are_ota_cascades`, `crates/dsp/src/mono/preset.rs::tests::arp_presets_keep_their_sound`, `crates/dsp/src/mono/ladder.rs::tests::cem_resonance_takes_the_bass_of_a_hot_input`, `crates/dsp/src/mono/ladder.rs::tests::ssm_keeps_more_bass_than_cem`, `crates/dsp/src/mono/model.rs::tests::sequential_ladders_are_their_chips`, `crates/dsp/src/mono/model.rs::tests::only_the_prophet_and_odyssey_have_a_rev_switch`, `crates/dsp/src/mono/voice.rs::tests::odyssey_rev_switch_is_12_or_24_db_per_octave`, `crates/dsp/src/mono/voice.rs::tests::filter_rev_switch_while_a_note_sounds`

### Requirement 14: Ring modulator, sub-oscillator, Osc 3 as modulator [MUST]

The mixer SHALL take a ring modulator, VCO 1 × VCO 2 (`RingLevel`), and a sub-oscillator, a square exactly one or two octaves below VCO 1 (`SubLevel`, `SubOctave`), band-limited like the VCOs. VCO 3 SHALL be usable as a modulator: `Vco3KeyFollow` off holds its pitch whatever the key, and `Vco3Low` drops it five octaves into the low-frequency range; its output stays a modulation source (`Vco3`) at any mixer level.

**Implementation:** `crates/dsp/src/mono/voice.rs::MonoVoice::render` (#33)

#### Scenario: ring modulation

- GIVEN VCO 1 at 300 Hz and VCO 2 at 500 Hz, only the ring modulator in the mixer
- WHEN the spectrum is measured
- THEN the energy is at 200 Hz and 800 Hz, and not at 300 Hz or 500 Hz

#### Scenario: the sub follows

- GIVEN a sub one octave down
- WHEN a pitch sweep and a vibrato are rendered
- THEN the sub's period is exactly twice VCO 1's, throughout

**Tests:** `crates/dsp/src/mono/voice.rs::tests::ring_modulation_has_sum_and_difference`, `crates/dsp/src/mono/voice.rs::tests::sub_is_exactly_an_octave_down`, `crates/dsp/src/mono/voice.rs::tests::osc3_low_and_unfollowed_is_a_fixed_modulator`

### Requirement 15: Poly-mod and LFO destinations [MUST]

The voice SHALL add, after the normals and the patch, five poly-mod amounts: filter envelope → VCO 2 pitch (`EnvFreq2`), VCO 1 → VCO 2 pitch (`OscFreq2`), filter envelope → pulse width (`EnvPw`), VCO 1 → pulse width (`OscPw`) and VCO 1 → cutoff (`OscCutoff`), and two modulation amounts, LFO → cutoff (`LfoCutoff`, ±24 semitones at 1, no mod wheel) and LFO → pulse width (`LfoPw`, ±0.45). On a model whose modulator is VCO 3 (the Minimoog, spec 005 Req 3) these, and the vibrato normal, read VCO 3 instead of the LFO. They add to a destination without taking it over from its normals or its patch. Scale: ±24 semitones of pitch, ±0.45 of pulse width, ±48 semitones of cutoff for the poly-mod amounts.

**Implementation:** `crates/dsp/src/mono/patch.rs::modulate` (#36)

#### Scenario: poly-mod adds to the normals

- GIVEN the filter envelope into the cutoff by the normal and VCO 1 → cutoff by poly-mod
- WHEN a note is held
- THEN the cutoff modulation is the sum of both

**Tests:** `crates/dsp/src/mono/patch.rs::tests::poly_mod_adds_to_the_normals`, `crates/dsp/src/mono/patch.rs::tests::the_modulation_source_is_the_lfo_or_osc3`
