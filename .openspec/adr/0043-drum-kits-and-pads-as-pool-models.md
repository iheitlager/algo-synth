# 0043: Drum machines and the Pad Sampler as pool models

**Status:** Accepted · **Date:** 2026-10-17 · **Extends:** 0009, 0010, 0011

## Context

ADR-0008 removed the Drums source to reach the ensemble, and ADR-0012 brought drums back as "a source of synthesized analog pads". By then a synth slot held a model (ADR-0009) and a voice pool (ADR-0011), and the mixer had strips and groups (ADR-0010). A separate source would have needed its own strip, its own MIDI routing and its own keyboard path.

The decisions were made across several issues and never recorded together:

- #114 (PR #131): the TR-808 kit in a synth slot, as voices of its pool.
- #140 (PR #157): the 808's sixteen voices and a re-voiced cowbell.
- #148 (PR #191): a TR-909 beside the 808, on the same pads.
- #124 (PR #155): a pad sampler after an Akai MPC, sixteen pads of samples.
- #162 (PR #182): individual outs, each pad of a kit to its own group, as on the 808's back panel. Egyptian Lover EQ'd the 808's snare and clap on channels of their own.

## Decision

**A drum machine is a `Model` on a synth slot whose pool plays pads instead of notes. The pads are in `crates/dsp/src/drums.rs` and `drums/pads.rs`, the models in `synth/tr808.rs`, `synth/tr909.rs` and `synth/padsampler.rs`, the pool's drum paths in `poly.rs`, the sampled pads in `padsampler.rs`.**

- **A model, not a source.** `Model::Tr808` and `Model::Tr909` run on `Engine::Drums(Machine)`, `Model::PadSampler` on `Engine::Pads`. A kit slot routes through the mixer like any synth (level, pan, sends, inserts, `Out`), plays a MIDI file's channel 10 when routed there, and plays from live keys. The model picker lists all three under Drums.
- **A pad is one voice, not a note.** `PolyVoice::Drum(PadVoice)` and `Pool::hit` bypass polyphony, unison and the key stack. A key plays the pad `Pad::from_gm` gives it: General MIDI's drum map, and every other key by its place in the octave from 36, so any key sounds. A pad hit again retriggers the voice already playing it. There is no note-off: `note_off` returns at once. The pool's voices, owners and global budget still apply; `adds_a_voice` counts a new pad only.
- **Choke and accent.** A closed hat chokes the open hat. A hit at `ACCENT_VELOCITY` (0.9, MIDI 115) or above is accented by `DrumAccent`.
- **All synthesized.** Every voice of both machines is built from oscillators, noise, resonators and filters in `pads.rs`. No ROM and no sample ships. The 808's cowbell is two BLEP squares at 540 and 800 Hz through a band-pass near 850 Hz (ADR-0007, #140).
- **One pad set, two machines.** `Pad` has eighteen pads: the 808's sixteen and the 909's `cr` and `rd` (#148). Each pad has tune, decay, tone and level parameters (`BdTune` …), plus `DrumAccent`; the kick also has `BdDrive` (#264). `drums::Machine::voice` gives each machine a stand-in for a pad it lacks: on the 909 `cl` and `cb` play `rs`, `ma` plays `ch`, the congas play the toms, `cy` plays `cr`; on the 808 `cr` and `rd` play `cy`. The stand-in's knobs and out apply. So one beat plays on either machine.
- **The Pad Sampler on the same terms.** Sixteen pads answer notes 36–51 (`FIRST_NOTE`), plus the four notes ADR-0036 adds. `Pool::hit_pad` plays one voice per pad, retriggered, and a hit chokes the other pads of its choke group (1–8). Each pad has a sample, tune, level, pan, decay, choke group, velocity to level and to start, a one-shot switch and an `Out` (`PadField`), set through `pad_set` and `pad_get` and mirrored in `params.ts` (ADR-0004). A pad that is not a one-shot fades on note-off. The voice writes both sides, so the slot has a stereo bus and its strip balances.
- **Individual outs.** Each kit pad has `Out` (0 the kit's strip, 1–8 a group) and `Pan`: `BdOut`/`BdPan` … `HcOut`/`HcPan` (438–469), `CrOut`/`CrPan` and `RdOut`/`RdPan` (#162, #148). `Pool::render_kit` pans each pad by equal power into the kit's stereo bus or into its group's `direct` input in `Mixer`, ahead of the group's inserts. A routed pad bypasses the kit's strip, inserts, fader and mute. Solos follow the direct feeds (`Mixer::set_feeds`). The Pad Sampler's `out` field does the same per pad (#220), panned by the pad's own pan.
- **Routing is in the song.** #162 kept per-pad routing out of the song text as mixing. ADR-0018 and ADR-0027 reversed that: the outs are synth parameters, so a `setting` line or the folded synth state carries `BdOut 3`, and a `pad` line carries `out 3` (ADR-0033). A pad's group counts as a group the song uses (#568).

## Consequences

- Everything a synth slot has, a kit has: mixer strip, sends, inserts, MIDI routing, live keys, presets, settings and automation by name.
- Drum lanes (ADR-0012, ADR-0026) play any of the three models by General MIDI note; ADR-0036 extends that to every Pad Sampler pad.
- Pad ids are appended, never renumbered: the 808's new voices after `DrumAccent` (398–429), the outs after the sends (438–469), the 909's pads after (471–482). Old songs load with every pad on the kit's strip.
- `render` allocates nothing: a pad is plain `Copy` data, the direct inputs and the kit's scratch block are allocated in `Mixer::new` and the pool, and pan gains are worked out when set (ADR-0002).
- The pool has sixteen voices and the kit eighteen pads; a hit beyond sixteen ringing pads steals one.
- The 909 is partly sampled hardware; its hats, crash and ride here approximate that character by synthesis.
- Other drum kits followed the same pattern on their own engines: `NesKit` (ADR-0034) and `VlKit` (ADR-0040).

## Alternatives considered

- **A separate Drums source** (ADR-0008's removed design, ADR-0012's wording): a second strip type, MIDI path and key path for one instrument. Rejected for a model on a slot.
- **ROM samples for the 808 and 909.** Ships binary data and ties the sound to one recording; tune, decay and tone would not reach inside a hit. Rejected for synthesis.
- **Roland 808/909 samples on the Pad Sampler.** Their provenance and licence are unclear (#130); kits ship only with a verified licence. Rejected.
- **Pads as polyphonic notes.** Two hits of one pad would overlap, and note-offs would cut one-shots. Rejected for one retriggered voice per pad.
- **A second kit for pads that need their own processing.** Doubles the knobs and splits one beat over two slots. Rejected for per-pad outs into a group's direct input.
