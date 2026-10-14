# 009: The Spectral Lab

Analysis of a WAV into partials, their additive resynthesis, and the lab window that plays them side by side with the original. Epic #192, first stages (#500–#503); noise (per partial, in place of a separate residual), transforms, morphs, the ways to play a partial set and the refined DX7 fit are in spec 010 (ADR-0032), which drops the Synclavier model. Decisions: ADR-0001, ADR-0002, ADR-0013, ADR-0017.

Common to every requirement: analysis and resynthesis are offline calls, not `render`. They may allocate, within the caps of Requirement 1, and SHALL never panic: any input gives a result or a negative code. Tests render known sounds offline at 48 kHz and analyse them natively.

### Requirement 1: FFT and STFT [MUST]

The engine SHALL have one in-place radix-2 FFT, std-only (ADR-0001), for power-of-two sizes from 64 to 16384, used by the analysis and by the tests that had their own. A short-time analysis SHALL window a mono signal with a 4-term Blackman–Harris window (default 4096 points, hop 256, both settable), whose sidelobes (−92 dB) stay under the peak floor so they never make tracks of their own, zero-pad it to twice its length with its centre at time zero, and give each frame's amplitude and phase at its centre. A signal SHALL be refused past 60 s at the engine's rate, and an analysis SHALL keep at most 256 peaks per frame.

**Implementation:** `crates/dsp/src/analysis/fft.rs::Fft`, `crates/dsp/src/analysis/stft.rs::Stft`, `crates/dsp/src/analysis.rs::analyse`

#### Scenario: a sine lands in its bin

- GIVEN a 1 kHz sine at 0.5 for one second
- WHEN it is analysed
- THEN each full frame's largest magnitude is in the bin nearest 1 kHz, and the FFT of a forward and inverse transform gives back the input within 1e-5

**Tests:** `crates/dsp/src/analysis/fft.rs::tests::only_powers_of_two_in_range`, `crates/dsp/src/analysis/fft.rs::tests::forward_then_inverse_is_identity`, `crates/dsp/src/analysis/fft.rs::tests::a_sine_peaks_in_its_bin`, `crates/dsp/src/analysis/stft.rs::tests::sizes_are_checked`, `crates/dsp/src/analysis/stft.rs::tests::a_sine_gives_its_amplitude_and_centre_phase`, `crates/dsp/src/analysis.rs::tests::frames_follow_the_hop`, `crates/dsp/src/analysis.rs::tests::bad_input_is_an_error_not_a_panic`, `crates/dsp/src/mono/noise.rs::tests`

### Requirement 2: Peaks and partial tracks [MUST]

Each frame's peaks SHALL be the local maxima of its magnitude above a floor (default −80 dB under the frame's largest), each placed between bins by a parabola through the log magnitudes of it and its neighbours, giving frequency, amplitude and phase. Peaks SHALL be joined across frames into partial tracks after McAulay and Quatieri (1986): a peak continues the track whose last frequency is nearest, within a deviation (default 50 cents), each track takes at most one peak a frame, an unmatched peak starts a track and a track without a peak for more than a few frames (default 3) ends. The result SHALL be, per track, its first frame and its frequency, amplitude and phase per frame, read by the view as arrays from wasm memory (ADR-0017).

**Implementation:** `crates/dsp/src/analysis/peaks.rs::peaks`, `crates/dsp/src/analysis/track.rs::Tracker`

#### Scenario: a sum of sines

- GIVEN 220, 330 and 1210 Hz sines at 0.4, 0.2 and 0.1
- WHEN they are analysed
- THEN there are three long tracks, each within 1 Hz and 0.5 dB of its sine over its frames

#### Scenario: a glide

- GIVEN a sine gliding from 440 to 660 Hz over a second
- WHEN it is analysed
- THEN one track follows it end to end

**Tests:** `crates/dsp/src/analysis/peaks.rs::tests::interpolation_finds_the_frequency_between_bins`, `crates/dsp/src/analysis/peaks.rs::tests::silence_and_the_floor_give_nothing`, `crates/dsp/src/analysis/peaks.rs::tests::the_loudest_are_kept_in_frequency_order`, `crates/dsp/src/analysis/track.rs::tests::nearest_peaks_continue_tracks`, `crates/dsp/src/analysis/track.rs::tests::a_short_gap_is_bridged_and_a_long_one_ends_the_track`, `crates/dsp/src/analysis/track.rs::tests::one_peak_continues_one_track`, `crates/dsp/src/analysis.rs::tests::three_sines_are_three_tracks`, `crates/dsp/src/analysis.rs::tests::a_glide_is_one_track`, `crates/dsp/src/analysis.rs::tests::silence_has_no_tracks`

### Requirement 3: Harmonic mode [SHOULD]

For a pitched sound the analysis SHOULD estimate a fundamental per frame from its peaks and read harmonic k's amplitude at k·f0, so each partial is a harmonic number rather than a free track. A frame without a clear fundamental SHALL be marked unvoiced.

**Implementation:** `crates/dsp/src/analysis/harmonic.rs::f0`, `crates/dsp/src/analysis/harmonic.rs::harmonics`

#### Scenario: an engine preset

- GIVEN the engine's band-limited sawtooth oscillator at A3
- WHEN it is analysed in harmonic mode
- THEN f0 is 220 Hz within 1 Hz and harmonic k's level falls as 1/k within 1 dB for k up to 16

**Tests:** `crates/dsp/src/analysis/harmonic.rs::tests::f0_of_a_rendered_saw`, `crates/dsp/src/analysis/harmonic.rs::tests::saw_harmonics_fall_as_one_over_k`, `crates/dsp/src/analysis/harmonic.rs::tests::noise_is_unvoiced`, `crates/dsp/src/analysis/harmonic.rs::tests::an_octave_below_never_wins`

### Requirement 4: Additive resynthesis [MUST]

The engine SHALL resynthesise the tracks with a bank of oscillators: each track's frequency and amplitude interpolated linearly between frames, its phase carried by integrating the frequency, and a track's start and end faded over one hop so it never clicks. Locked, the phase SHALL also land on each frame's measured phase (the nearest turn of it) by a constant frequency correction across the hop, after McAulay and Quatieri, so the resynthesis follows the original's waveform and can be subtracted from it; after a shift, a stretch or a reduction the phase SHALL run unlocked. The resynthesis SHALL come out as mono PCM at the engine's rate, which the lab loads into its own engine as a sample (ADR-0013). A partial at or above Nyquist SHALL be silent.

**Implementation:** `crates/dsp/src/analysis/additive.rs::resynthesise`

#### Scenario: round trip

- GIVEN the engine's sawtooth oscillator, analysed
- WHEN its tracks are resynthesised and the result analysed again
- THEN its f0 is within 1 Hz and its first 16 harmonics within 1 dB, and every sample is finite and bounded

#### Scenario: locked phase

- GIVEN the engine's sawtooth oscillator at 110 Hz, analysed
- WHEN it is resynthesised with the phase locked
- THEN the difference from the original is more than 40 dB under it, away from the ends

**Tests:** `crates/dsp/src/analysis/additive.rs::tests::round_trip_keeps_the_partials`, `crates/dsp/src/analysis/additive.rs::tests::output_is_finite_and_bounded`, `crates/dsp/src/analysis/additive.rs::tests::locked_resynthesis_follows_the_waveform`, `crates/dsp/src/analysis/additive.rs::tests::tracks_fade_in_and_out`, `crates/dsp/src/analysis/additive.rs::tests::above_nyquist_is_silent_and_nothing_is_nothing`

### Requirement 5: Reduction, pitch shift and time stretch [SHOULD]

A track SHOULD reduce to breakpoints, its first and last frame and every frame where the line between the breakpoints either side misses it by more than a tolerance (0.5 dB and 5 cents in the lab), and SHOULD expand back to a frame each. A top-N cut SHOULD keep the N tracks with the most energy. A pitch shift SHOULD scale every frequency by a ratio; a time stretch SHOULD be a resynthesis with the hop scaled by a factor; neither changes the other.

**Implementation:** `crates/dsp/src/analysis/edit.rs::reduce`, `crates/dsp/src/analysis/edit.rs::expand`, `crates/dsp/src/analysis/edit.rs::top_n`, `crates/dsp/src/analysis/edit.rs::shift`, `crates/dsp/src/analysis/additive.rs::resynthesise`

#### Scenario: shift and stretch

- GIVEN an analysed tone at 220 Hz lasting one second
- WHEN it is shifted by a fifth, or stretched by 2
- THEN the shifted resynthesis is at 330 Hz within 1 Hz and lasts a second, and the stretched one is at 220 Hz and lasts two

**Tests:** `crates/dsp/src/analysis/edit.rs::tests::reduction_keeps_the_shape`, `crates/dsp/src/analysis/edit.rs::tests::top_n_keeps_the_loudest`, `crates/dsp/src/analysis/edit.rs::tests::shift_moves_pitch_not_time`, `crates/dsp/src/analysis/edit.rs::tests::stretch_moves_time_not_pitch`

### Requirement 6: A native harness [SHOULD]

`cargo run --release -p algo-dsp --example spectral -- <in.wav> [--out <out.wav>]` SHOULD analyse a WAV, print its tracks (count, the longest, f0 in harmonic mode) and the time the analysis took, and with `--out` write the locked resynthesis and print how far under the original the difference is.

**Implementation:** `crates/dsp/examples/spectral.rs`

### Requirement 7: The lab window [MUST]

The view SHALL have a Spectral Lab window, `spectral-lab.html`, a third entry beside the app and the Assistant, opened from the transport bar's Lab button. It SHALL have its own AudioContext and engine and a Web Worker holding a second instance of `dsp.wasm` that runs the analysis and resynthesis and only relays (ADR-0017): `spectral_buf` and `spectral_analyse` take a WAV in (the frame count, or a negative WAV code, or an analysis code minus 10), `spectral_tracks_ptr` and `spectral_tracks_len` give the tracks flat (start, n, n frequencies, n amplitudes), and `spectral_render` with `spectral_wav_ptr` gives the resynthesis as a 32-bit float WAV that keeps the original's root note in a `smpl` chunk, so A and B play alike across the keys. Loading a WAV SHALL analyse it and draw its tracks over time and frequency, with the waveform, over a spectrogram (#519) of the side being played: the original or the resynthesis, 256 log-frequency bands a frame from 30 Hz to Nyquist computed by the engine (`spectral_gram_*`), painted in the turbo colour map, each layer switchable. A/B SHALL play the original and the resynthesis on the lab's own synths, from a keyboard and a button, switching between them on one key. The analysis settings, the top-N cut, the pitch shift and the time stretch SHALL apply again on change. Send SHALL hand the resynthesis to the main window over a `BroadcastChannel`, which loads it into its sample store; without a main window the lab SHALL say so. The lab SHALL never touch the main window's engine.

**Implementation:** `crates/dsp/src/analysis/lab.rs::Lab`, `crates/dsp/src/analysis/spectrogram.rs::spectrogram`, `web/src/audio/colormap.ts`, `crates/dsp/src/sample.rs::float_wav`, `crates/dsp/src/ffi.rs` (`spectral_buf`, `spectral_analyse`, `spectral_tracks_len`, `spectral_tracks_ptr`, `spectral_render`, `spectral_wav_ptr`), `web/spectral-lab.html`, `web/src/spectral-lab.ts`, `web/public/spectral-worker.js`, `web/src/components/lab/SpectralLab.vue`, `web/src/audio/spectral.ts`, `web/src/App.vue` (`serveLab`), `web/src/components/TransportBar.vue` (Lab)

#### Scenario: A/B

- GIVEN the lab with a WAV loaded and analysed
- WHEN A/B is switched while a key is held
- THEN the original and the resynthesis sound in turn, and the main window's song plays on unchanged

#### Scenario: send

- GIVEN the lab with a resynthesis and the app open
- WHEN Send is pressed
- THEN the app loads it into a free sample slot and the lab says so; without the app the lab says no main window answered

**Tests:** `crates/dsp/src/analysis/spectrogram.rs::tests::a_sine_lights_its_band`, `crates/dsp/src/analysis/spectrogram.rs::tests::silence_is_the_floor_and_bad_input_is_empty`, `crates/dsp/src/analysis/lab.rs::tests::a_wav_is_analysed_drawn_and_rendered`, `crates/dsp/src/analysis/lab.rs::tests::errors_are_codes_and_keep_nothing`, `crates/dsp/src/ffi.rs::tests::analysis_through_the_abi`, `web/src/audio/spectral.test.ts`
