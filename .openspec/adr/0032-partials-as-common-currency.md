# 0032: Partials as the common currency: one partial set from every source, transforms and morphs on it, played live through the table oscillator and the Modular

**Status:** Proposed · **Date:** 2026-10-10 · **Extends:** 0017, 0024 · **Amends:** #192 stages 4 and 5

## Context

Epic #192 took a WAV apart into partial tracks and built it back (ADR-0017, spec 009): an STFT, McAulay–Quatieri tracking, a harmonic mode, and a phase-locked additive resynthesis that follows the original to −75 dB on a steady tone. Its remaining stages were a noise residual (SMS), a Synclavier-style additive model, and a DX7 patch fitted by CMA-ES.

Resynthesis on its own only copies a sound. The question became what partials give that the engine's samplers, analog-modelled sources (BLEP oscillators, ladder and SVF filters), FM and the SuperCollider-subset Modular voice do not, and how the song language's patterns (ADR-0019) can play with it. A research pass (seven agents; sources below) gave these findings:

- **Commercial practice.** Hybrid synths transform partials with one knob per operation and morph per dimension.
  - Vital's spectral morph has twelve modes, read in its source: formant scale, harmonic scale, inharmonic scale, smear, low and high pass, phase disperse, Shepard tone, skew, random amplitudes and vocode. Each is one closed formula over the harmonics, driven by one amount.
  - Harmor and Razor run classic processing on the partials themselves:
    - filters, a phaser and unison on partial levels
    - blur and prism
    - a frequency shifter and a "stiff string" stretch
  - Alchemy morphs pitch, additive levels, the envelope and formants as separate handles, and requires both sources to be in the same analysis mode.
  - The Synclavier crossfades timbre frames analysed from a sample (up to about 50).
  - Plaits drives 24 harmonics from three macros by a closed formula, with one-pole smoothing ("absolutely essential") and normalisation.
- **Literature.** Loris (Fitz, Haken et al., CMJ 27(3), 2003; Tellman, Haken & Holloway, JAES 1995) is the proven morph, and its algorithm is documented to the formula:
  - **Labelling.** Partials are labelled by harmonic number against an f0 envelope.
  - **One partial per label.** A label keeps one partial, and the energy of the others is folded in as noise ("bandwidth").
  - **Time alignment.** The two sounds are aligned on feature times (dilation).
  - **Interpolation.**
    - Frequency, amplitude and bandwidth each follow their own morph envelope.
    - Amplitude is interpolated in the log domain with a shape constant.
    - A partial without a counterpart glides to where its harmonic would sit in the other sound.
  - **Perceptual caveat.** Caetano & Rodet (IEEE TASL 2013) showed that interpolating raw partial parameters is not perceptually linear. Interpolating the spectral envelope and the temporal envelope separately gives even morphs.
  - **Formant-preserving shifts.** These need a spectral envelope, and the "true envelope" (Röbel & Rodet, DAFx 2005) is the robust estimate.
- **The engine already has half of it.**
  - `analysis::harmonic` gives harmonic amplitudes per frame.
  - The table oscillator (`table.rs`, spec 006 Req 10) plays 64 waves of 31 harmonics with a position that crossfades between them. It runs inside the Mono voice, through its ladder or SVF.
  - Per-step locks exist (`.wt1pos("~ ~ 0.8 ~")`, #255).
  - The Modular voice, though, has no partial bank (no `Klang`/`DynKlang`), no data a voice can read, and at most 32 sine phases per voice. An FFT `PV_` chain does not fit its per-sample design.
- **Limits.** The analysis code is ours, but the reference implementations are not reusable as code: Loris is GPL-2.0+ and Vital is GPL-3.0, while this project is Apache-2.0. Plaits is MIT.

## Decision

**Every source the engine can describe becomes the same thing, a *partial set*. Transforms and morphs act on partial sets. A partial set is played offline as a sample, or live in two ways: as frames of a user wavetable through the Mono voice's filters, or as a partial bank in the Modular. Patterns sequence the result with the locks and signals the language already has.**

- **The partial set.**
  - **Shape.** A partial set is frames at a hop and rate, each partial a track of frequency, amplitude, phase and *noise* (bandwidth, 0 pure sine to 1 noise) per frame. Partials carry a harmonic label when an f0 is known, and 0 otherwise.
  - **Code.** It extends `analysis::track::Track`; nothing replaces it.
  - **Sources that fill it:**
    - the analysis of a WAV (spec 009)
    - the spectrum of a patch: oscillator harmonics × the filter's response at each harmonic, measured once per filter voicing, since the ladder is non-linear
    - three macros after Plaits
    - the frames of a wavetable
- **Noise travels with the partials (amends #192 stage 4).** Noise is not a separate SMS residual. Each partial takes the energy of the noise around it as bandwidth, after Fitz, Haken & Christensen (ICMC 2000). Resynthesis turns bandwidth into a sinusoid modulated by narrow-band noise. A morph then interpolates noise like any other dimension, where a separate residual could only be crossfaded.
- **Spectral envelope.** A partial set can carry a smooth spectral envelope per frame (true envelope, by iterated cepstral smoothing). It lets a pitch shift keep formants and makes formant a morph dimension of its own.
- **Transforms.**
  - Each is one function on a partial set with one amount, in `analysis::edit` beside `shift` and `top_n`:
    - harmonic stretch and inharmonic power-law stretch
    - formant scale
    - frequency shift (add hertz)
    - smear across partials
    - odd/even and group gain
    - spectral low and high pass with a resonant edge
    - freeze a frame
    - an envelope rewrite (decay by partial number)
  - The formulas follow the research; no GPL code is copied.
- **Morph (after Loris, reimplemented from the papers).**
  - Two partial sets are labelled, distilled to one partial per label, and aligned on feature times (attack, end of attack, release).
  - They are then interpolated with independent morph positions for **pitch, amplitude, noise and formant**. Amplitude and noise are interpolated in the log domain with a shape of 1e-5, and frequency is linear by default.
  - A partial without a counterpart fades in place, or glides to its label's place in the other set when that set has an f0.
  - Unlabelled partials crossfade.
- **Playing a partial set:**
  - **Offline.** Render to a sample, as the lab does now (ADR-0017). This also covers "send as multisample", one rendering per root.
  - **Live, as wavetable frames.**
    - A partial set is reduced to up to 64 frames of up to 31 harmonics and loaded into a user wavetable. The engine owns it, and it is written outside `render` like a sample (ADR-0013).
    - The PPG model's table oscillator plays it: `Wt1Table` names the table, and `Wt1Pos` moves through the frames, so a morph is a position.
    - It goes through the Mono voice's filters and envelopes.
    - This is what the Synclavier model was for, without a new model, and it is where partials meet the analog filters live.
  - **Live, as an LA voice (the D-50 blend).**
    - Sinusoidal models smear attacks, and the D-50 answered that problem in 1987 by splicing a short PCM attack onto a synthesised body.
    - A partial set's sound can be split at the end of its attack. The attack goes in as a sample for the LA partial's PCM slot (`Pcm1Sample`), and the body goes in as wavetable frames.
    - So a sound can be played as its own transient over a morphable, filterable body.
  - **Live, in the Modular: the SuperCollider subset grows, the language does not (extends ADR-0024).** Every addition is real sclang, so SynthDefs stay readable to anyone who knows SuperCollider:
    - **The `` `[...] `` literal** (a `Ref`), which `Klang` and `Klank` take as their specs.
    - **`Klang`, `DynKlang`:** a bank of sines with frequencies, amplitudes and phases; `freqscale` and `freqoffset` give pitch and a frequency shift. They read the sine table, never a transcendental per sample (ADR-0021), and are capped by `MAX_PHASES`.
    - **`Klank`, `DynKlank`:** a bank of resonators with frequencies, amplitudes and ring times, excited by any input (noise, an impulse, a sample's attack). Partials heard as resonances.
    - **`VOsc` and `Osc`:** wavetable oscillators. `VOsc` crossfades between consecutive tables by a fractional table number, the PPG's position in SuperCollider's words. Where SuperCollider names a buffer by number, the subset names a user wavetable, so `VOsc.ar(\pos.kr(0.3), freq)` plays an analysed sound's frames and morphs through them.
    - Every bank and table read is fixed in size per voice and allocated when the program is built.
    - No FFT `PV_` chain.
    - Plaits-style macros, which SuperCollider has no core unit for, stay a lab source that writes frames, not a new word.
- **Real-time rules (ADR-0002, ADR-0021).**
  - A live partial level changes through a one-pole smoother, and a partial set's sum is normalised so a sweep changes colour, not loudness.
  - A partial at or above Nyquist fades out over the top octave instead of being cut.
- **Sequencing.**
  - The table position, the morph positions, freeze (a 0/1 gate) and the transform amounts are registry parameters (ADR-0004). The existing per-step locks and signals sequence them: `.wt1pos("~ ~ 0.8 ~")`, `.wt1pos(sine.slow(8))`.
  - One addition to ADR-0019: a signal may be **sampled at each note-on** and held for the note, as Tidal samples a continuous pattern per event. This gives a timbre per note from a random or generative signal.
- **Clean room.** Loris and Vital are references by their papers and documented formulas only; their code is not read into this project. Plaits' MIT code may be adapted with its notice.
- **Stage 6 (DX7 match) is refined, not changed.**
  - Spectral losses give no useful gradient for FM frequency ratios (DDX7, ISMIR 2022). So the algorithm and the ratios are chosen from the partials, or by a small discrete search.
  - CMA-ES then fits the levels, envelope rates and feedback.
  - The loss is a multi-resolution STFT (L1 on magnitude and log magnitude) plus an envelope term.

## Consequences

- Sources the engine already has become morphable into each other: a sample, an analog patch's spectrum, additive macros and wavetables. No commercial synth combines all of these with live analog filters and a pattern language.
- The Synclavier model of #192 stage 5 is dropped as a model and kept as the inspiration. Its timbre frames become the PPG table oscillator's frames, its attack-plus-body becomes the D-50's PCM attack over those frames, and a live additive voice is the Modular's `Klang`/`VOsc`.
- `Track` gains a noise field. Resynthesis gains noise-modulated sinusoids. The SMS wording of #192 stage 4 is replaced.
- `table.rs` gains user tables beside its generated ones, written through a buffer like samples. The tables built at start stay as they are.
- The Modular's SuperCollider subset gains the `` ` `` literal and five UGens (`Klang`, `DynKlang`, `Klank`, `DynKlank`, `VOsc`/`Osc`), its first that read data; its programs stay fixed in size per voice. ADR-0021's stated limits (32 nodes, 8 oscillators) are out of date since ADR-0024 (512 nodes) and are corrected separately.
- Morphing is offline first, in the lab with two sources A and B. Live morphing comes through table positions, so it costs no more than the table oscillator does now.
- Spec 010 holds the requirements. Spec 009 keeps analysis, resynthesis and the lab, and its residual points to spec 010.
- Resonance is poorly identifiable from audio (every matching study drops it). A fitted or measured patch exposes cutoff as estimated and leaves resonance to the user.

## Alternatives considered

- **A Synclavier model (#192 stage 5).** A new model with four timbres per key and frames per timbre. It costs a panel, parameters and a voice. The table oscillator already plays frames through real filters, and a Modular bank covers live additive. Rejected.
- **An SMS residual (#192 stage 4 as written).** It gives faithful noise for one sound. But between two sounds it can only be crossfaded, and it needs its own path (a noise filter bank). Noise per partial morphs with the partial. Rejected for the partial set, kept as a possible lab analysis aid.
- **FFT `PV_` chains in the Modular.** SuperCollider's spectral vocabulary (`PV_MagFreeze`, `PV_BinShift`, `PV_Morph`) needs block buffers and a window of latency in a voice that runs per sample, and `PV_Morph` is not even core SuperCollider. Rejected. The same transforms run on partial sets instead.
- **Interpolating raw parameters without labels or alignment.** Cheapest. It gives beating and muddy middles between unrelated sounds, and Caetano & Rodet show it is not perceptually even. Rejected.
- **Learned morphing (DDSP, autoencoders, diffusion).** Strong results, but it needs trained models and a runtime the engine does not have (ADR-0001: std only, no dependencies). Rejected for now.
- **Porting Loris or Vital code.** Fastest, but GPL code cannot enter an Apache-2.0 project. Rejected. Their algorithms are reimplemented from the papers and formulas.

## Sources

- R. J. McAulay, T. F. Quatieri, "Speech analysis/synthesis based on a sinusoidal representation", IEEE Trans. ASSP, 1986. doi:10.1109/TASSP.1986.1164910
- X. Serra, J. O. Smith, "Spectral Modeling Synthesis", Computer Music Journal 14(4), 1990. doi:10.2307/3680788
- E. Tellman, L. Haken, B. Holloway, "Timbre morphing of sounds with unequal numbers of features", J. Audio Eng. Soc., 1995.
- K. Fitz, L. Haken, B. Christensen, "A new algorithm for bandwidth association in bandwidth-enhanced additive sound modeling", ICMC 2000.
- K. Fitz, L. Haken, S. Lefvert, C. Champion, M. O'Donnell, "Cell-Utes and Flutter-Tongued Cats: Sound Morphing Using Loris and the Reassigned Bandwidth-Enhanced Model", Computer Music Journal 27(3), 2003. doi:10.1162/014892603322482529
- A. Röbel, X. Rodet, "Efficient spectral envelope estimation and its application to pitch shifting and envelope preservation", DAFx 2005.
- M. Caetano, X. Rodet, "Musical instrument sound morphing guided by perceptually motivated features", IEEE Trans. ASLP, 2013. doi:10.1109/TASL.2013.2260154
- J. M. Grey, "Multidimensional perceptual scaling of musical timbres", JASA 61(5), 1977. doi:10.1121/1.381428; D. Wessel, "Timbre space as a musical control structure", Computer Music Journal 3(2), 1979. doi:10.2307/3680283
- J. Laroche, M. Dolson, "Improved phase vocoder time-scale modification of audio", IEEE Trans. Speech Audio Process., 1999. doi:10.1109/89.759041
- F. Caspe, A. McPherson, M. Sandler, "DDX7: Differentiable FM synthesis of musical instrument sounds", ISMIR 2022. arXiv:2208.06169
- A. Horner, J. Beauchamp, L. Haken, "Machine Tongues XVI: Genetic algorithms and their application to FM matching synthesis", Computer Music Journal 17(4), 1993. doi:10.2307/3680541
- P. Bogdan, "INSTRUMENTAL: Automatic synthesizer parameter recovery from audio via evolutionary optimization", 2026. arXiv:2603.15905 (preprint; weak evidence)
- Vital spectral morph (source, `src/synthesis/producers/spectral_morph.h`, GPL-3.0: formulas only); Mutable Instruments Plaits additive engine (source, `plaits/dsp/engine/additive_engine.cc`, MIT); Image-Line Harmor and Native Instruments Razor manuals; Apple Logic Pro Alchemy manual; SuperCollider documentation (`Klang`, `DynKlang`, `PV_*`).
