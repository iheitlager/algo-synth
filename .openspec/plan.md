# Plan

algo-synth is built **from working to working**: every MVP is something you can open in a browser and play. The route is **one monophonic voice → a second base (the ensemble) → time and text → more sources → the algorithms**. The algorithms come last on purpose: a generator is only as good as the voice it drives and the clock it runs on.

**Rerouted 2026-10-03 (ADR-0008):** after MVP 2 the road went straight to **M3, the ensemble**. Wave, Drums, the arrangement and the algo panes were removed, and M2, M4 and M5 deferred until the ensemble played.

**Rerouted again 2026-10-03 (ADR-0012):** the ensemble plays, so the deferred milestones come back, in their original order, around one idea: **the song is text**. A Tidal/Strudel-style notation with classic note names and durations, parsed and printed by the engine; the 16-step drum grid, generators and a language model all edit that text. Drums move forward into MVP 3 (the drum machine is the first thing the clock plays); MVP 6 becomes the samplers. The MIDI player stays a separate path, and a MIDI file also converts into the song (#173).

**Where it stands (v0.32):** M1 to M4 are built, and most of M5. Open: chord symbols (#103, the rest of MVP 4, #102), the Vivaldi score (#20) and per-synth drift (#21) of MVP 5, the PPG's 8-bit path of MVP 7, per-step parameter locks and LFO modulators of MVP 10, and MVP 11 (#108, Web MIDI in #10). The song language as built is in [docs/song.md](../docs/song.md).

**Five milestones, eleven MVPs.** Epics become GitHub issues with the `epic` label; their stories become sub-issues that cite spec requirements (`Refs: 001/Req-3`). Every MVP names a **value signal**: how you know it delivers.

| Milestone | MVPs | Value signal |
|---|---|---|
| **M1: A voice** (built) | **MVP 1** the pipeline · **MVP 2** the Mono voice | You play it for ten minutes without wanting a different synth |
| **M3: The ensemble** (built but #20, #21) | **MVP 5** sixteen synths play a score | RV 269 plays start to finish, six differently voiced synths, no glitches |
| **M2: Time and text** (built but #103) | **MVP 3** the drum machine · **MVP 4** loops and the arrangement | A beat and a four-track loop you'd keep, written in a few lines |
| **M4: More sources** (built) | **MVP 6** samplers · **MVP 7** Wave · **MVP 8** effects | A full track: kit, samples, bass, lead, pad, space |
| **M5: Algo** (MVP 9 built, MVP 10 mostly, MVP 11 next) | **MVP 9** generators · **MVP 10** evolving loops · **MVP 11** the model writes, and performance | Ten minutes of music you didn't write note by note, and want to hear again |

## Principles for every MVP

- **All music logic lands in `crates/dsp`** (ADR-0001). A feature that needs JavaScript beyond a message or a drawing is a design smell.
- **Offline-render tests.** Every source and effect is tested natively by rendering blocks and checking properties: finite, bounded, silent when it should be, the right pitch (zero crossings or a Goertzel bin), no DC. The browser is for listening, not for proving.
- **Performance budget.** Target: 16 voices plus the full mixer under 30% of one core at 48 kHz (25% until #342, when full polyphony sat at its edge), measured as render capacity in Chrome DevTools' WebAudio panel (More tools → WebAudio). Checked at the end of each milestone, not guessed; `make bench` measures it in V8.

---

## M1: A voice

### MVP 1: the pipeline *(built, v0.1)*

A sine from Rust through the AudioWorklet to the speakers, in the wide-screen layout.

- `crates/dsp`: C ABI, one engine per wasm instance, planar stereo blocks, a 16-voice test voice (table sine, AR envelope), parameter and source registries mirrored in TypeScript.
- `web/`: Vue view with the four panes; power, master gain, scope, playable keyboards and pads; computer keyboard.
- `make check`, CI, Podman + Caddy image.

**MVP 1b: play a file** *(built, v0.2, ahead of order)*. A MIDI file played straight from the engine: the SMF parser of MVP 5 brought forward, a sample-timed player per channel, each channel routed to a synth. It stays beside the song as its own transport (spec 002 Req 9).

### MVP 2: the Mono voice *(built, v0.3–v0.4, epic #2)*

The ARP 2600-style semi-modular voice, monophonic, playable from the on-screen and computer keyboard. Web MIDI input moved to MVP 11.

1. **Oscillators:** three VCOs with band-limited saw and pulse (PWM, a BLEP table: ADR-0007), triangle and sine; sync; noise (white, pink).
2. **Filter:** a 4-pole zero-delay-feedback ladder (the Moog sound) with resonance and drive, cutoff smoothed at control rate.
3. **Modulation:** ADSR and AR envelopes, LFO, sample-and-hold.
4. **Note handling:** one voice per owner (live input, each MIDI channel), since ADR-0011 a voice of the synth's pool; note priority (last, low, high), legato, glide (portamento).
5. **Normalled routing:** every module has a default connection, as on the 2600; a *patch* is a small table of overrides (source → destination, amount). The UI shows the normalled path and the patch.
6. **Presets:** a handful of patches (bass, lead, sync lead, bowed string for the ensemble).

## M2: Time and text

The song is text (ADR-0012, ADR-0015, ADR-0016). A song as the engine parses it today:

```
tempo 124
scale e dorian
track kit drums
track bass synth Minimoog
track lead synth Sh101

frag beat = kit /16
  bd x...x...x...x...
  sn ....X.......X..x
  ch euclid(7,16)
frag riff = bass
  "e2 g2 a2 g2 d3 c3 d3 e3"
frag tune = lead
  e4:4 g4:8 a4:8 b4:2
frag roam = lead live
  walk(e4,8,1)

section a 8: beat riff
section b 8: beat riff tune
section c 8: beat riff roam
arrange a b a c
```

### MVP 3: the drum machine *(built, v0.15–v0.31, epic #97)*

- **A sample-accurate clock in the engine** (ADR-0005, #98): tempo, swing, a transport (play, stop, position). Events fire on the exact sample, not on a JavaScript timer.
- **Drums** as a synth model of synthesized analog pads (#99, #114, #140): the TR-808's sixteen voices (kick, snare, toms, congas, rimshot, claves, clap, maracas, cowbell, cymbal, closed and open hats with a choke), per-pad tune, decay, tone, level, an accent and an individual out (#162). The TR-909 plays the same pads with its own sounds (#148).
- **The notation, first cut** (#100): `tempo`, `track`, and drum fragments as lanes (`x` hit, `X` accent, `.` rest). The engine parses it outside `render` and prints it back canonically; a bad text is rejected with line and column and the old song keeps playing.
- **The 16-step grid** (#101): a row of steps per pad. A click sends `set_step`; the engine edits the song and returns the text, so grid and text never disagree. The text is shown next to the grid, in a highlighting editor (#203), and can be edited directly.

### MVP 4: loops and the arrangement *(built but chord symbols, #102)*

- **Pitched fragments** on synth tracks (#163): mini-notation (`[ ]`, `~`, `*n`, `<a b>`, `?`) and classic notes with durations (`c4:4`, `e4:8.`). Fragments of any length (polymeter), looping. *(Built.)*
- **Chord symbols** in the notation (`c:m7`, `f:maj7`): a chord on a Poly track, the input of `arp`. *(Open, #103; chords are written as `[c4,e4,g4]` for now.)*
- **A live arpeggiator per synth** (#110): hold a chord, the engine plays it in time with the clock (up, down, up-down, as played, seeded random; octaves, rate, gate, latch). One arp core in `arp.rs` serves the live arp and `arp` in the notation (MVP 9). *(Built.)*
- **The arrangement** (#170, #171): sections of a number of bars, each naming the fragments that play; a loop region; a track routed to a synth or muted. The arranger shows and edits it as a timeline. *(Built.)*
- **Save and load** a song (#105): the `.song` file you download and open, plus the last song in `localStorage`. No server. The synth setup (patches, mixer) stays its own JSON file (spec 003 Req 7). *(Built.)*

## M3: The ensemble (second base)

### MVP 5: sixteen synths play a score *(built but #20, #21)*

The second way into the same model: a score instead of a generator. The MIDI player plays the score directly, and since #173 a file also converts into the song.

1. **Standard MIDI File parser** in Rust: total (never panics on bad input), tested with malformed files, types 0 and 1, tempo map. *(Built, v0.2.)*
2. **Mono only** (#18): Wave, Drums, the algo and arrangement panes removed for the reroute. *(Done; they came back in M2 and M4.)*
3. **Up to 16 synths** (#19), each with its own patch, preallocated, and each one of seven monosynth **models** (epic #28, ADR-0009, spec 005): ARP 2600, Minimoog, Pro-One, MS-20, Yamaha CS-15, Roland SH-101 and ARP Odyssey, with their own controls, filters, presets and panel colours, so the ensemble is different synths, not sixteen 2600s; a MIDI channel plays on one synth, a loaded file gets one per channel. *(Built.)* Then a little detune and timing humanization per synth, the way real machines drift. *(Open, #21.)*
4. **The score** (#20): a public-domain Vivaldi (RV 269, *La primavera*, 1st movement) from an openly licensed MIDI source, with its licence recorded next to it. *(Open; the demo is Pachelbel's Canon, written by `tools/make_demo_mid.py`.)*
5. **Polyphony** (epic #78, ADR-0011, spec 006): a voice pool per synth and eight polyphonic instruments (Prophet-5, Juno-106, Jupiter-8, Matrix-12, PPG Wave, Roland D-50, Yamaha DX7, Polymoog), so a MIDI file's chords play through the ensemble. *(Built, v0.17.)*
6. **A MIDI file converted into the notation** (#173), so a score can be edited, arranged, or handed to a generator. *(Built, v0.31.)*

## M4: More sources

### MVP 6: samplers *(built, v0.21–v0.30, epic #126)*

Two synth models that play samples (ADR-0013, spec 007): WAV bytes sent to the engine and parsed in Rust at load (not in `render`) into a store of 64 slots capped at 64 MiB (#122). The **multisampler** plays zones of the store across the keyboard, with key and velocity ranges, round robin, loops and release samples, through the synth's filter and envelope (#123). The **pad sampler** is sixteen pads after an MPC on notes 36–51, so a MIDI drum channel or a `sampler` track's pad lanes play it (#124, #164, #220). `make samples` fetches free, licensed packs, kits and a voice pack (#129, #130, #184); none are committed.

### MVP 7: Wave *(built as the PPG Wave model, spec 006)*

The PPG-style wavetable voice: 64-wave tables built from keyframes, the wave position swept by an envelope and LFO, band-limited by mip-mapped tables, 8 voices. The table oscillator also serves the D-50's PCM attacks. The optional 8-bit path (reduced bit depth and a lower internal rate) is not built (epic #209).

### MVP 8: effects *(built, v0.8–v0.29, epic #62, ADR-0010)*

Three insert slots on every synth strip and group (Overdrive, Distortion, Fuzz, EQ, Compressor, Vocoder), four sends per strip (pre or post fader) into four processors P1–P4 (Echo, Reverb, Chorus, Flanger, chainable in series), eight group buses, and a master equalizer, compressor and limiter. All preallocated; the slot order is the signal order, their count fixed.

## M5: Algo

### MVP 9: generators *(built, v0.28–v0.32, epic #169)*

- **Seeded PRNG in the engine** (same seed, same music), scales and keys (`scale e dorian`, eleven modes, #165, #201).
- **Generators are functions in the notation** (ADR-0012): `euclid(k, n, rotation)` for a lane or a rhythm (#165), `walk` over a scale and `arp` over given chords (#166). Each takes an explicit seed.
- **Live and frozen** (#167): a live fragment regenerates every cycle; freezing replaces the call with the events it produced, in the same notation.

### MVP 10: evolving loops *(mostly built)*

- **Markov** chains learned from a fragment (`markov`, #166), or from a score through the MIDI import (#173). *(Built.)*
- **Mutate:** `mutate(riff, amount, seed)`, a share of the notes changed each time (#166). *(Built.)*
- **Modulators:** automation lanes and scenes move any parameter over the arrangement (#172). *(Built.)* LFOs and random sources targeting any parameter, and per-step parameter locks, are not (epics #208, #215).
- **Loops that feed loops:** one generator's output as another's input (`markov` and `mutate` read an earlier fragment). *(Built for those two.)*

### MVP 11: the model writes, and performance *(next, #108)*

- **A language model writes the song.** It gets the current text and a request ("a busier snare in B", "continue for 16 bars", "a darker lead") and returns a new text. The engine's parser is the check: a parse error goes back to the model for another try, and nothing reaches the engine that did not parse. The model never touches the engine directly.
- **Where the call runs is on hold.** Options: (a) the browser calls the API directly with a key the user pastes in (no server, but the key lives in the browser); (b) a small proxy next to Caddy holds the key (a backend, against ADR-0006; a 63xx port); (c) no call from the app: the text is written in Claude Code or claude.ai and pasted in. (c) works now and needs no code; (a) or (b) need an ADR.
- **Performance:** quantized launching, Web MIDI in from a hardware keyboard (note on/off, velocity, pitch bend, mod wheel; #10, moved here from MVP 2), MIDI out to hardware, SIMD (`simd128`) and table optimizations where profiling says so. Scenes are built (#172).
- **MIDI export** as a converter from the notation to a MIDI file. Import is built (#173).

## Assumptions to confirm

- **Chrome first.** AudioWorklet and Web MIDI are best supported there; Safari and Firefox are checked, not targeted, until MVP 11.
- **Vue for the view.** The UI is TypeScript + Vue (ADR-0003). If the UI should itself be Rust/wasm (Leptos), that's a new ADR; the engine boundary doesn't change.
- **Score licensing.** Vivaldi's music is public domain; a given MIDI *file* may not be. MVP 5 picks a source with an explicit open licence.
