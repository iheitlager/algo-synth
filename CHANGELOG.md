# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.34.0] - 2026-10-05

### Added

- **Example songs:** six arranged drum loops with SH-101 bass lines in `examples/` (house, acid, electro, dub, jungle, deep techno), with cutoff, resonance and glide automation (#249).

## [0.33.0] - 2026-10-05

### Added

- **Chords by name (#103):** note fragments take chord symbols (`"c:m7 f:maj7 bb:sus4 g3:7"`, a bare root a major triad, `c:m7:2` in classic notes) and roman numerals in the song's key (`"<i VI III VII>"` plays Cm Ab Eb Bb after `scale c minor`, Am F C G after `scale a minor`), with explicit case (upper major, lower minor, then `o`, `o7`, `+`, `7`, `maj7`, `b`/`#` for borrowed chords). `arp(c:m7,up,16)` takes a chord by name, names print back as written, and the editor colours them. `frag … voicing` moves each chord to the inversion nearest the one before, within C3–C6.
- **Progressions feed other parts (#103):** `prog(4,7)` writes a seeded progression of triads in the song's key (tonic first, dominant last, moving by function), `root(chords)` plays the root of each chord of a frag as a bass line, and `arp(chords,updown,16)` arpeggiates a frag's chords one after another. One progression feeds the pad, the bass and the arp.
- **Comments in the song text are kept:** a `#` comment above an item or after it (a lane, a frag, a section, the tempo) comes back when the text is applied or a step is edited on the grid; before, they were erased. A comment whose item is gone moves to the end (#199).
- **Pad Sampler outs:** each pad has an Out, the sampler's own strip or straight into a group (the groups on the console), panned there as the pad is; soloing the sampler keeps the groups its pads go to, and removing a group sends its pads back to the strip (#220).
- **A reference for the song language:** `docs/song.md` lists every keyword (tempo, swing, scale, setting, track, frag, lanes and notes, generators, live, auto, scene, section, arrange, loop) with its syntax, limits and errors, and ends with a worked song; a test parses every example in it (#226).

### Changed

- **Drum outs:** the Drums menu lists the TR-808, TR-909 and Pad Sampler in that order; a drum pad's Out lists only the groups on the console; removing a group, or loading a setup that routes to a group it does not have, sends the strips and the kit pads that fed it to the master (#218).
- **Plan, vision and README:** `plan.md` marks what is built per MVP with its issues, `vision.md` describes the nineteen models, the song as text and the actual mixer, and the README lists the samplers, mixer and song, the `tools/`, `changes/` and `docs/` folders, and points to `CHANGELOG.md` for the version history (#230).
- **CI runs the tool tests:** `make test-tools` (the release and sample-fetch scripts) is a CI job, as `make check` already ran it (#231).
- **Tests for the engine bridge and the preset library:** `engine.ts` is tested against a fake AudioWorklet port (song decoding, every worklet message, the messages the view posts, setups and presets) and `library.ts` against an in-memory IndexedDB (#232).
- **UI test coverage:** `make coverage-web` reports line coverage of the UI (vitest with v8), with an HTML report in `web/coverage/` (#232).
- **A test proves `render` never allocates:** `crates/dsp/tests/render_no_alloc.rs` counts every allocation while a busy song plays (automation, scenes, live generators, chords) and fails on any; live generators assert they stay within the room they reserve (#233).
- **The engine tests live in `crates/dsp/src/engine/tests.rs`:** `engine.rs` keeps the engine (about 1 600 lines), as `song`, `notes` and `drums` already do (#234).
- **`make image` explains why it rebuilds:** the Makefile comment on `image` says how the commit in `ALGO_BUILD_SHA` keeps the image from shipping an old wasm (#198, #235).

### Fixed

- **No "Group 9":** the mixer's Out pull-down labelled None as "Group 9", and a strip sent to None showed a "feeds Group 9" tag; None is now None, with no tag (#217).
- **Drums keep playing with the loop (#223):** a drum track plays the TR-909 or pad sampler already in the rack instead of turning the first synth into an 808 (a regression of #210), and the text names that kit. The arranger's first section holds every frag and keeps the clock's place, so it no longer silences or stops the beat. The composer's bar and step readout and the step and piano-roll playheads follow the arrangement and its loop, and light only frags the section plays. The kit notice knows the 909 and the pad sampler.
- **Automation no longer allocates on the audio thread:** a song lane or scene on a strip parameter (fader, pan, send, mute) reworked the solos, which built vectors inside `render`; now only a solo or a route does, without allocating (#225).
- **Specs:** every `Implementation` and `Tests` reference resolves again, and `make test-tools` checks it; specs 001–006 match the code (voice pools, eighteen pads, three insert slots, nineteen models), a new spec 007 covers the samplers, and the song editor, build info, scale modes and song limits are specified (#229).

## [0.32.0] - 2026-10-05

### Added

- **Build details in the UI:** the transport bar shows the page version and the engine's, a click lists both with their commits, and a banner says when the page and `dsp.wasm` are from different builds (#197).
- **Two more scale modes:** `scale e phrygian-dominant` and `scale a harmonic-minor`, so walks play the raised third or seventh (#201).
- **A song editor with line numbers and highlighting (#203):** the composer's song text has a line-number gutter and colours keywords, names, numbers, notes, pads, steps, rests, generator calls, `target.Param`s and comments. The engine lexes (`song::lex`, exports `lex_buf`, `lex`, `lex_ptr`) on a second instance of `dsp.wasm` on the main thread, away from the audio. A parse error marks its line and column. Tab indents.
- **The composer sets up its synths:** `track bass synth Minimoog MiniBass` names a track's model and preset; left out, they are picked from the track's role (bass, lead, pad, arp, keys or drums) and written into the text, and each track plays on a synth of its own (#210).
- **Settings in the song:** `setting nile = Minimoog MiniLead: Cutoff 1200, Resonance 0.5` is a patch that lives only in the composer; `track lead synth nile` plays it (#210).

### Fixed

- **Monospace text where it was meant:** the song text, pad names, piano-roll keys and build details asked for `var(--mono, monospace)`, but `--mono` is a colour, so they fell back to the sans-serif font. They use a new `--font-mono` (IBM Plex Mono) (#203).
- **`setting` is highlighted:** the song editor colours the new `setting` keyword like `track` and `scene` (#210).

## [0.31.0] - 2026-10-04

### Added

- **Save and open the song (#105):** Save song downloads the song as `.song` text, Open… takes a `.song` beside a MIDI file and a setup, and the last song comes back when audio powers on. A file that does not parse shows its error in the composer while the song plays on.
- **Live arpeggiator (#110):** every Mono and Poly synth has an Arp section on its faceplate: on/off, mode (up, down, up-down, as played, random with a seed), 1–4 octaves, rate (1/8, 1/16, 1/8T, 1/16T), gate and latch. Held keys play as a pattern on the song's clock, starting at the first note on the next step; with the song stopped the arp waits, or plays on its own grid with Free run. The pattern is one function (`arp_note`) shared with the notation's `arp`.
- **A TR-909** beside the TR-808 in the Drums family: a punchy kick (a fast sweep, a click and drive), a snappy snare, three toms, rimshot, a four-burst clap, metallic hats, crash and ride, synthesized. The same pads as the 808, so a beat plays on either: a pad a machine lacks plays its nearest voice. The kit gains `cr` and `rd` (the 808 plays its cymbal for them); presets 909 Kit and Hard 909 (#148).
- **Generators for notes (#166):** on a note line, `arp([c4,e4,g4],up,16)` (up, down, updown or random with a seed), `walk(c4,8,1)` (a random walk on the song's scale), `markov(1,riff,3)` (a chain learned from an earlier frag, keeping its rhythm and pitches) and `mutate(riff,30,5)` (change a percent of its notes). All are seeded, bounded and print back canonically.
- **Live and frozen fragments (#167):** `frag w = lead live` makes a generator call play new notes every bar, the same ones on every run (the base seed mixed with the bar), prepared into buffers reserved at load so `render` does not allocate. `freeze` replaces the call with the bar it was playing, written as mini-notation (`freeze` and `frag_live` join the C ABI).
- **A piano roll for note fragments (#168):** the composer draws each note fragment as a grid of pitches and sixteenths with the step it plays now. Click to add a sixteenth, click a note to remove it, drag its end to stretch it; every edit goes to the engine, which prints the song back. A written fragment is rewritten as exact mini-notation (`"c4@12 ~@12 g4@24"`) when edited. A generator call shows its call and notes with a **Freeze** button, and is edited once frozen. `frag_events`, `event_*`, `frag_generated`, `note_add`, `note_remove` and `note_len` join the C ABI.
- **Sections and the arrangement** (#170, spec 002 Req 4, ADR-0015): `section <name> <bars>: <frags>`, `arrange <sections>` and `loop <first> <last>` in the song text. A section plays its own drum and note fragments from its first bar, looping inside it; the song wraps in the loop and stops after its last bar. New ABI calls `song_seek_bar`, `song_bars`, `song_entry`, `song_local`.
- **The arranger** (#171, spec 003 Req 12): the bottom pane (and under the composer) shows the arrangement as a timeline of sections with fragments, lanes and scenes switched per section, bars, order, loop and seek; every edit goes through the engine (`arr_edit`). The MIDI player is a tab away.
- **Scenes and automation** (#172, spec 002 Req 10): `auto` lanes (values over bars, or a ramp) and `scene`s for any parameter but the model and `Out`, by name on a track, `strip1`–`16`, `group1`–`8` or `master`. Lanes run once per block and write only changes; scenes land on a section's first step; knobs and faders follow (`auto_touched`).
- **MIDI import into the song (#173):** "Import as song" in the MIDI player turns the loaded file into song text: a `synth` track per channel, routed as the player had it, fragments of timed notes (`d5@0:6:90`, a third note form, with `frag … bars N`) snapped to the 48-tick grid, cut into 8-bar sections (shorter when a part is dense) with repeats sharing fragments and sections, and an `arrange` line. The demo Canon plays note for note as the player did. Songs may now hold 256 fragments and 256 sections.

### Changed

- **Releases from changelog fragments:** a PR adds `changes/<issue>.<added|changed|fixed>.md` instead of bumping the version, and `make release` collects them into a release, so PRs merged in parallel no longer collide on the version lines (#186).
- **Drum faceplates laid out like the machines:** the TR-808 and TR-909 show their pads left to right, each a column of knobs, and a pad's out is a pull-down (Master or a group, by its name) instead of a row of buttons (#194).

### Fixed

- **The image build no longer fills the podman disk:** `rust:1` ships its toolchain as a version while `rust-toolchain.toml` asks for `stable`, so rustup installed a second toolchain inside the `cargo build` layer on every `crates/` change (~1 GB a build). The Containerfile now installs the toolchain file's toolchain in its own cached layer (#183).

## [0.30.0] - 2026-10-04

### Added

- **A Voice pack:** sixteen short spoken phrases from the CMU ARCTIC speech databases (two speakers), one per pad of the pad sampler and named by what they say, for a vocoder's modulator or chopped speech. Fetched by `make samples` like every pack: the ledger holds only each file's URL and pinned checksum. Not CC0: the CMU notice, the authors and the conversion go into the kit and `CREDITS.txt` (#184).

## [0.29.0] - 2026-10-04

### Added

- **Pre/post-fader sends, and an on/off switch per send:** each send can be taken before the fader (washes, throws) and switched off without losing its level. Mute still silences both kinds (#144).
- **Individual outs for the drum kit:** each pad of a TR-808 goes to the kit's strip or straight into a group bus, panned there, so a clap or snare gets its own EQ, compressor and sends. Soloing the kit keeps its groups heard (#162).
- **A vocoder insert:** the strip it sits on is the carrier; its Key picks the synth whose raw signal (before its inserts, fader and mute) shapes it, so muting the modulator hides it. Sixteen 4th-order bands, with shift, release, unvoiced noise, width and dry (#161).
- **Out: None:** a strip can go nowhere, still feeding its sends and any vocoder keyed to it (#161).

### Fixed

- A new insert type was clamped to the compressor: the type's range now follows the list of types.

## [0.28.0] - 2026-10-04

Part of the algorithmic compositions epic (#169): notes and generators in the song text.

### Added

- **Note fragments on `synth` tracks (#163):** a fragment is one line, in mini-notation (`"c4 [e4 g4] ~ <c5 d5>?"`: `[ ]` subdivides, `~` rests, `*n`, `@n`, `<a b>` per bar, `?`, chords `[c4,e4,g4]`, `!` accents) or classic durations (`c4:4 e4:8. r:4`); mixing the two is an error with a line and column. Notes start and end on their exact samples on a grid of 48 ticks to the bar (ADR-0016); the clock fires ticks between steps and the engine ends notes from a fixed table, so `render` still allocates nothing. The composer shows the line.
- **`sampler` tracks (#164):** pad lanes for a pad sampler, note fragments for a multisampler, never both in one fragment.
- **`euclid(k,n,rot)` and `scale` (#165):** a drum lane (`bd euclid(3,8)`) or a note line (`euclid(5,8) c4`, `euclid(4,8) scale c4`) with Bjorklund's spreading, and a `scale c minor` line with nine modes. A generated lane keeps its call in the text until a step is edited. `track_kind`, `frag_notes_ptr`, `frag_notes_len` and `frag_bars` join the C ABI.

### Changed

- A `#` starts a comment only at the start of a line or after a space, so `c#4` keeps its sharp.

## [0.27.0] - 2026-10-04

### Added

- **Rename from the synth list:** a double-click on a synth's name in the rail edits it in place; a single click still selects it (#178).

### Changed

- **Instruments are named by family:** a drum machine added with + Synth is Drum 1, Drum 2…, a sampler Sampler 1…, a synth Synth N, each with the lowest free number. The name is kept when it is added, so adding, removing or switching other instruments never renames it, and an empty name restores the family default (#177).

## [0.26.1] - 2026-10-04

### Fixed

- **A stale `dsp.wasm` gets a banner:** the warning added in 0.24.1 sat at the right end of the one-row transport bar, which clips it, so with an engine older than the page a new model (the Pad Sampler) still just looked like an ARP 2600. The error now has a full-width row across the top (`role="alert"`).

## [0.26.0] - 2026-10-04

### Added

- **The composer on its own screen,** with a Play and Stop of its own (Stop goes back to the top), BPM, Swing and the bar and step it is on. The MIDI player is hidden there.

### Changed

- **Two transports:** the song and the MIDI file play, stop and seek apart. The transport bar's Play and Stop move only the MIDI file again; the composer's move only the song. Both may play at once. `song_play`, `song_stop` and `song_playing` join the C ABI, and `playing()` reports the MIDI file again.

## [0.25.0] - 2026-10-04

### Added

- **The TR-808's sixteen voices:** rimshot and claves (struck resonators that ring at the same level at any tune), maracas, cymbal (a low and a high band of the hat oscillators), a mid tom and three congas join the kit, with tune, decay, tone and level each (ids 398–429; old setups load), their General MIDI notes, their names in the song, and the faceplate in the hardware's order (#140).

### Fixed

- **The 808 cowbell** follows its circuit: two band-limited squares at 540 and 800 Hz through a band-pass near 850 Hz (Q 4.25), with a fast and a slow decay. It was centred near 2.2 kHz (#140).

## [0.24.1] - 2026-10-04

### Fixed

- **A stale `dsp.wasm` is reported:** the dev server reloads the JavaScript but only `make wasm` rebuilds the engine, so a newer page left a model the engine did not know as a plain ARP 2600 with no hint why (the Pad Sampler could not be selected). The engine now reports how many models it knows (`model_count`) and the transport bar warns when that is fewer than the page offers.
- The pad editor sits beside the pad grid, so the pads and their settings are visible together.

## [0.24.0] - 2026-10-04

User presets (epic #153, PR #158).

### Added

- **User presets** (epic #153, ADR-0014, spec 003 Req 11): save, load, rename and delete presets of a synth (model and sound, applied on the synth defaults so the strip stays), an insert slot, a processor and a strip (pan, sends, inserts; never fader, mute, solo or routing). Presets are stored by name in a browser library (IndexedDB) and exported or imported as `algo-synth.presets.json`; setups are unchanged. A picker shows factory and user presets, and those of other models or types; a dot marks a changed target. Copy and Paste in every preset menu, one clipboard per kind; a DX7 SysEx voice can be saved to the library. New ABI call `synth_defaults`.

## [0.23.0] - 2026-10-04

Samplers, second half: the drum/pad sampler (epic #126).

### Added

- **Pad sampler** (#124): the Pad Sampler model, in the Drums family, plays sixteen pads on notes 36–51 (C1 up) from the sample store. Each pad has a sample, tune, level, pan, decay, a choke group, how much velocity moves its level and its start point, and a one-shot switch; a pad is one retriggered voice and a hit chokes the other pads of its group. The synth has a stereo bus so each pad pans (the strip balances). Pads are set through `pad_set` and `pad_get`; a MIDI file's drum channel plays it, and a song's drum track can play it too: its lanes hit the pads their General MIDI notes name on the clock's steps (kick 36, snare 38, clap 39, hats 42 and 46).
- **Pad grid** (#125): a 4×4 pad grid to play and select, the selected pad's settings and waveform, and a kit browser; the sample slots are now shared with the multisampler panel.
- **Drum kits** (#130): `make samples` also builds Hydrogen drum kits into the gitignored `web/public/samples/kits` with a kit manifest the pad grid loads (first kit: Audiophob, CC0); the converter reads 8-bit WAV and AIFF.

### Changed

- The model picker offers only the models of the selected synth's family.

## [0.22.0] - 2026-10-04

The drum machine (epic #97, MVP 3).

### Added

- **The song as text** (ADR-0012): `tempo`, `swing`, drum tracks and fragments of lanes (`x` a hit, `X` an accent, `.` a rest, up to 64 steps, each lane looping on its own length), parsed and printed by the engine. A parse error gives its line and column and the playing song plays on; a new song plays from the next clock step. Lanes hit on the clock's exact samples, on the TR-808 slot a track is routed to (#100).
- **The composer**, a third view beside Synths and Mixer: a 16-step grid per drum fragment (a click cycles off, hit and accent; the playing step is lit), the song's text beside it (Apply or Ctrl+Enter), a synth per track, and BPM and Swing in the transport. Play and Stop work without a MIDI file (#101).

## [0.21.2] - 2026-10-04

### Fixed

- The MIDI player re-rendered every note of every lane on each 20 ms playhead update (#137): each lane is now one path built when its notes change, and a single playhead moves by transform. Playing the demo, the player's share of the main thread falls from 6–9% to under 0.2%, and no longer grows with the file's note count.

## [0.21.1] - 2026-10-04

### Fixed

- **Sample packs fill the store:** the store is shared by every pack and capped at 64 MiB, and nothing freed a pack's samples, so loading one pack after another ended in "too large for the sample store". Loading a pack now clears the synth's zones and frees the pack samples no other synth uses and the new pack does not reuse; the error says what to do.

## [0.21.0] - 2026-10-04

Samplers, first half (epic #126; the drum/pad sampler is next).

### Added

- **Sample store** (#122, ADR-0013): the engine parses PCM 16/24 and float WAV files (mono or stereo, root note and loop from the `smpl` chunk) and resamples them at load into 64 slots under a 64 MiB cap; JavaScript only forwards the bytes (`sample_buf`, `sample_load`).
- **Multisampler** (#123): the Sampler model plays zones of the store as voices of the pool. 64 zones per synth map a key range and velocity range to a sample with root, tune, level, a loop or sustain loop (crossfaded), round-robin turns and release zones; Catmull-Rom playback through the synth's filter and amplifier envelope; two presets (Sampler Keys, Sampler Pad). Zones are set through `zone_set` and read back with `zone_get`.
- **Sample packs** (#129): `make samples` fetches the CC0 FreePats Synth Pad Choir, Sweep Pad, New Age Pad, Synth Pad Bowed and Upright Piano KW (pinned SHA-256), converts them to mono WAV with their loops, and writes zones and `CREDITS.txt` to the gitignored `web/public/samples/`, which the static server ships.
- **Sampler faceplate** (#125): the pack browser, WAV drag-and-drop, sample slots with a memory meter, a zone map, a zone editor and the waveform with loop markers. The pad grid waits for the drum sampler.

## [0.20.0] - 2026-10-04

### Added

- **+ Synth chooses a family and model** (#132, spec 003 Req 9): a menu of the families (Mono, Poly, Drums) and their models; a family adds its first model, a model itself, each on its first preset. Models carry a `family`, and `FAMILIES` in `models.ts` lists them.

## [0.19.0] - 2026-10-04

### Added

- **Drums in a synth slot:** the TR-808 model plays the kit's eight pads as voices of the slot's pool. A key hits the pad General MIDI puts there (any other key by its place in the octave from 36), on the voice already playing that pad, so a pad retriggers as on the 808; hits have no note-off, the closed hat chokes the open hat, and velocity 115 and up is accented. Tune, decay, tone and level per pad and the accent are synth parameters (ids 365–397), with two presets (808 Kit, Tight Kit) and a faceplate. A MIDI file's drum channel plays on a kit slot it is routed to (#114, epic #97).

## [0.18.0] - 2026-10-04

### Added

- **Names** for MIDI lanes, synths (shared with their console strip) and group buses (#127, spec 003 Req 10): double-click a lane or the faceplate title, or the ✎ on a console tape, to rename in place; empty restores the default. An unnamed synth takes its first lane's name. Every label (tapes, feeds tags, Out selector, synth rail, route choices) follows, and names are saved in the setup's optional `names` field. View only: no engine change.

## [0.17.0] - 2026-10-04

Polyphonic synths (epic #78, PR #116).
### Added

- **Polyphony** (epic #78, spec 006, ADR-0011): each synth owns a voice pool of up to 16 voices with a global budget of 64, a shared LFO, unison with detune spread, analog variance and a stereo chorus (Modes I, II and I+II; stereo strips balance instead of pan).
- Eight polyphonic models: **Prophet-5** (#82), **Juno-106** (#83), **Jupiter-8** (#84), **Matrix-12** with a twenty-slot modulation matrix (#85), **PPG Wave** with generated wavetables (#87), **Roland D-50** with LA synthesis (#88), **Yamaha DX7** with six-operator FM after the msfa reference (#89, see `NOTICE`) and the **Polymoog** with Strings and Vox Humana (#96); six presets each (four for the Polymoog) and a faceplate each.
- **DX7 SysEx import** (#90): the engine reads single voices and 32-voice banks (framed or bare, any checksum); the faceplate loads a `.syx` file and picks a voice. JavaScript only forwards the bytes (`sysex_buf`, `sysex_load`, `sysex_apply`).
- `make bench` gains `poly pads` and `poly worst`: 64 voices of chord pads on every polyphonic model, about 19% of a core (#91).

## [0.16.0] - 2026-10-04

### Added

- The drum kit's pads, synthesized after the TR-808: kick (a sine falling to its tune), snare (two tuned sines and high-passed noise), clap (three noise bursts and a tail), closed and open hats (six square oscillators through a band- and a high-pass, the closed choking the open), two toms and a cowbell. Per-pad tune, decay, tone and level and a kit accent; everything is computed per hit, and the loudest hit stays within full scale. Pad names are the notation's (`bd sn cp ch oh lt ht cb`), notes General MIDI's. Playing the kit from a synth slot follows with the voice pool (#114) (#99, epic #97).

## [0.15.0] - 2026-10-04

### Added

- A sample-accurate clock in the engine: tempo (20–300 BPM), MPC-style swing (50–75%) and sixteenth-note steps whose samples come from their index, so nothing drifts; a tempo change takes effect from the next step. The render loop splits blocks at clock steps, play, stop and seek drive it with the MIDI player, and `tempo`, `swing` and `clock_step` reach it from JavaScript. Tempo and swing are song data (ADR-0012), not registry parameters (#98, epic #97).

## [0.14.0] - 2026-10-03

### Added

- The console's processor rack shows Chorus and Flanger with their knobs in real units, and a "← P1" toggle on P2–P4 that chains a processor to the one before it, drawn as a link between the modules (#94, epic #95).
- Chained processors: `P2In`, `P3In` and `P4In` (ids 150–152) make a processor hear the one before it as well as its sends, so effects combine in series; a chain cannot loop, the previous processor's return may be 0, and chorus and flanger get its stereo output (#93, epic #95).
- Chorus and flanger as processor types: a two-tap chorus (rate, depth, delay, spread, tone) and a flanger with feedback of either sign (rate, depth, manual, feedback, tone), stereo in and out, with a parabolic LFO and no transcendental per sample. `ProcType` gains `Chorus` and `Flanger`; the processor's return level now scales the effect's wet signal in the slot, not inside each effect (#92, epic #95).

## [0.13.0] - 2026-10-03

The ARP Odyssey (#64, PR #76).

### Added

- **ARP Odyssey**, a seventh model (#64, spec 005 Req 10): two VCOs with hard sync, ring mod and noise, a 24 dB ladder with its own `ODYSSEY` voicing (brighter and cleaner than the Moog) and a 6 dB high-pass after it, one ADSR to filter and VCA, LFO, portamento; a black-and-gold faceplate with no patch bay (it isn't modular; presets may still use patch slots). Presets `CurrieLead` (two detuned saws into a bright, driven ladder, legato glide, vibrato on the wheel, after Billy Currie's late-70s lead; an overdrive insert on its strip adds the grit) and `OdysseySync`. `make bench`'s `six models` scenario is now `all models`, with the Odyssey in the family.

## [0.12.0] - 2026-10-03

### Added

- The flexible console: groups are strips with an **Out** selector on every strip and group, a coloured tag under each tape saying which group it feeds, **+ Group** and remove, and a panel per insert slot. Drag a tape to reorder, collapse a strip to a sliver, hide it and bring it back from the bar. Dimming follows the engine's mute and solo through groups. A setup now also saves the groups on screen with their strip parameters and the console layout; setups without them leave both alone. New export `strip_count` (#61, epic #62).
- Insert slots on the group buses: the same three slots and six types as a strip's, working on the group's stereo bus (the compressor gives both sides one gain), before the group's fader (#60, epic #62).
- Group buses: eight stereo groups (strips 16–23) with their own fader, balance, sends, mute, solo and meter, and an `Out` on every strip and group (0 is the master, 1–8 a group). A group feeds only the master or a higher-numbered group, so routes can't loop; a soloed strip stays heard through its groups. Meters grow to 30 (#59, epic #62).
- Insert slots: three in series on every synth strip, each Off, Overdrive, Distortion, Fuzz, a compact three-band EQ or a Compressor, with a type and five knobs A–E whose meaning depends on the type (`I1Type`…`I3E`, ids 66–83); a slot button on each strip opens a panel with the type and its knobs in real units. Spec 002 Req 2, ADR-0010 (#58, epic #62).

### Changed

- The drive insert is now insert type Overdrive, Distortion or Fuzz: `DriveMode`, `DriveAmount`, `DriveTone` and `DriveLevel` are gone, later parameter ids move up by 14, and an older setup's drive becomes insert slot 1 of its synth (#58, epic #62).

## [0.11.0] - 2026-10-03

Synth faceplates (epic #67, PR #75).

### Added

- **Synth faceplates** (epic #67, spec 003 Req 9): the Synths view is a rail of tapes (model, synth number, routed MIDI channels, LED meter, mute, solo) beside one faceplate in the console's hardware style and the model's own palette, with the sections and control names of that instrument and wooden cheeks on the Minimoog and Pro-One. Rotary knobs with the console's popover and readouts in real units (Hz, ms, semitones, cents), LED switches, stepped selectors with drawn waveform icons, every envelope as a live curve with a knob per time and level, and a piano keyboard under it that lights keys held from the mouse or the computer keyboard (#69, #70, #71, #72).
- **Patch bay** for the ARP 2600, MS-20 and CS-15: the eight patch slots as a matrix (sources down, destinations across, a lit point in the source's colour per connection) with a knob and readout for each slot's amount (#73).
- `audio/faceplate.ts`: the maths behind the drawings (envelope curve, waveform icons, nearest step, readout units, patch cells), and `audio/models.test.ts`, which walks every model's description. 22 more vitest tests (#69, #70, #73).

### Changed

- `models.ts` describes how each control is drawn (knob range, scale, unit and reset value; envelopes; selectors), not only what it is. The knob popover is mounted once in `App.vue` for the synths and the mixer. The card grid and its slider panel are gone (#70, #71, #72).

## [0.10.0] - 2026-10-03

### Added

- The mixer console: a Synths | Mixer switch shows one thin strip per synth side by side (tape, drive, four sends, pan, mute, solo, fader and LED meter), the four processors as a rack and the master section with an EQ curve, a compressor transfer curve and stereo meters. Knobs turn by dragging and open a slider on click; double-click on a strip's tape shows the synth's panel. Replaces the mixer pane. Spec 003 Req 8 (#55, epic #56).
- Peak meters from the engine: per synth strip (after its fader), master left and right, and each processor's return, read by the worklet about 47 times a second through new exports `meters_ptr`, `meters_len` and `meters_clear` (#53, epic #56).
- Console primitives for the coming mixer view: rotary knob with a click-to-slider popover, fader with a dB taper, LED meter, bundled Barlow Condensed and IBM Plex Mono fonts (#52, epic #56).

## [0.9.0] - 2026-10-03

Famous and string presets for every synth model (PR #65).

### Added

- 17 presets, ids 14-30, after well-known sounds: ARP 2600 `R2D2`, `ShArp` (sample-and-hold arpeggio), `SolinaStrings`; Minimoog `LuckyMan` (portamento solo), `FunkBass`, `MoogStrings`; Pro-One `SyncSweep`, `PolyModBell`, `ProStrings`; MS-20 `Ms20Squelch`, `JetSweep`, `Ms20Strings`; CS-15 `BladeBrass`, `Cs15Strings`; SH-101 `AcidBass`, `SubPluck`, `Sh101Strings`. The names point at a character, they are not recreations of the original patches.

### Fixed

- SH-101: the locked pulse was in antiphase with the saw, so the two cancelled and halved the level (a saw plus a pulse sounded quieter than the saw alone). The pulse is inverted now; `Sh101Lead` is about four times louder.

## [0.8.0] - 2026-10-03

### Changed

- The mixer owns every strip parameter (`Param::is_strip`) and has four post-fader sends: `EchoSend` and `ReverbSend` become `Send1` and `Send2`, `Send3` and `Send4` are new (P1–P4). The mixer pane replaces the Returns pane and the Channel section on each card; the drive insert stays on the synth (#45, epic #50).
- The send effects are four processors P1–P4, each an Off, Echo or Reverb with a return and five 0..1 knobs whose meaning depends on the type. `EchoTime`…`ReverbReturn` (ids 70–78) are replaced by `P1Type`…`P4E`, and later parameter ids move up by 19; setups store names, so only the old effect names need a migration (#46, epic #50).
- Master compressor (feed-forward, stereo-linked; `CompThreshold`, `CompRatio`, `CompAttack`, `CompRelease`, `CompMakeup`, ids 120–124; ratio 1 is off) with a gain-reduction meter, and a brick-wall limiter in place of the soft clip: master gain now comes after the compressor, and nothing is bent below full scale any more (#47, epic #50).
- Master equalizer before the compressor: a low shelf, two parametric bands and a high shelf (`EqLowFreq`…`EqHighGain`, ids 125–134, ±15 dB); bands at 0 dB are skipped, so flat is bit-exact (#48, epic #50).
- Setups carry the whole mixer (strips, sends, processors, EQ, compressor) with no change to the save code, since they are in the registry. A setup from before the mixer was central still loads: `EchoSend` and `ReverbSend` become `Send1` and `Send2`, and the global echo and reverb parameters become the knobs of P1 and P2, with one warning listing what was migrated (#49, epic #50).

## [0.7.0] - 2026-10-03

Synth setups next to the MIDI file (#41, PR #44).

### Added

- Synth setups (#41): **Save setup** downloads `<song>.synths.json` with the synths on screen, each one's kind, model and parameters by name, the channel routing and the global parameters (master gain, echo, reverb); **Open…** takes a MIDI file, a setup or both. Unknown entries are skipped with one notice, a bad file changes nothing, a part-count mismatch warns. The last setup per MIDI file is kept in `localStorage`; Demo ships `demo.synths.json` (Bass and three Bowed string violins a little apart). Values are written as the shortest decimal for the same f32. Spec 003 Req 7.
- `GlobalParam` in `params.ts`: the parameters `Param::is_global` marks, mirrored and checked by the mirror test, so a setup stores them once (#41).
- vitest for the view's pure functions (`make test-web`, part of `make test` and CI) (#41).

## [0.6.0] - 2026-10-03

The family of monosynths (epic #28, PR #42).

### Added

- **A family of six monosynths** (epic #28, ADR-0009, spec 005): every synth slot has a model, `Param::Model`: ARP 2600, Minimoog, Sequential Pro-One, Korg MS-20, Yamaha CS-15 or Roland SH-101. One shared voice with the model deciding the filter and its voicing, the high-pass stage, which envelope moves the cutoff, decay-as-release and the modulation source; two presets per model (14 in all). A new or reset synth is an ARP 2600, and its sound is pinned to what it was before models (#29, #30, #34).
- **Filter ADSR** (`FenvAttack`..`FenvRelease`, modulation source `Fenv`): the ARP 2600 and SH-101 keep one envelope for filter and loudness, the others have two (#31).
- **Filter flavours:** a 12 dB state-variable filter with saturating states and a one-pole high-pass, three ladder voicings (Moog, Pro-One, SH-101) and two 12 dB voicings (the MS-20 self-oscillates, the CS-15 does not); `HpCutoff`, `HpResonance`, `EnvHpCutoff` (#32).
- **Ring modulator, sub-oscillator and Osc 3 as a modulator:** `RingLevel` (VCO 1 × VCO 2), `SubLevel`/`SubOctave` (a band-limited square at an exact half or quarter of VCO 1's pitch), `Vco3KeyFollow`/`Vco3Low` (#33).
- **Poly-mod and LFO destinations:** `EnvFreq2`, `OscFreq2`, `EnvPw`, `OscPw`, `OscCutoff` (Pro-One style) and `LfoCutoff`, `LfoPw`; they add after the normals and the patch. On the Minimoog, which has no LFO, Osc 3 is the modulation source (#35, #36).
- **Per-model panels and palettes:** the view draws each synth from its model's description (`models.ts`, `SynthPanel.vue`) with that instrument's sections, control names and colours, a model picker and a preset list per model (#30, #34-#39).
- `make bench` has `six models` and `family worst` scenarios: 16 synths across the family at 3.9% and 5.0% of a core (#40).

## [0.5.0] - 2026-10-03

### Added

- Send effects: a stereo echo (up to 2 s in ms, feedback below 1, tone, ping-pong) and an 8-line Hadamard reverb (size, damping, pre-delay), fed by each synth's sends and returned into the master; nine global parameters (ids 70–78) and a Returns strip. `make bench` now runs the whole chain (#26).
- Drive insert on each synth's bus: Off, Overdrive, Distortion and Fuzz with first-order ADAA, plus `DriveMode`, `DriveAmount`, `DriveTone`, `DriveLevel` (ids 66–69, presets may set them) and a Drive row on each card. Spec 004 Req 11 (#25).
- Mixer: each synth has its own bus with `Level`, `Pan` (equal power), `EchoSend`, `ReverbSend` (post-fader), `Mute` and `Solo`, summed into a stereo master before the soft clip; a mixer row on each card. Parameter ids 60–65 (#24).
- Up to 16 Mono synths, each with its own parameters, values and patch, preallocated in `Engine::new`; `MasterGain` stays global. Each synth has its own live voice, and a voice keeps the synth its note started on. Loading a MIDI file puts its parts on synths 0, 1, 2…; each part picks a synth or mutes. New exports `synth_count` and `synth_reset`; `set_param`, `param_value`, `mono_preset`, `note_on`, `note_off` take the synth first, and `route(channel, synth)` mutes on an unknown synth. Spec 004 Req 10 (#19).
- The view: one card per synth, colour-coded, with **+ Synth** and × (removing one mutes its parts); the keyboard plays the selected synth, and a held key releases on the synth it started on. `make bench` sets up all 16 synths (3.3% and 5.8% of a core) (#19).

### Removed

- Wave and Drums (the 32-voice pool and first timbres in `voice.rs`, `Source` and its mirror, Wave's `Attack`/`Release` parameters, the Wave/Drums cards and pads), the algo pane, the arrangement pane and their demo model (`model/song.ts`). The demo MIDI file loses its drum part. ADR-0008: Mono only, straight to the ensemble (#18).

### Changed

- C ABI: `note_on(note, velocity)` and `note_off(note)` lose the source argument; `route(channel, target)` plays on Mono with target 0 and mutes with anything else; every channel starts playing. Parameter ids from `Vco1Wave` on move down by two (#18).
- Layout: transport, synths, MIDI player; plan.md goes from MVP 2 straight to MVP 5 (M2, M4, M5 deferred); spec 001 Req 3, 002 Req 9 and 003 Req 1, 3, 5 follow (#18).

## [0.4.0] - 2026-10-03

A playable Mono (plan.md MVP 2, epic #2, PR #17): note handling and routing.

### Added

- `mono::voice`: one monophonic Mono voice per owner (live input and each MIDI channel), allocated in `Engine::new` instead of the shared pool. A 16-key stack with note priority (last, low, high), legato (a new key while one is held keeps the envelope; release falls back to the next held key) and glide (0-5 s, linear in semitones, exact in time). `Priority`, `Legato` and `Glide` parameters with a mirrored `NotePriority` id list, and a Keys row on the Mono card (#8).
- `mono::patch`: normalled routing and an 8-slot patch (spec 004 Req 7). Normals with amounts: ADSR → cutoff (`EnvCutoff`), key tracking (`KeyTrack`), LFO × mod wheel → pitch (`Vibrato`, `ModWheel`), ADSR → VCA. Sources VCO 1-3, noise, ADSR, AR, LFO, S&H, mod wheel, velocity, key; destinations VCO 1-3 pitch, pulse width, cutoff, resonance, VCA, LFO rate. An override replaces the normals to its destination. 28 parameters (24 patch fields, 4 normals) and `ModSource`/`ModDest` ids, mirrored; LFO, AR, normal and patch rows on the Mono card. The AR, LFO and S&H are audible now (#9).
- Presets carry normals and patches: Sync lead's ADSR → VCO 2 sweep, Bowed string's vibrato and velocity → cutoff, filter envelopes and key tracking on Bass and Lead (#9).
- `PitchTable`: note to increment in 1/16-semitone steps, so Mono's pitch moves every sample with no `exp2` in `render` (about 0.003 cents of error) (#8).

### Changed

- plan.md: note handling and routing are back in MVP 2 (instruments first); MVP 5 is the ensemble and the score again.
- `make bench` plays its 16 Mono voices from a 16-channel MIDI file, since live Mono is monophonic now (2.2% and 3.9% of a core).

## [0.3.0] - 2026-10-01

The Mono voice (plan.md MVP 2, epic #2, PR #15). Note handling and routing move to MVP 5, Web MIDI input to MVP 11.

### Added

- Spec 004: the Mono voice (plan.md MVP 2, epic #2), Req 1-9 with measurable scenarios; spec 001 Req 3 notes Mono's own voices (#3).
- `mono::osc`: three VCOs for Mono (saw, pulse, triangle, sine) with coarse and fine tune, level, pulse width and hard sync of VCO 2/3 to VCO 1, band-limited by a BLEP table; 15 new parameters and a `Waveform` id list, mirrored in `params.ts`; VCO controls on the Mono card (#4).
- `mono::noise`: white (seeded xorshift32) and pink (Paul Kellet's filter plus a DC blocker) noise into the Mono mixer, with level and colour parameters and a mirrored `NoiseColour` id list; the drums share the generator (#5).
- `mono::ladder`: a 4-pole zero-delay-feedback ladder low-pass with cutoff, resonance (self-oscillating from 0.8) and drive, replacing Mono's one-pole; `g` from a table built in `Engine::new`, cutoff smoothed in pitch over 2 ms, input saturated by a rational `tanh`; three parameters and a Ladder row on the Mono card (#6).
- `mono::env`: ADSR and AR envelopes with RC-style curves, each segment taking its set time from where it starts (within 1 ms from 1 ms to 10 s); the ADSR is Mono's VCA, with sliders on the Mono card. `mono::lfo`: an LFO (sine, triangle, saw, square, 0.01-50 Hz) and a sample-and-hold on noise; eight parameters mirrored in `params.ts`. The AR, LFO and S&H get destinations with routing (#9) (#7).
- `mono::preset`: Bass, Lead, Sync lead and Bowed string, as Rust data over one `DEFAULTS` table that is also the voice's starting state; a preset picker on the Mono card. Exports `mono_preset`, `param_count` and `param_value`: the worklet reports every parameter's value at start and after a preset, and the Mono controls show the engine's values instead of TypeScript defaults. Presets set parameters only until routing (#9) (#11).
- `make bench` (`tools/bench.mjs`): 16 Mono voices on the built `dsp.wasm` in Node's V8, a realistic and a worst-case patch, against the 25% budget; parameter ids read from `params.ts`. A performance counter in the transport bar: DSP load (average and peak share of real time per audio callback) and sounding voices, from the worklet about twice a second; new export `active_voices` (#12).
- ADR-0007: BLEP-table oscillators instead of polyBLEP, which leaves aliases at −22 dB where spec 004 asks for −60 dB.

### Changed

- Scope of MVP 2 (plan.md): Mono note handling and normalled routing (#8, #9) move to MVP 5, Web MIDI input (#10) to MVP 11; MVP 2 ends with presets. Until routing, the LFO, AR and sample-and-hold have no destination. Profiling now names Chrome DevTools' WebAudio panel, since `chrome://webaudio-internals` is gone (ADR-0002).
- The master output soft-clips: unchanged below 0.5, never past ±1, NaN and infinity silenced (PR #15 review).
- Spec 004 Req 1: BLEP table instead of polyBLEP; the sync bound is ±1.5 (band-limited steps ring), plus a pulse-width sweep scenario.
- Mono output lags the other sources by 8 samples (0.17 ms), the BLEP kernel's half-width.
- Mono's amplitude follows its own ADSR (`Adsr*` parameters), not the test voice's `Attack`/`Release`, which now shape only Wave.
- Mono's filter sits at a fixed cutoff (4 kHz by default) instead of tracking the key; key tracking returns as a normalled connection with routing (#9).

## [0.2.0] - 2026-10-01

A MIDI file player in the engine, ahead of the plan's order (plan.md MVP 1b, spec 002 Req 9).

### Added

- `smf`: a total Standard MIDI File parser (types 0 and 1, running status, tempo and track names); malformed input returns an error, never a panic.
- `player`: a file compiled once to sample-timed events through the tempo map; a transport (play, stop, seek, position) run inside `render`.
- Per-channel routing to Mono, Wave, Drums or mute, channel 10 defaulting to Drums; a bad file keeps the loaded song.
- `voice`: a first timbre per source (Mono: polyBLEP saw through a key-tracked low-pass; Wave: sine plus second harmonic; Drums: kick, toms, snare and hats chosen by GM note).
- C ABI exports for loading, parts, events, transport and routing; the worklet and `web/src/audio/engine.ts` carry them.
- `web/public/demo.mid`, Pachelbel's Canon, written by `tools/make_demo_mid.py` (public domain, no third-party licence).
- `clippy.toml`: the panic lints are relaxed inside tests.
- The view: Demo, Open MIDI…, Play/Stop and the position in the transport; a parts pane with a source picker, piano roll, playhead and click to seek per part.

### Changed

- Spec 001 Req 1 names the player exports; spec 002 Req 8 records the parser, Req 9 the playback.

## [0.1.0] - 2026-09-30

The base: the pipeline from Rust to the speakers, and the plan from one mono voice to a true algo synth (plan.md MVP 1).

### Added

- `crates/dsp`: the engine as a `wasm32-unknown-unknown` cdylib with a plain C ABI and no imports (ADR-0001). One engine per wasm instance, planar stereo 128-frame blocks, a 16-voice test voice (table sine, AR envelope) with oldest-voice stealing, master gain.
- Parameter and source registries (`Param`, `Source`: Mono, Wave, Drums) mirrored in `web/src/audio/params.ts`, the mirror checked by `cargo test` (ADR-0004).
- Real-time rules as lints: no `unwrap`, `expect`, `panic` or indexing; `unsafe` only in `ffi.rs` (ADR-0002).
- `web/`: Vue 3 + TypeScript view with transport (power, master, scope), algo pane, instruments pane (keyboards, drum pads, computer keyboard) and bar arrangement, over a static demo song (ADR-0003, ADR-0005).
- `web/public/worklet.js`: the AudioWorklet shim.
- Podman + Caddy image serving static files on port 6340; Vite dev on 6341 (ADR-0006).
- Grouped `make help`, `make check`, CI with actions pinned by commit, `cargo deny`.
- `.openspec/`: vision, plan (five milestones, eleven MVPs, the Vivaldi ensemble as second base), ADRs 0001-0006, specs 001-003.
