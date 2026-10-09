# Project conventions

## The boundary

All music logic lives in `crates/dsp` (ADR-0001): sources, mixer, sequencer,
generators, song model, parsers. `web/` only sends messages and draws. If a
feature needs JavaScript beyond that, stop and raise it.

`render` never allocates, locks or panics (ADR-0002). Allocate in
`Engine::new`; compute coefficients when a parameter changes, not per sample.

A new parameter or id goes in `params.rs` (or the enum it names) **and**
`web/src/audio/params.ts` (ADR-0004); `cargo test` fails otherwise.

## Tests

Test DSP natively by rendering blocks offline and checking properties
(finite, bounded, silent, pitch). The browser is for listening.

## Makefile target descriptions

The `## ...` comment after a target is what `make help` prints; keep it to
3-6 words, imperative or noun-phrase, no parenthetical asides. Put detail in a
`#` comment on the line(s) above the target instead.

## Ports

`make serve` → 6340, `make dev` → 6341: both one server, the app and `/api`
(ADR-0030). Vite's own server (6343) is only for UI work against a fake.
Keep new services in the 63xx range.

## Releases

A PR never bumps the version or edits `CHANGELOG.md`: it adds a fragment,
`changes/<issue>.<added|changed|fixed>.md`, with its changelog bullet(s)
(see `changes/README.md`). `make release` turns the fragments into a release
when one is wanted. Merge with `gh pr merge --auto --merge` so a PR merges
itself when CI is green (#186).
