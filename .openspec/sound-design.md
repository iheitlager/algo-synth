# The song language: sound design

The second half of the song language's definition: the lines that make a
sound rather than play one. Synth patches and Modular SynthDefs (`setting`),
the samples a track loads, its Sampler zones and pads (`samples`, `zone`,
`pad`), user
wavetables and attacks (`table`, `attack`),
and the mix (`strip`, `group`, `master`). The conventions, limits, grammar
notation and the composer's lines (tracks, clips, autos, scenes) are in
[language.md](language.md); what it says holds here too. Every block tagged
`song` is a whole song that parses and prints back the same.

## setting

```text
setting <name> = <Model> <Preset>[: <Param> <value>[, <Param> <value>]…]
```

A synth patch kept in the song: a factory preset and changes to it. A track
plays it by name. The preset must be the model's; parameters are the synth's
own (not `Model`, mixer or master ones).

Models: `Arp2600 Minimoog ProOne Ms20 Cs15 Sh101 Odyssey Tb303 Prophet5
Juno106 Jupiter8 Matrix12 PpgWave D50 Dx7 PolyMoog` (synths), `Tr808 Tr909` (drum
machines), `Sampler` (multisampler), `GeneralMidi` (the multisampler on a
program of the General MIDI bank), `PadSampler` (pads), `Modular` (a
SuperCollider SynthDef), `Sid` (the C64's chip), `Nes` (the NES's 2A03; with
`NesKit` also a drum machine, whose `samples` kit or `pad` lines play
through its DMC), `VlTone` (the Casio VL-1; with `VlKit` its
drums Po, Pi and Sha). Presets are factory names, as `MiniBass`,
`AcidBass`, `Tb303Acid`, `JunoPad`, `Kit909`; `Modular` has `ModularBasic`,
`ModularHoover`, `ModularKick`, `ModularSid`, `ModularDubSiren`; `Sid` has `SidLead`, `SidBass`,
`SidArp`, `SidSync`; `Nes` has `NesLead`, `NesBass`, `NesArp`, `NesKit`;
`VlTone` has `VlPiano`, `VlFantasy`, `VlViolin`, `VlFlute`, `VlGuitar`, `VlKit`;
`GeneralMidi` has `GmPiano`, `GmElectricPiano`, `GmVibraphone`, `GmMarimba`,
`GmXylophone`, `GmOrgan`, `GmNylonGuitar`, `GmAcousticBass`, `GmFingerBass`,
`GmStrings`, `GmTrumpet`, `GmFlute`.

```song
setting nile = Minimoog MiniLead: Cutoff 1200, Resonance 0.5
setting plain = Tr909 Hard909
track lead synth nile
track kit drums plain
```

### Modular code

A `Modular` setting may hold a SynthDef on the indented lines under it, kept
as written (comments, blank lines) and built at load; a mistake is a song
error at its line and column. Without code it plays its preset's.

```text
SynthDef(\<name>, { |freq = <hz>, gate = 1, <ctl> = <n>…| <statements>; <signal> }).add;
```

A subset of sclang, run once to build the voice:

- **Language:** numbers, `pi`, symbols `\x`, arrays, `var`, functions and
  closures, `value` (or `f.(x)`), `dup` and `!`, `do`, `collect`, `sum`, `size`, `reverse`,
  `first`, `last`, `at`, `product`, `Array.fill`, `Mix.fill`, `if` on
  numbers, `( … )` regions, keyword arguments, `mul`/`add`; `f <> g`
  composes two functions (`f.(g.(x))`), and `p.coin` and `array.choose`
  draw as the code builds, the same each time it is built (#547). A `nil`
  argument takes the default. Binary operators run left to right with no
  precedence, as in sclang; on signals `+ - * / **` (`**` keeps the sign
  of its left side, so a negative base stays finite).
- **Controls:** `freq` is the note's pitch (bent by the pitch wheel), `gate`
  its gate, `amp` the velocity times its default, `modwheel` the mod wheel
  (0..1); any other argument (or `\name.kr(n)`) is a knob. A second
  argument is the control's lag: `\freq.kr(440, 0.08)` glides from one
  legato note to the next (with `Polyphony 1`).
- **UGens** (`.ar`/`.kr`): `SinOsc Saw Pulse LFSaw LFTri LFPulse WhiteNoise
  PMOsc RLPF RHPF LPF HPF BPF MoogFF CombN CombL CombC DelayN DelayL DelayC Rand
  ExpRand LinExp EnvGen Mix Pan2 Splay FreeVerb FreeVerb2 Select Latch Decimator LFNoise0 LFNoise1 LeakDC Out
  Impulse Ringz Klang DynKlang Klank DynKlank VOsc Osc SidOsc SidEnv
  SidFilter NesPulse NesTri NesNoise NesDmc`.
  The SID's parts (#112) take their registers as numbers:
  `SidOsc.ar(freq, wave, width, chip, sync, ring)` plays `freq` on the 16-bit
  frequency register of a PAL chip, `wave` the waveform bits (1 triangle, 2
  saw, 4 pulse, 8 noise; added for the chip's combined waveforms), `width`
  the 12-bit pulse width, `chip` 0 the 6581's DAC or 1 the 8580's; `sync`
  resets it on a rising edge and `ring` flips its triangle while above 0
  (`ring: SidOsc.ar(freq * 1.5, 4)`). `SidEnv.kr(gate, attack, decay,
  sustain, release, chip)` is the chip's envelope, 4-bit rates (attack 2 ms
  to 8 s) and sustain, 0..1. `SidFilter.ar(in, cutoff, res, mode, chip,
  bias)` is its filter: an 11-bit cutoff, 4-bit resonance, mode bits 1
  low-pass, 2 band-pass, 4 high-pass, and the 6581's bias in octaves.
  The NES's channels (#112) are an NTSC 2A03's: `NesPulse.ar(freq, duty,
  volume)` on the 11-bit timer, duty 0..3 (12.5, 25, 50, 75 %), a 4-bit
  volume; `NesTri.ar(freq)` its 32-step triangle; `NesNoise.ar(period,
  short, volume)` the LFSR at a period index 0..15, `short: 1` its 93-step
  metallic mode; `NesDmc.ar(in, rate, direct)` the DMC on any signal,
  delta-encoded a bit at a time at rate index 0..15 (4.2 to 33 kHz), or 7-bit
  levels with `direct: 1`. Each swings about half its volume, −1..1 at
  full volume, silent at 0.
  `LeakDC.ar(in, coef)` (0.995 when left out) takes away a DC offset.
  `BPF.ar(in, freq, rq)` is the 12 dB filter's band output, at unity in
  its centre; `LinExp.kr(in, srclo, srchi, dstlo, dsthi)` maps a linear
  range onto an exponential one, unclipped; `clip2(x)` is `clip(x.neg, x)`.
  `Latch.kr(in, trig)` holds `in` from one rising edge of `trig` to the next
  (`Latch.kr(WhiteNoise.kr, LFPulse.kr(8))` is a random step eight times a
  second); `Decimator.ar(in, rate, bits)` samples `in` `rate` times a second
  and quantizes it to `2^bits` levels from −1 to 1 (`bits: 1` is two).
  `LFNoise0.kr(freq)` draws a random value in −1..1 `freq` times a second
  and holds it; `LFNoise1` joins the same points with straight lines. Both
  draw from the note's seed, as `Rand`, so a note renders the same each time.
- **Partials** (spec 010 Req 11): `` `[…] `` is a `Ref`, an array kept whole
  rather than expanded across channels.
  `Klang.ar(`[freqs, amps, phases], freqscale, freqoffset)` is a bank of
  sines, `Klank.ar(`[freqs, amps, ringtimes], input, freqscale, freqoffset,
  decayscale)` a bank of resonators struck by `input`; a `nil` list is all 1
  (phases 0). Their specs are numbers, each a knob (`freq 1`, `amp 1`,
  `ring 1`…); `DynKlang` and `DynKlank` take signals too. `Ringz.ar(in, freq,
  decaytime)` is one resonator, ringing `decaytime` seconds down to −60 dB;
  `Impulse.ar(freq)` clicks once a cycle, the first at the note's start
  (`Impulse.ar(0)` once: `Klank.ar(`[[400, 1071, 1820], nil, [1.2, 0.8, 0.5]],
  Impulse.ar(0))` is a struck bell). `VOsc.ar(bufpos, freq)` plays the user
  wavetables of the Spectral Lab (`table` lines) where SuperCollider reads
  buffers: `bufpos`'s whole number is the table, 0 to 7 for User 1–8, and
  its fraction the place across its waves, crossfaded, as the PPG's
  position; `Osc.ar(bufnum, freq)` plays a table's first wave. An empty table
  is silent. Phases other than 0 are not supported yet.
- **Switches:** `Select.kr(which, [a, b, …])` (and `.ar`) plays one of 1 to 8
  choices, its index truncated and clipped as SuperCollider's; an `if` on a
  signal is an error pointing to it. A knob used as a `Select` index is a
  switch, whole numbers from 0 through the choices: `|dir = 0|` in
  `Select.kr(dir, [saw.neg, saw])` flips between two positions.
- **Envelopes:** `Env.adsr(a, d, s, r)`, `Env.perc(a, r)`, `Env.asr(a, s, r)`,
  `Env(levels, times, curves, releaseNode)`; played by `EnvGen.kr(env, gate)`
  or `env.kr`. A voice ends when its envelopes do; without one it uses the
  synth's ADSR.
- **Signal methods:** `range exprange tanh softclip distort atan midiratio
  midicps neg round lag abs sqrt squared cubed min max pow clip`; `sqrt`
  keeps the sign, as the server's, and `clip(lo, hi)` (0 and 1 when left
  out) is `max(lo).min(hi)`; `round(step)` (step 1 when left out) steps to the
  nearest multiple, halves up, as SuperCollider's: `sweep.round(1/12)`;
  `lag(time)` (0.1 s when left out) smooths a signal, falling 60 dB in
  `time` from its first value, `lag(0)` passing it:
  `LFNoise0.kr(8).exprange(0.5, 3).lag(0.01)`.
  Numbers take also `reciprocal floor ceil asInteger cpsmidi ratiomidi
  dbamp ampdb`.
- **Stereo:** multichannel expansion; a final array of two channels is
  stereo (`Pan2`, `Splay`, `FreeVerb2`). `Mix` adds arrays channel by channel, as SuperCollider's
  `sum`: `Mix.fill(4, { |k| Pan2.ar(…, k / 1.5 - 1) })` keeps its spread (#709).
- **Filter voicings:** `voicing: \<synth>` on `MoogFF` (ladders: `arp2600
  minimoog proone prophet5 sh101 juno106 jupiter8 matrix12 ppgwave d50 odyssey
  tb303 prophet5rev1 odysseyrev2`, plus `drive: 0..1`), on `RLPF RHPF LPF` (12 dB:
  `ms20 cs15 polymoog jupiter8 matrix12 odysseyrev1`), on `HPF` (`ms20 cs15
  odyssey juno106`) renders as that synth's filter.
- **Knobs:** every number a UGen, an envelope or a `Rand` takes, and every
  control's default, is a knob, `Ctl1`…`Ctl32` in build order (the SynthDef's
  arguments but `freq` and `gate` first). Knobs are parameters of the track: `auto`, `snapshot`,
  `mod` reach them (`sub.Ctl1`).
  On a track that plays a setting, an argument's knob also goes by the
  argument's name, in any case: `|vib = 4|` (or `\vib.kr(4)`) is `.vib(4)`,
  `mod lead.vib = …`, `lead.vib` (#674). The name wins over the synth's own
  parameter of that name, prints as the code writes it (`.ctl1` too), and a
  name the setting has no knob for is an error listing those it has. `amp`
  and a UGen's numbers have no name. A knob without a unit in its name turns
  from 0 to twice its default, so `|vibmod = 0.5|` reaches 1 at most: give it
  the default you want room around. On a clip, a pattern method wins over a
  knob of its name: a knob `|fast = 2|` is `.ctl2` there, though `mod`, `auto`
  and snapshots reach it as `lead.fast`; completion says so. A strip target
  names no synth, so a knob goes by its track: `strip1.vib` is an error that
  says to write `lead.vib` (#688).
- **Limits:** 1024 nodes, 64 oscillators, 64 phases (a sine, an LFO, a
  `VOsc`; a `Klang` takes one per frequency), 8 filters, 8 envelopes, 32
  resonators (a `Klank` takes one per frequency), 32 delay lines, 64 random
  numbers, 64 latches, decimators, noises, lags, impulses and LeakDCs, 2
  reverbs, 32 knobs. A delay line holds 1024 samples (about
  0.02 seconds); a comb or delay takes as many as its `maxdelaytime` needs,
  so a voice's delays hold about 0.68 seconds in all: a slapback, a chorus
  or a short echo. A long echo is the strip's send to the master's Echo.

```song
setting hoover = Modular ModularBasic
  SynthDef(\hoover, { |freq = 440, cutoff = 2500, gate = 1|
      var sig = Pulse.ar(freq * [0.995, 1.005], SinOsc.kr([5, 4.3]).range(0.1, 0.4));
      RLPF.ar(Mix(sig) + Saw.ar(freq * 0.5), cutoff, 0.7, voicing: \ms20)
          * EnvGen.kr(Env.adsr(0.01, 0.3, 0.7, 0.4), gate)
  }).add;
track lead synth hoover
clip r = lead
  "c3 eb3 g3 <bb3 c4>"
auto open = lead.Ctl1 ramp 600 4000 /8
```

## samples

```text
samples <track> <id>
```

The samples a `sampler` or `drums` track wants: a pack (`instruments`) or a
kit (`kits`) of the samples manifest, by its id. The app fetches it and loads
it on the track's synth when the song loads; picking a pack or kit on a
sampler's panel writes the line. A drums track takes the kit of that id, any
other the pack (or the kit when no pack has it). An id no manifest names
loads nothing and the sampler's panel says so. One line per track, after the
track; the samples a track already has stay loaded without one.

`make samples` builds the packs and kits: the FreePats instruments, the
General MIDI programs (`gm-…`), and the kits `audiophob`, `cmu-arctic-voices`
and, for this machine only (no license: never committed or published), the
tidal-drum-machines ones (`roland-tr808`, `linndrum`, …) and Dirt-Samples'
`jazz` (#661), Strudel's `sound("jazz")`: `bd`, `sn`, `ch` and `oh` lanes
play its kick, snare and hats, its cowbell and effects sit on pads 8, 4, 5
and 7.

A `GeneralMidi` track needs no line (#652): its preset is a program of the
FluidR3 GM bank and names its pack, `GmMarimba` the `gm-marimba` pack, which
loads as if written. A `samples` line or zone lines take its place. The
packs are built by `make samples`; without them the panel says the pack is
missing and the song plays on.

```song
track keys sampler GeneralMidi GmMarimba
clip riff = keys
  "c4 e4 g4 [b4 c5]"
```

```song
track keys sampler Sampler SamplerKeys
track kit drums PadSampler
samples keys upright-piano-kw
samples kit audiophob
clip chords = keys
  "[c3,e3,g3] [a2,c3,e3]"
```

## zone

```text
zone <track> <n> = <source> [: <field> <value>, …]
source = <pack>/<sample> | @<hash>
field  = keylo keyhi vello velhi root tune level loop loopstart loopend seqlen seqpos release
```

A Sampler zone set by hand (#522), n from 1 to 64: the sample it plays and
its fields away from their defaults. The source is a sample of a pack, its
file name without `.wav` (`upright-piano-kw/C4vH`), or `@` and the first 32
hex digits of a dropped file's SHA-256, as in a `table` line. `keylo`,
`keyhi` and `root` are notes (`c2`) or 0–127, the root left out takes the
sample's own; `vello` and `velhi` 1–127; `tune` cents (−1200–1200); `level`
0–2; `loop` `off`, `loop` or `sustain`, its points in frames (0 and 0 take the
sample's); `seqlen` and `seqpos` 1–16 for round-robin; `release 1` sounds
when the key comes up.

Dropping a WAV on a Sampler, setting a zone or sending a multisample from the
lab writes the lines; a dropped file is kept in the browser that dropped it.
A track has a `samples` line or zone lines, not both: changing a pack's zones
by hand turns its `samples` line into zone lines, and picking a pack takes
them away. One line per zone, on a `sampler` track, after it. A zone whose
source cannot be found stays empty and the sampler's panel says so; the song
plays on.

```song
track keys sampler Sampler SamplerKeys
zone keys 1 = upright-piano-kw/C4vH : keyhi b3, root c4
zone keys 2 = upright-piano-kw/C4vH : keylo c4, tune 7, loop sustain
clip chords = keys
  "[c3,e3,g3] [a4,c5,e5]"
```

## pad

```text
pad <track> <n> = <source> [: <field> <value>, …]
field = tune level pan decay choke vellevel velstart oneshot out
```

A pad of a `PadSampler` set by hand (#527), or of a `Nes` (its DMC's
samples, on a drums track or its own synth track, #112), n from 1 to 16 (pad 1 answers
note 36, `bd`), as a zone: the sample it plays, by the same sources, and its
fields away from their defaults. `tune` is semitones (−24–24); `level` 0–2;
`pan` −1–1; `decay` seconds to fall 60 dB (0 plays the sample out); `choke` a
group 1–8 whose pads silence each other (0 none); `vellevel` and `velstart`
0–1, how much velocity moves the level and the start; `oneshot 0` lets the
key coming up fade the pad; `out` 0 for the sampler's strip or a group bus
1–8.

Dropping a WAV on a pad or changing a pad writes the lines. A track has a
`samples` line or pad lines, not both: changing a kit's pads by hand turns its
`samples` line into pad lines, and picking a kit takes them away. One line per
pad, on a `drums` or `sampler` track, after it. A pad whose source cannot be
found stays empty and the sampler's panel says so.

```song
track kit drums PadSampler
pad kit 1 = upright-piano-kw/C4vH : tune -12, decay 0.4
pad kit 3 = upright-piano-kw/C4vH : tune 7, choke 1, out 2
clip beat = kit
  bd x...x...x...x...
  sn ....x.......x...
```

## strip, group, master

```text
strip <track> | strip<n>: <Param> <value>[, <Param> <value>]…      n = 1–16
group <n> [<name>][: <Param> <value>[, …]]                         n = 1–8
master: <Param> <value>[, …]
```

The mix's starting values, set when the song loads and again only when the
line's text changes. A group may have a name and nothing else (`group 2 keys`): the
name shows on its strip, and naming a group in the mixer writes it here.

- Strip and group parameters: `Level Pan Mute Solo Out Key`, sends
  `Send1`–`Send4` (with `Send1Pre`, `Send1On`…), inserts `I1Type I1A`–`I1E`,
  `I2…`, `I3…`. Insert types: `Off Overdrive Distortion Fuzz Eq Comp Vocoder Bitcrush`.
  `Out` is `master`, `group1`–`group8` or `none`; a group goes only to a higher
  group.
- Master parameters: `MasterGain`, `CompThreshold CompRatio CompAttack
  CompRelease CompMakeup`, `EqLowFreq EqLowGain EqMid1Freq EqMid1Gain EqMid1Q
  EqMid2Freq EqMid2Gain EqMid2Q EqHighFreq EqHighGain`, processors `P1Type
  P1Return P1A`–`P1E` … `P4…`, `P2In`–`P4In`. Processor types: `Off Echo Reverb
  Chorus Flanger`; P1 is an Echo and P2 a Reverb by default, fed by `Send1`
  and `Send2` and returned at 1, so a send alone is heard (#660).
- The echo and the reverb in real units (#660), on P1's echo and P2's reverb
  (an error on a slot holding another effect): `EchoTime` in seconds (P1A,
  0.001–2), `EchoFeedback` (P1B, 0–0.95), `ReverbSize`, the tail in seconds
  (P2A, 0.1–10), `ReverbCutoff`, the low-pass in the tail in Hz (P2B,
  1000–20000). Printed as written; Write mixer keeps them so.

```song
track kit drums
track bass synth
strip kit: Level 0.9, Out group1
strip bass: Level 0.8, Pan -0.2, I1Type Overdrive, I1A 0.6, Send2 0.3
group 1 drums: Level 0.85, I1Type Comp
master: MasterGain 0.6, P2Return 0.3, EchoTime 0.375, ReverbSize 2.5
clip beat = kit
  bd x...x...x...x...
clip low = bass
  "c2 ~ c2 g1"
```

## Spectral sound (ADR-0032)

Sounds built from their partials rather than from a factory preset: user
wavetables and attacks the Spectral Lab analyses from a sample, a preset or
macros (`table`, `attack` below), and the Modular's banks of sines and
resonators (`Klang`, `DynKlang`, `Klank`, `DynKlank`, `Ringz`), the
`` `[…] `` literal that holds a partial list whole, and `VOsc`/`Osc` playing
the user tables (see *Partials* under [Modular code](#modular-code)).
Partial sets and morphs between them belong here as they are built (#192).


### table, attack

```text
table <n> = <source> [~ <source>] [: <change> <value>, …]
attack <n> = <source> [~ <source>] [: <change> <value>, …]
```

The recipe of a user wavetable or attack (ADR-0033): `table <n>` fills the PPG
Wave's Table "User n" (`Wt1Table`/`Wt2Table` 7 + n), `attack <n>` a D-50
partial's Source "User n" (`Pcm1Sample`/`Pcm2Sample` 8 + n), n from 1 to 8.
The source is a sample of a pack of the samples manifest, its file name
without `.wav` (`upright-piano-kw/C4vH`), `@` and the first 32 hex digits
of a dropped file's SHA-256 (which only the browser that kept the file has),
a factory preset the engine renders at a note (`synth:LaChoir:c4`), or
Plaits-style harmonics, timbre and morph macros (`plaits:0.3:0.5:0.8`).
`A ~ B` morphs two sources: `mpitch`, `mamp`, `mnoise` and `mformant` say
where between them (0 is A, 1 is B), and `sweep 1` makes a table run from A
to B across its waves instead of over the sound's time.
After a colon come the Spectral Lab's transforms by name (`stretch`,
`inharmonic`, `freqshift`, `formant`, `smear`, `oddeven`, `lowpass`, `lowres`,
`highpass`, `freeze`, `decay`, `noise`, `keepformants`), and the analysis's
`window` and `hop` when they are not 4096 and 256. The app analyses the source
and rebuilds the slot when the song loads; the lab's Send as table and Send
attack write the line. One line per slot of each kind. A slot whose source
cannot be found stays empty and the PPG's or D-50's panel says so; the song
plays on.

```song
table 1 = upright-piano-kw/C4vH : inharmonic 1.6, noise 1.5
table 2 = upright-piano-kw/C4vH ~ synth-pad-choir/C4 : sweep 1
attack 1 = upright-piano-kw/C4vH
track bell synth PpgWave PpgSweepPad
track hammer synth D50 LaFantasia
clip ring = bell .wt1table(8) .wt2table(8)
  "c4 ~ g4 ~"
clip hit = hammer .pcm1sample(9)
  "c4 ~ g4 ~"
```

## Idioms

**A Modular voice with a knob in motion:** a SynthDef sub whose cutoff (its
first knob, `Ctl1`) opens over the scene, and a snapshot for the break.

```song
tempo 132
scale g minor
setting rumble = Modular ModularBasic
  SynthDef(\rumble, { |freq = 55, cutoff = 260, gate = 1|
      var sig = SinOsc.ar(freq) + (Saw.ar(freq * 1.005) * 0.4);
      sig = MoogFF.ar(sig, cutoff, 2.2, voicing: \minimoog, drive: 0.5);
      (sig * 2).tanh * EnvGen.kr(Env.perc(0.004, 0.26), gate) * 0.7
  }).add;
track kit drums Tr909 Kit909
track sub synth rumble
clip kick = kit
  bd x...x...x...x...
clip roll = sub
  "~ g1 g1 g1 ~ g1 g1 bb1"
auto open = sub.Ctl1 ramp 200 1800 /8
snapshot bare: strip1.Mute 1
snapshot full: strip1.Mute 0
scene main 8: kick roll open [full]
scene break 4: roll [bare]
arrange main break main
```
