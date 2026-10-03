# 005: Synth models

The family of monosynths (epic #28): each of the 16 synth slots is one of six instruments, built from the shared Mono modules of spec 004. Decisions: ADR-0001, ADR-0002, ADR-0004, ADR-0009. Draft: requirements marked *(planned)* are not built yet.

Common to every requirement: `render` follows ADR-0002, every parameter and id is mirrored in `web/src/audio/params.ts` (ADR-0004), and a model's sound is an interpretation of the instrument: each requirement names the property it must have, and is tested on that. Tests render offline at 48 kHz.

### Requirement 1: Models [MUST]

Every synth SHALL have a model, `Param::Model` (`Arp2600`, `Minimoog`, `ProOne`, `Ms20`, `Cs15`, `Sh101`), held with the synth's parameters; a new or reset synth SHALL be an ARP 2600. A model SHALL decide which filter the voice uses and its voicing, whether a high-pass stage exists, which envelope drives the normalled cutoff, and whether the decay time also sets the release (ADR-0009). Every other parameter SHALL exist on every model. Each model SHALL have at least two presets; a preset SHALL set the model and every Mono parameter (spec 004 Req 9), and selecting a model in the view SHALL load that model's first preset. Unknown model ids SHALL be ignored.

**Implementation:** `crates/dsp/src/mono/model.rs::Model`, `crates/dsp/src/mono/preset.rs::Preset`, `crates/dsp/src/mono.rs::MonoParams` (#30)

#### Scenario: models are per synth

- GIVEN synth 0 an ARP 2600 and synth 1 a Minimoog, both on one note
- WHEN each renders
- THEN the outputs differ, and setting synth 1's model leaves synth 0's parameters and sound unchanged

#### Scenario: every preset is safe

- GIVEN each preset of each model
- WHEN a note across the keyboard is held and released
- THEN every sample is finite and bounded, and the voice falls silent after its release

**Tests:** `crates/dsp/src/mono/model.rs::tests::ids_round_trip`, `crates/dsp/src/engine.rs::tests::models_are_per_synth`, `crates/dsp/src/engine.rs::tests::models_sound_different`, `crates/dsp/src/mono/preset.rs::tests::every_preset_is_bounded`, `crates/dsp/src/mono/preset.rs::tests::every_preset_sets_its_model`, `crates/dsp/src/mono/preset.rs::tests::every_model_has_at_least_two_presets`, `crates/dsp/src/mono/preset.rs::tests::a_new_synth_is_an_arp_2600`

### Requirement 2: ARP 2600 [MUST]

The ARP 2600 SHALL be the voice of spec 004 as it was before models: three VCOs, noise, the Moog-voiced ladder, one ADSR driving the VCA and the normalled cutoff, an AR, an LFO and sample-and-hold, and the patch of Req 7, with no high-pass stage. Its output SHALL NOT change with the model parameter's introduction.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Arp2600`, `web/src/audio/models.ts` (#34)

#### Scenario: as before models

- GIVEN each of the four original presets
- WHEN A3 is held for half a second
- THEN rms, peak and two samples equal the values recorded before models, within 2e-5

**Tests:** `crates/dsp/src/mono/preset.rs::tests::arp_presets_keep_their_sound`, `crates/dsp/src/mono/voice.rs::tests::arp_cutoff_still_follows_the_adsr`, `crates/dsp/src/mono/preset.rs::tests::a_new_synth_is_an_arp_2600`

### Requirement 3: Minimoog [MUST]

The Minimoog SHALL have three VCOs, noise, a Moog-voiced ladder with overdrive in the mixer (`Drive`), a loudness ADSR and a separate filter ADSR, each with its decay time also setting its release, glide, low note priority, and Osc 3 usable as a modulator (spec 004 Req 13). It has no LFO: Osc 3 SHALL be the source of its vibrato and of `LfoCutoff` (the modulation normals), and its panel SHALL have no patch panel. Its normalled cutoff SHALL follow the filter ADSR, with key tracking in steps of off, 1/3, 2/3 and full.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Minimoog`, `crates/dsp/src/mono/voice.rs::MonoVoice::render` (#35)

#### Scenario: decay is release

- GIVEN a Minimoog with decay 0.1 s and release 4 s
- WHEN a held note is released
- THEN the loudness falls in about 0.1 s, not 4 s

#### Scenario: the filter contour moves the cutoff, not the loudness

- GIVEN a Minimoog with a long filter attack and an instant loudness attack
- WHEN a note is held
- THEN the loudness is full at once while the spectrum brightens over the filter attack

**Tests:** `crates/dsp/src/mono/voice.rs::tests::minimoog_decay_is_release`, `crates/dsp/src/mono/voice.rs::tests::minimoog_filter_contour_brightens_a_held_note`, `crates/dsp/src/mono/patch.rs::tests::the_modulation_source_is_the_lfo_or_osc3`, `crates/dsp/src/mono/voice.rs::tests::ladder_voicings_differ_and_stay_bounded`

### Requirement 4: Pro-One [MUST]

The Pro-One SHALL have two oscillators (A and B), the second synced to the first, noise, a 4-pole low-pass in its own voicing, a loudness ADSR and a filter ADSR, an LFO, and poly-mod: the filter envelope and oscillator B each modulate oscillator A's pitch and pulse width, and oscillator B the cutoff (spec 004 Req 14). Its oscillator A SHALL be VCO 2 and B VCO 1, so sync and poly-mod run in the direction the instrument has.

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

**Tests:** `crates/dsp/src/engine.rs::tests::ms20_band_limits_noise`, `crates/dsp/src/mono/svf.rs::tests::self_oscillation_is_bounded`

### Requirement 6: Yamaha CS-15 [MUST]

The CS-15 SHALL have two VCOs, noise, ring modulation of VCO 1 by VCO 2, a high-pass and a low-pass 12 dB filter in the Yamaha voicing (resonant but not self-oscillating), the low-pass moved by the filter ADSR and the high-pass by the AR envelope, a loudness ADSR, an LFO and sample-and-hold on the patch.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Cs15` (#38)

#### Scenario: each filter has its own envelope

- GIVEN the AR envelope into the high-pass and the filter ADSR into the low-pass
- WHEN a note is held
- THEN moving the AR amount changes the low end of the spectrum and not the high end, and moving the ADSR amount does the reverse

**Tests:** `crates/dsp/src/engine.rs::tests::cs15_filters_have_their_own_envelopes`

### Requirement 7: Roland SH-101 [MUST]

The SH-101 SHALL have one VCO whose saw and pulse are mixed (the pulse a phase-locked second VCO), a sub-oscillator one or two octaves down, noise, an IR3109-voiced 4-pole low-pass, a one-pole high-pass after it, a single ADSR driving the filter and the VCA, an LFO that can modulate pitch, cutoff and pulse width, and glide.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Sh101` (#39)

#### Scenario: one envelope moves both

- GIVEN the SH-101 with a cutoff envelope amount
- WHEN a note is held and released
- THEN the cutoff and the loudness follow the same envelope

**Tests:** `crates/dsp/src/engine.rs::tests::sh101_one_envelope_moves_cutoff_and_loudness`

### Requirement 8: Panels and colours [MUST]

The view SHALL draw each synth with a panel of its model: the sections, control names and order of that instrument, and a palette of its own (panel, lettering, trim and accent) as CSS variables, so the six are told apart at a glance. A panel SHALL show only the controls its instrument has and SHALL send edits only (spec 003 Req 6). Selecting a model SHALL send the model's first preset.

**Implementation:** `web/src/audio/models.ts`, `web/src/components/SynthPanel.vue`, `web/src/components/InstrumentsPane.vue` (#30)

#### Scenario: six instruments

- GIVEN six synths, one of each model
- WHEN the view is drawn
- THEN each card carries its model's name and palette, and the controls of its panel

**Tests:** `crates/dsp/src/params.rs::tests::typescript_mirror_matches`, `cd web && npm run typecheck`

### Requirement 9: Mixed ensemble budget [MUST]

Sixteen synths across all six models SHALL render within the performance budget of plan.md (25% of a core), and every model's presets SHALL be bounded at full master gain.

**Implementation:** `tools/bench.mjs` (#40)

#### Scenario: sixteen at once, six models

- GIVEN 16 synths cycling through the six models and their presets
- WHEN each plays a note at full master gain
- THEN 16 voices sound and every sample is finite and within ±1

**Tests:** `crates/dsp/src/engine.rs::tests::sixteen_synths_of_every_model_play_together`, `make bench`
