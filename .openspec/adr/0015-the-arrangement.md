# 0015: The arrangement: sections, scenes, automation and MIDI as notes

**Status:** Accepted · **Date:** 2026-10-04

## Context

ADR-0012 made the song a text the engine parses and plays, and so far the song holds drum fragments that all loop at once. Spec 002 Req 4 asks for an arrangement of sections; the user wants to chain drum steps, synth lines and hits, samples and MIDI files in one song, and to move any parameter over time, the mixer's included (epic #174). The MIDI player is still a second, separate timeline (spec 002 Req 9), and the M32 epic proposed scenes for the mixer (#145).

## Decision

**One arrangement in the song text plays everything: fragments of every kind are chained in sections, parameters move by scenes and automation lanes, and a MIDI file becomes notes in the song.**

- **Sections and the arrangement.** `section <name> <bars>: <fragments…>` names what plays for how long; `arrange <sections…>` gives their order, repeats by name. A fragment starts at its section's first bar and loops inside it, cut at its end. A loop region repeats a range of bars. A song without `arrange` plays every fragment as a loop, as before.
- **Tracks by source.** `drums` (lanes of pads), `synth` (notes), `sampler` (lanes of pads on a pad sampler, notes on a multisampler) and `midi` (notes converted from a file). The kind sets what a fragment on the track may hold.
- **Scenes.** `scene <name>: <target>.<Param> <value>, …` sets values on the first sample of a section that lists `[name]`. A scene is a jump, not a crossfade.
- **Automation lanes.** `auto <name> = <target>.<Param> <values> /<length>` is a fragment of values: stepped (a sequence of values, one per step) or a ramp (`ramp a b`), looping like any fragment and placed in sections like one.
- **Every parameter, by name.** A target is a track (its synth), `strip N`, `group N`, a processor `P1`–`P4` or `master`; the parameter is its registry name (ADR-0004). The engine resolves names when it parses the song and applies values at block rate through the same path `set_param` takes, so coefficients are computed per change and nothing allocates in `render` (ADR-0002). The view follows, as it does after a preset.
- **Precedence.** Within a block, a scene is applied first, then lanes in text order; a later write wins. A knob moved by hand holds until the next automated value for that parameter.
- **MIDI files become notes.** Importing a file writes a `midi` track per channel and note fragments in the song text, with sections of a set number of bars and the file's tempo. Notes are written with classic durations when they fit the grid and with exact positions (`@bar.beat.tick`) when they don't. The text is then the song, editable like any other; the MIDI player retires into the import.
- **Two files.** The song (`.song`, text) says what plays when; the setup (`.synths.json`, ADR-0014's neighbour) says what the synths and the mixer start as. The song names synths by their strip names (#127), so renaming in the view and editing the text agree.
- **The arranger** replaces the MIDI player pane: tracks as rows, sections as columns in order, fragments as blocks; every edit is a message to the engine, which prints the text back (ADR-0012).

## Consequences

- One clock and one text cover beats, lines, samples, imported scores and mixer moves; there is no second timeline.
- Scenes in the song cover the M32 epic's scenes (#145) for songs; a manual scene recall outside a song, if wanted, is a small view feature on top.
- Automation resolves names once per parse; a lane's cost in `render` is a value lookup and, when the value changes, a parameter update.
- A converted MIDI file can be long text; exact positions keep unquantised playing, at the price of a less readable fragment.
- Moving a knob that a lane automates is overwritten at the lane's next value; the view may mark automated controls.

## Alternatives considered

- **A MIDI clip referencing the file.** Exact and short, but the song would depend on a second file and the notes could not be edited. Rejected for conversion.
- **Scenes only.** Simpler, but no sweeps or fades. Rejected; scenes and lanes share one mechanism.
- **Automation recorded per sample or per step in the engine's own format.** Faster to play, unreadable as text. Rejected; values live in the song.
- **One project file holding song and setup.** Fewer files, but patches and the mixer are JSON and the song is notation; mixing them makes both harder to read and diff. Rejected for two files.
