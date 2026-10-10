# The song language

The normative definition of algo-synth's song text: what the engine's parser
accepts, what it means, its limits and its errors. `docs/song.md` is the guide
for people; where the two differ, the parser decides and this file is fixed.
Every block tagged `song` is a whole song that parses and prints back the same.

Grammar lines: `<x>` a placeholder, `[x]` optional, `x…` one or more, `a | b`
a choice; anything else is literal.

## Conventions

- **Lines.** One item per line. A top-level line starts at column 1 with a
  keyword: `tempo swing scale setting track strip group master frag auto scene
  mod section arrange loop`. An indented line (space or tab) belongs to the
  `frag` above it, or is code of the `Modular` `setting` above it. Blank lines
  are ignored. Words are split at whitespace.
- **Comments.** `#` at the start of a line or after whitespace starts a comment
  (so `c#4` is a note), also inside quotes; not in Modular code. Comments are
  kept: a comment line belongs to the item below it, a trailing one to its line.
- **Names** (settings, tracks, frags, autos, scenes, sections, groups):
  `[A-Za-z][A-Za-z0-9_]*`, at most 32 chars. Frags and autos share one
  namespace; the others have one each. A setting is not named as a model.
- **Parameters** are registry names (`Cutoff`, `Level`, `P2Return`), matched in
  any case, values in the parameter's units (Hz, seconds, 0–1), clamped to its
  range. The catalog lists each model's presets and each parameter's range.
- **Order.** A name is used after the line that makes it: settings before any
  track; `scale` before any frag; a track before its frags, mixer line and
  targets; a frag before the frags that read it and the sections that hold it;
  autos and scenes before their sections; sections before `arrange`.
- **Once.** At most one `tempo`, `swing`, `scale` and `arrange`; one mixer line
  per strip, group or master; one `mod` per target and parameter; one lane per
  pad in a frag; a name once in a section.
- **Defaults.** Empty text is a song: tempo 120, swing 50, silence. Without
  `arrange` every frag and auto loops from the start and no scene applies.
- **Canonical form.** The printer writes: tempo, swing, scale, settings, tracks,
  mixer lines, frags, autos, scenes, mods, sections, arrange, loop. It always
  writes `tempo` and `swing`, each track's model and preset, a drum frag's
  grid, flats as sharps (`eb4` → `d#4`), `mod` parameters in lower case, and
  Modular code as written. Layout and blank lines are not kept. Chord names
  print as written.
- **Errors.** The parser never panics. Text that is not a song gives the first
  problem: line, column (from 1) and a message; the playing song plays on.

### Limits

| what | range |
|---|---|
| song text | 1 MB |
| tempo / swing | 20–300 BPM / 50–75 % |
| tracks, settings | 16 each |
| changes in a setting, values in a scene | 32 |
| values on a mixer line | 48 |
| frags, sections, `arrange` entries, bars in a section or auto | 256 |
| steps in a lane | 1–64 |
| notes a line compiles to / bars before it repeats | 512 / 32 |
| words in a line of notes | 1100 |
| autos, scenes, `mod` lines plus parameter methods | 32 each |
| values in an auto | 64 |
| nodes in all signals together | 256 |

## tempo, swing

```text
tempo <bpm>        20 to 300, default 120
swing <percent>    50 (straight) to 75, default 50; moves off-beat sixteenths later, 67 ≈ triplet feel
```

```song
tempo 96.5
swing 58
```

## scale

```text
scale <root> <mode>
root  = a–g [# | b]
mode  = major minor dorian phrygian lydian mixolydian locrian pentatonic blues phrygian-dominant harmonic-minor
```

The song's key, read by `walk`, `prog`, roman numerals and `euclid … scale`.
`major`…`locrian`, `phrygian-dominant` (0 1 4 5 7 8 10) and `harmonic-minor`
(0 2 3 5 7 8 11) have seven notes; `pentatonic` (0 2 4 7 9) and `blues`
(0 3 5 6 7 10) do not, so `prog` and roman numerals refuse them.

```song
scale e phrygian-dominant
track lead synth
frag wander = lead
  walk(e4,8,3)
```

## setting

```text
setting <name> = <Model> <Preset>[: <Param> <value>[, <Param> <value>]…]
```

A synth patch kept in the song: a factory preset and changes to it. A track
plays it by name. The preset must be the model's; parameters are the synth's
own (not `Model`, mixer or master ones).

Models: `Arp2600 Minimoog ProOne Ms20 Cs15 Sh101 Odyssey Prophet5 Juno106
Jupiter8 Matrix12 PpgWave D50 Dx7 PolyMoog` (synths), `Tr808 Tr909` (drum
machines), `Sampler` (multisampler), `PadSampler` (pads), `Modular` (a
SuperCollider SynthDef). Presets are factory names, as `MiniBass`, `AcidBass`,
`JunoPad`, `Kit909`; `Modular` has `ModularBasic`, `ModularHoover`,
`ModularKick`.

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
  closures, `value`, `dup` and `!`, `do`, `collect`, `sum`, `size`, `reverse`,
  `first`, `last`, `at`, `Array.fill`, `Mix.fill`, `if` on numbers, keyword
  arguments, `mul`/`add`. Binary operators run left to right with no
  precedence, as in sclang; on signals only `+ - * /`.
- **Controls:** `freq` is the note's pitch (bent by the pitch wheel), `gate`
  its gate, `amp` the velocity times its default, `modwheel` the mod wheel
  (0..1); any other argument (or `\name.kr(n)`) is a knob.
- **UGens** (`.ar`/`.kr`): `SinOsc Saw Pulse LFSaw LFTri LFPulse WhiteNoise
  PMOsc RLPF RHPF LPF HPF MoogFF CombN CombL CombC DelayN DelayL DelayC Rand
  ExpRand EnvGen Mix Pan2 Splay FreeVerb FreeVerb2 Out`.
- **Envelopes:** `Env.adsr(a, d, s, r)`, `Env.perc(a, r)`, `Env.asr(a, s, r)`,
  `Env(levels, times, curves, releaseNode)`; played by `EnvGen.kr(env, gate)`
  or `env.kr`. A voice ends when its envelopes do; without one it uses the
  synth's ADSR.
- **Signal methods:** `range exprange tanh softclip distort atan midiratio
  midicps neg`; numbers take also `abs reciprocal squared cubed sqrt round
  floor ceil asInteger cpsmidi ratiomidi dbamp ampdb min max`.
- **Stereo:** multichannel expansion; a final array of two channels is
  stereo (`Pan2`, `Splay`, `FreeVerb2`).
- **Filter voicings:** `voicing: \<synth>` on `MoogFF` (ladders: `arp2600
  minimoog proone prophet5 sh101 juno106 jupiter8 matrix12 ppgwave d50 odyssey
  prophet5rev1 odysseyrev2`, plus `drive: 0..1`), on `RLPF RHPF LPF` (12 dB:
  `ms20 cs15 polymoog jupiter8 matrix12 odysseyrev1`), on `HPF` (`ms20 cs15
  odyssey juno106`) renders as that synth's filter.
- **Knobs:** every number a UGen, an envelope or a `Rand` takes, and every
  control's default, is a knob, `Ctl1`…`Ctl32` in build order (the SynthDef's
  arguments but `freq` and `gate` first). Knobs are parameters of the track: `auto`, `scene`,
  `mod` reach them (`sub.Ctl1`).
- **Limits:** 512 nodes, 64 oscillators, 32 phases, 8 filters, 8 envelopes,
  32 delays, 64 random numbers, 2 reverbs, 32 knobs. A comb or delay holds
  at most 0.02 seconds (a flanger, a chorus, a resonator, not an echo): a
  long echo is the strip's send to the master's Echo.

```song
setting hoover = Modular ModularBasic
  SynthDef(\hoover, { |freq = 440, cutoff = 2500, gate = 1|
      var sig = Pulse.ar(freq * [0.995, 1.005], SinOsc.kr([5, 4.3]).range(0.1, 0.4));
      RLPF.ar(Mix(sig) + Saw.ar(freq * 0.5), cutoff, 0.7, voicing: \ms20)
          * EnvGen.kr(Env.adsr(0.01, 0.3, 0.7, 0.4), gate)
  }).add;
track lead synth hoover
frag r = lead
  "c3 eb3 g3 <bb3 c4>"
auto open = lead.Ctl1 ramp 600 4000 /8
```

## track

```text
track <name> <kind> [<Model> [<Preset>] | <setting>] [mute] [solo]
kind = drums | synth | sampler
```

| kind | its frags hold | models |
|---|---|---|
| `drums` | lanes | `Tr808 Tr909 PadSampler` |
| `synth` | one line of notes | any but drum machines and samplers |
| `sampler` | lanes (`PadSampler`) or notes (`Sampler`) | `Sampler PadSampler` |

With a model alone the preset is the track's role's on that model, else the
model's first. With nothing, model and preset come from the role and are
printed; a `sampler` track without a model keeps the samples loaded. The role
is read from the name, else the notes:

| role | name holds | or notes are | default |
|---|---|---|---|
| drums | (any drums track) | | `Tr808 Kit808`; `Tr909 Kit909` if the name holds `909` |
| bass | `bass` `sub` | mostly below C3 | `Minimoog MiniBass` |
| pad | `pad` `chord` `string` `choir` | chords | `Juno106 JunoPad` |
| arp | `arp` `pluck` `seq` | only `arp(…)` | `Arp2600 ShArp` |
| keys | `key` `piano` `organ` | | `Dx7 FmElectricPiano` |
| lead | `lead` `melod` `solo` | anything else | `ProOne ProLead` |

`mute` stops the track's frags; while any track is `solo`, only soloed ones
play. Errors: a model that does not play the kind, a preset of another model,
an unknown model or setting.

```song
track kit909 drums
track bass synth Sh101 AcidBass
track pad synth
track keys synth Dx7 mute
frag deep = bass
  "e1 f1 e2 b1"
```

## strip, group, master

```text
strip <track> | strip<n>: <Param> <value>[, <Param> <value>]…      n = 1–16
group <n> [<name>]: <Param> <value>[, …]                           n = 1–8
master: <Param> <value>[, …]
```

The mix's starting values, set when the song loads and again only when the
line's text changes.

- Strip and group parameters: `Level Pan Mute Solo Out Key`, sends
  `Send1`–`Send4` (with `Send1Pre`, `Send1On`…), inserts `I1Type I1A`–`I1E`,
  `I2…`, `I3…`. Insert types: `Off Overdrive Distortion Fuzz Eq Comp Vocoder`.
  `Out` is `master`, `group1`–`group8` or `none`; a group goes only to a higher
  group.
- Master parameters: `MasterGain`, `CompThreshold CompRatio CompAttack
  CompRelease CompMakeup`, `EqLowFreq EqLowGain EqMid1Freq EqMid1Gain EqMid1Q
  EqMid2Freq EqMid2Gain EqMid2Q EqHighFreq EqHighGain`, processors `P1Type
  P1Return P1A`–`P1E` … `P4…`, `P2In`–`P4In`. Processor types: `Off Echo Reverb
  Chorus Flanger`; P1 is an Echo and P2 a Reverb by default, fed by `Send1`
  and `Send2`.

```song
track kit drums
track bass synth
strip kit: Level 0.9, Out group1
strip bass: Level 0.8, Pan -0.2, I1Type Overdrive, I1A 0.6, Send2 0.3
group 1 drums: Level 0.85, I1Type Comp
master: MasterGain 0.6, P2Type Reverb, P2Return 0.3
frag beat = kit
  bd x...x...x...x...
frag low = bass
  "c2 ~ c2 g1"
```

## frag

```text
frag <name> = <track> [/<grid> | live | bars <n>] [voicing] [.<method>(<args>)]…
  <lane>…            on a drums track, or a sampler track
  <line of notes>    exactly one, on a synth track, or a sampler track
```

A fragment is a loop on one track. A sampler frag holds lanes or notes, not
both; a line there is notes when it starts with `"` or `[`, or holds `:` or
`(`. A frag with nothing under it is an error. `/grid` is for lanes, `live`
and `voicing` for notes, `bars` for timed notes; methods come last. A frag of
notes, on a synth or a sampler track, takes no grid but the default `/16`:
`a note frag has no step grid`.

### Lanes

```text
<pad> <steps>…  |  <pad> euclid(<hits>,<steps>[,<rotation>])
pad  = bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr rd
step = x hit | X accent | o ghost | f flam | d drag | . rest, then maybe a ratchet 2–4 on x X o
grid = /16 (default) | /12 | /24 | /32 | /48     steps to a bar
```

Pads: kick, snare, clap, closed/open hat, low/mid/high tom, rimshot, claves,
maracas, cowbell, cymbal, low/mid/high conga, crash, ride. Spaces between steps
are for reading. Each lane loops on its own length, 1 to 64 steps
(polymeter). `euclid` spreads the hits evenly over its steps on the grid (no
spaces inside, no more hits than steps). A ghost plays softly; a flam adds one
soft grace 20 ms before its hit, a drag two (30 and 15 ms). Every frag keeps
its own grid in time with the others; swing moves a step's hits with it. A
digit after `x`, `X` or `o` is a ratchet: `x3` plays the hit three times,
evenly across its step (2 to 4), and counts as one step.

```song
tempo 124
track kit drums
frag beat = kit /16
  bd x...x...x...x...
  sn ....X.......X..x
  ch x.x. x.x. x.x. x.x.
  cb euclid(5,16,2)
  oh ..x
frag roll = kit /32
  sn o.o.o.o.x.x.x.x.xxxxxxxxXXXXXXXX
frag trip = kit /24
  sn ..x..x..x..x..x..X..X..X
frag rudiments = kit
  sn f...o.o.d...o.o.f.f.d.d.X...X...
frag rolls = kit
  sn x...x3..x...X4..
  ch x2.x.x2.x.x3.x.o2.x.
```

### Notes

A pitch is `a`–`g`, maybe `#` or `b`, an octave `0`–`9`: `c4` is middle C
(MIDI 60). `!` right after it accents it (`c4!`). A line is one of five
forms, never mixed; 48 ticks make a bar.

**Mini-notation:** one quoted cycle is one bar; its words share it.

```text
"<word>…"
word   = <pitch> | ~ | [<word>…] | [<pitch>,<pitch>…] | <<word>…> | <chord name>, then suffixes
suffix = *n repeat in its share (1–16) | @n weight in shares (1–48) | ? some bars only | & slide
```

`[a b]` groups share one word's time, `[c4,e4,g4]` is a chord, `<a b>` plays
one option per bar, `?` plays on a fixed, seeded set of bars (repeats after 8).
Suffixes come in any order, each once. Brackets nest at most 4 deep. `&`
(slide: the note runs one tick into the next; with Legato and Glide, as on
`AcidBass`, the pitch glides) goes on a note or chord, not a rest or group.

```song
track lead synth
frag riff = lead
  "c4 [e4 g4] ~ <c5 d5>?"
frag stabs = lead
  "~ [a3!,c4!,e4!] ~ ~ ~ [a3,c4,e4]@3"
frag slide = lead
  "c2& c3 ~ c2*2 eb2&"
```

**Classic:** notes one after another, each with a duration.

```text
<beat>…    beat = (<pitch> | [<pitch>,…] | <chord name> | r):<dur>[.][&]    dur = 1 2 4 8 16
```

`:4` a quarter; `.` one and a half (not on `:16`); `r` rests; `&` slides. The
line runs as many bars as it fills.

```song
track lead synth
frag tune = lead
  c4:4 e4:8 g4:8 [c4,e4,g4]:2 r:4. c5!:8 c4:8&
```

**Timed:** each note at a tick, for a length, maybe a velocity; notes may
overlap. A line containing `@` outside quotes is timed.

```text
<pitch>[!]@<start>:<length>[:<velocity>]…     start 0–1535, length 1–1536, velocity 1–127
```

The line ends at the bar of its last start, or `bars <n>` (1–32) on the frag
line says how long it is.

```song
track v synth
frag line = v bars 2
  d5@0:6 f#5@6:6:90 a4@12:24 d4@48:48:64
```

**Chords by name**, in mini-notation, classic and `arp`:

- Symbol: `<root>[<octave>][:<quality>]`, octave 4 when left out; a bare root
  (`c`, `bb`) is a major triad, but `c3` is one note: write `c3:maj`.
  Qualities: `maj m 7 maj7 m7 m7b5 dim dim7 aug sus2 sus4 6 m6 9 m9 add9`. In
  classic notes the last `:` is the duration: `c:m7:2`.
- Roman numeral: a degree of the song's seven-note `scale`, upper case major,
  lower case minor, maybe `b`/`#` first (borrowed), then `o` `o7` `+` `7` or
  `maj7`: `i VI bVII V7 viio7`. Change the `scale` and the progression moves.
- `voicing` on the frag line moves each chord to the notes nearest the chord
  before it, within C3–C6.

```song
scale c minor
track pad synth
frag prog = pad voicing
  "<i VI III VII>"
frag jazz = pad
  c:m7:2 f:7:2 bb:maj7:2 eb:maj7:2
frag turn = pad voicing
  "<ii7 V7 i bVII>"
```

**Euclid:** `euclid(<hits>,<steps>[,<rotation>]) <pitch>` plays the pitch on
the hits, the steps sharing one bar; `euclid(…) scale <pitch>` walks up the
song's scale from the pitch, hit by hit.

```song
scale a minor
track bass synth
frag pulse = bass
  euclid(5,8) a1
frag climb = bass
  euclid(7,16,2) scale a2
```

**Generators:** a call that writes the notes; the same call gives the same
notes every run.

| call | plays |
|---|---|
| `arp(<chord>,<mode>,<rate>)` | a chord (`[c4,e4,g4]` up to 8 notes, or a name) over one bar; mode `up` `down` `updown`; rate 2, 4, 8 or 16 notes a bar |
| `arp(<chord>,random,<rate>,<seed>)` | random order; only `random` takes a seed, and must |
| `arp(<frag>,<mode>,<rate>[,<seed>])` | the frag's chords, each arpeggiated from its own start (a frag name wins over a chord name) |
| `walk(<pitch>,<n>,<seed>)` | n notes (1–32) in one bar, a random walk on the scale; needs `scale` |
| `markov(<order>,<frag>,<seed>)` | a chain of order 1–3 learned from the frag's pitches and rhythm |
| `mutate(<frag>,<percent>,<seed>)` | the frag with 0–100 % of its notes moved or dropped |
| `prog(<bars>,<seed>)` | 1–16 bars of triads, one a bar: tonic first, dominant last, functional moves; needs a seven-note `scale` |
| `root(<frag>[,<octave>])` | the root of each of the frag's chords in octave 0–7 (default 2), a bass line |

A frag a call reads is a note frag above it, as it was at load (a `live`
frag's first bar).

```song
scale c minor
track pad synth
track bass synth
track arp synth
frag chords = pad voicing
  prog(4,7)
frag low = bass
  root(chords)
frag ripple = arp
  arp(chords,updown,16)
frag riff = arp
  "c4 eb4 g4 [bb4 g4]"
frag chain = arp
  markov(2,riff,3)
frag drift = arp
  mutate(riff,30,5)
frag dice = arp
  arp(c:m9,random,8,7)
```

**live:** `frag <name> = <track> live` with a generator call (any of the
table) plays new notes every bar: the seed mixed with the bar, the same every
run. Not on lanes, other forms of notes, `voicing` or pattern methods.

```song
scale e phrygian-dominant
track lead synth
frag sand = lead live
  walk(e4,16,7)
```

### Pattern methods

Written last on a note frag's line; applied in order to its notes at load. A
cycle is a bar; times round to the tick grid; the result keeps the limits of
512 notes and 32 bars. Not on lanes or `live` frags.

| method | does |
|---|---|
| `.fast(n)` `.slow(n)` | n (1–16) times faster or slower |
| `.rev()` | each bar backwards |
| `.palindrome()` | every other bar backwards |
| `.add(n)` `.sub(n)` | transpose by n semitones (−48 to 48) |
| `.ply(n)` | each note n (1–16) times in its own length |
| `.iter(n)` | each bar starts a further 1/n (1–16) of a bar in |
| `.degrade([p])` | drop each note with chance p (0–1, default 0.5), seeded |
| `.every(n, m)` | method m on every nth bar from the first: `every(4, rev)` |
| `.off(t, m)` | add a copy t of a bar later (0 < t < 1, `1/8` or `0.125`) with m |
| `.struct("x ~ x x")` | each bar on a rhythm of 1–48 steps: at each `x`, the notes sounding there |
| `.sometimes(m)` | m on about half the moments, seeded |
| `.scale(<root> <mode>)` | each note to the nearest note of the scale (down on a tie) |

The `m` of `every`, `off` and `sometimes` is a method without its dot:
`rev`, `fast(2)`, `add(12)`.

```song
track lead synth
frag riff = lead .fast(2) .every(4, rev) .off(1/8, add(12))
  "c4 eb4 g4 bb4"
frag grid = lead .struct("x ~ x x ~ x x ~") .degrade(0.2) .scale(c minor)
  "c4 d4 e4 f4"
```

### Parameter methods

`.<param>(<signal>)` on a frag line, after pattern methods, moves a parameter
of the frag's track (its synth or strip) while the frag plays, and puts it
back when it leaves. One per parameter per frag; a pattern method's name wins.
They count with `mod` lines toward 32.

```song
track kit drums
track bass synth Sh101
frag beat = kit /16 .send1(0.3)
  bd x...x...x...x...
frag acid = bass .cutoff(sine.slow(4).exprange(300, 3000)) .resonance(0.7)
  "c2 c2 eb2 <g2 bb1>"
frag plain = bass .cutoff("<400 900>")
  "c2 ~ c2 ~"
section a 4: beat acid
section b 4: beat plain
arrange a b
```

## Targets

`auto`, `scene` and `mod` write `<target>.<Param>`:

| target | parameters |
|---|---|
| `<track>` | its synth's and its strip's |
| `strip1`–`strip16` | that synth's and strip's |
| `group1`–`group8` | the group's strip parameters |
| `master` | the master parameters |

`Model` and `Out` can't be written.

## auto

```text
auto <name> = <target>.<Param> <value>… /<bars>
auto <name> = <target>.<Param> ramp <from> <to> /<bars>
```

An automation lane of 1–256 bars: values (at most 64) spread evenly, each
held for its share, or a linear ramp. Placed in sections like a frag, it
starts at the section's first bar and loops inside it.

```song
track kit drums
frag beat = kit
  bd x...x...x...x...
auto sweep = kit.Cutoff ramp 300 4000 /8
auto duck = strip1.Level 1 0.5 0.25 1 /1
section main 8: beat sweep duck
arrange main
```

## scene

```text
scene <name>: <target>.<Param> <value>[, <target>.<Param> <value>]…
```

Values set together, as a jump, on the first step of each section that lists
`[name]`. 1–32 values.

```song
track kit drums
track bass synth
frag beat = kit
  bd x...x...x...x...
scene drop: strip1.Mute 1, master.P2Return 0.4
scene back: strip1.Mute 0, bass.Cutoff 900
section main 4: beat [back]
section quiet 4: beat [drop]
arrange main quiet main
```

## mod

```text
mod <target>.<param> = <signal>
```

A signal written to a parameter for the whole song, once a block (128
samples), after autos and scenes, so it wins over them. It follows the song's
position (the same every run and after a seek). When it stops, the parameter
returns to its value before.

```text
signal = <term> [(+ | - | * | /) <term>]…      * and / bind tighter; ( ) group
term   = <number> | <source> | "<numbers>" | [<n>, …] | ( <signal> ), then .<method>(…)…
```

| source (0 to 1) | |
|---|---|
| `sine` `cosine` `saw` `tri` `square` | once a bar |
| `rand` | a new seeded value each sixteenth |
| `perlin` | a smooth seeded curve, a new value each bar |
| `lfo(<hz>[, <shape>])` | shape `sine` (default), `cosine`, `saw`, `tri`, `square` |
| `sine2` `cosine2` `saw2` `tri2` `square2` `rand2` | as Strudel's: the same from −1 to 1 |
| `irand(<n>)` | a whole number from 0 to n − 1 (n 1–64), new each sixteenth |
| `brand` `brandBy(<p>)` | 1 with chance p (0–1; 0.5 for `brand`), else 0, new each sixteenth |
| `"<300 800 1200>"` / `"0 0.5 1"` | numbers, one a bar / sharing the bar (at most 64) |
| `env(adsr)` `env(perc)` `env(<a>, <d>, <s>, <r>)` | per voice: an envelope per note, the synth's ADSR, a short one, or seconds |
| `[a, b, …]` `lfo([a, b, …])` | per voice: number or LFO rate i for voice slot i, round the list (at most 16) |

| method | |
|---|---|
| `.range(a, b)` | 0..1 onto a..b |
| `.exprange(a, b)` | the same, exponentially; a, b of one sign, not 0 |
| `.slow(n)` `.fast(n)` | n (> 0) times slower or faster |
| `.segment(n)` | n steady values a bar |
| `.lag(s)` | follow the input smoothly, s seconds |

**Strudel's parameter names** stand for ours in a `mod` line and a parameter
method, printed as written: `lpf` `ctf` (Cutoff), `lpq` (Resonance), `hpf`
(HpCutoff), `hpq` (HpResonance), `attack` `att`, `decay` `dec`, `sustain`
`sus`, `release` `rel` (the ADSR), `gain` (Level), `room` (Send2, the reverb),
`delay` (Send1, the echo). Values are at our scale: a resonance is 0..1, not a
Q; a `pan` is −1..1, not 0..1. Strudel's `distort` has no one parameter here.

A per-voice signal goes on a track whose synth is Mono or Poly (not FM, LA,
drums, samplers, Modular) and a voice parameter: `cutoff resonance vco1level
vco2level vco3level noiselevel ringlevel sublevel`; it can't `.lag`. Brackets
nest at most 32 deep.

```song
track lead synth Juno106 JunoPad
frag chords = lead
  "[c3,e3,g3] ~ [f3,a3,c4] ~"
mod lead.cutoff = env(perc).exprange(200, 4000) + lfo(3).range(0, 300)
mod lead.resonance = lfo([1, 3, 5]).range(0.1, 0.6)
mod strip1.pan = sine.slow(8).range(-1, 1)
mod master.p2return = perlin.slow(4).range(0.1, 0.4)
```

## section, arrange, loop

```text
section <name> <bars>: [<frag> | <auto> | [<scene>]]…      bars 1–256; "8:" or "8 :"
arrange <section>…                                         1–256 entries, repeats allowed
loop <first> <last>                                        bars of the arrangement, from 1, inclusive
```

A section is bars and what plays in them: each frag and auto from its first
bar, looping inside it; each scene on its first step. It may be empty (a
rest). `arrange` plays sections in order and the song stops after its last
bar. `loop` needs `arrange` and must end inside it: the song plays to `last`,
then repeats `first`–`last` for good.

```song
track kit drums
frag beat = kit /16
  bd x...x...x...x...
frag fill = kit /16
  sn ..x.
section intro 2: beat
section main 4 : beat fill
section gap 1:
arrange intro main main gap
loop 3 6
```

## Idioms

**Four on the floor:** a 909 with ghost snares, an open hat on the off-beat,
a flam and a fill to turn the phrase.

```song
tempo 124
swing 52
track kit drums Tr909 Kit909
strip kit: Level 0.9
frag beat = kit
  bd x...x...x...x...
  cp ....x.......x...
  sn ......o.....o..o
  oh ..x...x...x...x.
  ch x.xxx.xxx.xxx.xx
frag fill = kit
  bd x...x...x...x...
  sn f.o.x.o.x.x.xxXX
  cr X...............
section main 7: beat
section turn 1: fill
arrange main turn main turn
```

**Acid line:** sixteenths on an SH-101 `AcidBass`, accents and slides, the
cutoff on a lane that rises each section.

```song
tempo 128
track kit drums Tr909 Kit909
track bass synth Sh101 AcidBass
frag beat = kit
  bd x...x...x...x...
  oh ..x...x...x...x.
frag acid = bass
  "<[e1! e1 e2& e1 ~ e1! g1& e1 e2!& e1 ~ e1 a1& e1 g1!& e1] [e1! ~ e2& e1 ~ e1! b1& e1 e2! ~ g2& e2& d2& b1& a1& g1!]>"
auto cut = bass.Cutoff ramp 400 3200 /8
auto res = bass.Resonance 0.4 0.55 0.7 0.85 /4
section groove 8: beat acid cut res
arrange groove groove
```

**One progression:** a seeded progression voiced on a pad, its roots in the
bass and its chords arpeggiated, all following the `scale`.

```song
tempo 118
scale a minor
track pad synth Jupiter8 JupiterPad
track bass synth Sh101 Sh101Bass
track arp synth Juno106 JunoPluck
strip pad: Level 0.5, Send2 0.4
strip arp: Level 0.6, Send1 0.3
frag chords = pad voicing
  prog(8,11)
frag low = bass .cutoff(sine.slow(8).exprange(300, 1500))
  root(chords,1)
frag ripple = arp .off(3/16, add(12))
  arp(chords,updown,16)
section main 8: chords low ripple
arrange main main
```

**Polymeter:** percussion lanes of 3, 5, 7 and 12 steps over a straight kick,
and a euclid cowbell, so the pattern turns over slowly.

```song
tempo 122
track kit drums Tr909 Kit909
track perc drums Tr808 Kit808
frag beat = kit
  bd x...x...x...x...
  ch ..x.
frag poly = perc
  lc x..
  mc x.x..
  hc ..x..x.
  rs x..x..x.x.x.
  cb euclid(5,16,3)
section main 16: beat poly
arrange main
```

**Triplet feel and a live melody:** swing near 67, a /12 hat grid and a walk
that changes every bar.

```song
tempo 92
swing 66
scale d dorian
track kit drums
track lead synth Minimoog MiniLead
frag shuffle = kit /12
  bd x.....x.....
  sn ...x.....x..
  ch x.xx.xx.xx.x
frag tune = lead live
  walk(d4,6,5)
section main 8: shuffle tune
arrange main
```

**A Modular voice with a knob in motion:** a SynthDef sub whose cutoff (its
first knob, `Ctl1`) opens over the section, and a scene for the break.

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
frag kick = kit
  bd x...x...x...x...
frag roll = sub
  "~ g1 g1 g1 ~ g1 g1 bb1"
auto open = sub.Ctl1 ramp 200 1800 /8
scene bare: strip1.Mute 1
scene full: strip1.Mute 0
section main 8: kick roll open [full]
section break 4: roll [bare]
arrange main break main
```
