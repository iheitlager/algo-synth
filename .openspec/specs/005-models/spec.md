# 005: Synth models

The family of monosynths (epic #28): seven of the nineteen models a synth slot can be, built from the shared Mono modules of spec 004. The others are the polyphonic models of spec 006, the drum machines of spec 002 Req 1 and the samplers of spec 007. Decisions: ADR-0001, ADR-0002, ADR-0004, ADR-0009. Built in epic #28; every requirement names its code and tests.

Common to every requirement: `render` follows ADR-0002, every parameter and id is mirrored in `web/src/audio/params.ts` (ADR-0004), and a model's sound is an interpretation of the instrument: each requirement names the property it must have, and is tested on that. Tests render offline at 48 kHz.

### Requirement 1: Models [MUST]

Every synth SHALL have a model, `Param::Model`, held with the synth's parameters: one of the monosynths here (`Arp2600`, `Minimoog`, `ProOne`, `Ms20`, `Cs15`, `Sh101`, `Odyssey`), the polysynths of spec 006 (`Prophet5`, `Juno106`, `Jupiter8`, `Matrix12`, `PpgWave`, `D50`, `Dx7`, `PolyMoog`), the drum machines (`Tr808`, `Tr909`, spec 002 Req 1), the samplers (`Sampler`, `PadSampler`, spec 007) or `Modular` (Req 11); a new or reset synth SHALL be an ARP 2600. A model SHALL decide which filter the voice uses and its voicing, whether a high-pass stage exists, which envelope drives the normalled cutoff, and whether the decay time also sets the release (ADR-0009). Every other parameter SHALL exist on every model. Each model SHALL have at least two presets, among them well-known sounds of the instrument (named in `preset.rs`); a preset SHALL set the model and every Mono parameter (spec 004 Req 9), and selecting a model in the view SHALL load that model's first preset. Unknown model ids SHALL be ignored.

**Implementation:** `crates/dsp/src/mono/model.rs::Model`, `crates/dsp/src/mono/preset.rs::Preset`, `crates/dsp/src/mono.rs::MonoParams` (#30)

#### Scenario: models are per synth

- GIVEN synth 0 an ARP 2600 and synth 1 a Minimoog, both on one note
- WHEN each renders
- THEN the outputs differ, and setting synth 1's model leaves synth 0's parameters and sound unchanged

#### Scenario: every preset is safe

- GIVEN each preset of each model
- WHEN a note across the keyboard is held and released
- THEN every sample is finite and bounded, and the voice falls silent after its release

**Tests:** `crates/dsp/src/mono/model.rs::tests::ids_round_trip`, `crates/dsp/src/engine/tests.rs::models_are_per_synth`, `crates/dsp/src/engine/tests.rs::models_sound_different`, `crates/dsp/src/mono/preset.rs::tests::every_preset_is_bounded`, `crates/dsp/src/mono/preset.rs::tests::every_preset_sets_its_model`, `crates/dsp/src/mono/preset.rs::tests::every_model_has_at_least_two_presets`, `crates/dsp/src/mono/preset.rs::tests::a_new_synth_is_an_arp_2600`

### Requirement 2: ARP 2600 [MUST]

The ARP 2600 SHALL be the voice of spec 004 as it was before models: three VCOs, noise, the Moog-voiced ladder, one ADSR driving the VCA and the normalled cutoff, an AR, an LFO and sample-and-hold, and the patch of Req 7, with no high-pass stage. Its output SHALL NOT change with the model parameter's introduction.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Arp2600`, `web/src/audio/models.ts` (#34)

#### Scenario: as before models

- GIVEN each of the four original presets
- WHEN A3 is held for half a second
- THEN rms, peak and two samples equal the values recorded before models, within 2e-5

**Tests:** `crates/dsp/src/mono/preset.rs::tests::arp_presets_keep_their_sound`, `crates/dsp/src/mono/voice.rs::tests::arp_cutoff_still_follows_the_adsr`, `crates/dsp/src/mono/preset.rs::tests::a_new_synth_is_an_arp_2600`

### Requirement 3: Minimoog [MUST]

The Minimoog SHALL have three VCOs, noise, a Moog-voiced ladder with overdrive in the mixer (`Drive`), a loudness ADSR and a separate filter ADSR, each with its decay time also setting its release, glide, low note priority, and Osc 3 usable as a modulator (spec 004 Req 14). It has no LFO: Osc 3 SHALL be the source of its vibrato and of `LfoCutoff` (the modulation normals), and its panel SHALL have no patch panel. Its normalled cutoff SHALL follow the filter ADSR, with key tracking in steps of off, 1/3, 2/3 and full, set by two keyboard control switches. It SHALL have the Model D's on/off switches as parameters: a mixer switch per oscillator and for noise (`Vco1On`…`NoiseOn`), Glide (`GlideOn`), Decay (`DecayRelease`; off, the contours release at once), oscillator and filter modulation (`OscModOn`, `FilterModOn`), and an A-440 reference tone at the synth's output that sounds with no key held (`A440`). A switch that is off SHALL zero what its knob sets without losing the knob's value, and every switch but A-440 SHALL default to on (#308).

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Minimoog`, `crates/dsp/src/mono/voice.rs::MonoVoice::render`, `crates/dsp/src/mono.rs::MonoParams::set`, `crates/dsp/src/poly.rs::Pool::render` (#35, #308)

#### Scenario: decay is release

- GIVEN a Minimoog with decay 0.1 s and release 4 s
- WHEN a held note is released
- THEN the loudness falls in about 0.1 s, not 4 s

#### Scenario: the filter contour moves the cutoff, not the loudness

- GIVEN a Minimoog with a long filter attack and an instant loudness attack
- WHEN a note is held
- THEN the loudness is full at once while the spectrum brightens over the filter attack

#### Scenario: the switches

- GIVEN a Minimoog with Osc 1 at full level and its mixer switch off, or Decay off with a 2 s decay, or A-440 on and no key held
- WHEN it renders
- THEN Osc 1 is silent, a released note is silent within 50 ms, and a 440 Hz tone sounds

**Tests:** `crates/dsp/src/mono/voice.rs::tests::minimoog_decay_switch_off_releases_at_once`, `crates/dsp/src/mono/voice.rs::tests::mixer_switch_silences_its_source`, `crates/dsp/src/mono.rs::tests::switches_gate_their_knobs`, `crates/dsp/src/engine/tests.rs::a440_sounds_with_no_key_held`, `crates/dsp/src/mono/voice.rs::tests::minimoog_decay_is_release`, `crates/dsp/src/mono/voice.rs::tests::minimoog_filter_contour_brightens_a_held_note`, `crates/dsp/src/mono/patch.rs::tests::the_modulation_source_is_the_lfo_or_osc3`, `crates/dsp/src/mono/voice.rs::tests::ladder_voicings_differ_and_stay_bounded`

### Requirement 4: Pro-One [MUST]

The Pro-One SHALL have two oscillators (A and B), the second synced to the first, noise, a 4-pole low-pass in its own voicing, a loudness ADSR and a filter ADSR, an LFO, and poly-mod: the filter envelope and oscillator B each modulate oscillator A's pitch and pulse width, and oscillator B the cutoff (spec 004 Req 15). Its oscillator A SHALL be VCO 2 and B VCO 1, so sync and poly-mod run in the direction the instrument has.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::ProOne`, `crates/dsp/src/mono/patch.rs::modulate` (#36)

#### Scenario: poly-mod sweeps A

- GIVEN A synced to B and the filter envelope into A's pitch
- WHEN a note is held through the envelope
- THEN the sync sweep changes the spectrum of the held note over time

**Tests:** `crates/dsp/src/mono/voice.rs::tests::pro_one_poly_mod_sweeps_the_synced_oscillator`, `crates/dsp/src/mono/patch.rs::tests::poly_mod_adds_to_the_normals`

### Requirement 5: MS-20 [MUST]

The MS-20 SHALL have two VCOs, noise, ring modulation of VCO 1 by VCO 2, a high-pass then a low-pass 12 dB filter, each with its own resonance that SHALL self-oscillate, a loudness ADSR and a filter ADSR moving both cutoffs, an LFO, glide, and the patch of spec 004 Req 7 on its panel as the patch panel.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Ms20`, `crates/dsp/src/mono/svf.rs::Svf` (#37)

#### Scenario: both filters shape the sound

- GIVEN white noise through the MS-20 filters with the high-pass at 2 kHz and the low-pass at 8 kHz
- WHEN the spectrum is measured
- THEN it is down below 1 kHz and above 16 kHz, within the 12 dB slopes

#### Scenario: peak self-oscillates and stays bounded

- GIVEN maximum resonance on both filters and no input
- WHEN a cutoff sweep is rendered
- THEN the filter oscillates and every sample is finite and bounded

**Tests:** `crates/dsp/src/mono/voice.rs::tests::ms20_band_limits_noise`, `crates/dsp/src/mono/svf.rs::tests::self_oscillation_is_bounded`

### Requirement 6: Yamaha CS-15 [MUST]

The CS-15 SHALL have two VCOs, noise, ring modulation of VCO 1 by VCO 2, a high-pass and a low-pass 12 dB filter in the Yamaha voicing (resonant but not self-oscillating), the low-pass moved by the filter ADSR and the high-pass by the AR envelope, a loudness ADSR, an LFO and sample-and-hold on the patch.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Cs15` (#38)

#### Scenario: each filter has its own envelope

- GIVEN the AR envelope into the high-pass and the filter ADSR into the low-pass
- WHEN a note is held
- THEN moving the AR amount changes the low end of the spectrum and not the high end, and moving the ADSR amount does the reverse

**Tests:** `crates/dsp/src/mono/voice.rs::tests::cs15_filters_have_their_own_envelopes`, `crates/dsp/src/mono/svf.rs::tests::self_oscillation_is_bounded`

### Requirement 7: Roland SH-101 [MUST]

The SH-101 SHALL have one VCO whose saw and pulse are mixed (the pulse VCO 2, locked to VCO 1's phase and pitch by the model, and inverted so it adds to the rising saw instead of cancelling it), a sub-oscillator one or two octaves down, noise, an IR3109-voiced 4-pole low-pass, a one-pole high-pass after it, a single ADSR driving the filter and the VCA (it SHALL also be the filter envelope source of the poly-mod and envelope amounts, spec 004 Req 15), an LFO that can modulate pitch, cutoff and pulse width, and glide.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Sh101` (#39)

#### Scenario: one envelope moves both

- GIVEN the SH-101 with a cutoff envelope amount
- WHEN a note is held and released
- THEN the cutoff and the loudness follow the same envelope

**Tests:** `crates/dsp/src/mono/voice.rs::tests::sh101_one_envelope_moves_cutoff_and_loudness`, `crates/dsp/src/mono/voice.rs::tests::sh101_pulse_is_locked_to_the_saw`, `crates/dsp/src/mono/voice.rs::tests::sh101_saw_and_pulse_add_up`

### Requirement 8: Panels and colours [MUST]

The view SHALL draw each synth with a panel of its model: the sections, control names and order of that instrument, and a palette of its own (panel, lettering, trim and accent) as CSS variables, so the models are told apart at a glance. A panel SHALL show only the controls its instrument has and SHALL send edits only (spec 003 Req 6). Selecting a model SHALL send the model's first preset.

**Implementation:** `web/src/audio/models.ts`, `web/src/components/SynthFaceplate.vue`, `web/src/components/InstrumentsPane.vue` (#30, drawn as faceplates by spec 003 Req 9)

#### Scenario: every instrument

- GIVEN one synth of each model
- WHEN the view is drawn
- THEN each card carries its model's name and palette, and the controls of its panel

**Tests:** `crates/dsp/src/params.rs::tests::typescript_mirror_matches`, `cd web && npm run typecheck`

### Requirement 9: Mixed ensemble budget [MUST]

Sixteen synths across all the models SHALL render within the performance budget of plan.md (25% of a core), and every model's presets SHALL be bounded at full master gain.

**Implementation:** `tools/bench.mjs`, `crates/dsp/src/engine.rs::Engine` (#40)

#### Scenario: sixteen at once, every model

- GIVEN 16 synths cycling through the models and their presets
- WHEN each plays a note at full master gain
- THEN 16 voices sound and every sample is finite and within ±1

**Tests:** `crates/dsp/src/engine/tests.rs::sixteen_synths_of_every_model_play_together`, `make bench` (scenarios `all models` and `family worst`, 5.5% and 6.6% of a core on an Apple M4 Pro with seven models, the flexible mixer and effects)

### Requirement 10: ARP Odyssey [MUST]

The Odyssey (the Mk II and III of about 1975-81) SHALL be two VCOs (saw, pulse with width, triangle, sine) with VCO 2 hard-synced to VCO 1 on a switch, ring mod of VCO 1 by VCO 2, and white or pink noise in its mixer; a 24 dB ladder with a voicing of its own (`ODYSSEY`: brighter and cleaner than the Moog, a little bass kept under resonance) and a 6 dB high-pass *after* it; one ADSR normalled to both cutoff and VCA; an LFO to pitch, cutoff and pulse width, and portamento. It is not modular, so its faceplate has no patch bay (spec 003 Req 9): the AR and sample-and-hold are reached through the patch slots of presets and setups only. The faceplate SHALL be black and gold. Presets: **Currie lead** (two detuned saws, a bright, slightly resonant ladder driven into its saturator, legato glide, a quick vibrato on the wheel: the late-70s lead of Billy Currie with Gary Numan; an overdrive insert on its strip adds the grit, since presets don't set the mixer; no transcription ships) and **Odyssey sync** (VCO 2 synced to a silent VCO 1, its pitch swept by the ADSR). The bends of such a solo need a pitch-bend source (#10).

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Odyssey`, `crates/dsp/src/mono/model.rs::ODYSSEY`, `crates/dsp/src/mono/preset.rs` (`CurrieLead`, `OdysseySync`), `web/src/audio/models.ts` (#64)

#### Scenario: the high-pass after the ladder

- GIVEN an Odyssey playing A1 with the ladder open
- WHEN the high-pass goes from 20 Hz to 2 kHz
- THEN the level falls below 60%, while on the ARP 2600 the same setting changes nothing

#### Scenario: one envelope moves both

- GIVEN an Odyssey with the ADSR at full amount on the cutoff
- WHEN a note's attack runs
- THEN the cutoff follows the loudness at every point, and the filter ADSR plays no part

**Tests:** `crates/dsp/src/mono/voice.rs::tests::odyssey_high_pass_takes_out_the_lows`, `crates/dsp/src/mono/voice.rs::tests::odyssey_one_envelope_moves_cutoff_and_loudness`, `crates/dsp/src/mono/voice.rs::tests::ladder_voicings_differ_and_stay_bounded`, `crates/dsp/src/mono/model.rs::tests::single_envelope_models_follow_the_adsr`, `crates/dsp/src/mono/preset.rs::tests::every_preset_is_bounded`, `crates/dsp/src/mono/preset.rs::tests::every_model_has_at_least_two_presets`

### Requirement 11: Modular [SHOULD]

A `Modular` synth's voice SHALL be a SuperCollider SynthDef (ADR-0024, superseding the voice language of ADR-0020/0021), set on the synth by a preset (`ModularBasic`, `ModularHoover`, `ModularKick`) or by its code, and evaluated once at build time by a subset of sclang (functions, closures, `var`, `dup`, `Array.fill`, `Mix.fill`, `if` on numbers, left-to-right operators). Its UGens SHALL be `SinOsc Saw Pulse LFTri LFSaw LFPulse WhiteNoise PMOsc RLPF RHPF LPF HPF MoogFF CombL CombN CombC DelayN DelayL DelayC Rand ExpRand EnvGen Mix Out`, with `Env.adsr Env.perc Env.asr Env(levels, times, curves, releaseNode)`, `mul`/`add`, keyword arguments, multichannel expansion summed to mono, and on signals `range exprange tanh softclip distort atan midiratio midicps neg`; anything else SHALL be an error with a line and a column, and the synth SHALL play on as it was. Every number a UGen takes, an envelope's level or time, a `Rand`'s bound or a control's default SHALL be a knob, a live `Param::Ctl1`…`Ctl32` of the synth, and the code the engine hands out SHALL show the knobs' values in place of its numbers. What a program sizes SHALL be allocated for the synth's voices when its code is set, never in `render`; a program SHALL use at most 250 nodes, 64 oscillators, 32 phases, 8 filters, 8 envelopes, 32 delays and 64 random numbers, and its cost SHALL cap its voices. Each note SHALL copy the program when it starts and draw its random numbers from a seed, so a render repeats. A voice SHALL end when its envelopes do; one without SHALL sound through the synth's ADSR. An imported `MoogFF` SHALL be the Moog's transistor ladder at unity drive, and the old voice language's filters MAY name a synth's voicing (`ladder(sh101, …)`, `svf(ms20, lp, …)`, #307). `pan`, `Splay` and `FreeVerb2` SHALL wait for a stereo synth bus (#288).

**Implementation:** `crates/dsp/src/modular.rs::Program`, `crates/dsp/src/modular.rs::GraphVoice`, `crates/dsp/src/poly.rs::PolyVoice` (`Graph`), `crates/dsp/src/song.rs::Voice`, `crates/dsp/src/song.rs::Ctl`, `crates/dsp/src/params.rs::Param::ctl`, `crates/dsp/src/mono/preset.rs::Preset::code`, `crates/dsp/src/modular/sc.rs::compile`, `crates/dsp/src/engine.rs::Engine::set_code`, `web/src/audio/models.ts` (Modular)

#### Scenario: a sine plays its pitch

- GIVEN a Modular synth with the voice `sin(freq)`
- WHEN note 69 is held for a second
- THEN the output crosses zero upward 440 times, within two

#### Scenario: a voiced filter is the synth's

- GIVEN `saw(freq) |> ladder(sh101, 800, 0.9, 0.5)` and the SH-101's ladder fed the same saw at the same settings
- WHEN both are rendered
- THEN every sample is the same, and so for every ladder and SVF voicing

#### Scenario: a song's voice on its track

- GIVEN `voice beep = { sin(freq) * env(perc) }` and `track lead synth Modular beep` playing `a4`
- WHEN the song plays
- THEN the note sounds and dies away while its key is held

**Tests:** `crates/dsp/src/modular.rs::tests::fast_math_is_close`, `crates/dsp/src/modular.rs::tests::a_voice_prints_canonically_and_parses_back`, `crates/dsp/src/modular.rs::tests::errors_say_where`, `crates/dsp/src/modular.rs::tests::the_default_program_is_a_saw`, `crates/dsp/src/modular.rs::tests::voicing_words_name_the_synths`, `crates/dsp/src/modular.rs::tests::a_voiced_filter_renders_as_the_synths`, `crates/dsp/src/modular.rs::tests::drive_and_the_one_pole_high_pass_shape_the_sound`, `crates/dsp/src/mono/preset.rs::tests::every_modular_voice_compiles`, `crates/dsp/src/engine/tests.rs::a_modular_voice_sounds_bounded_deterministic_and_ends`, `crates/dsp/src/engine/tests.rs::a_modular_sine_plays_its_pitch`, `crates/dsp/src/engine/tests.rs::a_modular_voice_with_a_percussive_env_ends_while_held`, `crates/dsp/src/engine/tests.rs::a_song_voice_plays_on_its_track`, `crates/dsp/src/engine/tests.rs::the_modular_units_do_what_they_say`, `crates/dsp/src/engine/tests.rs::a_list_gives_each_voice_its_own_value`, `crates/dsp/src/engine/tests.rs::the_gabber_kick_sounds_right`, `crates/dsp/src/engine/tests.rs::the_hoover_sounds_right`, `crates/dsp/src/modular.rs::tests::range_reads_a_unipolar_source_from_0`, `crates/dsp/src/song/tests.rs::a_voice_line_and_a_modular_track_parse_and_print`, `crates/dsp/src/song/tests.rs::voice_errors_say_where`, `crates/dsp/src/modular/sc.rs::hoover::the_mono_hoover_builds`, `crates/dsp/src/engine/tests.rs::a_breakpoint_envelope_and_a_random_number_per_note`, `crates/dsp/src/engine/tests.rs::the_supercollider_hoover_plays_mono`, `crates/dsp/src/engine/tests.rs::a_synthdef_plays_with_live_knobs`, `crates/dsp/src/song/tests.rs::voice_controls_parse_resolve_and_print`, `crates/dsp/src/song/tests.rs::control_errors_say_where`, `crates/dsp/src/engine/tests.rs::a_voice_control_starts_holds_and_drives_the_sound`, `crates/dsp/src/song/lex.rs::tests::a_voice_and_its_controls`, `crates/dsp/tests/render_no_alloc.rs::a_busy_song_renders_without_allocating`
