# 0024: SuperCollider voices on the Modular synth's panel

**Status:** Proposed · **Date:** 2026-10-06 · supersedes the syntax, storage and Sound-screen parts of ADR-0020 and ADR-0021

## Context

ADR-0020 and ADR-0021 put a Modular voice in the song text in our own small language (`voice acid = { saw(freq) |> svf(lp, cutoff) }` with indented `ctl` lines) and designed it on a fourth main view, the Sound screen. Built (#277–#287), it works, but the user wants something else:

- **SuperCollider's own language,** so a SynthDef from the SuperCollider world can be pasted and played: UGen classes with `.ar`/`.kr`, `|args|` or `\name.kr(default)` controls, `Env`, `var`, and the build-time sclang that SynthDefs are written with (functions, `dup`, `Array.fill`, `if` on numbers).
- **No fourth view.** The code is edited on the Modular synth's own panel in the Synths view: a code editor with highlighting on the right, and on the left a module for each UGen in the code with a knob for each number it takes.
- **The code belongs to the synth,** like a patch: saved with the setup, in user presets, and in a song `setting`.

The acceptance target is a classic hoover written for SuperCollider: 20 detuned pairs of saws through short delays, a pitch envelope, `Splay`, `atan` saturation and a `FreeVerb2`, pasted as it is.

## Decision

**A Modular synth's voice is a SuperCollider SynthDef, evaluated once at build time by a subset of sclang into the fixed graph ADR-0021's runtime already plays; its numbers are live parameters shown as knobs on the UGen modules of the synth's panel.**

- **The language.** `SynthDef(\name, { |args| … })` (or a bare `{ … }`), with `.add`, `.play` or `.store` after it ignored. Controls are `|args|` with defaults or `\name.kr(default)`; `freq` and `gate` come from the note, and `amp` from the note's velocity times its default. A closed set of UGens maps onto the runtime's units: `SinOsc Saw Pulse LFTri LFSaw LFPulse WhiteNoise PMOsc` (`.ar`, or `.kr` as LFOs), `RLPF RHPF LPF HPF MoogFF`, `CombL CombN`, `EnvGen` with `Env.adsr Env.perc Env.asr` (and `env.kr`), `Mix`, `Out`, and on signals `+ - * /`, `.range .exprange .tanh .softclip .distort .neg`, with `mul`/`add` arguments and keyword arguments. Arrays expand as SuperCollider's multichannel expansion does; a voice that ends in an array sums it to mono until the synth bus is stereo.
- **sclang at build time.** As SuperCollider runs a SynthDef's function once to build its graph, so does the engine, when the code is set (never in `render`): numbers, symbols, arrays, functions with `|args|` and closures, `var` and reassignment, `value`, `dup` and `!`, `do`, `collect`, `sum`, `Array.fill`, `Mix.fill`, indexing, and `if` on numbers or booleans. Binary operators run left to right with no precedence, as in sclang. An `if` on a signal (SuperCollider's `Select`), recursion past a depth, `while`, routines and patterns are refused. Evaluation is bounded (depth, loop counts, steps), and the graph keeps ADR-0021's limits until state is sized at load (below).
- **A knob for every number a UGen takes.** A number written as a UGen's argument (a filter's `freq`, an `Env.adsr` time, a `mul`) or as a control's default compiles to a live parameter of the synth, `Param::Ctl1`… (32 of them), so turning its knob changes sounding notes at once and `mod` lines, lanes and scenes reach it. A number in a loop's body is one knob for every copy. The knob's range and taper come from the UGen input it feeds (a frequency 20–20000 Hz exponential, `rq` 0.05–2, a time 1 ms–10 s); a control's from its name and default.
- **The text follows the knobs.** The engine keeps the code as written and where each knob's number is in it; the text it hands out has each number replaced by the knob's current value, so the code and the knobs never disagree and the user's layout and comments stay.
- **The code is the synth's.** It lives with the synth's parameters, is set by a Modular preset or by the panel, saved in setups and user presets, and carried by a song `setting` (ADR-0018), so a song stays self-contained. The song's `voice` and `ctl` lines and the Sound screen go away.
- **State sized at load.** What a program sizes (node values, oscillators, phases, filters, delay lines, random numbers) lives in a state per voice slot of the synth's pool, allocated when its code is set and never shrunk, outside `render`; a voice holds only its note, gate and envelopes. A program may use 250 nodes, 64 oscillators, 32 phases, 8 filters, 8 envelopes, 32 delays and 64 random numbers, and a voice cap (a work budget over a rough cost per voice) bounds how many of its voices sound: four for the hoover, whose voice costs about 3% of a core (`make bench`, `sc hoover x4`). Init-rate randoms (`Rand`, `ExpRand`, a new number per note from a seed), breakpoint envelopes (`Env(levels, times, curves, releaseNode)` with SuperCollider's segment curves), `DelayN`/`L`/`C` and combs that output only the delayed signal, `.midiratio`, `.midicps` and `.atan` come with it.
- **Stereo.** A program whose last value is an array of two channels is stereo: its synth renders the left and right of its bus (the pad sampler's `stereo_bus`), which its strip then balances; an array of more channels sums to one side. `Pan2` (equal power from the sine table), `Splay` (constant gains with SuperCollider's level compensation) and `FreeVerb2`/`FreeVerb` (Jezar's Freeverb per voice, its buffers allocated with the state) make two sides (#288). Node references are 16-bit and a program may use 512 nodes, so the hoover of #216 fits as written, its voice at about 4.4% of a core with its reverb, three voices allowed.

## Consequences

- A SynthDef from the SuperCollider world plays when it stays inside the UGen set; anything outside it is an error with a line and a column naming what is missing.
- The editor and the knobs are two views of the same parameters: a knob is a parameter, the text shows its value.
- Thirty-two numbers per voice can be knobs; past that a number is a constant.
- ADR-0021's runtime stays: one program per synth copied into a voice at note-on, evaluated once a sample with no allocation or transcendental call.
- Later, two ways to take the build further, neither blocked by this design: compile a SynthDef in a second wasm instance off the audio thread and post the fixed program to the worklet as bytes; and generate a small wasm module per SynthDef from the node array, straight-line code instead of interpretation, instantiated by the worklet and called once a block.
- Because the code is the synth's own patch, a note of it can be rendered offline into a sample and played on with the samplers (#300).
- The changelog fragments that announced `voice`/`ctl` lines are rewritten before the release that would have shipped them.

## Alternatives considered

- **Our own lowercase language (ADR-0021).** Smaller, but not SuperCollider; a SynthDef could not be pasted. Superseded.
- **A closed set without sclang.** No interpreter, but `dup`, `Array.fill` and `Mix.fill` are how SynthDefs are written; the hoover could not be written. Rejected.
- **Embedding scsynth.** As ADR-0020: AGPL and a second engine. Rejected again.
- **Knobs only for `|args|`.** Simpler, but the modules on the panel would be empty for code that writes its numbers inline, as most SynthDefs do. Rejected.
