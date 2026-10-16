# 0038: Envelope times held per note

**Status:** Accepted, amended (#688) · **Date:** 2026-10-16 · **Amends:** ADR-0023

## Context

Strudel's `note("c2 [eb3,g3]").adsr("[.1 0]:.2:[1 0]")` gives the root a slow swell and the chord a pluck in one clip (#651). Here `.attack("0.1 0")` already parses as a parameter method (#204, the `attack` alias of #298), but as ADR-0019 writes any signal: one value for the synth, evaluated at the start of each block. That makes three faults:

- **The step before.** A note on a step boundary inside a block reads the value of the step before, since the method was evaluated at the block's start and the voice takes its attack in the same block.
- **One sustain for all.** The voice reads the sustain on every block, so the next note's sustain changes a note still held.
- **The release of whoever is last.** A note's release time is the synth's when its key goes up, which is often the next note's.

`.onNote` (spec 010 Req 12) samples a signal at a note's exact tick, which mends the first fault. It still writes the synth's one ADSR, so the other two remain. ADR-0023 gave each voice of a Mono or Poly synth an override table, but only for continuous parameters (the cutoff, the levels), evaluated per block.

## Decision

**On a clip line, `.attack`, `.decay`, `.sustain` and `.release` are held per note. Each note samples them at its own tick, as `.onNote` does, and its voices keep the values for the note's whole life in ADR-0023's table, which gains the four envelope times.**

- **Which methods.** These are the parameter methods on `AdsrAttack`, `AdsrDecay`, `AdsrSustain` and `AdsrRelease`, by any of their names: `attack`/`att`, `decay`/`dec`, `sustain`/`sus`, `release`/`rel`, or the registry's. They take any signal, as before: a number, `"0.1 0"`, `"<0.5 0.01>"`, or `rand.range(0.01, 0.3)`. Written on a clip, they are sampled per note without `.onNote`. A `mod` line, an `auto` and a snapshot stay as ADR-0019 has them: one value for the synth, in time.
- **When a value is taken.** A value is taken at the note's tick, the clock's position when the note starts, so a sequence steps with the bar as in Strudel. Notes on one tick, such as a chord, share a value. A `~` leaves the note the value it would have had. A strummed note (#604) or a grace note (#605) is pressed after or before its tick, from the hit queue. It takes its tick's value when it is pressed, so the value lands on its own voices. Before this ADR, `.onNote` sampled such a note when it was queued, and wrote to the voices of the note pressed before it.
- **Held by the voice.** On a synth whose voices are Mono voices, the multisampler's (the samplers and General MIDI), LA's (the D-50's partial 1) or a Modular voice's amplifier ADSR (#688), the four times join ADR-0023's override table after the ten continuous parameters; the mask has room for them. Sampling writes them for the slots the note pressed, before the voice next renders. The voice reads them in place of the synth's:
  - attack and decay when its envelope starts;
  - the sustain while the key is held;
  - the release when the key goes up.

  A note keeps its own times while later notes take theirs. Times are held in seconds, and the voice turns them into samples. The model's rules still apply: its ranges and sustain ceiling (#340), and the Minimoog's decay standing in for its release (#308).
- **Other synths.** A synth whose voices read no ADSR of this kind, such as the D-50's partial 2 or a Modular code's own `Env`, keeps one envelope per synth. Where the model reads the parameter at all, the value is written to the synth at the note's tick, as `.onNote` does, and held until the next note.
- **Values checked at parse.** When the signal is plain numbers (a number or a sequence), each time must be 0 to 10 seconds and the sustain 0 to 1. Otherwise it is an error at the signal's column. A time of 0 is Strudel's "at once": it is clamped to the shortest, 1 ms, as a computed signal always is.
- **When the clip stops.** A note's times stay with it: a slot drops them when it takes its next note, which then samples its own or follows the synth's (#688). So a note let go as its clip leaves keeps its release, and the next clip's notes don't inherit the stopped clip's values.

## Consequences

- `render` doesn't allocate: values are sampled at a note's tick into the pool's fixed table, as `.onNote` does today. The table grows from 10 to 14 floats per voice.
- Values are not stored on the note events, as #651 first proposed. Sampling at the tick gives the same value for a written line, and it works for live and generated clips (`arp`, `bass`, `walk`), whose events are made as they play.
- A per-voice signal (`env`, a list) may now write the four times too. Since attack and decay are read at note start, `.attack([0.01, 0.2])` spreads a chord's attacks by voice slot.
- `.attack(0.01)` on a clip is unchanged: every note holds 0.01.

## Alternatives considered

- **Values on the note events.** This is a field per time on `notes::Event`, written by a new pattern method at load. The value would follow the line rather than the bar. But live and generated clips don't keep events, and new pattern methods would shadow the `attack` alias. Rejected.
- **A whole ADSR per voice slot outside the table.** That is the same data with a second mechanism for clearing and copying. Rejected in favour of the table ADR-0023 already copies per render call.
- **Leave it to `.onNote`.** That needs no new rule, but it still leaves the sustain and the release shared. Rejected.
