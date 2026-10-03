# 0010: Mixer topology: strips, group buses and insert slots

**Status:** Accepted · **Date:** 2026-10-03

## Context

The mixer (ADR-0005, spec 002 Req 2) has one strip per synth, four sends into four effect processors, and a fixed master chain (equalizer, compressor, limiter). The drive insert is wired into each strip. Two things are missing: a way to group strips (a string section, the leads) so they share a fader, sends and processing, and a way to put an equalizer or compressor on a single channel without making every strip wider and every parameter list longer.

Both must stay inside ADR-0002: nothing allocates after `Engine::new`, nothing is computed per sample that can be computed on a parameter change, and the registry (ADR-0004) stays the one list of names and ids.

## Decision

**Strips and groups share one index space and one set of parameters; routing is a parameter; processing is a fixed number of insert slots whose type is a parameter.**

- **Addressing.** Strip indices 0–15 are the synths. Indices 16–23 are eight group buses. A strip parameter (`Level`, `Pan`, `Send1`–`Send4`, `Mute`, `Solo`, `Out`, the insert parameters) is the same id on every strip and is addressed as (strip, parameter), as `Level` is today. Synth parameters keep stopping at index 15. The engine's value table has a row per strip.
- **Routing.** `Out` says where a strip goes after its fader and pan: 0 is the master, 1–8 is group 1–8. A group may route only to the master or to a higher-numbered group, so the graph has no cycles and one pass in index order renders it: strips, then groups in ascending order, then the master chain. A route that is not allowed is ignored and the previous one kept. Strips are mono until the pan; a group is a stereo bus.
- **Sends** are post-fader on strips and on groups, into the same four processors P1–P4.
- **Solo** keeps a soloed strip audible through every group it passes through; a group stays audible when it is soloed or any soloed strip feeds it. Mute on a group silences what it carries.
- **Insert slots.** Each strip and each group has three insert slots, in series, before the fader. A slot has a type, `Type`, and five knobs A–E in 0..=1 whose meaning depends on the type (the pattern of the processors, ADR-0005 and spec 002). Types: Off, Overdrive, Distortion, Fuzz, EQ, Compressor.
  - Drive types: A amount, B tone, C level.
  - EQ, a compact three bands: A low gain, B mid frequency, C mid gain, D high gain, E mid Q.
  - Compressor: A threshold, B ratio, C attack, D release, E make-up.
- **Every slot holds every type**, allocated in `Engine::new`. Switching hands the signal to the new type; the old one is not heard again. A deselected type is reset, so no old state comes back, and nothing allocates. An Off slot passes the signal bit for bit.
- **Mono and stereo.** Each type has a mono entry point, used on strips, and a stereo one, linked for the compressor, used on groups. The master chain keeps its own full equalizer and compressor.
- **Counts are fixed:** 16 strips, 8 groups, 3 slots, 4 processors. Flexibility is in which slot holds what and where a strip goes, never in allocating at run time.
- **The drive insert moves into the slots.** `DriveMode`, `DriveAmount`, `DriveTone` and `DriveLevel` go; an older setup's drive becomes insert slot 1 of that strip. Setups store names, so only the old names need a migration.
- **Meters** grow to 16 strips, 8 groups, the master pair and the four returns.
- **The view** (epic #56's console) shows groups as strips with a bracket over their members, an Out selector and the slot types on each strip. Order, collapsed and hidden strips are layout; they live in the setup file and the engine never sees them.

## Consequences

- The registry grows by about nineteen ids (three slots of type and five knobs, plus `Out`), not by hundreds: the per-strip values live in the table, not in the registry.
- A strip that uses no inserts and no group costs what it does today. Eight stereo group buses and 48 slots add a small, fixed amount of work; `make bench` runs them on.
- Adding a fourth slot, a ninth group or an insert type is a constant or a variant and a parameter map, with the mirror test (ADR-0004) catching the view.
- A group cannot feed an earlier group. A chain of sections that need the opposite order has to renumber; that is the price of one pass and no feedback.
- The master keeps its fixed chain; per-strip equalizers are the compact one, not a copy of the master's.

## Alternatives considered

- **Free routing between any buses.** Needs cycle detection at run time and a topological sort on every change. Rejected for a fixed order that cannot cycle.
- **Insert chains of any length.** Needs allocation when one is added. Rejected for three preallocated slots.
- **One "Drive" type with a mode knob.** A knob that picks one of four modes is awkward to operate and to show. Rejected for three drive types.
- **A full four-band equalizer on every strip.** Ten parameters per slot against five knobs, and a wide strip. Rejected for the compact three bands; the master has the full one.
- **Group buses as a separate parameter space.** Doubles the ids and the setup shape. Rejected for one index space.
