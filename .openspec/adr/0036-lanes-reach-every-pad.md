# 0036: Lanes reach every pad of a Pad Sampler

**Status:** Accepted, extended by ADR-0037 · **Date:** 2026-10-15 · **Extends:** 0012

## Context

A lane plays its pad's General MIDI note (`Pad::note`), and a Pad Sampler plays pad *i* for note 36 + *i*. So from a song, a Pad Sampler reached only the eleven pads whose notes fall in 36–51. Pads 4, 5, 7, 8 and 12 had no lane, and the lanes `cb`, `ma`, `cl`, `cy`, `lc`, `mc` and `hc` fell past pad 15 and played nothing (#662). The kits `make samples` fetches lay their pads out in Hydrogen's instrument order (`tools/fetch_samples.py`, `PADS`), and most put a cowbell on 8, a shaker on 5 and a tambourine on 7. Not all do: the 808 kit has claves on 4, maracas on 5 and congas on 7 and 12, the 909 kit second kicks, snares and hats.

## Decision

**Both: the percussion names reach the pads most kits put them on, and a lane may name any pad by number.**

- **Names.** On a Pad Sampler, four General MIDI notes past the pads play a pad: cowbell (56) pad 8, maracas (70) pad 5, low conga (64) pad 4, high conga (62) pad 12 (`padsampler.rs::pad_of`). The lanes `cb`, `ma`, `lc` and `hc` play them; so does a MIDI file's drum channel or a keyboard. The notes of the lanes do not change, so the TR-808 and TR-909 play every lane as before.
- **Numbers.** `p0` to `p15` play note 36 + *n*: that pad, whatever the kit holds. With them every pad is reachable, and a lane that means "the kit's pad 7" says so.
- **Errors.** Where a track names its model, a lane that model has no pad for is a parse error at the lane's pad: `cl`, `cy` or `mc` on a Pad Sampler, a pad by number on anything else. A track whose model is picked later (a drum track with no model, a `sampler` track that keeps its samples) is not checked. Two lanes on one note (`bd` and `p0`) are the same pad.

## Consequences

- A lane holds a `LanePad`, a `Pad` or a number. Across the FFI a number's id is `PAD_NUMBER` + *n*, mirrored in `params.ts`; the composer draws it as `p7`.
- No new pad: the drum machines, their knobs and their outs keep their eighteen.
- A NES drum key whose pad holds a sample plays it for the four notes too, as it does for the pads' own.

## Alternatives considered

- **Names only, with a new `tb` for the tambourine.** A new `Pad` needs a voice on the 808 and 909, knobs and an out, and still misnames the 808 and 909 kits' pads.
- **Numbers only.** Every pad reachable, but `cb` on a Pad Sampler stays silent, and a beat written for the 808 loses its cowbell on a sampled kit.
- **A per-kit map from names to pads.** The kit is loaded at run time, not known to the text; the same song would parse differently by kit.

## Later decisions

- **Extended (ADR-0037):** a parameter its track's model never reads is a parse error too, and that check also runs on a track whose model is picked for its role, once picked (#673). Lanes stay checked only where the model is named.
