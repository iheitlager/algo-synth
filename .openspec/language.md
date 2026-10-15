# The song language

The normative definition of algo-synth's song text: what the engine's parser
accepts, what it means, its limits and its errors, with examples. Where it and
the parser differ, the parser decides and this file is fixed. Every block
tagged `song` is a whole song that parses and prints back the same.

It comes in two parts. This one is the composer's: the conventions and limits
both parts share, then tempo, scale, tracks, clips, autos, snapshots, mods
and scenes. [sound-design.md](sound-design.md) is the sound designer's:
`setting` and its Modular code, `samples`, `zone` and `pad`, `table` and `attack`, and the mix
(`strip`, `group`, `master`).

Grammar lines: `<x>` a placeholder, `[x]` optional, `x…` one or more, `a | b`
a choice; anything else is literal.

## Conventions

- **Lines.** One item per line. A top-level line starts at column 1 with a
  keyword: `tempo swing scale setting track samples zone pad table attack
  strip group master clip auto snapshot mod scene arrange loop`. An indented line (space or tab) belongs to the
  `clip` above it, or is code of the `Modular` `setting` above it. Blank lines
  are ignored. Words are split at whitespace.
- **Comments.** `#` at the start of a line or after whitespace starts a comment
  (so `c#4` is a note), also inside quotes; not in Modular code. Comments are
  kept: a comment line belongs to the item below it, a trailing one to its line.
- **Names** (settings, tracks, clips, autos, snapshots, scenes, groups):
  `[A-Za-z][A-Za-z0-9_]*`, at most 32 chars. Clips and autos share one
  namespace; the others have one each. A setting is not named as a model.
- **Parameters** are registry names (`Cutoff`, `Level`, `P2Return`), matched in
  any case, values in the parameter's units (Hz, seconds, 0–1), clamped to its
  range. The catalog lists each model's presets and each parameter's range.
- **Order.** A name is used after the line that makes it: settings before any
  track; `scale` before any clip; a track before its clips, mixer line and
  targets; a clip before the clips that read it and the scenes that hold it;
  autos and snapshots before their scenes; scenes before `arrange`.
- **Once.** At most one `tempo`, `swing`, `scale` and `arrange`; one mixer line
  per strip, group or master; one `mod` per target and parameter; one lane per
  pad in a clip; a name once in a scene.
- **Defaults.** Empty text is a song: tempo 120, swing 50, silence. Without
  `arrange` every clip and auto loops from the start and no snapshot applies.
- **Canonical form.** The printer writes: tempo, swing, scale, settings, tracks,
  mixer lines, clips, autos, snapshots, mods, scenes, arrange, loop. It always
  writes `tempo` and `swing`, each track's model and preset, a drum clip's
  grid, flats as sharps (`eb4` → `d#4`), `mod` parameters in lower case, and
  Modular code as written. Layout and blank lines are not kept. Chord names
  print as written.
- **Errors.** The parser never panics. Text that is not a song gives the first
  problem: line, column (from 1) and a message; the playing song plays on.
- **Ableton's words** (ADR-0031, #486). A `clip` is one track's loop
  (Ableton's clip), a `scene` the bars and clips that play together (Ableton's
  scene, what Launch starts), a `snapshot` mixer values set as a scene starts.
  Songs written before used other words, which the parser still reads and the
  printer writes in today's:

  | Before | Now |
  |---|---|
  | `frag` | `clip` |
  | `section` | `scene` |
  | `scene <name>: …` (no bars) | `snapshot <name>: …` |

### Limits

| what | range |
|---|---|
| song text | 1 MB |
| tempo / swing | 20–300 BPM / 50–75 % |
| tracks, settings | 16 each |
| changes in a setting, values in a snapshot | 32 |
| values on a mixer line | 48 |
| clips, scenes, `arrange` entries, bars in a scene or auto | 256 |
| steps in a lane | 1–64 |
| notes a line compiles to / bars before it repeats | 512 / 32 |
| words in a line of notes | 1100 |
| autos, snapshots, `mod` lines plus parameter methods | 32 each |
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
scale <root> <mode>  |  scale <root>:<mode>
root  = a–g [# | b] [octave 0–9]
mode  = a name from the table below; Strudel's colons stand for its dashes: c:minor:pentatonic
```

The song's key, read by `walk`, `prog`, roman numerals, `euclid … scale` and
degrees in a clip without `.scale`, counted from the root in its octave, or
octave 3 when none is written (`scale c1 chromatic`, #575); the rest read
only the root's note name. The modes are
Strudel's, which are tonal's 92 scales (#576), named as words a song can hold:
tonal's spaces are dashes, `#n` is `sharp-n` (a `#` starts a comment), and
Messiaen's modes are `messiaen-1`…`messiaen-7`. A mode goes by any of its
names and prints by the first. Only the seven-note modes have degrees for
`prog` and roman numerals. A name that is not a mode is an error that offers
the nearest one.

| notes | mode (also) | semitones |
|---|---|---|
| 5 | `pentatonic` (`major-pentatonic`) | 0 2 4 7 9 |
| 5 | `ionian-pentatonic` | 0 4 5 7 11 |
| 5 | `mixolydian-pentatonic` (`indian`) | 0 4 5 7 10 |
| 5 | `ritusen` | 0 2 5 7 9 |
| 5 | `egyptian` | 0 2 5 7 10 |
| 5 | `neapolitan-major-pentatonic` | 0 4 5 6 10 |
| 5 | `vietnamese-1` | 0 3 5 7 8 |
| 5 | `pelog` | 0 1 3 7 8 |
| 5 | `kumoijoshi` | 0 1 5 7 8 |
| 5 | `hirajoshi` | 0 2 3 7 8 |
| 5 | `iwato` | 0 1 5 6 10 |
| 5 | `in-sen` | 0 1 5 7 10 |
| 5 | `lydian-pentatonic` (`chinese`) | 0 4 6 7 11 |
| 5 | `malkos-raga` | 0 3 5 8 10 |
| 5 | `locrian-pentatonic` (`minor-seven-flat-five-pentatonic`) | 0 3 5 6 10 |
| 5 | `minor-pentatonic` (`vietnamese-2`) | 0 3 5 7 10 |
| 5 | `minor-six-pentatonic` | 0 3 5 7 9 |
| 5 | `flat-three-pentatonic` (`kumoi`) | 0 2 3 7 9 |
| 5 | `flat-six-pentatonic` | 0 2 4 7 8 |
| 5 | `scriabin` | 0 1 4 7 9 |
| 5 | `whole-tone-pentatonic` | 0 4 6 8 10 |
| 5 | `lydian-sharp-5p-pentatonic` | 0 4 6 8 11 |
| 5 | `lydian-dominant-pentatonic` | 0 4 6 7 10 |
| 5 | `minor-sharp-7m-pentatonic` | 0 3 5 7 11 |
| 5 | `super-locrian-pentatonic` | 0 3 4 6 10 |
| 6 | `major-blues` | 0 2 3 4 7 9 |
| 6 | `blues` (`minor-blues`) | 0 3 5 6 7 10 |
| 6 | `minor-hexatonic` | 0 2 3 5 7 11 |
| 6 | `augmented` | 0 3 4 7 8 11 |
| 6 | `piongio` | 0 2 5 7 9 10 |
| 6 | `prometheus-neapolitan` | 0 1 4 6 9 10 |
| 6 | `prometheus` | 0 2 4 6 9 10 |
| 6 | `mystery-1` | 0 1 4 6 8 10 |
| 6 | `six-tone-symmetric` | 0 1 4 5 8 9 |
| 6 | `whole-tone` (`messiaen-1`) | 0 2 4 6 8 10 |
| 6 | `messiaen-5` | 0 1 5 6 7 11 |
| 7 | `major` (`ionian`) | 0 2 4 5 7 9 11 |
| 7 | `minor` (`aeolian`) | 0 2 3 5 7 8 10 |
| 7 | `melodic-minor` | 0 2 3 5 7 9 11 |
| 7 | `harmonic-minor` | 0 2 3 5 7 8 11 |
| 7 | `dorian` | 0 2 3 5 7 9 10 |
| 7 | `lydian` | 0 2 4 6 7 9 11 |
| 7 | `mixolydian` (`dominant`) | 0 2 4 5 7 9 10 |
| 7 | `phrygian` | 0 1 3 5 7 8 10 |
| 7 | `locrian` | 0 1 3 5 6 8 10 |
| 7 | `locrian-major` (`arabian`) | 0 2 4 5 6 8 10 |
| 7 | `double-harmonic-lydian` | 0 1 4 6 7 8 11 |
| 7 | `altered` (`super-locrian`, `diminished-whole-tone`, `pomeroy`) | 0 1 3 4 6 8 10 |
| 7 | `locrian-sharp-2` (`half-diminished`, `aeolian-b5`) | 0 2 3 5 6 8 10 |
| 7 | `mixolydian-b6` (`melodic-minor-fifth-mode`, `hindu`) | 0 2 4 5 7 8 10 |
| 7 | `lydian-dominant` (`lydian-b7`, `overtone`) | 0 2 4 6 7 9 10 |
| 7 | `lydian-augmented` | 0 2 4 6 8 9 11 |
| 7 | `dorian-b2` (`phrygian-sharp-6`, `melodic-minor-second-mode`) | 0 1 3 5 7 9 10 |
| 7 | `ultralocrian` (`superlocrian-bb7`, `superlocrian-diminished`) | 0 1 3 4 6 8 9 |
| 7 | `locrian-6` (`locrian-natural-6`, `locrian-sharp-6`) | 0 1 3 5 6 9 10 |
| 7 | `augmented-heptatonic` | 0 3 4 5 7 8 11 |
| 7 | `dorian-sharp-4` (`ukrainian-dorian`, `romanian-minor`, `altered-dorian`) | 0 2 3 6 7 9 10 |
| 7 | `lydian-diminished` | 0 2 3 6 7 9 11 |
| 7 | `leading-whole-tone` | 0 2 4 6 8 10 11 |
| 7 | `lydian-minor` | 0 2 4 6 7 8 10 |
| 7 | `phrygian-dominant` (`spanish`, `phrygian-major`) | 0 1 4 5 7 8 10 |
| 7 | `balinese` | 0 1 3 5 7 8 11 |
| 7 | `neapolitan-major` | 0 1 3 5 7 9 11 |
| 7 | `harmonic-major` | 0 2 4 5 7 8 11 |
| 7 | `double-harmonic-major` (`gypsy`) | 0 1 4 5 7 8 11 |
| 7 | `hungarian-minor` | 0 2 3 6 7 8 11 |
| 7 | `hungarian-major` | 0 3 4 6 7 9 10 |
| 7 | `oriental` | 0 1 4 5 6 9 10 |
| 7 | `flamenco` | 0 1 3 4 6 7 10 |
| 7 | `todi-raga` | 0 1 3 6 7 8 11 |
| 7 | `persian` | 0 1 4 5 6 8 11 |
| 7 | `enigmatic` | 0 1 4 6 8 10 11 |
| 7 | `major-augmented` (`major-sharp-5`, `ionian-augmented`, `ionian-sharp-5`) | 0 2 4 5 8 9 11 |
| 7 | `lydian-sharp-9` | 0 3 4 6 7 9 11 |
| 8 | `bebop` | 0 2 4 5 7 9 10 11 |
| 8 | `diminished` (`whole-half-diminished`) | 0 2 3 5 6 8 9 11 |
| 8 | `messiaen-4` | 0 1 2 5 6 7 8 11 |
| 8 | `purvi-raga` | 0 1 4 5 6 7 8 11 |
| 8 | `spanish-heptatonic` | 0 1 3 4 5 7 8 10 |
| 8 | `bebop-minor` | 0 2 3 4 5 7 9 10 |
| 8 | `bebop-major` | 0 2 4 5 7 8 9 11 |
| 8 | `bebop-locrian` | 0 1 3 5 6 7 8 10 |
| 8 | `minor-bebop` | 0 2 3 5 7 8 10 11 |
| 8 | `ichikosucho` | 0 2 4 5 6 7 9 11 |
| 8 | `minor-six-diminished` | 0 2 3 5 7 8 9 11 |
| 8 | `half-whole-diminished` (`dominant-diminished`, `messiaen-2`) | 0 1 3 4 6 7 9 10 |
| 8 | `kafi-raga` | 0 3 4 5 7 9 10 11 |
| 8 | `messiaen-6` | 0 2 4 5 6 8 10 11 |
| 9 | `composite-blues` | 0 2 3 4 5 6 7 9 10 |
| 9 | `messiaen-3` | 0 2 3 4 6 7 8 10 11 |
| 10 | `messiaen-7` | 0 1 2 3 5 6 7 8 9 11 |
| 12 | `chromatic` | 0 1 2 3 4 5 6 7 8 9 10 11 |

```song
scale e phrygian-dominant
track lead synth
clip wander = lead
  walk(e4,8,3)
```

```song
scale c chromatic
track lead synth
clip run = lead
  "0 .. 12"
```

## track

```text
track <name> <kind> [<Model> [<Preset>] | <setting>] [mute] [solo]
kind = drums | synth | sampler
```

| kind | its clips hold | models |
|---|---|---|
| `drums` | lanes | `Tr808 Tr909 PadSampler Nes` (`Nes NesKit`; its `samples` kit or `pad` lines play through the DMC) |
| `synth` | one line of notes | any but drum machines and samplers |
| `sampler` | lanes (`PadSampler`) or notes (`Sampler`) | `Sampler PadSampler` |

A `<setting>` is a patch the song keeps (see
[sound-design.md](sound-design.md#setting)). With a model alone the preset is the track's role's on that model, else the
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

`mute` stops the track's clips; while any track is `solo`, only soloed ones
play. Errors: a model that does not play the kind, a preset of another model,
an unknown model or setting.

```song
track kit909 drums
track bass synth Sh101 AcidBass
track pad synth
track keys synth Dx7 mute
clip deep = bass
  "e1 f1 e2 b1"
```

## clip

```text
clip <name> = <track> [/<grid> | live | bars <n>] [voicing] [.<method>(<args>)]…
  <lane>…            on a drums track, or a sampler track
  <line of notes>    exactly one, on a synth track, or a sampler track
```

A clip is a loop on one track. A sampler clip holds lanes or notes, not
both; a line there is notes when it starts with `"` or `[`, or holds `:` or
`(`. A clip with nothing under it is an error. `/grid` is for lanes, `live`
and `voicing` for notes, `bars` for timed notes; methods come last. A clip of
notes, on a synth or a sampler track, takes no grid but the default `/16`:
`a note clip has no step grid`.

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
soft grace 20 ms before its hit, a drag two (30 and 15 ms). Every clip keeps
its own grid in time with the others; swing moves a step's hits with it. A
digit after `x`, `X` or `o` is a ratchet: `x3` plays the hit three times,
evenly across its step (2 to 4), and counts as one step.

```song
tempo 124
track kit drums
clip beat = kit /16
  bd x...x...x...x...
  sn ....X.......X..x
  ch x.x. x.x. x.x. x.x.
  cb euclid(5,16,2)
  oh ..x
clip roll = kit /32
  sn o.o.o.o.x.x.x.x.xxxxxxxxXXXXXXXX
clip trip = kit /24
  sn ..x..x..x..x..x..X..X..X
clip rudiments = kit
  sn f...o.o.d...o.o.f.f.d.d.X...X...
clip rolls = kit
  sn x...x3..x...X4..
  ch x2.x.x2.x.x3.x.o2.x.
```

### Notes

A pitch is `a`–`g`, maybe `#` or `b`, an octave `0`–`9`: `c4` is middle C
(MIDI 60). `!` right after it accents it (`c4!`: louder; on a TB-303 it
opens the filter through the accent sweep). A line is one of five forms,
never mixed; 48 ticks make a bar.

A number where a pitch goes is a **degree** of the scale, `-24` to `24`: `0`
is the root, `-1` the scale note below it, and past the last degree it goes
on into the next octave. The clip's `.scale(a1 minor-pentatonic)` makes the
degrees notes, after the methods before it (so `.add(1)` before it moves a
degree up one); without `.scale`, the song's `scale` line does, from its root
in octave 3 (or the octave on the line); with neither, a degree is an error.
Not in generators, a scale walk or a `voicing` clip. `0 .. 12` (or `0..12`)
is every degree from the first to the last, downward when the last is lower,
sharing the word's time as a `[ ]` group would; suffixes after it are the
range's (`0 .. 3@2`), and the start takes none.

**Mini-notation:** one quoted cycle is one bar; its words share it.

```text
"<word>…"
word   = <pitch> | <degree> | <degree> .. <degree> | ~ | [<word>…] | [<pitch>,<pitch>…] | <<word>…> | <chord name>, then suffixes
suffix = *n repeat in its share (1–16) | @n weight in shares (1–48) | ? some bars only | & slide
```

`[a b]` groups share one word's time, `[c4,e4,g4]` is a chord, `<a b>` plays
one option per bar, `?` plays on a fixed, seeded set of bars (repeats after 8).
Suffixes come in any order, each once. Brackets nest at most 4 deep. `&`
(slide: the note runs one tick into the next; with Legato and Glide, as on
`AcidBass` and the TB-303's presets, the pitch glides on one gate, also into
the same note) goes on a note or chord, not a rest or group.

```song
track lead synth
clip riff = lead
  "c4 [e4 g4] ~ <c5 d5>?"
clip stabs = lead
  "~ [a3!,c4!,e4!] ~ ~ ~ [a3,c4,e4]@3"
clip slide = lead
  "c2& c3 ~ c2*2 eb2&"
clip arp = lead .fast(8) .scale(a1 minor-pentatonic)
  "<0 2 1 3 2>"
```

**Classic:** notes one after another, each with a duration.

```text
<beat>…    beat = (<pitch> | [<pitch>,…] | <chord name> | r):<dur>[.][&]    dur = 1 2 4 8 16
```

`:4` a quarter; `.` one and a half (not on `:16`); `r` rests; `&` slides. The
line runs as many bars as it fills.

```song
track lead synth
clip tune = lead
  c4:4 e4:8 g4:8 [c4,e4,g4]:2 r:4. c5!:8 c4:8&
```

**Timed:** each note at a tick, for a length, maybe a velocity; notes may
overlap. A line containing `@` outside quotes is timed.

```text
<pitch>[!]@<start>:<length>[:<velocity>]…     start 0–3071, length 1–3072, velocity 1–127
```

The line ends at the bar of its last start, or `bars <n>` (1–64) on the clip
line says how long it is.

```song
track v synth
clip line = v bars 2
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
- `voicing` on the clip line moves each chord to the notes nearest the chord
  before it, within C3–C6.

```song
scale c minor
track pad synth
clip prog = pad voicing
  "<i VI III VII>"
clip jazz = pad
  c:m7:2 f:7:2 bb:maj7:2 eb:maj7:2
clip turn = pad voicing
  "<ii7 V7 i bVII>"
```

**Euclid:** `euclid(<hits>,<steps>[,<rotation>]) <pitch>` plays the pitch on
the hits, the steps sharing one bar; `euclid(…) scale <pitch>` walks up the
song's scale from the pitch, hit by hit.

```song
scale a minor
track bass synth
clip pulse = bass
  euclid(5,8) a1
clip climb = bass
  euclid(7,16,2) scale a2
```

**Generators:** a call that writes the notes; the same call gives the same
notes every run.

| call | plays |
|---|---|
| `arp(<chord>,<mode>,<rate>)` | a chord (`[c4,e4,g4]` up to 8 notes, or a name) over one bar; mode `up` `down` `updown`; rate 2, 4, 8 or 16 notes a bar |
| `arp(<chord>,random,<rate>,<seed>)` | random order; only `random` takes a seed, and must |
| `arp(<clip>,<mode>,<rate>[,<seed>])` | the clip's chords, each arpeggiated from its own start (a clip name wins over a chord name) |
| `walk(<pitch>,<n>,<seed>)` | n notes (1–32) in one bar, a random walk on the scale; needs `scale` |
| `markov(<order>,<clip>,<seed>)` | a chain of order 1–3 learned from the clip's pitches and rhythm |
| `mutate(<clip>,<percent>,<seed>)` | the clip with 0–100 % of its notes moved or dropped |
| `prog(<bars>,<seed>)` | 1–16 bars of triads, one a bar: tonic first, dominant last, functional moves; needs a seven-note `scale` |
| `root(<clip>[,<octave>])` | the root of each of the clip's chords in octave 0–7 (default 2), a bass line |

A clip a call reads is a note clip above it, as it was at load (a `live`
clip's first bar).

```song
scale c minor
track pad synth
track bass synth
track arp synth
clip chords = pad voicing
  prog(4,7)
clip low = bass
  root(chords)
clip ripple = arp
  arp(chords,updown,16)
clip riff = arp
  "c4 eb4 g4 [bb4 g4]"
clip chain = arp
  markov(2,riff,3)
clip drift = arp
  mutate(riff,30,5)
clip dice = arp
  arp(c:m9,random,8,7)
```

**live:** `clip <name> = <track> live` with a generator call (any of the
table) plays new notes every bar: the seed mixed with the bar, the same every
run. Not on lanes, other forms of notes, `voicing` or pattern methods.

```song
scale e phrygian-dominant
track lead synth
clip sand = lead live
  walk(e4,16,7)
```

### Pattern methods

Written last on a note clip's line; applied in order to its notes at load. A
cycle is a bar; times round to the tick grid; the result keeps the limits of
512 notes and 64 bars. Not on lanes or `live` clips.

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
| `.euclid(k, n[, rot])` | `struct` with k hits spread over n (1–48) steps, rotated left by rot: `euclid(5,8)` is `struct("x ~ x x ~ x x ~")` |
| `.sometimes(m)` | m on about half the moments, seeded |
| `.scale(<root> <mode>)` | each note to the nearest note of the scale (down on a tie); each degree a note, from the root in its octave (`a1`) or octave 3 (`a`) |
| `.scale(<<root>…>[/n] [<mode>])` | a progression: the roots (up to 16) take turns, a bar each or n (1–16) bars each, by the bar a note starts in; `a1:dorian` gives a root its own mode, and the shared mode may go when each has one |
| `.scale(<clip> <mode>)` | the roots from an earlier clip (ADR-0035): each note takes, as degree 0, the lowest note the clip plays where the note starts, in its octave; the two loop round together (at most 64 bars), and before the clip's first note its last chord sounds; a clip's name wins over a root |
| `.oct(n)` | n (−4 to 4) octaves up or down; after `.scale`, since it moves notes, not degrees |

Write the harmony once and let other clips follow it: one edit to it moves
them all at the same bar.

```song
track bass synth
track lead synth
clip pedal = bass .slow(4)
  "<a1 d1>"
clip arp = lead .fast(8) .scale(pedal minor-pentatonic) .oct(1)
  "<0 2 1 3 2>"
```

The `m` of `every`, `off` and `sometimes` is a method without its dot:
`rev`, `fast(2)`, `add(12)`.

```song
track lead synth
clip riff = lead .fast(2) .every(4, rev) .off(1/8, add(12))
  "c4 eb4 g4 bb4"
clip grid = lead .struct("x ~ x x ~ x x ~") .degrade(0.2) .scale(c minor)
  "c4 d4 e4 f4"
clip arp = lead .fast(8) .scale(<a1 d2>/4 minor-pentatonic)
  "<0 2 1 3 2>"
```

A progression runs the line on until both come round together: the `arp`
above is 5 bars of degrees against 8 of roots, so it is 40 bars long and
its five-step cycle runs on unbroken through the change of root.

### Parameter methods

`.<param>(<signal>)` on a clip line, after pattern methods, moves a parameter
of the clip's track (its synth or strip) while the clip plays, and puts it
back when it leaves. One per parameter per clip; a pattern method's name wins.
They count with `mod` lines toward 32.

A `~` in a sequence is a step with no value: the parameter has its own value
there, so `.cutoff("~ ~ 800 ~")` locks the cutoff on one step of four and lets
go after it (a per-step lock). Anything combined with a `~` has no value on that
step either. A per-voice signal takes no `~`.

```song
track kit drums
track bass synth Sh101
clip beat = kit /16 .send1(0.3)
  bd x...x...x...x...
clip acid = bass .cutoff(sine.slow(4).exprange(300, 3000)) .resonance(0.7)
  "c2 c2 eb2 <g2 bb1>"
clip plain = bass .cutoff("<400 900>")
  "c2 ~ c2 ~"
clip lock = kit .send1("~ ~ ~ ~ ~ ~ 0.6 ~ ~ ~ ~ ~ ~ ~ 0.6 ~")
  sn ....x.......x...
scene a 4: beat acid
scene b 4: beat plain
arrange a b
```

## Targets

`auto`, `snapshot` and `mod` write `<target>.<Param>`:

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
held for its share, or a linear ramp. Placed in scenes like a clip, it
starts at the scene's first bar and loops inside it.

```song
track kit drums
clip beat = kit
  bd x...x...x...x...
auto sweep = kit.Cutoff ramp 300 4000 /8
auto duck = strip1.Level 1 0.5 0.25 1 /1
scene main 8: beat sweep duck
arrange main
```

## snapshot

```text
snapshot <name>: <target>.<Param> <value>[, <target>.<Param> <value>]…
```

Values set together, as a jump, on the first step of each scene that lists
`[name]`. 1–32 values.

```song
track kit drums
track bass synth
clip beat = kit
  bd x...x...x...x...
snapshot drop: strip1.Mute 1, master.P2Return 0.4
snapshot back: strip1.Mute 0, bass.Cutoff 900
scene main 4: beat [back]
scene quiet 4: beat [drop]
arrange main quiet main
```

## mod

```text
mod <target>.<param> = <signal>
```

A signal written to a parameter for the whole song, once a block (128
samples), after autos and snapshots, so it wins over them. It follows the song's
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
| `"<300 800 1200>"` / `"0 0.5 1"` | numbers, one a bar / sharing the bar (at most 64); `~` a step with no value |
| `env(adsr)` `env(perc)` `env(<a>, <d>, <s>, <r>)` | per voice: an envelope per note, the synth's ADSR, a short one, or seconds |
| `[a, b, …]` `lfo([a, b, …])` | per voice: number or LFO rate i for voice slot i, round the list (at most 16) |

| method | |
|---|---|
| `.range(a, b)` | 0..1 onto a..b |
| `.exprange(a, b)` | the same, exponentially; a, b of one sign, not 0 |
| `.slow(n)` `.fast(n)` | n (> 0) times slower or faster |
| `.segment(n)` | n steady values a bar |
| `.lag(s)` | follow the input smoothly, s seconds |
| `.onNote` | sampled as each note of the track starts, held for the note: `.wt1pos(rand.onNote)` gives each note its own timbre |

**Strudel's parameter names** stand for ours in a `mod` line and a parameter
method, printed as written: `lpf` `ctf` (Cutoff), `lpq` (Resonance), `hpf`
(HpCutoff), `hpq` (HpResonance), `attack` `att`, `decay` `dec`, `sustain`
`sus`, `release` `rel` (the ADSR), `gain` (Level), `room` (Send2, the reverb),
`delay` (Send1, the echo). Values are at our scale: a resonance is 0..1, not a
Q; a `pan` is −1..1, not 0..1. Strudel's `distort` has no one parameter here.

A per-voice signal goes on a track whose synth is Mono or Poly (not FM, LA,
drums, samplers, Modular) and a voice parameter: `cutoff resonance vco1level
vco2level vco3level noiselevel ringlevel sublevel wt1pos wt2pos`; it can't `.lag`.
`.onNote` modulates a track; on a voice parameter of a Mono or Poly synth
each voice holds its note's value, on any other the synth holds it until the
next note. It can't take `env` or a list. Brackets
nest at most 32 deep.

```song
track lead synth Juno106 JunoPad
clip chords = lead
  "[c3,e3,g3] ~ [f3,a3,c4] ~"
mod lead.cutoff = env(perc).exprange(200, 4000) + lfo(3).range(0, 300)
mod lead.resonance = lfo([1, 3, 5]).range(0.1, 0.6)
mod strip1.pan = sine.slow(8).range(-1, 1)
mod master.p2return = perlin.slow(4).range(0.1, 0.4)
```

## scene, arrange, loop

```text
scene <name> <bars>: [<clip> | <auto> | [<snapshot>]]…      bars 1–256; "8:" or "8 :"
arrange <scene>…                                           1–256 entries, repeats allowed
loop <first> <last>                                        bars of the arrangement, from 1, inclusive
```

A scene is bars and what plays in them: each clip and auto from its first
bar, looping inside it; each snapshot on its first step. It may be empty (a
rest). `arrange` plays scenes in order and the song stops after its last
bar. `loop` needs `arrange` and must end inside it: the song plays to `last`,
then repeats `first`–`last` for good.

```song
track kit drums
clip beat = kit /16
  bd x...x...x...x...
clip fill = kit /16
  sn ..x.
scene intro 2: beat
scene main 4 : beat fill
scene gap 1:
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
clip beat = kit
  bd x...x...x...x...
  cp ....x.......x...
  sn ......o.....o..o
  oh ..x...x...x...x.
  ch x.xxx.xxx.xxx.xx
clip fill = kit
  bd x...x...x...x...
  sn f.o.x.o.x.x.xxXX
  cr X...............
scene main 7: beat
scene turn 1: fill
arrange main turn main turn
```

**Acid line:** sixteenths on a TB-303, accents and slides, the cutoff on a
lane that rises each scene.

```song
tempo 128
track kit drums Tr909 Kit909
track bass synth Tb303 Tb303Acid
clip beat = kit
  bd x...x...x...x...
  oh ..x...x...x...x.
clip acid = bass
  "<[e1! e1 e2& e1 ~ e1! g1& e1 e2!& e1 ~ e1 a1& e1 g1!& e1] [e1! ~ e2& e1 ~ e1! b1& e1 e2! ~ g2& e2& d2& b1& a1& g1!]>"
auto cut = bass.Cutoff ramp 400 3200 /8
auto res = bass.Resonance 0.4 0.55 0.7 0.85 /4
scene groove 8: beat acid cut res
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
clip chords = pad voicing
  prog(8,11)
clip low = bass .cutoff(sine.slow(8).exprange(300, 1500))
  root(chords,1)
clip ripple = arp .off(3/16, add(12))
  arp(chords,updown,16)
scene main 8: chords low ripple
arrange main main
```

**Polymeter:** percussion lanes of 3, 5, 7 and 12 steps over a straight kick,
and a euclid cowbell, so the pattern turns over slowly.

```song
tempo 122
track kit drums Tr909 Kit909
track perc drums Tr808 Kit808
clip beat = kit
  bd x...x...x...x...
  ch ..x.
clip poly = perc
  lc x..
  mc x.x..
  hc ..x..x.
  rs x..x..x.x.x.
  cb euclid(5,16,3)
scene main 16: beat poly
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
clip shuffle = kit /12
  bd x.....x.....
  sn ...x.....x..
  ch x.xx.xx.xx.x
clip tune = lead live
  walk(d4,6,5)
scene main 8: shuffle tune
arrange main
```

**A whole song:** Sand and Nile, a maqsum beat on an 808, a Minimoog bass, an
SH-101 "oud" from a setting, a live arp and a pad, in E phrygian dominant.

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
clip beat = kit /16
  bd x.......x.......
  lc ..x...x.....x...
  ma x.x.x.x.x.x.x.x.
  hc euclid(5,16,3)
clip roll = kit /16
  lc ..x...x.....x...
  hc x.x.x.x.xxxxXXXX

clip groove = bass
  "e2 ~ [e2 f2] ~ e2 ~ [g#2 f2] e2"
clip call = lead
  e4:8 f4:8 g#4:4 a4:8 g#4:8 f4:4 e4:2 r:2
clip answer = lead
  mutate(call,25,3)
clip sand = arp live
  arp([e4,g#4,b4,d5],random,16,7)
clip hold = pad
  "[e3,g#3,b3] <[f3,a3,c4] [d3,f3,a3]>"

auto open = lead.Cutoff ramp 600 3000 /8
snapshot bare: bass.Mute 1, pad.Level 0.4
snapshot full: bass.Mute 0, pad.Level 0.8

scene intro 4: hold beat
scene theme 8: beat groove call open [full]
scene reply 8: beat groove answer sand
scene desert 4: roll hold sand [bare]
scene outro 2: hold
arrange intro theme reply desert theme reply outro
loop 5 28
```
