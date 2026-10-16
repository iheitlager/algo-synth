# 0039: Pitch and pan per note

**Status:** Accepted, amended by #716 (the part of a sample), #717 (fit) and #718 (speed) · **Date:** 2026-10-16

## Context

Strudel's "lefthand" example warbles its pitch (`.add(perlin.range(0, .5))`), layers a copy a tenth of a semitone up (`.add("…,.1")`) and plays a reversed copy on the right (`.jux(rev)`). An event (ADR-0016) had a whole note, and the voices of a pool are matched by owner and note (ADR-0011), so neither a fraction of a semitone nor a side of the stereo field could reach a voice. Pan was the strip's only (ADR-0010): two notes of one track could not sit apart, and `.jux(rev)` took two tracks with hard-panned strips. Every pitched voice already plays `note + trim + bend`, where `trim` is the per-voice pitch the pool sets each block for unison and analog drift (ADR-0023). The drum kit already pans voice by voice, through a scratch block onto its stereo bus.

## Decision

**An event carries cents off its note and a pan. The pool keeps both per voice slot, set as the note is pressed: the cents through the voice's pitch trim, and the pan as two gains onto a stereo bus.**

- **Cents.** `Event.cents`, −50 to 49, with `note` the nearest key, so note-offs, voice matching, drums, lanes and the views keep their whole note. `.add`, `.sub` and `.add([…])` take semitones to the cent (`.add(0.5)`, at most two decimals); whole ones leave the cents alone. The pool adds a slot's cents to its `trim` with unison and drift, so every voice that reads `trim` (Mono, Poly, LA, FM, Modular, the samplers, SID and NES) plays them, at no cost per sample. A drum pad ignores them, as it ignores `trim`.
- **A signal on `add`.** `.add(<signal>)` (and `.sub`) is held on the clip and read by the engine at each note's tick, as `.onNote` is (ADR-0019), its value added to the event's cents. One per clip; not a per-voice signal (ADR-0023), `.onNote` or one with rests. So a warble moves over the whole song, not over the clip's loop.
- **Pan.** `Event.pan`, −100 to 100, 0 where the strip puts it. `.jux(m)` sets the line to −100 and its copy with m to +100, as Strudel's does. The engine tells the pool the pan of the note it is about to press, so a note re-uses only a voice on its own side: a line and its copy may share a note and a tick. The pool works out a slot's equal-power gains when the note is pressed, never per sample.
- **A pool goes stereo while a note has a pan.** While any of its sounding voices has one, the engine gives the synth a stereo bus. Each voice renders into the pool's scratch block and is added to both sides at its gains, and a voice without a pan at the middle's (−3 dB each), as a mono strip at its centre puts it. The strip then balances instead of panning (ADR-0010). A stereo Modular program keeps writing both sides itself.
- **Chorus on the mid.** A synth that is stereo only for its pans keeps its chorus: the chorus runs on the mid and the side is added back, so a voice panned left stays left. A synth that was already stereo (a stereo Modular program) skips its chorus as before.

## Amendment (#716): the part of its sample a note plays

Strudel's breakbeat recipes chop a break into parts (`.chop(16)`) and play them by index (`.slice(8, "0 1 <2 3>")`). A sampler always played a whole sample from its start, which only VelStart moved.

- **The event carries the part.** `Event.region`, from and to as fractions of the sample in steps of 1/65535, `to` 0 meaning the end, so every existing event plays the whole sample. Methods write it: `chop` splits a note's part into n, `slice` names one of n. A part of a part is a sub-range, so `slice` then `chop` cuts a slice finer.
- **It reaches the voice, not the slot.** As cents and pan do, the engine hands it to the pool right after pressing the note. But the pad sampler doesn't render through slots, so the pool sets it on the voices the note pressed: `PadVoice` and `SamplerVoice`. Both find their frames as they start, rounded to whole frames, so a region costs nothing per sample. Other voices ignore it.
- **A part plays once.** A part stops at its end and ignores a zone's loop. A pad's choke group, and a note on the same pad, cut the part before, which is Strudel's `.cut(1)`.
- **Not on lanes.** A drum clip's lanes hold no part, so `chop` and `slice` are for a clip of notes: `"c2" .chop(16)` on a sampler track plays pad 0's break.

- **Fit (#717).** `Event.fit` holds the ticks a note's part of the sample plays over, set to the note's length when `fit` is written, as Strudel's `fit` sets its speed then: a `chop` or `slice` after it cuts the ticks with the part, a `ply` keeps them, so a plied part starts again at its fitted rate, a stutter. The engine turns them into output frames at the tempo when the note is pressed and hands them over with the region; the voice sets its rate once, as it finds its frames, whatever its tune or pitch. A tempo change refits the next note, not one playing. Strudel's `s("amen/4")` is a note four bars long: `.slow(4)`, not a new spelling. 

- **Speed, a sweep and coarseness (#718).** `Event.play` holds a speed in hundredths (100 as is, below 0 backwards), a sweep added by the note's end, and the samples each output value holds, as Tidal's `speed`, `accelerate` and `coarse`. `speed` multiplies, so Strudel's `mul(speed(-1))` is the same method spelled its way, and a reverse reverses whatever played. Part, fit, speeds and coarseness travel to the voice as one `Playback`, and a shared `Cursor` plays it: it starts on the part's last frame going backwards, adds the sweep's step to its speed each frame, and holds a value `coarse` frames, so a sweep costs one addition a sample and nothing is computed with powers. The multisampler follows its note's pitch while unfitted, so a bend still bends a reversed part.

Rejected: a part per pad (a `PadField`), which can't give each hit of one pad its own part.

## Consequences

- `render` doesn't allocate: cents, pan and gains are fields of the fixed slots, the scratch block is the pool's, and an `add` signal is evaluated once per note. A panned pool costs one more pass over each voice's block.
- A song without fractions, signals on `add` or `jux` renders as before: whole semitones give the same events, and a pool without a pan renders to its mono bus.
- While a jux'd synth is stereo, the strip's pan is a balance, and a send takes half of each side, so its sends hear it 3 dB lower than a mono synth's.
- A mono synth (one voice) plays one side at a time. A drum clip's lanes hold no pan, so `jux` is for a clip of notes.
- No MIDI export reads events, so nothing rounds the cents; the views show the note.

## Alternatives considered

- **A float note.** Exact, but `Event` is ordered and compared as music (`sort_key`), and note-offs and voice matching would need rounding everywhere. Rejected for whole notes plus cents.
- **Cents as a per-voice signal (ADR-0023).** Pitch is not in the table, and the table is per block, so a note would start at its key and move a block later. Rejected: the trim is set before the note's first sample.
- **`jux` as a hidden second track.** No engine change, but it needs a second synth, its own strip and its own settings, against one clip on one track. Rejected.
- **Per-note pan by automating the strip.** Pans every voice of the track at once, so the line and its copy could not sit apart. Rejected.
