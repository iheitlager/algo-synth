# Plan

algo-synth is built **from working to working**: every MVP is something you can open in a browser and play. The route is **one monophonic voice → a second base (the ensemble) → time and text → more sources → the algorithms**. The algorithms come last on purpose: a generator is only as good as the voice it drives and the clock it runs on.

**Rerouted 2026-10-03 (ADR-0008):** after MVP 2 the road went straight to **M3, the ensemble**. Wave, Drums, the arrangement and the algo panes were removed, and M2, M4 and M5 deferred until the ensemble played.

**Rerouted again 2026-10-03 (ADR-0012):** the ensemble plays, so the deferred milestones come back, in their original order, around one idea: **the song is text**. A Tidal/Strudel-style notation with classic note names and durations, parsed and printed by the engine; the 16-step drum grid, generators and a language model all edit that text. Drums move forward into MVP 3 (the drum machine is the first thing the clock plays); MVP 6 becomes the Sampler. The MIDI player stays a separate import path.

**Five milestones, eleven MVPs.** Epics become GitHub issues with the `epic` label; their stories become sub-issues that cite spec requirements (`Refs: 001/Req-3`). Every MVP names a **value signal**: how you know it delivers.

| Milestone | MVPs | Value signal |
|---|---|---|
| **M1: A voice** | **MVP 1** the pipeline · **MVP 2** the Mono voice | You play it for ten minutes without wanting a different synth |
| **M3: The ensemble** | **MVP 5** six monosynths play Vivaldi | RV 269 plays start to finish, six differently voiced synths, no glitches |
| **M2: Time and text** (next) | **MVP 3** the drum machine · **MVP 4** loops and the arrangement | A beat and a four-track loop you'd keep, written in a few lines |
| **M4: More sources** | **MVP 6** Sampler · **MVP 7** Wave · **MVP 8** effects | A full track: kit, samples, bass, lead, pad, space |
| **M5: Algo** | **MVP 9** generators · **MVP 10** evolving loops · **MVP 11** the model writes, and performance | Ten minutes of music you didn't write note by note, and want to hear again |

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

## M2: Time and text

The song is text (ADR-0012). A sketch of where M2 ends:

```
tempo 124  key e:dorian
track kit  drums
track bass mono:minimoog
track lead mono:sh101

frag beat = kit /16
  bd  x...x...x...x...
  sn  ....X.......X..x
  ch  x.x.x.x.x.x.x.x.

frag riff = bass "e2 g2 a2 g2 d3 c3 d3 e3" /16
frag tune = lead "e4:4 g4:8 a4:8 b4:2"

arrange
  A 8: beat riff
  B 8: beat riff tune
  A 8
```

### MVP 3: the drum machine

- **A sample-accurate clock in the engine** (ADR-0005): tempo, swing, a transport (play, stop, position). Events fire on the exact sample, not on a JavaScript timer.
- **Drums** come back as a source of synthesized analog pads (was MVP 6): bridged-T kick with pitch envelope, snare (tone plus filtered noise), clap (multi-burst noise), closed and open hats (six square oscillators through a band-pass, choke group), toms, cowbell. Per-pad tune, decay, tone, level, and accent.
- **The notation, first cut:** `tempo`, `track`, and drum fragments as lanes (`x` hit, `X` accent, `.` rest). The engine parses it outside `render` and prints it back canonically; a bad text is rejected with line and column and the old song keeps playing.
- **The 16-step grid:** a row of steps per pad. A click sends `set_step`; the engine edits the song and returns the text, so grid and text never disagree. The text is shown next to the grid and can be edited directly.

### MVP 4: loops and the arrangement

- **Pitched fragments** on Mono and Poly tracks: mini-notation (`[ ]`, `~`, `*n`, `<a b>`, `?`) and classic notes with durations (`c4:4`, `e4:8.`). Fragments of any length (polymeter), looping.
- **Chord symbols** in the notation (`c:m7`, `f:maj7`): a chord on a Poly track, the input of `arp`.
- **A live arpeggiator per synth:** hold a chord, the engine plays it in time with the clock (up, down, up-down, as played, seeded random; octaves, rate, gate, latch). One arp core in `arp.rs` serves the live arp and `arp` in the notation (MVP 9).
- **The arrangement:** sections of a number of bars, each naming the fragments that play; a loop region; mute per track.
- **Save and load** a song: the text file you download and open, plus the last session in `localStorage`. No server. The synth setup (patches, mixer) stays its own JSON file (spec 003 Req 7).

## M3: The ensemble (second base)

### MVP 5: six monosynths play Vivaldi

The second way into the same model: a score instead of a generator. With the arrangement deferred (ADR-0008), the MIDI player plays the score directly.

1. **Standard MIDI File parser** in Rust: total (never panics on bad input), tested with malformed files, types 0 and 1, tempo map. *(Done in v0.2.)*
2. **Mono only** (#18): Wave, Drums, the algo and arrangement panes removed.
3. **Up to 16 Mono instances** (#19, done), each with its own patch, preallocated, and each one of six **models** (epic #28, ADR-0009, spec 005): ARP 2600, Minimoog, Pro-One, MS-20, Yamaha CS-15, Roland SH-101, with their own controls, filters, presets and panel colours, so the ensemble is six different synths, not six 2600s; a MIDI channel plays on one instance, a loaded file gets one per channel. Then a little detune and timing humanization per instance, the way six real machines drift (#21).
4. **The score** (#20): a public-domain Vivaldi (RV 269, *La primavera*, 1st movement) from an openly licensed MIDI source, with its licence recorded next to it.
5. *(Later, MVP 11)* A MIDI file converted into the notation, so a score can be edited, arranged, or handed to a generator (MVP 10: Markov learned from Vivaldi).

## M4: More sources

### MVP 6: Sampler

A source that plays samples: WAV bytes sent to the engine and decoded in Rust at load (not in `render`), one-shot or pitched across the keyboard, start, end, loop points and an envelope. A sample kit sits behind the same pad names as the Drums (`bd`, `sn`, …), so a fragment plays on either. Samples are the user's own files; none ship with the app unless their licence is recorded.

### MVP 7: Wave

The PPG-style wavetable voice: 64-wave tables built from keyframes, the wave position swept by an envelope and LFO, band-limited by mip-mapped tables, 8 voices, and the optional 8-bit path (reduced bit depth and a lower internal rate) for the character.

### MVP 8: effects

Inserts (drive, filter, crush, chorus), two send buses (tempo-synced delay, algorithmic reverb), and a master compressor and limiter. All preallocated; the order of inserts is editable, their count fixed.

## M5: Algo

### MVP 9: generators

- **Seeded PRNG in the engine** (same seed, same music), scales and keys (`key e:dorian`).
- **Generators are functions in the notation** (ADR-0012): `euclid(k, n, rotation)` for a lane or a rhythm, `walk` over a scale, `arp` over given chords. Each takes an explicit seed.
- **Live and frozen:** a live fragment regenerates every cycle; freezing replaces the call with the events it produced, in the same notation.

### MVP 10: evolving loops

- **Markov** chains learned from a fragment (or, through a MIDI import converter, from a score).
- **Mutate:** `| mutate 0.1`, a probability that each cycle changes a step (note, velocity, rest).
- **Modulators:** LFOs and random sources targeting any parameter; per-step parameter locks.
- **Loops that feed loops:** one generator's output as another's input (a Euclid rhythm gating a walk's notes).

### MVP 11: the model writes, and performance

- **A language model writes the song.** It gets the current text and a request ("a busier snare in B", "continue for 16 bars", "a darker lead") and returns a new text. The engine's parser is the check: a parse error goes back to the model for another try, and nothing reaches the engine that did not parse. The model never touches the engine directly.
- **Where the call runs is on hold.** Options: (a) the browser calls the API directly with a key the user pastes in (no server, but the key lives in the browser); (b) a small proxy next to Caddy holds the key (a backend, against ADR-0006; a 63xx port); (c) no call from the app: the text is written in Claude Code or claude.ai and pasted in. (c) works as soon as the notation exists and needs no code; (a) or (b) need an ADR.
- **Performance:** scenes and quantized launching, Web MIDI in from a hardware keyboard (note on/off, velocity, pitch bend, mod wheel; moved here from MVP 2), MIDI out to hardware, SIMD (`simd128`) and table optimizations where profiling says so.
- **MIDI import and export** as converters between a MIDI file and the notation, separate from the player.

## Assumptions to confirm

- **Chrome first.** AudioWorklet and Web MIDI are best supported there; Safari and Firefox are checked, not targeted, until MVP 11.
- **Vue for the view.** The UI is TypeScript + Vue (ADR-0003). If the UI should itself be Rust/wasm (Leptos), that's a new ADR; the engine boundary doesn't change.
- **Score licensing.** Vivaldi's music is public domain; a given MIDI *file* may not be. MVP 5 picks a source with an explicit open licence.
