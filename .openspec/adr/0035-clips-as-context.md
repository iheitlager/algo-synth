# 0035: Clips as context — harmony and rhythm from a named clip

**Status:** Accepted · **Date:** 2026-10-15 · **Extends:** 0016, 0019

## Context

A song's harmony is written again in every clip that follows it. Three clips over the same two roots each carry their own `.scale(<a1 d1>/4 minor-pentatonic)`, and the copies drift: one clip's D sits an octave from another's, and changing the progression means editing every line. Live, that is the edit you make most.

Generators already read an earlier clip by name (`arp(prog,up,16)`, `root(prog)`, `markov(1,riff,3)`, `mutate(riff,30,5)`, ADR-0016). Pattern methods (ADR-0019) cannot: their arguments are numbers, roots and modes. Other systems keep *what the harmony is* apart from *which note of it to play*: SuperCollider's `\root`, `\degree` and `\octave` event keys and `Pchain`; Strudel's `n("0 1 2").chord("<Am7 Dm7>").voicing()`; FoxDot's `var` and players that follow players; Live 12's song-wide scale. Epic #594 builds on that.

## Decision

**A pattern method may read an earlier clip for its harmony or its rhythm. The clip is the context; nothing else names one.**

- **What can be read.** The harmony: `.scale(<clip> <mode>)` takes its roots from the notes the clip plays (#591), and `.chord(<clip>)` makes degrees count the chord's own notes (#592). The rhythm: `.struct(<clip>)` (#600). A clip's notes at a moment are those starting at the latest start at or before it, lowest first; the lowest is its root, in its own octave.
- **Which clip.** An earlier note clip, by name, as for generators: one written later, or the clip itself, is a parse error with a line and a column. The source is the earlier clip's events after its own methods. A live clip as a source gives its first bar, as it does to a generator. Where a word could be a clip or a pitch, a clip name wins, as in `arp`.
- **Phase.** A follower repeats until it and its source come round together (the least common multiple of their lengths, as `.scale(<a1 d2>/4 …)` does with its progression), at most 64 bars. Each note looks the source up at its own start, modulo the source's length; before the source's first note in its cycle, the chord sounding is the source's last, since it loops. A follower and its source start together (the song's first bar, or a scene's), so this is the song bar while both play. A follower in a scene without its source still follows the source as written.
- **Live clips.** A live clip's pitch methods (`.scale`, `.chord`, `.oct`, `.add`, `.sub`, #593) look the source up at `cycle × length + local`, in `step_live`, on the bar it has just generated: a scan of the source's events, nothing allocated (ADR-0002).
- **An edit.** Each apply recompiles the song: a source and every clip that follows it switch at the same bar line, so one edit to the harmony changes the whole song in step.
- **Still a closed set** (ADR-0019): no variables, no `let`. The only names are clips, which the song already has.

## Consequences

- Pattern methods get the clip sources that generators have (`song.rs`, the `srcs` closure). The methods that store a source keep its events, as `Gen::ArpProg` does, so they can apply again without looking it up.
- A song can keep its harmony in one clip that also plays (a pad, a pedal note) and change it with one edit, live.
- A chain of clips reading clips makes an apply cost more; #589 measures it.
- The highlighter and the autocompleter (#595) offer clip names where a source goes.

## Alternatives considered

- **A progression on the song's `scale` line.** One place too, but one harmony for the whole song: a scene could not change it, and a clip could not play it. Rejected.
- **Variables (`let harm = <a1 d1>/4`).** Familiar from Strudel and SuperCollider, but a second kind of name beside clips, against ADR-0019's closed set. Rejected.
- **Each clip its own harmony (as before).** No new rules, but the copies drift and a key change is an edit per clip. Kept: a clip may still write its own `.scale(…)`.
- **Look the source up at the follower's local bar.** Simpler, but a follower and a source of different lengths drift apart, and an edit to the source lands out of phase. Rejected.
