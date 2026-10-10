# 0031: Ableton's words: clip, scene, snapshot

**Status:** Accepted · **Date:** 2026-10-10 · **Amends:** the terms of [0015](0015-the-arrangement.md) and [0018](0018-the-song-is-the-session.md)

## Context

ADR-0015 named the song's parts in its own words:
- a **frag**: one track's loop;
- a **section**: bars and the frags, autos and scenes that play in them;
- a **scene**: a mixer recall, values set together on a section's first step.

The Launch epic (#491) brings Ableton's Session view to the composer: scenes launched live from the computer keyboard, then from an Akai APC mini mk2. In Ableton a **clip** is one track's pattern and a **scene** is a row of clips launched together. Those are our frag and our section. Our "scene" means something else, a mixer snapshot, which Ableton has no word for. Two vocabularies would leave "scene" meaning two things in one view.

## Decision

**The song, the engine, the app, the assistant and the docs speak Ableton's words (#486):**

| Before | Now | Is |
|---|---|---|
| `frag` | `clip` | one track's loop |
| `section` | `scene` | bars and the clips, autos and snapshots that play in them; what Launch starts |
| `scene` | `snapshot` | mixer and parameter values set together on a scene's first step |

`arrange`, `loop`, `auto`, `mod`, `strip`, `group` and `master` keep their names. Mixer parameters (`P1`–`P4`, `Send1`–`Send4`) keep theirs too, since saved presets and setups name them.

- **The printer writes only the new words** (canonical form, ADR-0012). The parser, the comment keeper and the highlighter also read the old ones (`song::current_keyword`):
  - `frag` reads as `clip`;
  - `section` reads as `scene`;
  - `scene <name>: …`, a name and a colon with no bars, reads as `snapshot`. A scene always has bars (`scene <name> <bars>: …`), so the two cannot be confused.
- **A song written before reads as the same song,** and becomes current when it is printed.
- **Identifiers follow the words:** `Clip`, `Scene`, `Snapshot`, the FFI and the messages between the threads. The words that mean something else keep them:
  - a synth faceplate's sections;
  - the mixer's master section;
  - "clip" for a signal over full scale.

## Consequences

- One vocabulary from the song text to the Launch view and an APC grid. Someone who knows Ableton reads a song at once.
- Old songs, autosaves and assistant answers in the old words still load. Nothing writes those words any more.
- `CHANGELOG.md`, the changelog fragments and the ADRs before this one keep the words of their time.
- "Clip" now also names a song's loop beside a signal clipping (the gate's `clip` finding). The context tells them apart.

## Alternatives considered

- **Keep the words and call the live view "Launch" only.** No migration, but "scene" would mean a snapshot in the text and a launched row on the APC.
- **Rename only the user-facing words.** A smaller diff, but every reader of the code would translate between `Frag` and clip for good.
- **`snap` or `recall` for the snapshot.** Shorter, but `snapshot` says what it is in `[snapshot]` lists and in the UI.
