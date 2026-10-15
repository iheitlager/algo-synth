# 0027: Autocommit: the song follows the synths and the synths the song

**Status:** Accepted, amended by #522 and #527 (zones and pads fold) and #588 (the text stays as typed) · **Date:** 2026-10-07

## Context

ADR-0012 made the song text the source of truth, and ADR-0018 (Proposed) made it the whole session, with the setup file as an import. But the two still meet only by button. The text reaches the engine on Apply or Ctrl+Enter. A synth's sound reaches the text only through *Save as setting*, and the mixer through *Write mixer to song*. A library preset (ADR-0014) and an opened setup go straight to the synths. ADR-0018 says so on purpose: "A moved fader is not written back into the text by itself."

In use, that leaves the text and the sound drifting apart until someone remembers a button. The song says one thing, the synths play another, and a reload loses what was never committed. The user wants one state: change a setting and the song changes; change the song and the sound changes; no apply or save buttons in between. The setup file then has nothing left to save.

## Decision

**The engine folds live changes into the song, and the song text applies as it is typed. The commit buttons and Save setup go.**

- **Live changes fold into the song.** A parameter change marks its track or strip changed. About five times a second, outside `render`, the engine folds the changes in:
  - a track's sound becomes a `setting` named after the track (what *Save as setting* does today)
  - the mixer becomes `strip`, `group` and `master` lines (what *Write mixer to song* does today)

  It then reprints the song and sends it once. A knob drag gives a few song updates, not one per message. A library preset or an opened setup is a set of parameter changes, so it lands in the song the same way.
- **Driven values stay out.** A value a lane, a scene or a `mod` is writing is not folded into the text, so a moving lane never freezes into a number.
- **The text applies as it is typed.** *(Amended by #588, below: on a finished line, not on a pause.)* About half a second after typing stops, a text that parses is applied: at once when the song is stopped, from the next bar while it plays (ADR-0012, #208). One that does not parse shows its error and changes nothing. Ctrl+Enter applies at once.
- **Typing wins.** While an edit is being typed and not yet applied, folded changes wait. When it applies, they are folded in on top, so a keystroke is never overwritten by a knob.
- **The Modular code is the same.** A SynthDef builds as it is typed and goes into the track's setting (ADR-0024).
- **Every shown synth is a track,** with or without fragments, so the song holds every sound on screen. Adding a synth adds a track; removing one removes it.
- **Buttons that go:**
  - *Save as setting*, *Write mixer to song*, and the Apply buttons of the song text and the Modular code
  - *Save setup*: *Open…* still reads a `.synths.json`, as an import into the song (ADR-0018)
- **The view keeps the screen:** pane layout, collapsed and hidden strips, kept in the browser.

This supersedes ADR-0018's "A moved fader is not written back into the text by itself" and its *Write mixer to song*, and ADR-0015's two files.

## Consequences

- One state: what plays is what the text says, and a saved `.song` is the whole piece. A reload loses nothing that has a song form.
- The text gets busier while tweaking. The printer writes only values that differ from their preset or default, as today.
- Folding runs outside `render` (ADR-0002) and throttled; the engine already has the pieces (`add_setting`, `write_mixer`).
- What has no song form yet stays live-only, marked in the view, until it gets one: arpeggiator settings, sampler zones and pads, and loaded samples (ADR-0018's `samples`). Sampler zones, pads and their samples got one in #522 and #527 (ADR-0033's `zone` and `pad` lines). The same goes for a sound needing more changes than a setting holds (32).
- Specs change:
  - 002: the session folds back
  - 003: Req 5 drops Save as setting, Req 7 becomes the import only, Req 15 drops the Modular Apply
- Tracked in epic #362: #356 to #361.

## Amendment (#588): the text stays as typed

Applying on a pause played half-typed words, and the answer to an apply put the printed song in the editor: lines moved, defaults appeared, blank lines and indents went, under the caret.

- **The song text applies when a line is done:** on Enter, when the caret leaves a line it was typed on (arrow keys, a click, leaving the editor), or on Ctrl+Enter. A pause applies nothing. Folding stays held while an edit is not applied, so a knob waits for the line. The Modular code still builds on a pause.
- **An apply never rewrites the text.** The engine keeps the text as typed when the song it loaded is what the text says. When loading changed the song (a kit picked from the rack), it keeps the printed text, which says so.
- **An edit from outside the editor writes only what it changed.** A step click, a piano-roll edit, a fold or a track edit leaves the text alone when the song stays as the text says. Otherwise it rewrites only the lines of the items it changed, as the engine prints them (`song::splice`), keeping their indent and side comment. A new item goes after the item before it, and a gone one is taken out. Items are what the comments pass names: a top-level line with a Modular setting's code, and each lane or note line of a clip. When the result would not parse back to the song, the whole song is printed instead. A rewritten line is canonical within itself: `x... x...` comes back as `x...x...`.
- **Format prints it all:** a Format command (Shift+Alt+F, or the composer's button) prints the whole text as the engine writes it, applying the draft first. It is the only way the engine reprints the whole text on purpose.
- **The words that play light up in the text as typed.** A clip's lines are found by their words. A note line written otherwise than it prints is matched word for word, and lights nothing when its words don't line up.
