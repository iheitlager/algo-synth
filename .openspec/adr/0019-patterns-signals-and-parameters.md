# 0019: One language for patterns, signals and parameters

**Status:** Proposed · **Date:** 2026-10-05

## Context

The notation (ADR-0012, ADR-0016) has mini-notation, classic durations, drum lanes and generator calls (`euclid`, `arp`, `walk`, `markov`, `mutate`); ADR-0015 added scenes and automation lanes by `target.Param`. What it lacks next to Strudel and SuperCollider is the *activity inside the text*: transforming a pattern (`.fast(2)`, `.rev()`, `.every(4, …)`, `.off(1/8, …)`), and treating a value as a signal instead of a number (`sine.range(200, 2000).slow(4)`, `SinOsc.kr(1).exprange(100, 2e3)`).

Two open issues ask for parts of this: #204 (fluent methods on a fragment that set its synth's parameters) and #208 (SuperCollider-style modulation on the fixed synths). Built apart, they would become two small languages with two precedence rules. The user wants both musical and mathematical constructs, and one way to control synths and the mixer.

## Decision

**One expression language in the song text, with three kinds of value: patterns of events, signals of numbers, and plain numbers. Methods chain on any of them; parameters are methods.**

- **Addressing.** A parameter is reached as `<target>.<param>`: the target a track (its synth), a strip or group by name (ADR-0018), `stripN`, `groupN` or `master`, the parameter its registry name in lower case (`cutoff`, `level`, `p2return`; ADR-0004). The model and `Out` cannot be modulated, as ADR-0015 says.
- **Pattern methods (musical).** On a fragment or a quoted sequence: `fast slow rev every off add sub scale struct ply iter palindrome degrade sometimes`. They transform the events at compile time, per cycle, so a live fragment (ADR-0016) still regenerates per bar with nothing allocated in `render`. Randomness is seeded (ADR-0005).
- **Signals (mathematical).** Continuous sources `sine saw tri square rand perlin`, `lfo(rate, shape)` and `env(shape)`, combined with `+ - * /`, and shaped with `range exprange quantize lag segment slow fast`. A bracket list expands into one signal per channel (`lfo([1, 3])`: one per stereo side, or per voice), as SuperCollider's multichannel expansion does. A signal runs at control rate (per block, smoothed); audio rate belongs to the modular voice (ADR-0020).
- **Parameters are methods.** On a fragment, a parameter method sets that parameter on the fragment's synth while the fragment plays: `frag bass = acid "c2 c2 eb2 <g2 bb1>" .cutoff(sine.slow(4).range(300, 3000)) .res(0.7)`. On a mixer line it sets the strip: `strip kit .level(0.8) .pan(sine.slow(8))`. A standalone line `mod <target>.<param> = <expr>` holds one for the whole song. A value is a number, a mini-notation sequence of numbers (`"<300 800 1200>"`, one per cycle) or a signal.
- **Precedence.** The knob, the setting (#210) or the mixer line (ADR-0018) sets the base value. A modulation writes over it while it plays, and when it stops the base value comes back. When several write the same parameter in one block, scenes go first, then lanes, then methods and `mod` lines, all in text order: the last write wins. A knob turned by hand moves the base value and holds until the next written value.
- **Compiled once.** The parser turns every expression into a fixed pool of nodes, sized at load, with a stated limit per song. `render` evaluates the pool once per block and writes a parameter only when its value changes, through the `set_param` path, so coefficients are computed per change (ADR-0002). An expression is printed back canonically.
- **A closed set.** No user functions, no loops, no variables beyond named fragments, lanes and voices. The vocabulary grows when a song needs a word, not before.

## Consequences

- #204 and #208 stages 1 to 5 become one implementation, in two steps: addressing and parameter methods first, then pattern methods and signals (#215).
- Automation lanes (`auto … ramp`) become a special case of a signal; they stay as written and print as before.
- The highlighter (#203) gains classes for methods and signals; the editor can mark a modulated knob in the view, which only draws the value the engine reports.
- The CPU cost grows with the number of modulated parameters: a node pool per song bounds it, and the 16-synth budget of plan.md is measured with modulation on every synth.
- A language model gets a richer notation it partly knows from Strudel and SuperCollider; the parser stays the check (ADR-0012).

## Alternatives considered

- **Fluent methods only (#204 as written).** Smaller, but no signals and no mathematics, and #208 would add a second syntax later. Rejected.
- **`mod` lines only (#208 as written).** One syntax, but every change to a fragment's sound moves away from the fragment. Kept as one of the two forms, not the only one.
- **Evaluate expressions per sample.** Smoother sweeps, but the cost of an interpreter in `render` on every synth. Rejected; per-block evaluation with smoothing, and audio rate only inside the modular voice.
- **Strudel's syntax verbatim (JavaScript method chains).** Familiar, but it implies a JavaScript runtime and its scheduler, which ADR-0001 and ADR-0012 rule out. The notation stays ours, close enough to read.
