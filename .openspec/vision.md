# Vision

## What algo-synth is

algo-synth is a **synthesizer you compose with**, in a browser tab. Open the page, press power, and you have three instruments, a mixer, a bar-based arrangement and a set of algorithmic loops that write music into it. Nothing is installed and no audio touches a server: the whole engine is Rust compiled to wasm and runs on the browser's audio thread.

It is a personal instrument first: built for playing and for learning DSP and generative music, from first principles, in Rust.

## The three sources

| Source | Lineage | Character |
|---|---|---|
| **Mono** | ARP 2600 (with a Moog ladder filter option) | Semi-modular monophonic voice: three VCOs, noise, a 4-pole filter, envelopes, LFO, and *normalled* routing that a patch overrides. The workhorse: bass, lead, and six of them as an ensemble. |
| **Wave** | PPG Wave | Wavetable: 64-wave tables built from keyframes, swept by envelope and LFO, polyphonic, with an optional 8-bit path for the grit. |
| **Drums** | Analog drum machines (808/909 lineage) | A kit where every pad is a small synth model (a bridged-T kick, a noise-plus-tone snare, metallic hats), not samples. |

More may come; each is one variant of one Rust enum, not a plugin system.

## Composing: three ways to fill a clip

The arrangement is classic: **tracks** down, **bars** across, **clips** on the grid. A clip holds a **pattern** (steps with note, velocity, length, probability and per-step parameter locks). A pattern comes from one of three places:

1. **By hand:** played in or drawn.
2. **By an algo loop:** a seeded **generator** (Euclid, Markov, random walk, arpeggiator, mutate) with a key and scale, writing into a track. *Live* loops generate a new pattern every cycle; *frozen* loops commit one to a clip.
3. **From a score:** a MIDI file imported as clips, one track per part. The showcase is the one the internet loves: **six ARP 2600s playing Vivaldi**, six Mono voices, each with its own patch, playing a public-domain score.

The same model, the same clock, the same mixer for all three. That's the point: a hand-played riff, a Euclidean kick and a Vivaldi violin line are all just clips.

## Sound path

`source → up to 4 inserts → fader/pan → 2 sends (delay, reverb) → master (comp/limiter)`. A fixed layout, preallocated, not a free patch graph; the Mono voice is where patching lives.

## What it is not

- **Not a DAW.** No audio recording, no plugin hosting.
- **Not a sample player.** Sounds are synthesized.
- **Not a server app.** The container serves files; there are no accounts and no backend.
- **Not a clone.** "PPG-style" and "2600-style" describe lineages, not emulations of specific circuits.
