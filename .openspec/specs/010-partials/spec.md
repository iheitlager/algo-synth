# 010: Partials, transforms and morphs

A sound as a partial set, from any source the engine has; transforms and morphs on partial sets; and three ways to play one: as a sample, as a PPG wavetable through the analog filters with a D-50 attack, and in the Modular's SuperCollider subset. Epic #192, after its first stages (spec 009). Decisions: ADR-0001, ADR-0002, ADR-0013, ADR-0017, ADR-0021, ADR-0024, ADR-0032.

Status: proposed. The implementation and test paths below are where the work goes; none of it is built yet.

Common to every requirement:
- **Offline work.** Analysis, transforms and morphs are offline calls (ADR-0017). They may allocate within spec 009's caps (60 s, 256 partials a frame), and they never panic.
- **Live playback.** It follows ADR-0002 and ADR-0021: fixed arrays sized when a table or program is loaded, the sine table instead of a transcendental per sample, a one-pole smoother on every partial level that changes, and a fade over the top octave for any partial that moves towards Nyquist.
- **Clean room.** Loris (GPL-2.0+) and Vital (GPL-3.0) are followed by their papers and formulas only. Plaits (MIT) code may be adapted with its notice.
- **Tests.** Every new parameter is in the registry and its mirror (ADR-0004). Tests render known sounds offline at 48 kHz.

### Requirement 1: The partial set [MUST]

A partial set SHALL be frames at a hop and rate. Each partial is a track with a start frame and, per frame, a frequency, an amplitude, a phase and a noise (bandwidth) value from 0 (a pure sine) to 1 (noise). It also carries a harmonic label: its harmonic number against an f0, or 0 for none. It SHALL extend spec 009's tracks, and an analysis without noise SHALL give noise 0 everywhere, so spec 009's results are unchanged.

A partial set MAY carry an f0 per frame and a spectral envelope per frame (Requirement 3).

**Implementation:** `crates/dsp/src/analysis/track.rs::Track`, `crates/dsp/src/analysis/partials.rs::PartialSet`

#### Scenario: spec 009 still holds

- GIVEN the analysis of spec 009's sum of three sines
- WHEN it is read as a partial set
- THEN its tracks, frequencies and amplitudes are those of spec 009, and every noise value is 0

**Tests:** `crates/dsp/src/analysis/partials.rs::tests::an_analysis_is_a_partial_set_with_no_noise`

### Requirement 2: Noise per partial [MUST]

The analysis SHALL give each partial the energy of the noise around it as bandwidth, after Fitz, Haken & Christensen (ICMC 2000):
- The spectral energy of each frame not explained by its partials SHALL be assigned to the nearest partial.
- The partial's amplitude SHALL then be `sqrt(sine² + noise energy)`, and its bandwidth `noise energy / total`.

Resynthesis SHALL play a partial with bandwidth β as a sinusoid amplitude-modulated by narrow-band noise. The modulation is a low-passed noise source whose depth follows √β, from a fixed seed, so a render repeats exactly. A morph SHALL interpolate bandwidth like any other dimension.

This replaces #192 stage 4's separate SMS residual for the partial set (ADR-0032).

**Implementation:** `crates/dsp/src/analysis/bandwidth.rs::associate`, `crates/dsp/src/analysis/additive.rs::resynthesise`

#### Scenario: a breathy tone

- GIVEN a 220 Hz sine plus white noise 20 dB under it
- WHEN it is analysed and resynthesised with noise
- THEN the 220 Hz partial has bandwidth above 0 and below 0.5, and the resynthesis's energy between the harmonics is within 3 dB of the original's, where without noise it would be at least 20 dB under

#### Scenario: a pure tone

- GIVEN the engine's sawtooth
- WHEN it is analysed with noise
- THEN every long partial's bandwidth is under 0.05

**Tests:** `crates/dsp/src/analysis/bandwidth.rs::tests::noise_goes_to_the_nearest_partial`, `crates/dsp/src/analysis/bandwidth.rs::tests::a_breathy_tone_keeps_its_breath`, `crates/dsp/src/analysis/bandwidth.rs::tests::a_saw_has_almost_no_bandwidth`, `crates/dsp/src/analysis/additive.rs::tests::noise_resynthesis_repeats_exactly`

### Requirement 3: The spectral envelope [SHOULD]

The analysis SHOULD estimate a smooth spectral envelope per frame by the true-envelope method (Röbel & Rodet, DAFx 2005):
- cepstral smoothing of the log spectrum, repeated with the envelope raised to the spectrum's peaks at each step, until it lies over them within 1 dB or after a bounded number of steps
- a cepstral order set from the frame's f0 when known, so the envelope does not follow single harmonics

It SHOULD be read as an amplitude at any frequency, so a partial moved in pitch can take the envelope's level at its new frequency.

**Implementation:** `crates/dsp/src/analysis/envelope.rs::true_envelope`, `crates/dsp/src/analysis/envelope.rs::Envelope::at`

#### Scenario: a formant

- GIVEN the engine's sawtooth at 110 Hz through a band-pass at 1 kHz
- WHEN its envelope is estimated
- THEN the envelope peaks within a third of an octave of 1 kHz and lies within 1 dB over the harmonics' peaks

**Tests:** `crates/dsp/src/analysis/envelope.rs::tests::the_envelope_finds_a_formant`, `crates/dsp/src/analysis/envelope.rs::tests::the_envelope_lies_over_the_peaks`

### Requirement 4: Transforms [MUST]

Each transform SHALL be one function on a partial set with one amount (and an optional second where named), offline, beside `shift` and `top_n` (spec 009 Req 5). Each SHALL leave the set's length and frame times alone unless it says otherwise:

| Transform | What it does to partial k (frequency fₖ, amplitude aₖ, label n) |
|---|---|
| harmonic stretch, s | fₖ → f₀·((n−1)·s + 1) for labelled partials; others by their ratio to f₀ |
| inharmonic stretch, s | fₖ → fₖ·s^(log₂ n / log₂ N), so the higher a partial, the further it moves (N the highest label) |
| frequency shift, Δ Hz | fₖ → fₖ + Δ (inharmonic: the classic frequency shifter) |
| formant scale, r | aₖ → aₖ · E(fₖ / r) / E(fₖ), with E the envelope (Req 3): formants move, pitch stays |
| smear, m | aₖ ← (1−m)·aₖ + m·aₖ₋₁ along the partials in frequency, then normalised to the set's energy |
| odd/even, b | odd labels × (1−b)·2, even × b·2, clamped to 0…1 each side |
| spectral low/high pass, c, q | aₖ × a two-pole magnitude response at cutoff c with resonance q, on the partials themselves |
| freeze, t | every frame takes frame t's frequencies, amplitudes and noise (the length stays) |
| decay by number, d | aₖ(t) × exp(−d · n · t): the higher partials die first (Harmor's Pluck); d < 0 the other way |
| noise amount, g | bandwidth × g, clamped to 0…1 |

After any transform, a partial at or above Nyquist SHALL be faded out over the top octave.

**Implementation:** `crates/dsp/src/analysis/edit.rs` (`stretch`, `inharmonic`, `freq_shift`, `formant_scale`, `smear`, `odd_even`, `spectral_filter`, `freeze`, `decay_by_number`, `noise_amount`)

#### Scenario: a saw becomes a bell

- GIVEN the engine's sawtooth at 220 Hz
- WHEN it is stretched inharmonically by 1.5
- THEN harmonic 1 stays at 220 Hz and harmonic 8 moves above 8·220·1.4 Hz

#### Scenario: formants stay

- GIVEN Requirement 3's formant at 1 kHz
- WHEN the partials are shifted a fifth up and formant-scaled by 1/1.5
- THEN the resynthesis's envelope still peaks within a third of an octave of 1 kHz

**Tests:** `crates/dsp/src/analysis/edit.rs::tests::stretch_moves_harmonics_apart`, `crates/dsp/src/analysis/edit.rs::tests::inharmonic_moves_high_partials_most`, `crates/dsp/src/analysis/edit.rs::tests::freq_shift_adds_hertz`, `crates/dsp/src/analysis/edit.rs::tests::formants_stay_when_pitch_moves`, `crates/dsp/src/analysis/edit.rs::tests::smear_keeps_energy`, `crates/dsp/src/analysis/edit.rs::tests::freeze_holds_a_frame`, `crates/dsp/src/analysis/edit.rs::tests::decay_by_number_kills_the_highs_first`, `crates/dsp/src/analysis/edit.rs::tests::nothing_passes_nyquist`

### Requirement 5: Labels and distillation [MUST]

To morph, a partial set SHALL be labelled and distilled, after Loris:
- **Labelling.** A partial SHALL take the label `round(mean over its frames of fₖ / f0(t))`, from the frame's f0 (spec 009 Req 3), unweighted. Frames without an f0 SHALL not count, and a partial without any SHALL keep label 0.
- **Distillation.** For each label, the longest partial SHALL be kept.
  - The parts of shorter partials with the same label SHALL be spliced in where the kept partial is silent, with a fade either side (5 ms).
  - The energy of whatever overlaps SHALL be added to the kept partial as noise: `a' = sqrt(a² + o²)`, `β' = (a²·β + o²) / (a² + o²)`.
  - A distilled set has at most one partial per label at any time.

**Implementation:** `crates/dsp/src/analysis/morph.rs::label`, `crates/dsp/src/analysis/morph.rs::distill`

#### Scenario: a broken harmonic

- GIVEN a sawtooth whose 5th harmonic is tracked as two partials with a gap of two frames between them
- WHEN the set is labelled and distilled
- THEN there is one partial with label 5, spanning both

**Tests:** `crates/dsp/src/analysis/morph.rs::tests::labels_follow_f0`, `crates/dsp/src/analysis/morph.rs::tests::distilling_joins_a_broken_harmonic`, `crates/dsp/src/analysis/morph.rs::tests::overlap_becomes_noise`, `crates/dsp/src/analysis/morph.rs::tests::no_f0_no_label`

### Requirement 6: Time alignment [SHOULD]

Before a morph, two partial sets SHOULD be aligned on feature times: the attack onset, the end of the attack (the energy peak) and the release (the last fall below −30 dB of the peak), found from the set's energy envelope or given by hand. Each set's frame times SHOULD be mapped piecewise-linearly so its features fall on the shared times, the mean of the two sets' unless given. Before the first feature they SHOULD be scaled, and after the last shifted.

**Implementation:** `crates/dsp/src/analysis/morph.rs::features`, `crates/dsp/src/analysis/morph.rs::dilate`

#### Scenario: a short and a long attack

- GIVEN a sound with a 10 ms attack and one with a 200 ms attack
- WHEN both are aligned
- THEN both reach their peaks at 105 ms

**Tests:** `crates/dsp/src/analysis/morph.rs::tests::features_find_attack_and_release`, `crates/dsp/src/analysis/morph.rs::tests::dilation_aligns_the_peaks`

### Requirement 7: Morph [MUST]

The engine SHALL morph two labelled, distilled and aligned partial sets A and B into one, with four positions from 0 (A) to 1 (B): **pitch, amplitude, noise and formant**. Each position is a number or a curve over time.

- **Matched pairs.**
  - A partial in A and one in B with the same label SHALL make one partial at the union of their frame times.
  - Frequency is interpolated linearly by pitch; log frequency is an option.
  - Amplitude is interpolated by `(x+s)·((y+s)/(x+s))^α − s`, clamped at 0, with α the amplitude position and s = 1e-5.
  - Noise is interpolated the same way by its position.
- **Formant.** The formant position SHALL interpolate the two spectral envelopes (Req 3) in log amplitude. A partial's amplitude SHALL then be re-weighted by that envelope at its morphed frequency, over the amplitude morph. Without envelopes it has no effect.
- **A partial without a counterpart.**
  - When the other set has an f0, it SHALL glide to its label's place there (`label · f0_other(t)`) while its amplitude goes to 0 by the amplitude position.
  - When the other set has no f0, it SHALL fade in place.
- **Unlabelled partials** SHALL be crossfaded by the amplitude position.
- **Pre-cleaning.** Frames below −90 dB SHALL have their frequency drawn to `label · f0` before interpolating, so near-silent noise never glides audibly.

**Implementation:** `crates/dsp/src/analysis/morph.rs::morph`, `crates/dsp/src/analysis/morph.rs::Positions`

#### Scenario: ends

- GIVEN any two sets A and B
- WHEN they are morphed at all positions 0, and then all positions 1
- THEN the result is A, then B, within 0.1 dB and 1 cent on every partial

#### Scenario: pitch of one, timbre of the other

- GIVEN a sawtooth at 220 Hz (A) and a square at 330 Hz (B)
- WHEN the pitch position is 0 and amplitude 1
- THEN the result is at 220 Hz with only odd harmonics

#### Scenario: an orphan glides

- GIVEN A with harmonics 1–10 and B with harmonics 1–5
- WHEN they are morphed at amplitude 0.5
- THEN A's harmonic 8 is present at half its level (in the log sense) and moves towards 8 · f0(B)

**Tests:** `crates/dsp/src/analysis/morph.rs::tests::ends_are_the_sources`, `crates/dsp/src/analysis/morph.rs::tests::pitch_of_one_timbre_of_the_other`, `crates/dsp/src/analysis/morph.rs::tests::orphans_glide_to_their_place`, `crates/dsp/src/analysis/morph.rs::tests::amplitude_morph_is_logarithmic`, `crates/dsp/src/analysis/morph.rs::tests::formant_morph_moves_the_formant`, `crates/dsp/src/analysis/morph.rs::tests::unlabelled_partials_crossfade`

### Requirement 8: Partial sets from other sources [SHOULD]

A partial set SHOULD come from more than a WAV:

- **A patch's spectrum.**
  - The harmonics of an oscillator wave (saw 1/k, square odd 1/k, pulse sin(kπd)/k, triangle odd 1/k²) at a pitch, times the response of a filter at each harmonic, as one frame, or as frames over an envelope's time.
  - The response SHALL be measured once per model, filter voicing, cutoff and resonance step, by running the filter itself on sines, because the ladder is non-linear. It SHALL be read by interpolation, so the patch's cutoff and resonance stay parameters of the set.
- **Macros (after Plaits, MIT).** Three values, harmonics, timbre and morph, SHALL give 24 harmonic amplitudes by Plaits' closed form, normalised by their sum.
- **A wavetable.** Each wave of a table SHALL be one frame of up to 31 harmonics, by an FFT of the wave.

**Implementation:** `crates/dsp/src/analysis/sources.rs::patch_spectrum`, `crates/dsp/src/analysis/sources.rs::FilterResponse`, `crates/dsp/src/analysis/sources.rs::macros`, `crates/dsp/src/analysis/sources.rs::from_table`

#### Scenario: a filtered saw

- GIVEN a sawtooth at 110 Hz through the Minimoog ladder at 1 kHz with no resonance
- WHEN its patch spectrum is made, and the same patch is rendered by the engine and analysed
- THEN the two agree within 1.5 dB on the first 20 harmonics

**Tests:** `crates/dsp/src/analysis/sources.rs::tests::patch_spectrum_matches_the_engine`, `crates/dsp/src/analysis/sources.rs::tests::macros_are_normalised`, `crates/dsp/src/analysis/sources.rs::tests::a_table_wave_is_its_harmonics`

### Requirement 9: User wavetables (the PPG path) [MUST]

The engine SHALL hold user wavetables beside its generated ones (spec 006 Req 10):
- **Storage.** A fixed number of slots (8), each of up to 64 waves of 256 samples, written through a buffer outside `render` (as a sample, ADR-0013) and never allocated in it.
- **Rendering.** A partial set SHALL render into a user table: up to 64 frames chosen evenly over its length, or at given times, each frame's labelled partials up to 31 harmonics written as one wave. Inharmonic partials are rounded to the nearest harmonic, and anything above harmonic 31 is dropped.
- **Selection.** `Wt1Table` and `Wt2Table` SHALL reach the user tables after the generated ones, and `Wt1Pos`/`Wt2Pos` SHALL move through their frames as through any table. So the PPG model plays an analysed sound through its filters and envelopes, and a morph between two frames is a position.

**Implementation:** `crates/dsp/src/table.rs::UserTables`, `crates/dsp/src/analysis/frames.rs::to_table`, `crates/dsp/src/ffi.rs` (`table_buf`, `table_load`, `spectral_table`), `crates/dsp/src/params.rs` (`Wt1Table`, `Wt2Table` ranges), `web/src/audio/params.ts`

#### Scenario: an analysed sound on the PPG

- GIVEN a sawtooth rendered into user table 0
- WHEN the PPG model plays C4 with `Wt1Table` on user table 0 at position 0
- THEN the output's harmonics fall as 1/k within 1.5 dB up to the 16th

#### Scenario: a morph as a position

- GIVEN a table whose first wave is a saw and whose last is a square
- WHEN `Wt1Pos` sweeps from 0 to 1
- THEN the even harmonics fall from saw level to under −40 dB, and every sample is finite and bounded

**Tests:** `crates/dsp/src/table.rs::tests::user_tables_load_outside_render`, `crates/dsp/src/analysis/frames.rs::tests::a_saw_becomes_a_saw_table`, `crates/dsp/src/engine/tests.rs::the_ppg_plays_a_user_table`, `crates/dsp/src/engine/tests.rs::a_table_position_is_a_morph`

### Requirement 10: Attack and body (the D-50 path) [SHOULD]

A partial set SHOULD split at the end of its attack (Requirement 6's feature), to give two parts:
- **The attack:** the original's samples up to there plus a 10 ms fade, as a sample for an LA partial's PCM slot (`Pcm1Sample`/`Pcm2Sample` taking a user sample after the built-in ones).
- **The body:** the partials after it, as a user table (Requirement 9).

The D-50 model playing both SHALL sound the original's transient over a body that its filter, envelopes and table position shape.

**Implementation:** `crates/dsp/src/analysis/frames.rs::split_attack`, `crates/dsp/src/la.rs`, `crates/dsp/src/params.rs` (`Pcm1Sample`, `Pcm2Sample` ranges)

#### Scenario: a piano's hammer

- GIVEN an analysed piano note
- WHEN it is split and played on the D-50 model at its own root
- THEN the first 30 ms differ from the original by less than −20 dB, and the body's harmonics follow the original's within 3 dB at 200 ms

**Tests:** `crates/dsp/src/analysis/frames.rs::tests::split_finds_the_attack`, `crates/dsp/src/engine/tests.rs::the_d50_plays_attack_and_body`

### Requirement 11: Partials in the Modular (the SuperCollider path) [SHOULD]

The Modular's subset of sclang (ADR-0024) SHOULD gain these words, each behaving as in SuperCollider within the voice's fixed limits:

- **The `` `[ ... ] `` literal** (a `Ref`): an array kept whole rather than expanded across channels.
- **`Klang.ar(specs, freqscale, freqoffset)` and `DynKlang.ar/kr(...)`:**
  - a bank of sines from `` `[freqs, amps, phases] ``, read from the sine table
  - each sine taking a phase from `MAX_PHASES`, and a bank past it refused at build time with its line and column
  - `DynKlang`'s specs may be signals; `Klang`'s are numbers
- **`Klank.ar(specs, input, freqscale, freqoffset, decayscale)` and `DynKlank`:** a bank of two-pole resonators from `` `[freqs, amps, ringtimes] `` excited by `input`, each taking a filter state from a new per-voice limit (32).
- **`VOsc.ar(bufpos, freq, phase)` and `Osc.ar(bufnum, freq, phase)`:**
  - wavetable oscillators on the user tables (Requirement 9), where SuperCollider would read buffers
  - `VOsc` crossfades between waves by the fractional part of `bufpos`, as the PPG's position does; `Osc` reads one wave
  - a table number past the slots is silent

A number in any of these SHALL be a knob as every number in a SynthDef is (ADR-0024).

**Implementation:** `crates/dsp/src/modular/sc.rs` (lexer: `` ` ``; `ugen()`: `Klang`, `DynKlang`, `Klank`, `DynKlank`, `VOsc`, `Osc`), `crates/dsp/src/modular.rs` (`MAX_RESONATORS`, table reads)

#### Scenario: a bank of sines

- GIVEN `{ Klang.ar(`[[220, 440, 660], [0.3, 0.2, 0.1], nil]) }`
- WHEN a note plays
- THEN the output holds 220, 440 and 660 Hz at those levels within 0.5 dB

#### Scenario: an analysed sound through a Moog filter

- GIVEN user table 0 from an analysis and `{ |freq| MoogFF.ar(VOsc.ar(\pos.kr(0.3), freq), \cutoff.kr(2000)) }`
- WHEN a note plays and `pos` moves
- THEN the timbre follows the table's frames, `pos` and `cutoff` are knobs, and every sample is finite and bounded

#### Scenario: too many partials

- GIVEN `Klang` with 40 sines
- WHEN the code is built
- THEN it is refused with a message naming the limit, and the synth plays on as it was

**Tests:** `crates/dsp/src/modular/sc.rs::tests::backtick_keeps_an_array_whole`, `crates/dsp/src/modular/sc.rs::tests::klang_is_a_bank_of_sines`, `crates/dsp/src/modular/sc.rs::tests::dynklank_rings`, `crates/dsp/src/modular/sc.rs::tests::vosc_crossfades_tables`, `crates/dsp/src/modular/sc.rs::tests::too_many_partials_are_refused`

### Requirement 12: Sequencing timbre [SHOULD]

The table positions, the morph positions of a live morph, freeze as a 0/1 gate, and the transform amounts that run live SHOULD be registry parameters, sequenced by the language as any other: per-step locks (`.wt1pos("~ ~ 0.8 ~")`), signals (`.wt1pos(sine.slow(8))`) and automation lanes.

A signal SHOULD also be sampleable at each note-on and held for that note (`.wt1pos(rand.onNote)`, the name to be settled with #514), as Tidal samples a continuous pattern per event. That gives each note its own timbre from a random or generative signal.

**Implementation:** `crates/dsp/src/song` (parameter methods, a per-note-on hold), `.openspec/language.md` (sound-design part, #514)

#### Scenario: a timbre per note

- GIVEN a clip of eight notes on the PPG with `.wt1pos(rand.onNote)`
- WHEN it plays
- THEN each note holds one position for its length, and the positions differ between notes

**Tests:** `crates/dsp/src/engine/tests.rs::a_signal_held_per_note`, `crates/dsp/src/engine/tests.rs::a_locked_table_position`

### Requirement 13: Morphing in the lab [SHOULD]

The Spectral Lab (spec 009 Req 7) SHOULD:
- hold two sounds, A and B, each from a WAV, a patch spectrum or macros
- morph them with the four positions as sliders, and the transforms of Requirement 4 as controls
- offer three ways to send the result:
  - as a sample (as now)
  - as a multisample, one rendering per root note over a range, into a Sampler's zones
  - as a user table, or as an attack and a body (Requirements 9 and 10)

**Implementation:** `crates/dsp/src/analysis/lab.rs`, `crates/dsp/src/ffi.rs` (`spectral_*`), `web/src/components/lab/SpectralLab.vue`, `web/src/audio/spectral.ts`

#### Scenario: a morph sent as a table

- GIVEN A a sawtooth and B an analysed bell in the lab
- WHEN the amplitude position is swept into a 16-wave table and sent
- THEN the main window's PPG synth can play it with `Wt1Pos` moving from the saw to the bell

**Tests:** `crates/dsp/src/analysis/lab.rs::tests::two_sources_morph`, `crates/dsp/src/analysis/lab.rs::tests::a_morph_renders_a_table`, `web/src/audio/spectral.test.ts`

### Requirement 14: Fitting a DX7 voice (stage 6, refined) [MAY]

A DX7 fit MAY choose its algorithm and operator ratios from the partial set (simple ratios between partial clusters and f0, or a small discrete search), then fit levels, envelope rates and feedback by CMA-ES. The loss would be a multi-resolution STFT (L1 on magnitude and log magnitude) plus an envelope term, with the step API, worker and `.syx` export of #192 stage 6. Spectral losses give no gradient for ratios (DDX7, ISMIR 2022), which is why the ratios are chosen first.

**Implementation:** `crates/dsp/src/analysis/fm_fit.rs`

**Tests:** `crates/dsp/src/analysis/fm_fit.rs::tests::an_engine_dx7_preset_is_recovered`
