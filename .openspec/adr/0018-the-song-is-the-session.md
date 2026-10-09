# 0018: The song is the session

**Status:** Proposed · **Date:** 2026-10-05

## Context

ADR-0015 split a piece into two files: the song (`.song`, what plays when) and the setup (`.synths.json`, what the synths and the mixer start as), and ADR-0012 kept setups as JSON. Since then #210 let the song pick each track's model and preset and hold its own patches (`setting`), so half of a setup already lives in the song. The other half does not: the song cannot give a strip its starting level, pan, sends, inserts or `Out`, cannot set the master or the processors, cannot name a group, and cannot say which samples a sampler track plays. Scenes and lanes reach those parameters, but only inside an `arrange`, and only as changes.

The setup file is also parsed and planned in TypeScript (`setup.ts`: `parseSetup`, `applyPlan`), which bends ADR-0001: the view decides what state the engine gets. And the user wants the composer language, in the spirit of Strudel and SuperCollider, to describe the whole instrument: one text that a person, a diff or a language model can read and that sounds the same when loaded anywhere.

ADR-0015 also says the song names synths by their strip names (#127); the code names them by track names. The two namespaces drift.

## Decision

**A song text describes the whole session: tracks and their synths, the mixer, groups, master and processors, and the samples it plays. The setup file becomes an import.**

- **Mixer lines.** `strip <track|stripN>: <Param> <value>, …`, `group <n> [<name>]: <Param> <value>, …` and `master: <Param> <value>, …` give starting values by registry name (ADR-0004), `Out` and insert and processor types included, by their type names (`I1Type Overdrive`, `P1Type Echo`, `Out group2`). The engine resolves them at parse time, as it does for scenes; a bad name or value is an error with a line and a column.
- **Applied like settings.** On load the engine sets these values, and on a reload it sets again only those whose text changed (as #210 does for settings), so a fader moved by hand survives an Apply of unrelated text. A moved fader is not written back into the text by itself; *Write mixer to song* prints the current mixer as these lines, through the engine.
- **One namespace.** A strip is named by its track and a group by its `group` line; the view shows those names. Strips without a track keep the default names (`Synth N`, #177). This replaces ADR-0015's "the song names synths by their strip names".
- **Samples by name.** `samples <track> <id>` names a pack or kit of the sample manifest. The engine cannot fetch (ADR-0013), so it records the wish and reports it; the view fetches and loads the files into slots, as it does today when a pack is picked, and the engine binds the slots to the track. A song whose samples are missing still plays, silent on that track, and says so.
- **The setup file is an import.** Opening a `.synths.json` writes the matching lines into the song, as a MIDI import writes notes (ADR-0015); old files keep opening. Saving writes the song. The parsing and planning move from `setup.ts` into the engine.
- **What stays in the view:** pane layout, collapsed and hidden strips, colours, and the MIDI player's channel routes. They are about the screen, not the sound.

## Consequences

- A `.song` alone reproduces a piece: synths, patches, mix and samples. A language model gets and returns one text.
- The engine gains a mixer section in the parser and printer, and `setup.ts` shrinks to the view-only state and the import's file handling, which brings the setup back under ADR-0001.
- Spec 002 gains a requirement for the session lines. Spec 003 Req 7 (setups) is rewritten as an import, and its migration rules stay in force for old files.
- A song is longer: a full mix is a few dozen lines. The canonical printer writes only values that differ from the defaults, so a plain song stays short.
- Presets (ADR-0014) are not changed: the browser library still stores synth, insert and strip presets by name, and a song may name a factory preset or a `setting`, never a library preset, so a song never depends on one browser's library.

## Alternatives considered

- **Keep two files (ADR-0015).** Clean on paper, but the song already holds patches (#210) and the split leaves the mix outside the language. Rejected.
- **Embed the JSON setup in the song text.** One file, but two notations in it and the JSON still parsed in the view. Rejected.
- **Write every hand change back into the text at once.** The text would then always match the knobs, but every fader move would rewrite the song, and the text the user is typing would change under them. Rejected in favour of an explicit *Write mixer to song*.

## Later decisions

- **Taken further by ADR-0027:** the explicit *Write mixer to song* rejected above went too; the engine folds live synth and mixer changes into the song. The rest stays proposed until #214.
