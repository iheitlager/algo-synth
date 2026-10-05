# algo-synth

An algorithmic synthesizer that runs entirely in the browser, with the whole engine in Rust compiled to wasm on the audio thread. Up to sixteen synths (monosynths, polysynths, drum machines and samplers) play through a mixer with groups and effects. A song written as text, with generators and an arranger, plays them, and so does a MIDI player.

## Version: 0.34.0

- **Mono:** one shared voice: three band-limited VCOs, ring mod and sub, noise, a 4-pole ladder or a 12 dB high-pass/low-pass pair, ADSR, filter ADSR and AR, LFO, normalled routing with patch overrides, poly-mod.
- **Up to 16 synths, nineteen models:** add synths as you need them. Seven are monosynths: ARP 2600, Minimoog, Pro-One, MS-20, CS-15, SH-101 and Odyssey, each with its own faceplate (knobs, envelope curves, patch bay), colours and presets. A MIDI file plays each part on its own synth.
- **Polyphony:** each synth owns a pool of up to 16 voices (a global budget of 64) with unison, analog drift and a stereo chorus. Eight polyphonic models join the family: Prophet-5, Juno-106, Jupiter-8, Matrix-12 (with its modulation matrix), PPG Wave (wavetables), Roland D-50 (LA synthesis), Yamaha DX7 (six-operator FM, loads `.syx` voices and banks) and the Polymoog, each with presets and its own faceplate.
- **Drums:** a synth slot can be a TR-808 or a TR-909, the same pads with each machine's sounds, so a beat plays on either. The TR-808: its sixteen voices (kick, snare, three toms and three congas, rimshot, claves, clap, maracas, cowbell, cymbal, closed and open hats), synthesized, with tune, decay, tone and level per pad and an accent. Keys play the pads by General MIDI's drum map, so a MIDI file's drum channel plays on it.
- **Samplers:** a multisampler that maps WAV samples across keys and velocities, with loops and round robin, and a pad sampler of sixteen MPC-style pads. `make samples` fetches free packs, kits and a voice pack.
- **Mixer:** a console with three insert slots per strip (drive, EQ, compressor, vocoder), four sends into four processors (echo, reverb, chorus, flanger), eight group buses, and a master EQ, compressor and limiter.
- **The song is text:** drum lanes, notes in mini-notation or classic durations, generators (`euclid`, `walk`, `arp`, `markov`, `mutate`, live or frozen), sections and an arrangement, automation and scenes, in a highlighting editor beside a step grid, piano roll and arranger. A MIDI file converts into a song.
- **Setups and presets:** save the synths, their models, patches, mixer and routing as `<song>.synths.json` next to the MIDI file, and keep presets of synths, inserts, processors and strips in a library.
- **No backend:** the container serves static files.

What changed in each version is in [CHANGELOG.md](CHANGELOG.md). See [.openspec/plan.md](.openspec/plan.md) for the road from one mono voice to a true algo synth.

## Quick start

```bash
make dev      # Vite on http://localhost:6341 (rebuilds dsp.wasm first)
make serve    # Podman + Caddy on http://localhost:6340
make check    # every CI gate: lint, deny, tests, typecheck, build
make bench    # 16 Mono voices in V8 against the 25% CPU budget
make          # all targets
```

Needs Rust (stable, the `wasm32-unknown-unknown` target comes from `rust-toolchain.toml`), Node 24, and for `make serve` Podman (`podman machine init && podman machine start` once on a Mac). Open it in Chrome and press **Power on**; play with the on-screen keys or the computer keyboard (`a`…`;`).

## Layout

```
crates/dsp/     the engine (cdylib → dsp.wasm): C ABI, voices, mixer, song, generators, samplers
web/            Vue view; public/worklet.js is the audio-thread shim
tools/          bench, demo MIDI, sample fetcher, release script, spec link check
changes/        changelog fragments, one per PR, collected by make release
docs/           the song language
.openspec/      vision, plan, ADRs, specs
Containerfile   wasm → web → Caddy
```

## Documentation

- [The song language](docs/song.md): every keyword of the composer's song text, with examples
- [Vision](.openspec/vision.md) · [Plan](.openspec/plan.md) · [ADRs](.openspec/adr/index.md) · [Specs](.openspec/README.md#specifications)

## Licence

Apache-2.0
