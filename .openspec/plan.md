# Plan

algo-synth is built **from working to working**: every MVP is something you can open in a browser and play. The route is **one monophonic voice → time → a second base (the ensemble) → more sources → the algorithms**. The algorithms come last on purpose: a generator is only as good as the voice it drives and the clock it runs on.

**Rerouted 2026-10-03 (ADR-0008):** after MVP 2 the road goes straight to **M3, the ensemble**. Wave, Drums, the arrangement and the algo panes are removed for now; M2, M4 and M5 are deferred until the ensemble plays, and the order below is the original one.

**Five milestones, eleven MVPs.** Epics become GitHub issues with the `epic` label; their stories become sub-issues that cite spec requirements (`Refs: 001/Req-3`). Every MVP names a **value signal**: how you know it delivers.

| Milestone | MVPs | Value signal |
|---|---|---|
| **M1: A voice** | **MVP 1** the pipeline · **MVP 2** the Mono voice | You play it for ten minutes without wanting a different synth |
| **M2: Time** *(deferred)* | **MVP 3** the step sequencer · **MVP 4** the arrangement | A four-track loop you'd keep |
| **M3: The ensemble** (next) | **MVP 5** six monosynths play Vivaldi | RV 269 plays start to finish, six differently voiced synths, no glitches |
| **M4: More sources** *(deferred)* | **MVP 6** Drums · **MVP 7** Wave · **MVP 8** effects | A full track: kit, bass, lead, pad, space |
| **M5: Algo** *(deferred)* | **MVP 9** live loops · **MVP 10** evolving loops · **MVP 11** performance | Ten minutes of music you didn't write note by note, and want to hear again |

## Principles for every MVP

- **All music logic lands in `crates/dsp`** (ADR-0001). A feature that needs JavaScript beyond a message or a drawing is a design smell.
- **Offline-render tests.** Every source and effect is tested natively by rendering blocks and checking properties: finite, bounded, silent when it should be, the right pitch (zero crossings or a Goertzel bin), no DC. The browser is for listening, not for proving.
- **Performance budget.** Target: 16 voices plus the full mixer under 25% of one core at 48 kHz, measured as render capacity in Chrome DevTools' WebAudio panel (More tools → WebAudio). Checked at the end of each milestone, not guessed.

---

## M1: A voice

### MVP 1: the pipeline *(this base)*

A sine from Rust through the AudioWorklet to the speakers, in the wide-screen layout.

- `crates/dsp`: C ABI, one engine per wasm instance, planar stereo blocks, a 16-voice test voice (table sine, AR envelope), parameter and source registries mirrored in TypeScript.
- `web/`: Vue view with the four panes; power, master gain, scope, playable keyboards and pads; computer keyboard.
- `make check`, CI, Podman + Caddy image.

**MVP 1b: play a file** *(v0.2, ahead of order)*. A MIDI file played straight from the engine: the SMF parser of MVP 5 brought forward, a sample-timed player per channel, each channel routed to a source, and a first timbre per source. It's a stand-in: MVP 3 (the clock) and MVP 5 (import as tracks and clips) replace the player; MVP 2, 6 and 7 replace the timbres (spec 002 Req 9).

### MVP 2: the Mono voice

The ARP 2600-style semi-modular voice, monophonic, playable from the on-screen and computer keyboard. Web MIDI input moved to MVP 11.

1. **Oscillators:** three VCOs with band-limited saw and pulse (PWM, a BLEP table: ADR-0007), triangle and sine; sync; noise (white, pink).
2. **Filter:** a 4-pole zero-delay-feedback ladder (the Moog sound) with resonance and drive, cutoff smoothed at control rate.
3. **Modulation:** ADSR and AR envelopes, LFO, sample-and-hold.
4. **Note handling:** one monophonic voice per owner (live input, each MIDI channel) instead of the shared pool; note priority (last, low, high), legato, glide (portamento).
5. **Normalled routing:** every module has a default connection, as on the 2600; a *patch* is a small table of overrides (source → destination, amount). The UI shows the normalled path and the patch.
6. **Presets:** a handful of patches (bass, lead, sync lead, bowed string for the ensemble).

## M2: Time

### MVP 3: the step sequencer

- **A sample-accurate clock in the engine** (ADR-0005): tempo, swing, a transport (play, stop, position). Events fire on the exact sample, not on a JavaScript timer.
- **Song as data:** the UI sends patterns to the engine in a compact binary format, written into a buffer the engine allocated at init; `render` only reads. Spec 002 fixes the format.
- One 16-step pattern on one track: note, velocity, length, probability, tie.

### MVP 4: the arrangement

- Tracks (each owning one source instance), clips on the bars grid, a loop region, fader and pan per track.
- Patterns of any length (polymeter), clips looping inside their span.
- **Save and load** a song: a JSON file you download and open, plus the last session in `localStorage`. No server.

## M3: The ensemble (second base)

### MVP 5: six monosynths play Vivaldi

The second way into the same model: a score instead of a generator. With the arrangement deferred (ADR-0008), the MIDI player plays the score directly.

1. **Standard MIDI File parser** in Rust: total (never panics on bad input), tested with malformed files, types 0 and 1, tempo map. *(Done in v0.2.)*
2. **Mono only** (#18): Wave, Drums, the algo and arrangement panes removed.
3. **Up to 16 Mono instances** (#19, done), each with its own patch, preallocated, and each one of six **models** (epic #28, ADR-0009, spec 005): ARP 2600, Minimoog, Pro-One, MS-20, Yamaha CS-15, Roland SH-101, with their own controls, filters, presets and panel colours, so the ensemble is six different synths, not six 2600s; a MIDI channel plays on one instance, a loaded file gets one per channel. Then a little detune and timing humanization per instance, the way six real machines drift (#21).
4. **The score** (#20): a public-domain Vivaldi (RV 269, *La primavera*, 1st movement) from an openly licensed MIDI source, with its licence recorded next to it.
5. **Polyphony** (epic #78, ADR-0011, spec 006): a voice pool per synth and seven polyphonic instruments (Prophet-5, Juno-106, Jupiter-8, Matrix-12, PPG Wave, Roland D-50, Yamaha DX7), so a MIDI file's chords play through the ensemble.
6. *(Later, with MVP 4)* Import as tracks and `score` clips that can be edited, muted, or handed to a generator (MVP 10: Markov learned from Vivaldi).

## M4: More sources

### MVP 6: Drums

Analog-style pad models: bridged-T kick with pitch envelope, snare (tone plus filtered noise), clap (multi-burst noise), closed and open hats (six square oscillators through a band-pass, choke group), toms, cowbell. Per-pad tune, decay, tone, and a global accent.

### MVP 7: Wave

The PPG-style wavetable voice: 64-wave tables built from keyframes, the wave position swept by an envelope and LFO, band-limited by mip-mapped tables, 8 voices, and the optional 8-bit path (reduced bit depth and a lower internal rate) for the character.

### MVP 8: effects

Inserts (drive, filter, crush, chorus), two send buses (tempo-synced delay, algorithmic reverb), and a master compressor and limiter. All preallocated; the order of inserts is editable, their count fixed.

## M5: Algo

### MVP 9: live loops

- **Seeded PRNG in the engine** (same seed, same music), scales and keys.
- **Generators:** Euclid (k, n, rotation), random walk over a scale, arpeggiator over held or given chords.
- **Live and frozen:** a live loop writes a new pattern every cycle; freezing commits the current one as a clip.
- The algo pane edits loops: generator, parameters, scale, target track, seed.

### MVP 10: evolving loops

- **Markov** chains learned from a clip or an imported score.
- **Mutate:** a probability that each cycle changes a step (note, velocity, rest).
- **Modulators:** LFOs and random sources targeting any parameter; per-step parameter locks.
- **Loops that feed loops:** one generator's output as another's input (a Euclid rhythm gating a walk's notes).

### MVP 11: performance

Scenes and quantized launching, Web MIDI in from a hardware keyboard (note on/off, velocity, pitch bend, mod wheel; moved here from MVP 2), MIDI out to hardware, SIMD (`simd128`) and table optimizations where profiling says so.

## Assumptions to confirm

- **Chrome first.** AudioWorklet and Web MIDI are best supported there; Safari and Firefox are checked, not targeted, until MVP 11.
- **Vue for the view.** The UI is TypeScript + Vue (ADR-0003). If the UI should itself be Rust/wasm (Leptos), that's a new ADR; the engine boundary doesn't change.
- **Score licensing.** Vivaldi's music is public domain; a given MIDI *file* may not be. MVP 5 picks a source with an explicit open licence.
