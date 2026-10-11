# 0033: User tables and attacks are recipes in the song, rebuilt when it loads

**Status:** Accepted, extended by #522 (Sampler zones) · **Date:** 2026-10-11 · **Extends:** 0018, 0032

## Context

ADR-0032 gave the PPG eight user wavetables and the D-50 eight user attacks, filled from the Spectral Lab (#515). They lived for the session only. A song that picks "User 1" (`Wt1Table 8`) played silence after a reload until the table was sent again, and an example song could not use them at all (#521).

What is there to build on:

- **Samples are already restored by reference** (ADR-0018). A `samples keys upright-piano-kw` line names a pack of the samples manifest, and the view fetches it when the song loads, since the engine cannot fetch (ADR-0013). An unknown id is not an error: the track is silent and the panel says so.
- **Analysis is deterministic.** It is Rust (ADR-0017): the same WAV, rate, window, hop and transforms give the same table.
- **The browser keeps one IndexedDB database** for the preset library (ADR-0014). Nothing keeps user audio.
- **Hand-set Sampler zones are live-only** (ADR-0027), so dropped Sampler samples are a separate question (#522).

## Decision

**A user table or attack is stored in the song as the recipe that makes it: its source sample, the analysis and the transforms. The view rebuilds it in a worker when the song loads. A source that is not in the manifest is kept in the browser by its hash.**

- **Two song lines:**
  ```song
  table 1 = upright-piano-kw/C4vH : inharmonic 1.6, noise 1
  attack 1 = upright-piano-kw/C4vH
  ```
  - `table <n>` fills the PPG's "User n" (`Wt1Table`/`Wt2Table` 7 + n). `attack <n>` fills the D-50's "User n" (`Pcm1Sample`/`Pcm2Sample` 8 + n), with n from 1 to 8.
  - **The source** is `<pack>/<sample>`, a sample of a manifest pack by its file name without `.wav`, or `@<hash>`, the first 32 hex digits of a dropped file's SHA-256 (`@`, since `#` starts a comment).
  - **After a colon**, the transforms by name (the lab's `stretch`, `inharmonic`, `freqshift`, `formant`, `smear`, `oddeven`, `lowpass`, `lowres`, `highpass`, `freeze`, `decay`, `noise`, `keepformants`), and the analysis's `window` and `hop` when they are not 4096 and 256. Only values that differ from the defaults are printed.
  - The engine parses and prints the lines like any other (ADR-0012) and keeps one line per slot of each kind. One recipe type in Rust serves the song and the lab, so the lab writes exactly what the song reads.
- **Rebuilt in a worker, not on the audio thread.**
  1. When a song loads, the view fetches each recipe's source: a manifest pack's file, or the browser's store for a hash.
  2. It passes the bytes to a `spectral-worker.js` instance, the lab's worker (ADR-0017): a second `dsp.wasm` that analyses with noise per partial, applies the transforms and returns the table or attack.
  3. The view then loads that into the user slot (`table_load`/`attack_load`).
  4. A recipe already loaded with the same text is not rebuilt, so editing another line of the song costs nothing.
- **Dropped sources are kept in the browser.** A WAV dropped into the lab is stored in its own IndexedDB database (`algo-synth-audio`), keyed by the hash its recipe names. It is separate from the preset library, so neither database's version moves.
- **The lab writes the recipe.** "Send as table" and "Send attack" load the slot and set the slot's line in the main window's song, so autocommit (ADR-0027) keeps it. A "From pack" picker analyses a manifest sample directly, which makes the recipe shareable.
- **Missing is said, not silent.** A recipe whose source cannot be found leaves its slot empty and says so. So does a track that plays a user slot no recipe fills. The PPG and D-50 panels name the slot and the reason. The song still plays (as ADR-0018 has it for samples).
- **The rest stays as it is.** `samples` lines, the preset library, and live-only Sampler zones. Dropped Sampler samples follow in #522.

## Consequences

- A song holding user tables is self-contained text: an example song can use them as long as its packs are fetched (`make samples`), as `samples` lines already need.
- A recipe is rebuilt at the AudioContext's rate. At 44.1 kHz instead of 48 kHz its table differs by the analysis's resolution, not audibly in kind.
- An `@hash` source plays only in the browser that kept it. Elsewhere its slot is reported missing.
- Loading a song with recipes takes the analyses' time (about 50–150 ms each natively, more in wasm) in a worker. The song plays meanwhile, and the slots fill in as they finish.
- The lab's window, hop and noise analysis go into the recipe. Noise per partial is always analysed for a recipe, and `noise 0` clears it.
- Spec 002 gains the two lines, spec 010 Req 9 and 10 the recipes, and spec 003 the panels' missing-slot note.

## Alternatives considered

- **Keep the rendered tables in IndexedDB.** It is simple to restore, but a song would no longer be self-contained or shareable, and an example song could not use a user table. Rejected for the song; the store keeps sources, not results.
- **Put the table's numbers in the song.** 64 × 256 values a table is far too much for a text the composer reads (ADR-0012). Rejected.
- **Rebuild in the main engine.** That needs no worker, but an analysis on the audio thread is a glitch (ADR-0002). Rejected.

## Later decisions

- **#517:** a recipe may morph two sources (`A ~ B` with `mpitch`, `mamp`, `mnoise`, `mformant`, `sweep`), and a source may be one the engine makes, a preset at a note (`synth:LaChoir:c4`) or Plaits macros (`plaits:h:t:m`), which needs no fetch.

## Extended (#522): Sampler zones

Dropped Sampler samples and the zones set on them by hand get the same treatment as a recipe's source:

- **A zone is a line.** `zone keys 1 = @3fa9… : keylo c2, keyhi e3, root c3` names the sample by its source (`@<hash>` or `<pack>/<sample>`) and writes only the fields away from their defaults; one line per zone, numbered as the engine's zones.
- **A pack or its zones, not both.** A track keeps its `samples` line while its zones are the pack's. The first change by hand turns them into zone lines, the pack's samples named `<pack>/<sample>`; picking a pack takes the zone lines away.
- **The engine writes them.** The view tells the engine where each slot's sample came from when it loads one (`sample_source_set`), and keeps a dropped or lab-made WAV in `algo-synth-audio` by its hash. A zone set by hand marks its synth, and the fold (ADR-0027) prints the synth's zones as its track's lines once every zone's sample has a source. A pack's or the song's layout is laid out in one go (`zones_settle`) and does not fold.
- **The view restores them.** When a song loads, the view loads each source a synth does not hold yet into a slot and lays the zones out; a source it cannot find leaves its zone empty and the sampler's panel says so.
- **Pads stay live-only** for now; the same form for the pad sampler is a follow-up.
