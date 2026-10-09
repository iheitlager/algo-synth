# 0026: Drum hits on their lane's grid, queued a step ahead

**Status:** Accepted, extended by #242 · **Date:** 2026-10-07 · complements ADR-0016 (note events on a tick grid), which stays as it is

## Context

Drum lanes fired once per clock step, a sixteenth: `play_step` played every lane's step `k` there, and the only grid was `/16`. Drummers' rolls need more (#353): triplets (`/12`, `/24`), thirty-seconds (`/32`), and flams and drags, whose soft grace strokes fall 15–30 ms *before* the hit. Note frags have a finer grid, 48 ticks a bar (ADR-0016), but it cannot hold a thirty-second (1.5 ticks) and nothing in the engine could fire before its tick.

Raising the clock to 96 or 192 ticks a bar would give `/32`, but it changes the note grid, the arp's rates, the piano roll and dozens of tests that count ticks, for a need only drum lanes have.

## Decision

**A drum lane keeps its own grid, placed between the clock's steps; hits that fall between steps wait in a fixed queue for their sample.**

- A drum frag keeps its grid (`/12`, `/16`, `/24`, `/32`, `/48`). Step n of a lane on `g` steps a bar lies at n·16/g clock steps. A hit on a clock step plays when the step fires, as before, so `/16` lanes are unchanged. A hit between steps is placed between the two steps as a tick between steps is (`Clock::between_sample`), so swing moves it with its step.
- The hits between steps wait in a queue of 256 in the engine, filled when a step fires and emptied as the clock reaches them. The render loop splits its blocks at the next queued hit as it does at steps and ticks, so every hit lands on its exact sample. Nothing allocates; a full queue plays a hit at once rather than drop it.
- When step `k` fires, the engine also looks at step `k + 1` and queues the grace strokes of its flams and drags that fall before it. Graces shrink together to fit after the lane's hit before and the clock step before, so they stay in order however fast and fine the lane. The look-ahead reads the song as it is at step `k`, so at a bar where a new song takes over, the downbeat's graces come from the old one.
- The note grid stays at 48 ticks a bar.

## Consequences

- Drum lanes can be written on triplet and thirty-second grids, with ghost notes, flams and drags (spec 002 Req 3), and they stay in time with note frags.
- A flam on the very first step of play has no step before it to sound its grace in.
- Note frags still cannot play thirty-seconds; if they need them, the tick grid can be raised later without touching drum lanes.

## Later decisions

- **Extended (#242), ratchets:** a digit 2–4 after `x`, `X` or `o` plays the hit that many times, evenly across its step's span. The first hit stays where the step puts it; the others go through the same queue between steps, so they land on their exact samples on every grid and move with swing. Graces stay before the first hit.
