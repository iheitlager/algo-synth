// Mirror of crates/dsp/src/params.rs and source.rs (ADR-0004).
// Rust is the source of truth; `cargo test` fails if a line here drifts.
// Keep the `Name: id,` shape: the test greps for it.

export const Param = {
  MasterGain: 0,
  Attack: 1,
  Release: 2,
} as const
export type ParamId = (typeof Param)[keyof typeof Param]

export const Source = {
  Mono: 0,
  Wave: 1,
  Drums: 2,
} as const
export type SourceId = (typeof Source)[keyof typeof Source]
