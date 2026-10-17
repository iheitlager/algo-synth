# 0047: A track names its model and preset, a setting holds a patch in the song

**Status:** Accepted · **Date:** 2026-10-17 · **Extends:** 0012, 0015 · built in #210 (PRs #211, #212)

## Context

ADR-0015 split a piece into two files: the song (`.song`) says what plays when, the setup (`.synths.json`) what the synths and the mixer start as. A `track` line named only a kind (`drums`, `synth`, `sampler`), so the song could not say what a track sounds like. A song pasted into a fresh composer, or written by a language model (ADR-0012), played on whatever synths happened to be in the rack.

#210 asks the song to choose: a track names its model and preset, and when it does not, the composer picks them from the track's role and writes them into the text. Picking a sound and tweaking it are two steps, so the song also needs a place for a tweaked patch.

## Decision

**A `track` line names its synth after its kind, as a model and a factory preset or as a `setting`, a patch that lives in the song; left out, the model and preset are picked from the track's role and printed.**

- **The track line.** `track <name> <kind> [<Model> [<Preset>] | <setting>]`. A model that does not play the kind, a preset of another model, or an unknown model or setting is a parse error. With a model alone the preset is the role's preset on that model, else the model's first.
- **The setting line.** `setting <name> = <Model> <Preset>: <Param> <value>, …` is a factory preset and up to 32 changes to it, written before the tracks; `track lead synth nile` plays it. A setting changes the synth's own parameters: `Model`, strip and global (master) parameters are a parse error. A setting is not named as a model.
- **The role.** A `drums` track is drums. Otherwise the name says it (`bass` `sub`; `pad` `chord` `string` `choir`; `arp` `pluck` `seq`; `key` `piano` `organ`; `lead` `melod` `solo`), else the notes do: only `arp(…)` clips make an arp, any chord a pad, a median note below C3 (MIDI 48) a bass, anything else a lead. Each role has a default preset (`MiniBass`, `JunoPad`, `ShArp`, `FmElectricPiano`, `ProLead`); drums get `Kit808`, or `Kit909` when the name holds `909`. A `sampler` track without a model keeps the samples it has. The rules live in `song.rs` (`Role`, `role`, `pick`).
- **Printed.** The canonical printer writes the picked model and preset, so a printed song says what it plays and plays the same when loaded again. (Since #588 an apply leaves the typed text as typed; the pick shows when the song is next printed.)
- **A synth of its own.** On load each track with a patch is routed to its own synth: a free one already on the patch's model (a sampler keeps its samples), else the first free one. The patch is set on it.
- **Set again only when the text changes it.** A reload sets a track's patch only when the track is new or its model, preset, changes or code differ from the last load, so a knob turned by hand survives an Apply of unrelated text.

## Consequences

- A song carries its sounds: a `.song` alone gives each track the model and patch it was written for. Half of a setup moved into the song; ADR-0018 moved the rest and made the song the session.
- A short song still sounds sensible: `track bass synth` plays a bass, and the text shows which.
- The editor's lexer colours `setting` as a keyword (#212). Spec 002 Req 11 holds the rules; `.openspec/language.md` § track and `.openspec/sound-design.md` § setting document them.
- Built on since:
  - ADR-0018: a song may name a factory preset or a `setting`, never a library preset (ADR-0014).
  - ADR-0027: a track's live sound folds into a `setting` named after the track; the arpeggiator's parameters fold in too (ADR-0048, #228).
  - ADR-0037: a setting's parameter its model never reads is a parse error.
  - #327: a MIDI import sets each part's patch on the synth it routes the part to.
  - Spec 002 Req 11: a picked drum kit plays a free kit already in the rack (an 808, a 909, a pad sampler) and the text names it.

## Alternatives considered

- **Patches only in the setup file (ADR-0015's two files).** The song would stay pure notation, but it would not play the same without its setup, and a language model would have to write two files in two notations. Rejected; ADR-0018 later folded the setup in altogether.
- **Library presets by name in the song.** Short, but the library is one browser's (ADR-0014); the song would not play elsewhere. Rejected: a song names a factory preset or holds the values in a `setting`.
- **A model required on every track.** Explicit, but a quick sketch or a model-written song would need a preset name for every line before it played. Rejected for the role pick, which is printed, so the text still ends up explicit.
