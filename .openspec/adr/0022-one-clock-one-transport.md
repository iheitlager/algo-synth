# 0022: One clock, one transport

**Status:** Accepted, amended by 0029 · **Date:** 2026-10-06

## Context

The app had two transports. The top bar's Play, Stop and seek drove the MIDI player (spec 002 Req 9): a `Sequence` compiled from the file, its own sample position, and channel routes from each MIDI channel to a synth. The composer and the arranger each had their own Play and Stop, driving the song's clock (spec 002 Req 5). Nothing tied the two: both could run at once over the same synths, and the top bar was disabled unless a MIDI file was loaded, so the main transport did not play the main thing.

ADR-0015 already said that the MIDI player retires into the import and that there is one clock with no second timeline, and ADR-0018 made the song the whole session. MIDI import (#173) turns a file into song text. Only the player itself was left (#283).

## Decision

**The song's clock is the only clock and the top bar's transport the only transport. A MIDI file is an import.**

- **One transport.** Play goes on from where the song paused or stopped, Pause holds the place, and Stop goes back to the top (`song_play`, `song_pause`, `song_stop`). The top bar shows the song's position as bar, step and section. The composer and the arranger have no transport of their own; the arranger keeps its playhead and click-to-seek.
- **No MIDI player.** The engine's `Sequence`, `play`, `stop`, `seek`, the channel routes and `Owner::Channel` go. The analog-variance seed keeps the offset of the old channel owners, so song voices sound as they did.
- **A MIDI file is imported when opened.** Its tracks, one per channel in channel order, go to synths 0, 1, 2…, as loading the file into the player did. A setup picked with the file applies once it is imported.
- **The view keeps no MIDI state.** The player pane, the per-file session, the channel routes in setups and the lane names go. An old setup with `routes` still opens, with a notice that they are ignored. This amends ADR-0018's "what stays in the view": the MIDI player's channel routes are no longer there.
- **Amended by ADR-0029:** one clock per instance. With decks, the main engine's clock leads and each worker deck's song clock follows its tempo and starts on a block it names.

## Consequences

- One timeline: nothing plays that the song does not, and every pane shows the same position.
- A MIDI file plays only as well as it imports. The import keeps the first tempo only, snaps to 48 ticks a bar, assumes 4/4 bars, ends the song at the last note's start and is bounded by the song's limits (tracks, fragments, sections, bars). The player played any file as it was. Better import fidelity is a follow-up, not a reason to keep a second player.
- Stop no longer works as a pause, as the player's did; Pause does that now.
- The bench (`make bench`) plays its held notes as a song routed one track per synth, so it still measures the same load.
- Spec 002 Req 9 is retired and Reqs 1, 5 and 8 change; spec 003 loses the MIDI player pane, lane names and per-file sessions; specs 001, 004 and 007 drop their references to the player.

## Alternatives considered

- **Keep the player beside the song, and only make the top bar drive the song.** One less button, but still two clocks in the engine and a second timeline that nothing should use. Rejected.
- **Keep the player for files that do not import well.** It would hide import bugs instead of fixing them. Rejected.
- **Make Stop a pause.** A drum machine's Stop goes back to the top, and the song's always has. Kept, with a Pause beside it.
