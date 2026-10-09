# 0012: The song is text

**Status:** Accepted, amended by [0015](0015-the-arrangement.md), [0022](0022-one-clock-one-transport.md) and [0028](0028-an-llm-proxy-beside-caddy.md) · **Date:** 2026-10-03

## Context

ADR-0005 sets out tracks, patterns, clips, an arrangement and algo loops, with the song sent from the view as a binary format (spec 002 Req 6). ADR-0008 deferred all of it, and Drums with it, to reach the ensemble. The ensemble now plays: sixteen synth slots with their own models (ADR-0009), a mixer with groups and effects (ADR-0010), polyphony on the way (ADR-0011).

What is missing is a way to make music in algo-synth itself: define loops, program a beat on a 16-step grid, arrange fragments into a piece, let generators write parts, and let a language model write or change them. The MIDI file player plays someone else's music; it is not that model, and growing it into one (import as clips, a MIDI writer) would tie composition to a file format meant for exchange.

A binary song format serves the engine but nobody else: not a person reading a song, not a diff, not a language model. Live-coding systems (TidalCycles, Strudel) show that a compact text notation can be the whole interface to loops and generators, and language models already know that notation well.

## Decision

**A song is a text in a small notation, parsed by the engine. The text is the source of truth; every view of it (the drum grid, later a piano roll) edits it through the engine.**

- **One model.** A **track** is a synth slot (strip 0–15) and owns its source: a Mono or Poly model, the Drums kit, later the Sampler. A **fragment** is a loop: events on a beat grid, of any length. An **arrangement** is a list of sections, each a number of bars and the fragments that play in it. The **clock** runs in `render` (ADR-0005) and plays the arrangement sample-accurately.
- **Mini-notation, as in Tidal and Strudel.** A quoted sequence divides one cycle (a bar) evenly; `[ ]` subdivides, `~` rests, `*n` repeats, `<a b>` alternates per cycle, `?` plays with a probability. Words are notes, drum pads or samples, depending on the track.
- **Classic notes as well.** Pitches are note names with an octave (`c4`, `f#3`, `bb2`). A note may carry a classic duration (`c4:4` a quarter, `e4:8` an eighth, `g4:8.` a dotted eighth, `:16`, `:2`, `:1`). A sequence whose notes all carry durations is laid out by duration, one after another, the way a score reads; one without durations divides the cycle. Mixing the two in one sequence is a parse error, not a guess.
- **Drum lanes.** A drum fragment is one lane per pad, one character per step: `x` a hit, `X` an accented hit, `.` a rest. Sixteen characters are the 16-step grid; any other length is allowed (polymeter).
- **Generators are functions in the notation**: `euclid(k, n, rotation)`, `walk`, `arp`, later `markov` and `mutate`. They take an explicit seed and are deterministic (ADR-0005): the same text gives the same music. A *live* fragment regenerates every cycle; *freezing* replaces the call with the events it produced, in the same notation.
- **The engine parses, the view sends.** The view sends the text as bytes into a buffer, the way it sends a MIDI file (`midi_buf`, `midi_load`). The engine parses and compiles it in that call, outside `render`, into its own buffers; `render` only reads. A text that does not parse is rejected with a line, a column and a message, and the song that is playing keeps playing. A new song takes over at the next bar.
- **Edits go through the engine.** A click on the drum grid is a message (`set_step(fragment, lane, step, level)`); the engine changes the song and prints it back as text, which the view shows. Printing is canonical: parsing a printed song gives the same song. So the grid, the text and the engine never disagree, and no music logic lands in `web/` (ADR-0001).
- **A language model writes text, nothing else.** It gets the song and a request and returns a new song; the engine's parser is the check, and its error goes back to the model for another try. The engine never knows a model was involved. *Where the model call runs* (in the browser with the user's key, behind a small proxy next to Caddy, or outside the app by pasting) was held open; ADR-0028 settles it: behind a proxy next to Caddy, on localhost.
- **Drums come back** as a source of synthesized analog pads (plan.md MVP 3), reversing that part of ADR-0008. The Sampler is a later source behind the same pad and note names.
- **The MIDI player stays separate** (spec 002 Req 9). Importing a MIDI file into the notation, or writing one out, is a later converter, not part of this model.

## Consequences

- One notation serves hand-written loops, the drum grid, generators and a language model; a song file is something a person can read, diff and paste.
- The engine gains a parser and a printer. Both are total: they never panic, whatever the bytes (as `smf.rs`).
- Spec 002 Req 3, 4, 6 and 7 are rewritten around fragments, the arrangement and the text format; the binary song format of Req 6 is dropped.
- Parsing allocates, so a song change costs an allocation on the call that loads it, as a MIDI file does today; `render` still never allocates (ADR-0002).
- A canonical printer means a song's layout (blank lines, spacing) is not kept across an edit from the grid. Comments are kept (#199): the parser ties each `#` comment to the item above or beside it and the printer puts it back, so Apply and grid edits no longer erase them; comments are not music, so they do not count when two songs are compared.
- The notation is ours, not Strudel's: close enough to read and for a model to write, without promising compatibility.

## Alternatives considered

- **A binary song format** (spec 002 Req 6 as written). Compact and quick to read, but unreadable to people and models, and every editor needs its own encoder in TypeScript. Rejected.
- **JSON or TOML.** Readable and easy to parse, but a sixteen-step beat or a melody becomes dozens of lines. Rejected for the mini-notation; setups (synth patches, the mixer) stay JSON (spec 003 Req 7).
- **Run Strudel in the view.** A complete system, but its scheduler is JavaScript, which breaks ADR-0001 and ADR-0005 (the clock in the engine). Rejected.
- **Grow the MIDI player into the composition model.** Ties composition to an exchange format with ticks and channels. Rejected; MIDI stays import and export.

## Later decisions

- **Superseded in part by ADR-0015 and ADR-0022:** the MIDI player does not stay separate. A MIDI file is imported into the song as notes when opened, and the song's clock is the only one.
- **Since ADR-0008 and ADR-0009:** a track plays a synth slot with a model; there are no sources. The drums came back as drum models (TR-808, TR-909) and the samplers as models on a slot, not as sources.
- **Amended by ADR-0028:** the model call runs behind a server on localhost (see the Decision above).
