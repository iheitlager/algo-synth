# 010: Partials, transforms and morphs

A sound as a partial set, from any source the engine has; transforms and morphs on partial sets; and three ways to play one: as a sample, as a PPG wavetable through the analog filters (with its attack on a D-50 partial), and in the Modular's SuperCollider subset. Epic #192, after its first stages (spec 009). Decisions: ADR-0001, ADR-0002, ADR-0013, ADR-0017, ADR-0021, ADR-0024, ADR-0032.

Status: proposed. The implementation and test paths below are where the work goes, marked (planned) until they exist; none of it is built yet.

Common to every requirement:
- **Offline work.** Analysis, transforms and morphs are offline calls (ADR-0017). They may allocate within spec 009's caps (60 s, 256 partials a frame), and they never panic.
- **Live playback.** It follows ADR-0002 and ADR-0021: fixed arrays sized when a table or program is loaded, the sine table instead of a transcendental per sample, a one-pole smoother on every partial level that changes, and a fade over the top octave for any partial that moves towards Nyquist.
- **Clean room.** Loris (GPL-2.0+) and Vital (GPL-3.0) are followed by their papers and formulas only. Plaits (MIT) code may be adapted with its notice.
- **Tests.** Every new parameter is in the registry and its mirror (ADR-0004). Tests render known sounds offline at 48 kHz.

### Requirement 1: The partial set [MUST]

A partial set SHALL be frames at a hop and rate. Each partial is a track with a start frame and, per frame, a frequency, an amplitude, a phase and a noise (bandwidth) value from 0 (a pure sine) to 1 (noise). It also carries a harmonic label: the rounded mean, over its frames that have an f0, of its frequency over that f0, unweighted; 0 when none of its frames has one. It SHALL extend spec 009's tracks, so spec 009's results are unchanged:
- An empty noise list reads as 0 everywhere.
- An analysis without noise association gives noise 0.

**Implementation:** `crates/dsp/src/analysis/track.rs::Track`, `crates/dsp/src/analysis/peaks.rs::Peak`, `crates/dsp/src/analysis/harmonic.rs::label`

#### Scenario: a labelled saw

- GIVEN the engine's sawtooth at 220 Hz, analysed and labelled
- WHEN its long partials under 5 kHz are read
- THEN each is labelled `round(f / 220)`, and every noise value is 0 without association

**Tests:** `crates/dsp/src/analysis/harmonic.rs::tests::labels_follow_f0`, `crates/dsp/src/analysis/bandwidth.rs::tests::a_breathy_tone_has_bandwidth`

### Requirement 2: Noise per partial [MUST]

An analysis asked for noise SHALL turn each frame's peaks into partials with bandwidth, after Fitz, Haken & Christensen (ICMC 2000):
- **What counts as noise.** A peak more than 40 dB under the frame's loudest is noise, not a partial, and is removed. The energy of every bin outside all peaks' main lobes (±8 bins, the Blackman–Harris lobe zero-padded twice) is noise too, converted to a sine's amplitude squared by the window's lobe energy.
- **Where it goes.** All of that noise energy SHALL go to the nearest partial within 500 Hz. Noise further away is dropped, so noise stays where it was.
- **The result.** The partial's amplitude SHALL become `sqrt(sine² + noise)`, and its bandwidth `noise / total`.

**Resynthesis.**
- A partial with bandwidth β SHALL be a sinusoid amplitude-modulated by `sqrt(1 − β) + sqrt(β)·ζ`. ζ is noise low-passed at 500 Hz with unit variance, from a fixed seed per partial, so its energy is kept and a render repeats exactly.
- A morph SHALL interpolate bandwidth like any other dimension.

This replaces #192 stage 4's separate SMS residual for the partial set (ADR-0032).

**Implementation:** `crates/dsp/src/analysis/bandwidth.rs::associate`, `crates/dsp/src/analysis/stft.rs::Stft::lobe_energy`, `crates/dsp/src/analysis.rs::Settings`, `crates/dsp/src/analysis/additive.rs::resynthesise`

#### Scenario: a breathy tone

- GIVEN a 440 Hz sine at 0.5 plus white noise at 0.05
- WHEN it is analysed and resynthesised with noise, and again without
- THEN the energy beside the partial (100–300 Hz and 600–800 Hz) is within 5 dB of the original's with noise, and more than 40 dB further under without it (measured: −4.2 dB and −59 dB)

#### Scenario: a pure tone

- GIVEN the engine's sawtooth
- WHEN it is analysed with noise
- THEN every long partial under 8 kHz has bandwidth under 0.05 (the weakest near Nyquist pick up the oscillator's aliasing residue)

**Tests:** `crates/dsp/src/analysis/bandwidth.rs::tests::noise_goes_to_the_nearest_partial`, `crates/dsp/src/analysis/bandwidth.rs::tests::a_breathy_tone_has_bandwidth`, `crates/dsp/src/analysis/bandwidth.rs::tests::a_saw_has_almost_no_bandwidth`, `crates/dsp/src/analysis/additive.rs::tests::a_breathy_tone_keeps_its_breath`, `crates/dsp/src/analysis/additive.rs::tests::noise_resynthesis_repeats_exactly`

### Requirement 3: The spectral envelope [SHOULD]

The analysis SHOULD estimate a smooth spectral envelope per frame by the true-envelope method (Röbel & Rodet, DAFx 2005):
- **Its input.** It runs on a log spectrum of 512 points drawn through the frame's peaks (straight in log amplitude between neighbours) rather than the full STFT. The envelope is only read at partial frequencies, and this keeps it cheap enough to recompute as the lab's controls move.
- **The iteration.** It raises the spectrum to the envelope and smooths it cepstrally, at an order of Nyquist over f0 (or 25 without an f0), until it lies over the peaks within 1 dB, at most 40 times.
- **Reading it.** It SHOULD be readable as an amplitude at any frequency, so a partial moved in pitch can take the envelope's level at its new frequency.

**Implementation:** `crates/dsp/src/analysis/envelope.rs::true_envelope`, `crates/dsp/src/analysis/envelope.rs::Envelope::at`

#### Scenario: a formant

- GIVEN harmonics of 110 Hz under a resonance at 1 kHz
- WHEN its envelope is estimated
- THEN it peaks within a third of an octave of 1 kHz and lies within 1.5 dB over every peak under 5 kHz

**Tests:** `crates/dsp/src/analysis/envelope.rs::tests::the_envelope_finds_a_formant`, `crates/dsp/src/analysis/envelope.rs::tests::the_envelope_lies_over_the_peaks`

### Requirement 4: Transforms [MUST]

Each transform SHALL be one function on a partial set with one amount, offline, beside `shift` and `top_n` (spec 009 Req 5), leaving the set's length and frame times alone unless it says otherwise:

| Transform | What it does to partial k (frequency fₖ, amplitude aₖ, label n) |
|---|---|
| harmonic stretch, s | labelled: fₖ × ((n−1)·s + 1)/n, so harmonic 1 stays |
| inharmonic stretch, s | labelled: fₖ × s^(log₂ n / log₂ N), N the highest label: the higher, the further |
| frequency shift, Δ Hz | fₖ + Δ; at or below 0 Hz the partial falls silent |
| formant scale, r | aₖ × E(fₖ / r) / E(fₖ), E the envelope (Req 3) of the partials as they are, clamped to ±40 dB |
| smear, m | per frame, along the partials in frequency: aₖ ← (1−m)·aₖ + m·aₖ₋₁ (the already smeared one below), then scaled back to the frame's energy |
| odd/even, b | odd labels × min(1, 2(1−b)), even × min(1, 2b) |
| spectral low/high pass, c, q | aₖ × a two-pole magnitude at cutoff c, Q from 0.707 to 10 as q goes 0 to 1 |
| freeze, t | the partials sounding at frame t, held there for the whole sound; the rest gone |
| decay by number, d | aₖ(t) × exp(−d · n · t), t seconds from the start (n 1 when unlabelled), never more than +40 dB |
| noise amount, g | bandwidth × g, clamped to 0…1 |

**The order the lab applies them in:**
1. freeze
2. the stretches
3. odd/even
4. decay
5. smear
6. the filters
7. formant scale
8. noise
9. frequency shift
10. the pitch shift

Formant scale comes before the pitch shift because the envelope describes the partials as they are. A formant-preserving shift by s therefore scales the formants by 1/s first and then shifts.

Resynthesis SHALL fade a partial out over the last tenth of the band below Nyquist, so a partial pushed up by a transform leaves without a click. A whole top octave would dull every sound above a quarter of the rate.

**Implementation:** `crates/dsp/src/analysis/edit.rs` (`stretch`, `inharmonic`, `freq_shift`, `formant_scale`, `smear`, `odd_even`, `spectral_filter`, `freeze`, `decay_by_number`, `noise_amount`), `crates/dsp/src/analysis/lab.rs::Lab::set`, `crates/dsp/src/ffi.rs` (`spectral_set`), `web/src/audio/spectral.ts` (`Edit`, `NO_EDITS`), `web/src/components/lab/SpectralLab.vue`

#### Scenario: a saw becomes a bell

- GIVEN a labelled saw at 220 Hz of 16 harmonics
- WHEN it is stretched inharmonically by 1.5
- THEN harmonic 1 stays at 220 Hz, harmonic 8 moves by 1.5^0.75 ≈ 1.355, harmonic 16 by 1.5, and the higher a harmonic the further it moves

#### Scenario: formants stay

- GIVEN harmonics of 110 Hz under a resonance at 1 kHz
- WHEN they are formant-scaled by 1/1.5 and then shifted a fifth up
- THEN the loudest partial is still within a third of an octave of 1 kHz

#### Scenario: the lab

- GIVEN the lab with a saw analysed
- WHEN odd/even is set to 0
- THEN the resynthesis changes, and the table sent to the PPG has no second harmonic. The view's ids and defaults mirror the engine's.

**Tests:** `crates/dsp/src/analysis/edit.rs::tests::stretch_moves_harmonics_apart`, `crates/dsp/src/analysis/edit.rs::tests::inharmonic_moves_high_partials_most`, `crates/dsp/src/analysis/edit.rs::tests::freq_shift_adds_hertz`, `crates/dsp/src/analysis/edit.rs::tests::formants_stay_when_pitch_moves`, `crates/dsp/src/analysis/edit.rs::tests::smear_keeps_energy`, `crates/dsp/src/analysis/edit.rs::tests::odd_even_hollows_the_sound`, `crates/dsp/src/analysis/edit.rs::tests::spectral_filter_shapes_and_resonates`, `crates/dsp/src/analysis/edit.rs::tests::freeze_holds_a_frame`, `crates/dsp/src/analysis/edit.rs::tests::decay_by_number_kills_the_highs_first`, `crates/dsp/src/analysis/edit.rs::tests::nothing_passes_nyquist`, `crates/dsp/src/analysis/edit.rs::tests::noise_amount_scales_the_share`, `crates/dsp/src/analysis/lab.rs::tests::transforms_reach_the_render_and_the_table`, `crates/dsp/src/analysis/lab.rs::tests::the_view_mirrors_the_transforms`

### Requirement 5: Labels and distillation [MUST]

To morph, a partial set SHALL be labelled and distilled, after Loris:
- **Labelling.** A partial SHALL take the label `round(mean over its frames of fₖ / f0(t))`, from the frame's f0 (spec 009 Req 3), unweighted. Frames without an f0 SHALL not count, and a partial without any SHALL keep label 0.
- **Distillation.** For each label, the longest partial SHALL be kept.
  - The parts of shorter partials with the same label SHALL be spliced in where the kept partial is silent, with a fade either side (5 ms).
  - The energy of whatever overlaps SHALL be added to the kept partial as noise: `a' = sqrt(a² + o²)`, `β' = (a²·β + o²) / (a² + o²)`.
  - A distilled set has at most one partial per label at any time.

**Implementation:** (planned) `crates/dsp/src/analysis/morph.rs::label`, `crates/dsp/src/analysis/morph.rs::distill`

#### Scenario: a broken harmonic

- GIVEN a sawtooth whose 5th harmonic is tracked as two partials with a gap of two frames between them
- WHEN the set is labelled and distilled
- THEN there is one partial with label 5, spanning both

**Tests:** (planned) `crates/dsp/src/analysis/morph.rs::tests::labels_follow_f0`, `crates/dsp/src/analysis/morph.rs::tests::distilling_joins_a_broken_harmonic`, `crates/dsp/src/analysis/morph.rs::tests::overlap_becomes_noise`, `crates/dsp/src/analysis/morph.rs::tests::no_f0_no_label`

### Requirement 6: Time alignment [SHOULD]

Before a morph, two partial sets SHOULD be aligned on feature times: the attack onset, the end of the attack (the energy peak) and the release (the last fall below −30 dB of the peak), found from the set's energy envelope or given by hand. Each set's frame times SHOULD be mapped piecewise-linearly so its features fall on the shared times, the mean of the two sets' unless given. Before the first feature they SHOULD be scaled, and after the last shifted.

**Implementation:** (planned) `crates/dsp/src/analysis/morph.rs::features`, `crates/dsp/src/analysis/morph.rs::dilate`

#### Scenario: a short and a long attack

- GIVEN a sound with a 10 ms attack and one with a 200 ms attack
- WHEN both are aligned
- THEN both reach their peaks at 105 ms

**Tests:** (planned) `crates/dsp/src/analysis/morph.rs::tests::features_find_attack_and_release`, `crates/dsp/src/analysis/morph.rs::tests::dilation_aligns_the_peaks`

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

**Implementation:** (planned) `crates/dsp/src/analysis/morph.rs::morph`, `crates/dsp/src/analysis/morph.rs::Positions`

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

**Tests:** (planned) `crates/dsp/src/analysis/morph.rs::tests::ends_are_the_sources`, `crates/dsp/src/analysis/morph.rs::tests::pitch_of_one_timbre_of_the_other`, `crates/dsp/src/analysis/morph.rs::tests::orphans_glide_to_their_place`, `crates/dsp/src/analysis/morph.rs::tests::amplitude_morph_is_logarithmic`, `crates/dsp/src/analysis/morph.rs::tests::formant_morph_moves_the_formant`, `crates/dsp/src/analysis/morph.rs::tests::unlabelled_partials_crossfade`

### Requirement 8: Partial sets from other sources [SHOULD]

A partial set SHOULD come from more than a WAV:

- **A patch's spectrum.**
  - The harmonics of an oscillator wave (saw 1/k, square odd 1/k, pulse sin(kπd)/k, triangle odd 1/k²) at a pitch, times the response of a filter at each harmonic, as one frame, or as frames over an envelope's time.
  - The response SHALL be measured once per model, filter voicing, cutoff and resonance step, by running the filter itself on sines, because the ladder is non-linear. It SHALL be read by interpolation, so the patch's cutoff and resonance stay parameters of the set.
- **Macros (after Plaits, MIT).** Three values, harmonics, timbre and morph, SHALL give 24 harmonic amplitudes by Plaits' closed form, normalised by their sum.
- **A wavetable.** Each wave of a table SHALL be one frame of up to 31 harmonics, by an FFT of the wave.

**Implementation:** (planned) `crates/dsp/src/analysis/sources.rs::patch_spectrum`, `crates/dsp/src/analysis/sources.rs::FilterResponse`, `crates/dsp/src/analysis/sources.rs::macros`, `crates/dsp/src/analysis/sources.rs::from_table`

#### Scenario: a filtered saw

- GIVEN a sawtooth at 110 Hz through the Minimoog ladder at 1 kHz with no resonance
- WHEN its patch spectrum is made, and the same patch is rendered by the engine and analysed
- THEN the two agree within 1.5 dB on the first 20 harmonics

**Tests:** (planned) `crates/dsp/src/analysis/sources.rs::tests::patch_spectrum_matches_the_engine`, `crates/dsp/src/analysis/sources.rs::tests::macros_are_normalised`, `crates/dsp/src/analysis/sources.rs::tests::a_table_wave_is_its_harmonics`

### Requirement 9: User wavetables (the PPG path) [MUST]

The engine SHALL hold user wavetables beside its generated ones (spec 006 Req 10):
- **Storage.** 8 slots, each 64 waves of 256 samples, loaded through a buffer outside `render` (as a sample, ADR-0013). A slot is allocated when it is loaded, never in `render`, and an empty slot reads as silence.
- **Rendering a partial set.** `WAVES` frames SHALL be chosen evenly from the analysis's first voiced frame to its last. Each frame's partials, with the lab's transforms but not its pitch shift, SHALL go to the harmonic nearest their frequency over the frame's f0, the first 31 of them; an inharmonic partial is rounded, since a table holds whole harmonics only. They SHALL be written as one wave in sine phase, so neighbouring waves never cancel when crossfaded, and normalised to a peak of 1, since the synth's envelope gives the loudness. An unvoiced frame SHALL take the nearest voiced frame's harmonics. Without a voiced frame there is no table.
- **Selection.** `Wt1Table` and `Wt2Table` SHALL reach the user tables as 8 to 15, after the generated ones. `Wt1Pos`/`Wt2Pos` SHALL move through their waves as through any table. So the PPG model plays an analysed sound through its filters and envelopes, and a morph between two waves is a position.
- **Sending.** The lab SHALL send a table to a user slot in the main window, which the PPG panel offers as "User 1" to "User 8".

**Implementation:** `crates/dsp/src/table.rs::UserTables`, `crates/dsp/src/table.rs::TableSet`, `crates/dsp/src/analysis/frames.rs::to_table`, `crates/dsp/src/engine.rs::Engine::load_user_table`, `crates/dsp/src/ffi.rs` (`user_buf`, `table_load`, `spectral_table`, `spectral_values_ptr`), `crates/dsp/src/params.rs` (`Wt1Table`, `Wt2Table` ranges), `web/public/worklet.js` (`userTable`), `web/src/audio/engine.ts` (`loadUserTable`), `web/src/audio/spectral.ts` (`sendTable`, `serveLab`), `web/src/audio/models.ts` (`WAVETABLES`)

#### Scenario: an analysed saw on the PPG

- GIVEN the engine's sawtooth, analysed and rendered into a table
- WHEN its middle wave is measured
- THEN its harmonics fall as 1/k within 1.5 dB up to the 16th, and its peak is 1

#### Scenario: a morph as a position

- GIVEN a user table whose first wave is a saw and whose last is a square
- WHEN the PPG plays it at position 0 and at position 1
- THEN the second harmonic is about 6 dB under the first at 0 and more than 30 dB under it at 1. An empty user slot is silence.

**Tests:** `crates/dsp/src/analysis/frames.rs::tests::a_saw_becomes_a_saw_table`, `crates/dsp/src/analysis/frames.rs::tests::noise_makes_no_table`, `crates/dsp/src/engine/tests.rs::the_ppg_plays_a_user_table_and_its_position_morphs`, `crates/dsp/src/ffi.rs::tests::user_tables_and_attacks_through_the_abi`, `web/src/audio/spectral.test.ts`

### Requirement 10: The attack on the D-50 [SHOULD]

A D-50 partial cannot play a wavetable: it is either a PCM attack or a synthesised oscillator, as the D-50's was. So an analysed sound SHOULD give the D-50 its attack, and the body SHALL stay the D-50's own synth partial, which is the instrument's own recipe.

- **The attack.** The original SHOULD be cut up to its loudest 20 ms, at least 30 ms long, then faded out over 10 ms. It goes into one of 8 user attack slots, at the original's root.
- **Selection.** `Pcm1Sample`/`Pcm2Sample` SHALL reach the slots as 9 to 16, after the generated attacks, and the D-50 panel offers them as "User 1" to "User 8".
- **Sending.** The lab SHALL send the attack to a user slot in the main window.

The body as partial frames belongs to the PPG (Requirement 9). The two can be layered on two tracks.

**Implementation:** `crates/dsp/src/analysis/frames.rs::split_attack`, `crates/dsp/src/table.rs::UserTables`, `crates/dsp/src/la.rs`, `crates/dsp/src/engine.rs::Engine::load_user_attack`, `crates/dsp/src/ffi.rs` (`attack_load`, `spectral_attack`, `spectral_root`), `crates/dsp/src/params.rs` (`Pcm1Sample`, `Pcm2Sample` ranges), `web/src/audio/engine.ts` (`loadUserAttack`), `web/src/audio/spectral.ts` (`sendAttack`), `web/src/audio/models.ts` (`PCM`)

#### Scenario: an attack on partial 1

- GIVEN a 50 ms 1 kHz burst in user attack 1
- WHEN the D-50 plays it on partial 1 (PCM 9)
- THEN the first 40 ms hold 1 kHz at more than ten times the level an empty slot gives

**Tests:** `crates/dsp/src/analysis/frames.rs::tests::split_finds_the_attack`, `crates/dsp/src/engine/tests.rs::the_d50_plays_a_user_attack`, `crates/dsp/src/ffi.rs::tests::user_tables_and_attacks_through_the_abi`

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

**Implementation:** (planned) `crates/dsp/src/modular/sc.rs` (lexer: `` ` ``; `ugen()`: `Klang`, `DynKlang`, `Klank`, `DynKlank`, `VOsc`, `Osc`), `crates/dsp/src/modular.rs` (`MAX_RESONATORS`, table reads)

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

**Tests:** (planned) `crates/dsp/src/modular/sc.rs::tests::backtick_keeps_an_array_whole`, `crates/dsp/src/modular/sc.rs::tests::klang_is_a_bank_of_sines`, `crates/dsp/src/modular/sc.rs::tests::dynklank_rings`, `crates/dsp/src/modular/sc.rs::tests::vosc_crossfades_tables`, `crates/dsp/src/modular/sc.rs::tests::too_many_partials_are_refused`

### Requirement 12: Sequencing timbre [SHOULD]

The table positions, the morph positions of a live morph, freeze as a 0/1 gate, and the transform amounts that run live SHOULD be registry parameters, sequenced by the language as any other: per-step locks (`.wt1pos("~ ~ 0.8 ~")`), signals (`.wt1pos(sine.slow(8))`) and automation lanes.

A signal SHOULD also be sampleable at each note-on and held for that note (`.wt1pos(rand.onNote)`, the name to be settled with #514), as Tidal samples a continuous pattern per event. That gives each note its own timbre from a random or generative signal.

**Implementation:** (planned) `crates/dsp/src/song` (parameter methods, a per-note-on hold), `.openspec/language.md` (sound-design part, #514)

#### Scenario: a timbre per note

- GIVEN a clip of eight notes on the PPG with `.wt1pos(rand.onNote)`
- WHEN it plays
- THEN each note holds one position for its length, and the positions differ between notes

**Tests:** (planned) `crates/dsp/src/engine/tests.rs::a_signal_held_per_note`, `crates/dsp/src/engine/tests.rs::a_locked_table_position`

### Requirement 13: Morphing in the lab [SHOULD]

The Spectral Lab (spec 009 Req 7) SHOULD:
- hold two sounds, A and B, each from a WAV, a patch spectrum or macros
- morph them with the four positions as sliders, and the transforms of Requirement 4 as controls
- offer three ways to send the result:
  - as a sample (as now)
  - as a multisample, one rendering per root note over a range, into a Sampler's zones
  - as a user table, or as an attack and a body (Requirements 9 and 10)

**Implementation:** (planned) `crates/dsp/src/analysis/lab.rs`, `crates/dsp/src/ffi.rs` (`spectral_*`), `web/src/components/lab/SpectralLab.vue`, `web/src/audio/spectral.ts`

#### Scenario: a morph sent as a table

- GIVEN A a sawtooth and B an analysed bell in the lab
- WHEN the amplitude position is swept into a 16-wave table and sent
- THEN the main window's PPG synth can play it with `Wt1Pos` moving from the saw to the bell

**Tests:** (planned) `crates/dsp/src/analysis/lab.rs::tests::two_sources_morph`, `crates/dsp/src/analysis/lab.rs::tests::a_morph_renders_a_table`, `web/src/audio/spectral.test.ts`

### Requirement 14: Fitting a DX7 voice (stage 6, refined) [MAY]

A DX7 fit MAY choose its algorithm and operator ratios from the partial set (simple ratios between partial clusters and f0, or a small discrete search), then fit levels, envelope rates and feedback by CMA-ES. The loss would be a multi-resolution STFT (L1 on magnitude and log magnitude) plus an envelope term, with the step API, worker and `.syx` export of #192 stage 6. Spectral losses give no gradient for ratios (DDX7, ISMIR 2022), which is why the ratios are chosen first.

**Implementation:** (planned) `crates/dsp/src/analysis/fm_fit.rs`

**Tests:** (planned) `crates/dsp/src/analysis/fm_fit.rs::tests::an_engine_dx7_preset_is_recovered`
