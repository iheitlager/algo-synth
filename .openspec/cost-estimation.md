# Cost estimation

What algo-synth would have cost a traditional team, by basic COCOMO, set
against what it took. Measured on Mon 2026-10-12 at v0.42.0 (`b99ba5a`).

## What it took

- **12 days**, Wed 2026-09-30 to Sun 2026-10-11.
- **661 commits** and **206 merged pull requests**.
- **34 ADRs** and **10 specs** in `.openspec/`.
- One person deciding, with Claude Code writing most of the code.

## Size

`tokei`, excluding `node_modules`, `target` and `dist`: 323 files, 101k
lines, **85.8k lines of code**. Hidden directories (`.openspec/`) are not
counted.

| Bucket | Code lines | Share |
|---|---|---|
| **Production** | **~47.0k** | 55% |
| Tests: inline `#[cfg(test)]` modules, `tests.rs`, `*.test.ts`, `tools/test_*.py`, `tests/spike` | ~30.8k | 36% |
| Generated or vendored: tree-sitter `parser.c` and headers, grammar JSON, sample manifests, eval cases | ~7.5k | 9% |

### Production code by area

| Area | Code lines |
|---|---|
| Rust `crates/dsp` | ~31.9k |
| Rust `crates/assist` | ~2.6k |
| Web: TypeScript 5.9k, Vue 4.9k (script, template, style), CSS 0.1k | ~10.9k |
| Tooling: release and sample scripts, tree-sitter grammar, scanner and bindings | ~1.7k |

### `crates/dsp` by module

| Module | Production | Tests | What it is |
|---|---|---|---|
| `song` | 4.6k | 3.1k | song model, parser, signals and automation |
| `modular` | 4.6k | 1.3k | modular synth; `sc.rs` alone is 2.9k |
| `mono` | 3.6k | 3.4k | mono voice and presets |
| `synth` | 3.0k | 0.0k | DX7, LA and the other poly models |
| `notes` | 2.5k | 1.3k | note language and generators |
| `analysis` | 2.0k | 1.4k | spectral and pitch analysis |
| `params` | 1.8k | 0.1k | parameter tables (ADR-0004) |
| `fx` | 1.6k | 1.3k | effects |
| `ffi` | 1.5k | 0.4k | the wasm boundary |
| `fm` | 0.9k | 0.6k | FM synthesis |
| `engine` | 0.8k | 7.4k | the render loop |
| `drums` | 0.8k | 0.4k | TR-808 and TR-909 voices |
| rest: mixer, samplers, poly, MIDI, arp, clock, … | ~4.1k | ~3.5k | |

Test density is uneven: `engine`, `mono` and `poly` carry as much test
code as production code or more; `synth` has almost none.

## COCOMO

Basic COCOMO (Boehm, 1981): effort = a·KLOC^b person-months, schedule =
2.5·effort^d months. Cost uses `scc`'s defaults: $56,286 a year times an
overhead of 2.4.

Boehm counts delivered source lines, without test drivers or generated
code, so production code is the right input. The all-code row shows what
a naive count gives.

| Input | Mode | Effort | Schedule | Team | Cost |
|---|---|---|---|---|---|
| All code, 85.8k | organic | 257 pm | 20.6 mo | 12.5 | $2.9M |
| All code, 85.8k | semi-detached | 439 pm | 21.0 mo | 20.9 | $4.9M |
| **Production, 47.0k** | **organic** | **137 pm** | **16.2 mo** | **8.4** | **$1.5M** |
| Production, 47.0k | semi-detached | 224 pm | 16.6 mo | 13.5 | $2.5M |
| `crates/dsp` production, 31.9k | organic | 91 pm | 13.9 mo | 6.6 | $1.0M |
| `crates/dsp` production, 31.9k | semi-detached | 145 pm | 14.3 mo | 10.2 | $1.6M |

The fair mode lies between organic and semi-detached: the real-time rules
(ADR-0002) push it past organic, one codebase with one decision-maker keeps
it short of embedded.

## By phase

Boehm's phase split for an organic project of this size, applied to the
137 person-months:

| Phase | Share | Person-months | Here |
|---|---|---|---|
| Requirements and plans | +6% | ~8 | the ADRs and specs |
| Product design | 16% | ~22 | the ADRs: the dsp/web boundary, no allocation in `render`, the song model |
| Detailed design | 24% | ~33 | made while coding |
| Code and unit test | 38% | ~52 | written by Claude |
| Integration and test | 22% | ~30 | written by Claude |

With analysis and testing taken as free, coding alone is ~52
person-months: four person-years.

## Reading it

- COCOMO prices lines because people wrote them slowly and coordinated
  over them. When lines cost close to nothing, a line count says how much
  was built, not how hard it was.
- Design did not vanish; it moved into the ADRs, made by one person
  without meetings or handovers.
- COCOMO knows nothing of what comes after: years of use, users, upkeep.

## Reproducing

```sh
tokei --exclude node_modules --exclude target --exclude dist .
tokei $(git ls-files 'web/src/*.test.ts')
```

Rust production and test lines are split per file: a file named `tests.rs`
or under `tests/` or `examples/` is test code; otherwise the lines from
`#[cfg(test)]` to the end are. Comment and blank lines are not counted.
