# 0040: The Casio VL-1 as a chip model, its code as parameters

**Status:** Accepted · **Date:** 2026-10-16 · **Extends:** 0007, 0034

## Context

#723 asks for the Casio VL-1 (VL-Tone, 1981): a monophonic calculator keyboard with ten tones, an "ADSR" mode programmed as an 8-digit number, and a rhythm unit of ten preset rhythms played on three drum sounds. Its sound is what Trio's "Da Da Da" is made of.

Unlike the SID and the NES (ADR-0034), there is no public register-level documentation. What the public record gives:

- the tones: Piano, Fantasy, Violin, Flute, Guitar I; and in ADSR mode Guitar II, English Horn, Electro I–III;
- the code's digits, in order: waveform, attack, decay, sustain level, sustain time, release, vibrato, tremolo;
- the tones are filtered square waves of varied pulse widths;
- the drums, by Casio's own names: Po (30 ms), Pi (20 ms) and Sha (160 ms);
- the rhythms: March, Waltz, 4-Beat, Swing, Rock-1, Rock-2, Bossa Nova, Samba, Rhumba, Beguine.

## Decision

**A `VlTone` model in the Chips family on an engine of its own (`Engine::Vl`), whose parameters are the code's digits. Its parts are in `crates/dsp/src/chip/vl_voice.rs`.**

- **The code as parameters.** `VlWave`, `VlAttack`, `VlDecay`, `VlSustain`, `VlSustainTime`, `VlRelease`, `VlVibrato` and `VlTremolo` each take 0–9, as the digits do, so a VL-1 code reads off a setting. `VlOctave` is the three-position switch. Sustain time 9 holds while the key is down; below it the release starts on its own.
- **Tones as 16-step, 4-bit waveforms** built from pulses of several widths summed (a quarter pulse over an octave square for the Piano, four halving pulses as a staircase saw for the Violin, a square for the Flute, and so on). Their steps are band-limited with the chips' BLEP ring (ADR-0007), then a fixed one-pole low-pass and the output coupling stand in for the RC filter and the speaker.
- **Monophonic presets**, as the instrument is (`Polyphony 1`); a pool of eight lets a setting play chords anyway.
- **`VlKit` makes it a drum machine** as `NesKit` does the NES: a General MIDI key plays Po on the kicks and toms, Pi on the rim shot, claves and cowbell, Sha on the rest. Every key sounds, so the kit passes the preset sweeps. A `drums` track takes `VlTone`.
- **The rhythms are clips, not a sequencer in the voice:** the example song `vl-tone.song` writes the ten as lanes, so they are edited, layered and arranged as any other.

## Consequences

- One model, ten parameters (`Vl*` 636–645, mirrored in `params.ts`), six presets (`VlPiano`, `VlFantasy`, `VlViolin`, `VlFlute`, `VlGuitar`, `VlKit`).
- `render` allocates nothing: the voice is plain `Copy` data; coefficients are worked out once a block.
- Everything not in the public record is a reconstruction and says so: the waveforms, the digits' times (2 ms to 2.5 s), the vibrato's and tremolo's rates and depths, Po's and Pi's pitches, and the rhythms' hits. A VL-1 owner's measurements would replace them without changing the parameters.
- Left out: the real-time sequencer, the calculator, and the "one key play" mode.

## Alternatives considered

- **A Mono-voice preset** (ADR-0009): the Mono voice's oscillators and envelopes are not the code's digits, and a setting could not be read as a VL-1 code.
- **The rhythms inside the voice** (a pattern number and a tempo): authentic, but a pattern would be fixed, out of the song's clock and lanes, and could not be edited.
- **A Modular SynthDef in the example song**: no new model, but the VL-1 would be one song's patch, with no panel, presets or kit.
