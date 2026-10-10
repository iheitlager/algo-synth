# algo-synth

An algorithmic synthesizer that runs entirely in the browser, with the whole engine in Rust compiled to wasm on the audio thread. Up to sixteen synths (monosynths, polysynths, drum machines and samplers) play through a mixer with groups and effects. A song written as text, with generators and an arranger, plays them from one transport; a MIDI file opens as a song.

## Version: 0.40.0

- **Mono:** one shared voice: three band-limited VCOs, ring mod and sub, noise, a 4-pole ladder or a 12 dB high-pass/low-pass pair, ADSR, filter ADSR and AR, LFO, normalled routing with patch overrides, poly-mod.
- **Up to 16 synths, nineteen models:** add synths as you need them. Seven are monosynths: ARP 2600, Minimoog, Pro-One, MS-20, CS-15, SH-101 and Odyssey, each with its own faceplate (knobs, envelope curves, patch bay), colours and presets. A MIDI file plays each part on its own synth.
- **Polyphony:** each synth owns a pool of up to 16 voices (a global budget of 64) with unison, analog drift and a stereo chorus. Eight polyphonic models join the family: Prophet-5, Juno-106, Jupiter-8, Matrix-12 (with its modulation matrix), PPG Wave (wavetables), Roland D-50 (LA synthesis), Yamaha DX7 (six-operator FM, loads `.syx` voices and banks) and the Polymoog, each with presets and its own faceplate.
- **Drums:** a synth slot can be a TR-808 or a TR-909, the same pads with each machine's sounds, so a beat plays on either. The TR-808: its sixteen voices (kick, snare, three toms and three congas, rimshot, claves, clap, maracas, cowbell, cymbal, closed and open hats), synthesized, with tune, decay, tone and level per pad and an accent. Keys play the pads by General MIDI's drum map, so a MIDI file's drum channel plays on it.
- **Samplers:** a multisampler that maps WAV samples across keys and velocities, with loops and round robin, and a pad sampler of sixteen MPC-style pads. `make samples` fetches free packs, kits and a voice pack.
- **Mixer:** a console with three insert slots per strip (drive, EQ, compressor, vocoder), four sends into four processors (echo, reverb, chorus, flanger), eight group buses, and a master EQ, compressor and limiter.
- **The song is text:** drum lanes, notes in mini-notation or classic durations, generators (`euclid`, `walk`, `arp`, `markov`, `mutate`, live or frozen), scenes and an arrangement, automation and snapshots, in a highlighting editor beside a step grid, piano roll and arranger. Opening a MIDI file converts it into a song.
- **Setups and presets:** save the synths, their models, patches and mixer as `<song>.synths.json`, and keep presets of synths, inserts, processors and strips in a library.
- **One server:** `algo-synth serve` serves the app and the assistant on one port, localhost only; the music runs in the browser.

What changed in each version is in [CHANGELOG.md](CHANGELOG.md). See [.openspec/plan.md](.openspec/plan.md) for the road from one mono voice to a true algo synth.

## The song language and its relatives

A song is one text file: the instruments, their sound, the mix, the patterns and the arrangement. A taste, from [examples/songs/clockwork-arps.song](examples/songs/clockwork-arps.song):

```
tempo 128
scale f minor
setting glass = Modular ModularBasic
  SynthDef(\glass, { |freq = 440, cutoff = 1200, gate = 1|
      var env = EnvGen.kr(Env.perc(0.002, 0.3), gate);
      var sig = { Pulse.ar(freq * Rand(0.99, 1.01), Rand(0.2, 0.5)) }.dup(2);
      RLPF.ar(sig, cutoff * (env * 3 + 1), 0.3, voicing: \ms20) * env * 0.5
  }).add;
track pad synth Jupiter8 JupiterPad
track glass synth glass
track k9 drums Tr909 Kit909

clip kick = k9 /16
  bd x...x...x...x...
clip chords = pad voicing
  prog(8,11)
clip glassy = glass .every(4, rev)
  arp(chords,updown,16)

mod glass.ctl1 = perlin.slow(4).exprange(700, 3500)
scene arps 16: kick chords glassy
arrange arps
```

Little of it is new on its own; it borrows on purpose:

| In the song | Borrowed from |
|---|---|
| `"f2 [c3 f2] <a#2 c3>"`, `~`, `*4`, `@3`, `?` | Strudel and TidalCycles mini-notation |
| `.every`, `.off`, `.degrade`, `.palindrome`, `.iter`, `.struct` | Strudel's pattern methods (ADR-0019) |
| `sine.slow(8).range(…)`, `perlin`, `lfo(…)` | Strudel's signals, a modular's LFOs |
| `euclid(5,16,2)` | Euclidean rhythms (Toussaint), as in Tidal and many sequencers |
| `SynthDef(…)` under a `setting` | SuperCollider's sclang (ADR-0024) |
| `bd x...x...` lanes | drum-machine step grids |
| `c5:2 g#4:4 c5:4.` | LilyPond and MML durations |
| `d5@0:6:90` | a MIDI event list |
| `clip`, `scene`, `arrange`, `loop` | a tracker's patterns and order list, a DAW's clips and arrangement |
| `auto`, `snapshot`, `strip`, `group`, `master` | a DAW's automation lanes and a desk's recall sheet |
| `prog`, `root`, `markov`, `mutate`, `walk` | algorithmic composition, made deterministic |

Its nearest relatives are TidalCycles with SuperDirt (patterns playing SuperCollider SynthDefs) and Csound (an orchestra of instruments beside a score). Where algo-synth differs:

- **The text and the GUI are one song.** The step grid, piano roll, synth faceplates and mixer all edit the text, and the engine prints it back in one canonical form, comments kept (ADR-0012). A song typed by hand and one built with the mouse are the same file.
- **It renders the same every time.** Generators, live clips, random numbers in a SynthDef and analog drift are all seeded, so the same text gives the same audio, sample for sample, after a seek and in tests.
- **SuperCollider code without SuperCollider.** A subset of sclang is built into a fixed program the engine runs on the audio thread without allocating; every number in it becomes a knob, saved back into the code.
- **Code synths beside modelled instruments.** A SynthDef can borrow a modelled instrument's filter (`voicing: \ms20`) and play in the same song as a Prophet-5 voiced down to its oscillators, envelopes and VCA.
- **Generators read the song.** `root(chords)`, `arp(chords, …)` and `markov(2, riff, 7)` take other clips as their input, so one progression can drive the pad, the bass and the arps.

Every keyword, with examples, is in [.openspec/language.md](.openspec/language.md).

## Quick start

```bash
make dev      # the app and the assistant on http://localhost:6341, rebuilt as you edit
make serve    # the same from one Podman container on http://localhost:6340
make check    # every CI gate: lint, deny, tests, typecheck, build
make bench    # 16 Mono voices in V8 against the 30% CPU budget
make          # all targets
```

Needs Rust (stable, the `wasm32-unknown-unknown` target comes from `rust-toolchain.toml`), Node 24, and for `make serve` Podman (`podman machine init && podman machine start` once on a Mac). Open it in Chrome and press **Power on**; play with the on-screen keys or the computer keyboard (`a`…`;`).

## The assistant

A language model can write and change the song (ADR-0028, epic #381). The app's own server, `algo-synth serve` (ADR-0030), answers the Assistant under `/api`: it holds the keys, runs the model and checks every song on the engine. It serves `127.0.0.1` only, so there is no login. Without keys the app works as before, and the assistant offers no provider.

**Keys come from 1Password** and never reach the browser or git. `op.env` holds references only:

1. In 1Password, make an item **algo-synth** in the vault **Labs** with a field per provider you use: `Anthropic`, `Mistral`, `Gemini`, `OpenRouter`. Or edit `op.env` to point at your own vault and item.
2. `make env-check` lists each key as ok or missing, never its value. A provider whose key is missing is simply not offered.

**Run it:**

```bash
make dev      # algo-synth serve on 127.0.0.1:6341, keys from 1Password; reload to see an edit
make serve    # the same, one container, published on 127.0.0.1:6340
```

`OP=` runs either without 1Password: the app serves and the assistant offers no provider.

- **A self-hosted model:** Ollama, llama.cpp's server, vLLM or LM Studio, anything with an OpenAI-compatible API. Set `ASSIST_SELF_HOSTED_URL` (for example `http://127.0.0.1:11434/v1`; from the container, `http://host.containers.internal:11434/v1`) and `ASSIST_SELF_HOSTED_MODELS` (for example `qwen3:32b`).
- **A machine without the 1Password app:** export `OP_SERVICE_ACCOUNT_TOKEN` and `op run` uses it. `OP=` skips 1Password when the keys are exported already.
- **Choosing models:** the allowlist per provider is `crates/assist/providers.json`. `ASSIST_EFFORT` sets Anthropic's effort (default `high`).

## Layout

```
crates/dsp/     the engine (cdylib → dsp.wasm): C ABI, voices, mixer, song, generators, samplers
crates/assist/  the assistant's server and its song tools (check, render, catalog)
web/            Vue view; public/worklet.js is the audio-thread shim
tools/          bench, demo MIDI, sample fetcher, release script, spec link check
changes/        changelog fragments, one per PR, collected by make release
.openspec/      vision, plan, ADRs, specs, the song language
Containerfile   wasm and server → web → one image
```

## Documentation

- [The song language](.openspec/language.md): every keyword of the composer's song text, its meaning, limits and errors, with examples
- [Vision](.openspec/vision.md) · [Plan](.openspec/plan.md) · [ADRs](.openspec/adr/index.md) · [Specs](.openspec/README.md#specifications)

## Licence

Apache-2.0
