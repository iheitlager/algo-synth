# The song language

A song is text (ADR-0012). The composer shows it, the engine parses it and plays
it, and every edit in the view goes back through the engine, which prints the
text again. This page lists every keyword: its syntax, an example, its limits
and the errors it gives.

Every example tagged `song` here is a whole song; a test parses each one and
checks that its printed form parses back the same
(`crates/dsp/src/song/tests.rs`).

## The basics

One item to a line. A line starts with a keyword: `tempo`, `swing`, `scale`,
`setting`, `track`, `frag`, `auto`, `scene`, `section`, `arrange` or `loop`.
An indented line belongs to the `frag` above it: a lane of pads, or a line of
notes.

- **Comments:** `#` starts one at the start of a line or after a space, so the
  sharp in `c#4` stays a sharp. Comments are kept: the printer puts each back
  beside its item (#199).
- **Names** (tracks, settings, frags, autos, scenes, sections) are a letter,
  then letters, digits or `_`, at most 32 characters. Frags and autos share
  one set of names; the others each have their own.
- **Order:** a name is used after the line that makes it. A setting comes
  before the tracks, a track before its frags, the scale before the frags,
  frags, autos and scenes before the sections that hold them, and sections
  before `arrange`.
- **Errors:** the parser never panics. Text that is not a song gives the first
  problem found, with a line and a column, and the song that was playing plays
  on.
- **Canonical form:** the engine prints the song back in a fixed order and
  layout: tempo, swing, scale, settings, tracks, frags, autos, scenes,
  sections, arrange, loop. Blank lines and alignment are not kept, and flats
  print as sharps (`eb4` as `d#4`).

An empty text is a song too: 120 BPM, no swing, silence.

## tempo

`tempo <bpm>`, 20 to 300. Without it the song is at 120.

```song
tempo 124
```

## swing

`swing <percent>`, 50 (straight) to 75. It moves the off-beat sixteenths
later; about 67 is a triplet feel. Without it the song is at 50.

```song
tempo 96
swing 58
```

## scale

`scale <root> <mode>`: the key that walks and `scale` fills step through. The
root is a letter `a` to `g`, maybe `#` or `b`. The modes:

| mode | steps from the root |
|---|---|
| `major` | 0 2 4 5 7 9 11 |
| `minor` | 0 2 3 5 7 8 10 |
| `dorian` | 0 2 3 5 7 9 10 |
| `phrygian` | 0 1 3 5 7 8 10 |
| `lydian` | 0 2 4 6 7 9 11 |
| `mixolydian` | 0 2 4 5 7 9 10 |
| `locrian` | 0 1 3 5 6 8 10 |
| `pentatonic` | 0 2 4 7 9 |
| `blues` | 0 3 5 6 7 10 |
| `phrygian-dominant` | 0 1 4 5 7 8 10 |
| `harmonic-minor` | 0 2 3 5 7 8 11 |

A song has one scale, and it goes before the frags.

```song
scale e phrygian-dominant
track lead synth
frag wander = lead
  walk(e4,8,3)
```

## setting

`setting <name> = <Model> <Preset>[: <Param> <value>, …]` is a synth patch
that lives in the song (#210): a factory preset and the changes made to it. A
track plays it by name.

```song
setting nile = Minimoog MiniLead: Cutoff 1200, Resonance 0.5
setting plain = Tr909 Hard909
track lead synth nile
track kit drums plain
```

- The preset must be one of the model's. Models: `Arp2600`, `Minimoog`,
  `ProOne`, `Ms20`, `Cs15`, `Sh101`, `Odyssey`, `Prophet5`, `Juno106`,
  `Jupiter8`, `Matrix12`, `PpgWave`, `D50`, `Dx7`, `PolyMoog`, `Tr808`,
  `Tr909`, `Sampler`, `PadSampler`. Presets are the factory names, as
  `MiniBass` or `JunoPad`.
- The changes are the synth's own parameters by their registry name (ADR-0004),
  as `Cutoff` or `AdsrRelease`, in the parameter's units (Hz, seconds, 0–1);
  the engine clamps a value into its range. Mixer and master parameters
  (`Level`, `Pan`, `MasterGain`…) and `Model` are refused.
- Settings go before the tracks; a name may not be a model's.
- At most 16 settings, 32 changes each.

## track

`track <name> <kind> [<Model> [<Preset>] | <setting>]`. The kind says what the
track's frags hold:

| kind | frags hold | plays on |
|---|---|---|
| `drums` | lanes of pads | `Tr808`, `Tr909` or `PadSampler` |
| `synth` | one line of notes | any synth model |
| `sampler` | lanes of pads, or a line of notes | `PadSampler` (pads) or `Sampler` (notes) |

After the kind comes the synth (#210): a model and a preset, a model alone, a
setting, or nothing.

- **Model and preset:** `track bass synth Sh101 AcidBass`.
- **Model alone:** the preset is the track's role's on that model, else the
  model's first.
- **Nothing:** a model and preset are picked from the track's role and written
  into the text, and each track plays on a synth of its own. A `sampler` track
  without a model keeps the samples already loaded.

The role comes from the name first, then from the notes:

| role | the name holds | or the notes are | default preset |
|---|---|---|---|
| drums | (any `drums` track) | | `Kit808`, `Kit909` if the name holds `909` |
| bass | `bass`, `sub` | mostly below C3 | `MiniBass` |
| pad | `pad`, `chord`, `string`, `choir` | chords | `JunoPad` |
| arp | `arp`, `pluck`, `seq` | only `arp(...)` calls | `ShArp` |
| keys | `key`, `piano`, `organ` | | `FmElectricPiano` |
| lead | `lead`, `melod`, `solo` | anything else | `ProLead` |

```song
track kit drums
track kit909 drums
track bass synth Sh101 AcidBass
track pad synth
track keys synth Dx7
track low synth
frag deep = low
  "e1 f1 e2 b1"
```

This prints as `track kit drums Tr808 Kit808`, `track kit909 drums Tr909
Kit909`, `track pad synth Juno106 JunoPad`, `track keys synth Dx7
FmElectricPiano` and `track low synth Minimoog MiniBass` (a bass by register).

At most 16 tracks. Errors: a model that does not play the kind (`track kit
drums Minimoog`), a preset of another model, an unknown model or setting.

## frag

`frag <name> = <track> [/16 | live | bars <n>]`, then indented lines. A
fragment is a loop on one track: lanes on a drum track, one line of notes on a
synth track. A sampler frag holds either, not both.

At most 256 frags. A frag with nothing under it is an error.

### Lanes

On a `drums` or `sampler` track, each indented line is a pad and its steps:
`x` a hit, `X` an accent, `.` a rest. Spaces inside the steps are only for
reading. `/16` after the track says the steps are sixteenths; it is the only
grid and may be left out.

```song
tempo 124
track kit drums
frag beat = kit /16
  bd x...x...x...x...
  sn ....X.......X..x
  ch x.x. x.x. x.x. x.x.
  cb euclid(5,16,2)
  oh ..x
```

- **Pads:** `bd` kick, `sn` snare, `cp` clap, `ch` closed hat, `oh` open hat,
  `lt` `mt` `ht` toms, `rs` rimshot, `cl` claves, `ma` maracas, `cb` cowbell,
  `cy` cymbal, `lc` `mc` `hc` congas, `cr` crash, `rd` ride. The 808 plays its
  cymbal for `cr` and `rd`.
- **Polymeter:** each lane loops on its own length (above, `oh` every three
  sixteenths), 1 to 64 steps.
- **Euclid:** `euclid(hits,steps)` or `euclid(hits,steps,rotation)` spreads the
  hits as evenly as it can; 1 to 64 steps, no more hits than steps.
- One lane per pad in a frag.

### Notes

On a `synth` track the frag holds one indented line of notes, in one of five
forms, never mixed in a line. A `sampler` track reads a line as notes when it
is quoted, a chord or has a `:` (mini-notation, classic or timed); anything
else there is a lane.

Notes are a letter, maybe `#` or `b`, and an octave 0 to 9: `c4` is middle C (MIDI
60), `f#2`, `eb5`. A `!` after a note accents it: `c4!`. A line compiles to
at most 512 notes and repeats after at most 32 bars.

**Mini-notation:** one quoted cycle, one bar. The words share the bar.

| write | means |
|---|---|
| `c4 e4 g4` | three notes, a third of a bar each |
| `~` | a rest |
| `[e4 g4]` | a group: the two share one word's time |
| `[c4,e4,g4]` | a chord |
| `<c5 d5>` | one option per bar: `c5`, then `d5` the next bar |
| `c4*4` | four times in its share (1 to 16) |
| `c4@3` | three shares of the bar instead of one (1 to 48) |
| `c4?` | plays on some bars only, the same bars every run (repeats after 8) |

Brackets nest at most 4 deep.

```song
track lead synth
frag riff = lead
  "c4 [e4 g4] ~ <c5 d5>?"
frag stabs = lead
  "~ [a3!,c4!,e4!] ~ ~ ~ [a3,c4,e4]@3"
frag hats = lead
  "c6*4 ~ [c6 c6]*2 ~"
```

**Classic:** notes one after another, each with a duration: `:1` whole, `:2`
half, `:4` quarter, `:8` eighth, `:16` sixteenth, a `.` after it for one and a
half (not on a sixteenth). `r:4` rests, `[c4,e4,g4]:2` is a chord. The line
runs as many bars as its notes fill.

```song
track lead synth
frag tune = lead
  c4:4 e4:8 g4:8 [c4,e4,g4]:2 r:4. c5!:8
```

**Chords by name** (#103), in mini-notation and classic notes alike. A
symbol is a root, a letter with maybe `#` or `b` and an octave (4 when left
out), then `:` and a quality: `maj m 7 maj7 m7 m7b5 dim dim7 aug sus2 sus4 6
m6 9 m9 add9`. A bare root is a major chord; with an octave and no quality it
stays one note (`c3`), so write `c3:maj` for the chord. In classic notes the
last `:` is the duration: `c:m7:2`.

A roman numeral is a degree of the song's `scale` (it needs one with seven
notes). Its case is its quality, upper major and lower minor, then `o`
(diminished), `o7`, `+` (augmented), `7` or `maj7`; a `b` or `#` before it
shifts the root, for borrowed chords. Change the `scale` line and the whole
progression moves with it: `"<i VI III VII>"` plays Cm Ab Eb Bb in C minor,
Am F C G in A minor.

`voicing` at the end of the frag line moves each chord to the notes nearest
the chord before it, between C3 and C6, so the hands stay close. `arp` takes a
chord by name as well. A name prints back as written.

```song
scale c minor
track pad synth
frag prog = pad voicing
  "<i VI III VII>"
frag jazz = pad
  c:m7:2 f:7:2 bb:maj7:2 eb:maj7:2
frag turn = pad voicing
  "<ii7 V7 i bVII>"
frag ripple = pad
  arp(c:m9,updown,16)
```

**Timed:** `pitch@start:length[:velocity]` puts each note at a tick, 48 to the
bar, for a number of ticks, with an optional velocity of 1 to 127. Notes may
overlap. It is the form MIDI import writes. A start is under 32 bars (1536
ticks), a length 1 to 1536 ticks. The line ends at the bar of its last start,
or say how long it is with `bars <n>` (1 to 32) on the frag line.

```song
track v synth
frag line = v bars 2
  d5@0:6 f#5@6:6:90 a4@12:24 d4@48:48:64
```

**Euclid:** `euclid(hits,steps[,rotation]) <note>` plays the note on the hits.
With `scale` before the note, the hits walk up the song's scale from it
instead (it needs a `scale` line).

```song
scale a minor
track bass synth
frag pulse = bass
  euclid(5,8) a1
frag climb = bass
  euclid(7,16,2) scale a2
```

**Generators:** a call that writes the notes. Each is seeded, so the same
call gives the same notes every run.

| call | plays |
|---|---|
| `arp([c4,e4,g4],up,16)` | the chord, `up`, `down` or `updown`, at a rate of 2, 4, 8 or 16 notes a bar |
| `arp([c4,e4,g4],random,16,7)` | `random` order; only `random` takes a seed, and it must |
| `walk(c4,8,1)` | 8 notes (1 to 32) on a random walk on the song's scale, seed 1; needs a `scale` line |
| `markov(1,riff,3)` | a chain of order 1 to 3 learned from frag `riff`, keeping its rhythm and pitches, seed 3 |
| `mutate(riff,30,5)` | `riff` with 30 percent (0 to 100) of its notes moved or dropped, seed 5 |
| `prog(4,7)` | 4 bars (1 to 16) of triads in the song's key, seed 7: it starts on the tonic, ends on the dominant, and moves between tonic, subdominant and dominant chords as common practice does; needs a `scale` line with seven notes |
| `root(chords)` | the root of each chord of frag `chords`, in octave 2, as a bass line; `root(chords,3)` for another octave |
| `arp(chords,up,16)` | frag `chords` arpeggiated chord by chord, each from its own start (a frag's name wins over a chord name) |

A chord has at most 8 notes. `markov`, `mutate`, `root` and `arp` over a frag
read a note frag written above them, as it was when the song loaded: over a
`live` frag they follow its first bar's notes.

One progression can feed the pad, the bass and the arp:

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
```

```song
scale c minor
track lead synth
frag riff = lead
  "c4 eb4 g4 [bb4 g4]"
frag chain = lead
  markov(2,riff,3)
frag drift = lead
  mutate(riff,30,5)
frag up = lead
  arp([c4,eb4,g4],updown,16)
frag dice = lead
  arp([c4,eb4,g4,bb4],random,8,7)
```

### live

`frag <name> = <track> live` makes a generator call play new notes every bar:
the call's seed mixed with the bar, so a run is the same every time. Only a
call (`arp`, `walk`, `markov` or `mutate`) can be live, and only on a note
frag. **Freeze** in the composer replaces the call with the bar it was
playing, written as mini-notation.

```song
scale e phrygian-dominant
track lead synth
frag sand = lead live
  walk(e4,16,7)
```

## auto

`auto <name> = <target>.<Param> <values…> /<bars>` or
`auto <name> = <target>.<Param> ramp <from> <to> /<bars>`: an automation lane
(ADR-0015). Values are spread evenly over the lane's bars, each held for its
share; a ramp goes from one value to the other. A lane is placed in sections
like a frag and loops inside them.

The target is a track (its synth and its strip), `strip1`–`strip16`,
`group1`–`group8` or `master` (master gain, compressor, EQ and the
processors' `P1A`… `P2Return`…). The parameter is its registry name; it must
belong to the target. `Model` and `Out` can't be automated.

```song
track kit drums
frag beat = kit
  bd x...x...x...x...
auto sweep = kit.Cutoff ramp 300 4000 /8
auto duck = strip3.Level 1 0.5 0.25 1 /1
section main 8: beat sweep duck
arrange main
```

At most 32 autos, 64 values each, 1 to 256 bars. Without `arrange` every lane
loops on its own.

## scene

`scene <name>: <target>.<Param> <value>, …` sets values together on the first
step of each section that lists it as `[name]`. A scene is a jump, not a
fade. Targets and parameters are as for `auto`.

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

At most 32 scenes, 32 values each. Without `arrange` no scene is applied.

## section

`section <name> <bars>: <frags, autos and [scenes]…>`. A section is a number
of bars (1 to 256) and what plays in them. Each frag and auto starts at the
section's first bar and loops inside it; a scene lands on its first step. A
section may hold nothing (a gap of silence). `8:` and `8 :` both work.

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

At most 256 sections. A name listed twice in one section is an error.

## arrange

`arrange <sections…>`: the order sections play in; a section may play more
than once. One `arrange` line to a song, at most 256 entries. The song stops
after its last bar. Without `arrange` every frag and auto loops at once.

## loop

`loop <first> <last>`: repeat bars `first` to `last` of the arrangement,
counting from 1, both included. The song plays up to `last`, then goes back
to `first` for good. A loop needs an `arrange` line and must end inside it.

## Limits

| what | most |
|---|---|
| tracks, settings | 16 |
| changes in a setting, values in a scene | 32 |
| frags, sections, entries in `arrange` | 256 |
| steps in a lane | 64 |
| notes a line compiles to | 512 |
| bars before a line of notes repeats | 32 |
| autos, scenes | 32 |
| values in an auto | 64 |
| bars in a section or an auto | 256 |
| tempo, swing | 20–300, 50–75 |
| song text | 1 MB |

## A worked example

Sand and Nile: a maqsum beat on an 808, a Minimoog bass, an SH-101 "oud" from
a setting, a live arp and a pad, in E phrygian dominant.

```song
# Sand and Nile
tempo 104
swing 54
scale e phrygian-dominant

setting oud = Sh101 Sh101Lead: Cutoff 1800, Resonance 0.45, Glide 0.05
track kit drums Tr808 Kit808
track bass synth Minimoog MiniBass
track lead synth oud
track arp synth
track pad synth

# maqsum: dum tek . tek dum . tek .
frag beat = kit /16
  bd x.......x.......
  lc ..x...x.....x...
  ma x.x.x.x.x.x.x.x.
  hc euclid(5,16,3)
frag roll = kit /16
  lc ..x...x.....x...
  hc x.x.x.x.xxxxXXXX

frag groove = bass
  "e2 ~ [e2 f2] ~ e2 ~ [g#2 f2] e2"
frag call = lead
  e4:8 f4:8 g#4:4 a4:8 g#4:8 f4:4 e4:2 r:2
frag answer = lead
  mutate(call,25,3)
frag sand = arp live
  arp([e4,g#4,b4,d5],random,16,7)
frag hold = pad
  "[e3,g#3,b3] <[f3,a3,c4] [d3,f3,a3]>"

auto open = lead.Cutoff ramp 600 3000 /8
scene bare: bass.Mute 1, pad.Level 0.4
scene full: bass.Mute 0, pad.Level 0.8

section intro 4: hold beat
section theme 8: beat groove call open [full]
section reply 8: beat groove answer sand
section desert 4: roll hold sand [bare]
section outro 2: hold
arrange intro theme reply desert theme reply outro
loop 5 28
```
