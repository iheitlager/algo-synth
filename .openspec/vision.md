# Vision

## What algo-synth is

algo-synth is a **synthesizer you compose with**, in a browser tab. Open the page, press power, and you have up to sixteen instruments, a mixer, a song written as text and a set of algorithmic generators that write music into it. Nothing is installed and no audio touches a server: the whole engine is Rust compiled to wasm and runs on the browser's audio thread.

It is a personal instrument first: built for playing and for learning DSP and generative music, from first principles, in Rust.

## The instruments

Each of the sixteen synth slots is one of nineteen models, in four families. Every model is a variant of one Rust enum and plays through the same voice pool (ADR-0011), not a plugin system.

| Family | Models | Character |
|---|---|---|
| **Mono** | ARP 2600, Minimoog, Pro-One, MS-20, CS-15, SH-101, Odyssey | One shared semi-modular voice (spec 004): three VCOs, noise, a 4-pole ladder or 12 dB filters, envelopes, LFO, and *normalled* routing that a patch overrides. Each model sets the filter, envelopes and panel of its instrument (spec 005). |
| **Poly** | Prophet-5, Juno-106, Jupiter-8, Matrix-12, PPG Wave, D-50, DX7, Polymoog | A pool of voices with stealing, unison and analog drift (spec 006); the PPG plays wavetables, the D-50 PCM attacks over a synth body, the DX7 six-operator FM and loads `.syx` banks. |
| **Drums** | TR-808, TR-909, Pad Sampler | The 808 and 909 are kits where every pad is a small synth model (a bridged-T kick, a noise-plus-tone snare, metallic hats), not samples; the same pads on both, so a beat plays on either. The Pad Sampler is sixteen MPC-style pads of samples (spec 007). Each pad can go to its own group bus. |
| **Samplers** | Sampler | A multisampler that maps samples across keys and velocities, with loops, round robin and release samples (spec 007). |

## Composing: the song is text

A song is a short text the engine parses, prints and plays (ADR-0012, ADR-0015): `track`s name their synth, `clip`ments hold the music, `scene`s say which clips play for how many bars, and `arrange` puts the scenes in order. A clip comes from one of three places:

1. **By hand:** drum lanes of steps (`bd x...x...`) and notes in mini-notation or classic durations (`"c4 [e4 g4] ~"`, `e4:4 g4:8`), typed or edited in the step grid and piano roll.
2. **By a generator:** a seeded function in the notation (`euclid`, `walk`, `arp`, `markov`, `mutate`) on the song's scale. A *live* clip plays new notes every cycle; *freezing* writes the notes it played back into the text.
3. **From a score:** a MIDI file played by the MIDI player, one synth per part, or converted into song text, one track per part.

Automation lanes and snapshots move any parameter over the arrangement. The same clock, the same synths and the same mixer serve all three; a hand-played riff, a Euclidean kick and a line from a score are all just clips.

## Sound path

`source → 3 insert slots → fader/pan → 4 sends → 4 processors (echo, reverb, chorus, flanger) → master (EQ, compressor, limiter)`, with eight group buses that any strip can feed, each with its own three insert slots (ADR-0010). A fixed layout, preallocated, not a free patch graph; the Mono voice is where patching lives.

## What it is not

- **Not a DAW.** No audio recording, no plugin hosting.
- **Not a sample library.** The samplers play your own WAV files or free packs that `make samples` fetches; none are committed. The TR-808, the TR-909 and the synths are synthesized.
- **Not a server app.** The container serves files; there are no accounts and no backend.
- **Not a clone.** "PPG-style" and "2600-style" describe lineages, not emulations of specific circuits.
