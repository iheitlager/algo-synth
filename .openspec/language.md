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
| `drums` | lanes | `Tr808 Tr909 PadSampler Nes VlTone` (`Nes NesKit`; its `samples` kit or `pad` lines play through the DMC; `VlTone VlKit`, the VL-1's Po, Pi and Sha) |
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
clip <name> = <track> [/<grid> | live | bars <n> | <clip>] [voicing[(<style>[,<anchor>])]] [.<method>(<args>)]…
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
pad  = bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr rd | p0 … p15
step = x hit | X accent | o ghost | f flam | d drag | . rest, then maybe a ratchet 2–4 on x X o
grid = /16 (default) | /12 | /24 | /32 | /48     steps to a bar
```

Pads: kick, snare, clap, closed/open hat, low/mid/high tom, rimshot, claves,
maracas, cowbell, cymbal, low/mid/high conga, crash, ride. On a Pad Sampler a
lane plays pad *n* for note 36 + *n*: `bd` to `rd` reach the pads under their
General MIDI notes, and `cb`, `ma`, `lc` and `hc` the pads most kits put a
cowbell (8), a shaker (5) and congas (4, 12) on; `p0` to `p15` play a pad by
number, whatever the kit holds there (`p7` the tambourine on most). Where the
track names its model, a lane it has no pad for is an error: `cl`, `cy` or
`mc` on a Pad Sampler, a pad by number on anything else. Spaces between steps
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

### Beat lines

```text
"<word> …"     one quoted line of mini-notation (as under Notes) in place of the lanes
word = a pad | sd hh rim (Strudel's sn ch rs) | 0 … 15 (a pad by number, as p0 … p15)
```

A drum clip may hold one beat line instead of lanes (#659), as Strudel
writes `s("bd sd [hh oh] hh")` or `n("0 1 [2 3] 2")`. It is the same
mini-notation as a line of notes: `[ ]` shares a slot, `~` rests, `[bd,ch]`
hits two pads at once, `*n` repeats, `@n` weights, `<a b>` takes turns by bar,
`(k,n[,rot])` is euclid, and `!` accents. It has no `?`, `&` or `^`: a chance,
ghost, flam or drag is a lane's step. A number is a pad by number, so it, like
`p0`–`p15`, needs a Pad Sampler where the track names its model.

The line is compiled to the lanes a grid would have, one for each pad in the
order they first play, and the drum sequencer shows them. With no grid on the
clip line, it takes the first of `/16 /12 /24 /48` every hit starts on, so a
straight beat stays on `/16`; a written grid is kept, and a hit off it is an
error at its word. A line that takes turns runs as many bars as it takes to come
round, at most 64 steps a lane. The clip prints the line as written; editing a
step in the sequencer turns it into those lanes, in the text too.

```song
track kit drums
clip fwd = kit
  "bd sn [ch oh] ch"
clip turns = kit
  "[bd hh] [~ hh] [sd! hh] [~ <hh oh>]"
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
word   = <pitch> | <degree> | <degree> .. <degree> | ~ | [<word>…] | [<word>… | <word>…] | [<pitch>,<pitch>…] | <<word>…> | <chord name>, then suffixes
euclid = (k,n[,rot]) right after the word: it plays on k hits over n steps of its share
grace  = ^u ^l ^f ^d ^m after a note or degree: graces before it, or a mordent
suffix = *n repeat in its share (1–16) | @n weight in shares (1–48) | ?[p] some bars only | ?<condition> | & slide
weight = %n after an option of [a | b], last (1–16): drawn n times as often as an option without
```

`[a b]` groups share one word's time, `[c4,e4,g4]` is a chord, `<a b>` plays
one option per bar, `<a b>/n` each for n (1–16) bars (#700), `[a | b c]` one option at random each bar, filling the
group's time (#614: seeded as `?` is, repeats after 8; an option may be a
sequence; not with a chord's commas; `[c2%3 | d#2 | g1]` weighs c2 three
times out of five, #616, and a sequence takes its weight on its brackets,
`[[c3 a#2]%2 | g2]`), `?` plays on a fixed, seeded half of the bars and `?0.3`
on about 3 in 10 (0 to 1, at most three decimals; repeats after 8).
After the `?` can come a trig condition instead, as on Elektron's
sequencers:

| suffix | plays |
|---|---|
| `?1:4` | in bar 1 of every 4 of the line (`a:b`, 1 ≤ a ≤ b ≤ 16) |
| `?pre` | when the last condition before it in the bar held (`?pre` itself does not count) |
| `?first` | in the first bar of its scene (a launched scene's first time round only), the first bar outside a scene, or the bar a new song takes over on |
| `?last` | in the last bar of its scene; never outside one |
| `?not…` | the opposite: `?not1:4`, `?notpre`, `?notfirst`, `?notlast` |

```song
track lead synth
clip lead = lead
  "c4 e4?1:4 g4?3:4 ~ b4?0.3 d5?pre a4?last"
```

Graces decorate a note or a degree, as a drum lane's flams do: soft notes
just before the beat, which stays where it is. `^u` is a grace from the
scale note above, `^l` from the one below (a degree's next degree; a note's
whole tone, which `.scale` moves onto its scale), each 30 ms before; `^f` a
flam, the same note 20 ms before; `^d` a drag, the same note 30 and 15 ms
before; `^m` a mordent, the note, the one above and the note in the first
sixteenth of its share (a share shorter than that plays the note). Graces
shrink to fit a fast tempo, a grace before a bar's first note plays at the
end of the bar before, and on a rest, group, chord or range `^` is an error.

`c2(3,8)` splits the word's share into 8 steps and plays it on 3 of them,
spread as `euclid(3,8) c2` spreads them over a bar, rotated left by a third
number (`c2(3,8,2)`), as in Strudel and Tidal. It goes on a note, a degree,
a chord name, `[…]` (a chord too: `[c4,e4](3,8)`, not on one of its notes)
and `<…>`, right after the word, before the other suffixes; each `*n` repeat
has the rhythm. Suffixes come in any order, each once. Brackets nest at most 4 deep. `&`
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
clip offbeat = lead .scale(a1 minor)
  "0(3,8) <4 5>(5,8,2)"
clip lead = lead .scale(a3 minor)
  "0^u 2 4^f 3^m"
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
line says how long it is. `bars <n>` also sets how long an `arp` of a chord
runs.

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
- An ending on a symbol or a numeral: `/1`, `/2`, `/3` is the inversion, the
  lowest 1, 2 or 3 notes an octave up (`c:maj7/1` is e g b c); `/<root>` puts
  that note at the bottom, the chord's own as an inversion (`c:maj7/e`, the
  same notes) or else in the octave below the root (`c/f#`). `c:m7/g:2` in
  classic notes. A numeral's ending is never another numeral: no `V/V`.
- `voicing` on the clip line moves each chord to the notes nearest the chord
  before it, within C3–C6. `voicing(<style>[,<anchor>])` spreads that close
  chord: `close` (the same as `voicing`), `open` (the second note from the
  bottom an octave up), `drop2` and `drop3` (the second or third note from
  the top an octave down), `shell` (the root at the bottom, then only the
  third and seventh), `quartal` (fourths of the song's scale stacked from
  the root, as many as the chord has notes) or `lefthand` (#698: a jazz
  pianist's rootless left hand from Strudel's dictionary, for m7, 7, maj7,
  m7b5, dim7, m6 and their kin, within F3–A4, each chord the voicing whose
  top note moves least; another quality is voiced close). An anchor note
  (`c5`) caps the top of every chord, the chords sitting in the three octaves
  below it (a left hand in the 16 semitones below it). The
  voices still lead from chord to chord, and a slash chord's bass stays at
  the bottom.

```song
scale c minor
track pad synth
clip prog = pad voicing
  "<i VI III VII>"
clip jazz = pad
  c:m7:2 f:7:2 bb:maj7:2 eb:maj7:2
clip turn = pad voicing
  "<ii7 V7 i bVII>"
clip keys = pad voicing(drop2,c5)
  "<ii7 V7/1 i c:m7/g>"
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
| `arp(<chord>,<mode>[:<octaves>],<rate>)` | a chord (`[c4,e4,g4]` up to 8 notes, or a name) over one bar, or the clip's `bars <n>` (1–64), running on across the bar lines; rate 2, 4, 8 or 16 notes a bar, or the triplets 3, 6, 12 or 24; `:2` to `:4` repeat the chord an octave up each time (`up:2`), as the live arp's octaves |
| `arp(<chord>,random,<rate>,<seed>)` | random order; only `random` takes a seed, and must |
| `arp(<clip>,<mode>[:<octaves>],<rate>[,<seed>])` | the clip's chords, each arpeggiated from its own start (a clip name wins over a chord name) |
| `walk(<pitch>,<n>,<seed>)` | n notes (1–32) in one bar, a random walk on the scale; needs `scale` |
| `markov(<order>,<clip>,<seed>)` | a chain of order 1–3 learned from the clip's pitches and rhythm |
| `mutate(<clip>,<percent>,<seed>)` | the clip with 0–100 % of its notes moved or dropped |
| `prog(<bars>,<seed>[,<size>][,<beats>][,<style>])` | 1–16 bars of chords on a seven-note `scale`, one a bar: tonic first, dominant last, functional moves. After the seed, in any order: a size `triad` (default), `7th`, `9th` or `sus` (1 4 5, 1 2 5 on the dominant); beats a chord lasts, 1–16 (`2`), or up to 16 in turn (`"4 2 2"`), the last chord cut at the end; a style `classic` (default), `pop` (I to IV or V), `jazz` (ii … V I), `modal` (I to I, no V) or `andalusian` (i VII VI V) |
| `root(<clip>[,<octave>])` | the root of each of the clip's chords in octave 0–7 (default 2), a bass line |
| `bass(<clip>,<style>[,<octave>][,<seed>])` | a bass line on the clip's chords, roots in octave 0–7 (default 2): `fifths` (the root on a chord's first beat, its fifth two beats on), `octave` (root and octave in eighths) or `walk` (the root on each chord's first beat, a step above or below the next root on the beat before a change, chord notes or scale steps between); only `walk` takes a seed, and must |

A clip a call reads is a note clip above it, as it was at load (a `live`
clip's first bar).

An arp's modes are the live arp's (#612), Tidal's orders; over `c e g b`:

| mode | plays |
|---|---|
| `up` / `down` | c e g b / b g e c |
| `updown` / `downup` | c e g b g e / b g e c e g: turning without repeating the ends |
| `up&down` / `down&up` | c e g b b g e c / b g e c c e g b |
| `converge` / `diverge` | c b e g / g e b c: outside in, inside out |
| `disconverge` | c b e g e b c |
| `pinkyup` / `pinkyupdown` | c b e b g b / c b e b g b g b e b |
| `thumbup` / `thumbupdown` | c e c g c b / c e c g c b c g c e |
| `asplayed` | the notes in the order written: `[g4,c4,e4]` plays g c e |
| `random` | a note drawn each step, by the seed |

With octaves the order runs over the whole span: `up:2` on `[c4,e4]` is
c4 e4 c5 e5.

```song
scale c minor
track pad synth
track bass synth
track arp synth
clip chords = pad voicing
  prog(4,7)
clip jazz = pad voicing
  prog(4,7,7th,"4 2 2",jazz)
clip low = bass
  root(chords)
clip walking = bass
  bass(chords,walk,5)
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
clip rise = arp bars 3
  arp(c:m7,up:2,12)
```

**live:** `clip <name> = <track> live` with a generator call (any of the
table) plays new notes every bar: the seed mixed with the bar, the same every
run. Not on lanes, other forms of notes or `voicing`. Of the pattern methods it
takes only those that change pitches, note by note, each bar as it is made:
`.scale`, `.chord`, `.oct`, `.add(n)` and `.sub(n)` (#593); so a live melody follows
a harmony clip: `clip sand = lead live .chord(harm)`.

```song
scale e phrygian-dominant
track lead synth
clip sand = lead live
  walk(e4,16,7)
```

### Pattern methods

Written last on a note clip's line; applied in order to its notes at load. A
cycle is a bar; times round to the tick grid; the result keeps the limits of
512 notes and 64 bars. Not on lanes, but on a drum clip made from one; a `live` clip takes only the pitch
methods (`.scale`, `.chord`, `.oct`, `.add(n)`, `.sub(n)`).

| method | does |
|---|---|
| `.fast(n)` `.slow(n)` | n times faster or slower: a whole number or a ratio, each side 1–16 (`3/4`, `1.5`, #702); the line repeats until it fills whole bars, so `.fast(3/4)` of a bar plays three times in four |
| `.rev()` | each bar backwards |
| `.palindrome()` | every other bar backwards |
| `.add(n)` `.sub(n)` | transpose by n semitones (−48 to 48), a fraction too, to the cent (`.add(0.5)`, at most two decimals, #699): the note plays between the keys on every pitched model |
| `.add(<signal>)` `.sub(<signal>)` | semitones read from a signal of the song's time as each note starts, `.add(perlin.range(0, 0.5))`, Strudel's pitch warble (#699); one a clip, not `env`, a list, `.onNote` or `~`; not on a `live` clip or a drum clip |
| `.add([0,4,7])` | a copy of each note per offset, sounding together: degrees before `.scale`, semitones after; `.add([0,0.1])` layers a copy ten cents up |
| `.ply(n)` | each note n (1–16) times in its own length |
| `.iter(n)` | each bar starts a further 1/n (1–16) of a bar in |
| `.shuffle(n)` | each bar cut into n (1–16) parts played in a new order each bar, every part once (#615) |
| `.scramble(n)` | n parts drawn from each bar's, some twice and some left out; both seeded, a note cut at its part's end, the line round in 8 bars (#615) |
| `.degrade([p])` | drop each note with chance p (0–1, default 0.5), seeded |
| `.every(n, m)` | method m on every nth bar from the first: `every(4, rev)` |
| `.off(t, m)` | add a copy t of a bar later (0 < t < 1, `1/8` or `0.125`) with m |
| `.struct("x ~ x x")` | each bar on a rhythm of 1–48 steps: at each `x`, the notes sounding there |
| `.struct(<clip>)` | the rhythm of an earlier clip (ADR-0035): at each start of its notes, or each hit of a drum clip (any pad), the notes sounding there, for as long as its note (a drum hit: a step); the two loop round together (at most 64 bars) |
| `.euclid(k, n[, rot])` | `struct` with k hits spread over n (1–48) steps, rotated left by rot: `euclid(5,8)` is `struct("x ~ x x ~ x x ~")` |
| `.sometimes(m)` | m on about half the moments, seeded |
| `.often(m)` `.rarely(m)` | m on about 3 in 4, or 1 in 4, of the moments (#700) |
| `.almostNever(m)` `.almostAlways(m)` | m on about 1 in 10, or 9 in 10, of the moments |
| `.sometimesBy(p, m)` | m on a chance p (0–1) of the moments; a moment a rarer chance takes, a likelier one takes too |
| `.scale(<root> <mode>)` | each note to the nearest note of the scale (down on a tie); each degree a note, from the root in its octave (`a1`) or octave 3 (`a`) |
| `.scale(<<root>…>[/n] [<mode>])` | a progression: the roots (up to 16) take turns, a bar each or n (1–16) bars each, by the bar a note starts in; `a1:dorian` gives a root its own mode, and the shared mode may go when each has one |
| `.scale(<clip> <mode>)` | the roots from an earlier clip (ADR-0035): each note takes, as degree 0, the lowest note the clip plays where the note starts, in its octave; the two loop round together (at most 64 bars), and before the clip's first note its last chord sounds; a clip's name wins over a root |
| `.chord(<clip>)` | each degree a note of the chord an earlier clip plays where it starts (ADR-0035): 0 its lowest note, on up an octave past the top and down below the root, `[0,2]` two at once; a note moves to the nearest chord note; `"0 1 2 3"` is an up arpeggio |
| `.oct(n)` | n (−4 to 4) octaves up or down; after `.scale`, since it moves notes, not degrees |
| `.jux(m)` | the line on the left and a copy with method m on the right, on its one track, as Strudel's (#701, ADR-0039): `.jux(rev)`; while a note is panned the synth is stereo, its strip balancing, its chorus kept; a mono synth plays one side at a time; not on a drum clip |
| `.arp("0 3 <2 0> [1 3]")` | the line's own chords played by index, as Strudel's arp (#696): each index, in mini-notation, the note of the chord sounding where it starts, 0 the lowest, wrapping round the chord with no octave added (`.chord` adds one); methods before it change the chords, after it the notes; the line and the indexes round together; after `.scale` |
| `.strum(ms)` | each chord's notes (those starting together) `ms` (1–100) milliseconds apart from the lowest up, from the highest down when below 0; a single note and every note's end stay on the tick; inside `every`, `off` and `sometimes` too |

`.fast`, `.slow`, `.add`, `.sub`, `.ply` and `.degrade` also take a value a
bar, in quotes, as Strudel does (#629): `.add("<0 3 5>")` adds 0, 3 and 5 in
turn, and the line plays until it and the values come round together (at most
64 bars). A nested `< >` takes its own turns, so `"<0 <1 -1>>"` is 0, 1, 0,
−1, and `/n` holds each of its values n (1–16) bars: `"<0 12>/16"` (#700). Several values side by side, or in `[ ]`, split the bar (#676):
`.add("0 3")` adds 0 to the notes starting in its first half and 3 to the
rest, and `.add("<0 [3 5]>")` nests; `[0,2,4]` with commas stays one value, a
stack. A method that moves notes in time (`.fast("1 2")`) moves the whole line,
and each part keeps the notes that land in it. On a `live` clip, `.add` and
`.sub` take theirs from the bar of the song.

While the song plays, the value in force lights in the clip's line, for each
bar and each part of a split one, while the clip plays. A method inside
`every`, `off` or a chance (`sometimes` and its kin), one followed by any method but `.add`, `.sub`,
`.oct`, `.scale` and `.chord` (`.add(…) .fast(…)`), and one on a `live` clip
light nothing: their bars are not the clip's. A value a bar holds at most 64
values, nested at most 32 deep.

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

The rhythm too: the bass plays on every kick, and follows an edit to it.

```song
track kit drums
track bass synth
clip beat = kit
  bd x..x..x...x.x...
clip bass = bass .struct(beat) .scale(a1 minor)
  "0 4 2 3"
```

A clip can be made from an earlier clip and methods, with no line of its
own: `clip back = right fwd .rev()` plays `fwd`'s notes, after its own
methods, through `.rev()` (#649); two tracks panned apart, as below, give
each side its own synth, and `.jux(rev)` puts both on one. A clip written later or the clip itself is an error at the
name; the clip follows an edit to its source, and takes none of its own. Not
with `voicing` (the source's holds).

A drum clip is made from a drum clip the same way (#653), on a drums or a
sampler track: its steps, ghosts, flams and ratchets kept, through the
methods of time (not `add`, `sub`, `scale`, `oct`, `chord` or `strum`), back
on the source's grid. A hit lands in the step it starts in; one that neither
starts nor ends on a step (`fast(3)` on `/16`) is an error, as is a lane past
64 steps. Where two hits share a step, the stronger plays: an accent, a hit,
a flam or drag, a ghost. A clip of notes is made from one of notes.

```song
track left synth
track right synth
strip left: Pan -1
strip right: Pan 1
clip fwd = left
  "c4 eb4 [g4 bb4] c5"
clip back = right fwd .rev()
```

```song
track hatl drums
track hatr drums
strip hatl: Pan -1
strip hatr: Pan 1
clip hats = hatl /16
  ch x.oxx.o.x3...xo.o
clip stah = hatr hats .every(2, rev)
```

The `m` of `every`, `off` and the chances is a method without its dot:
`rev`, `fast(2)`, `add(12)`.

```song
track lead synth
clip riff = lead .fast(2) .every(4, rev) .off(1/8, add(12))
  "c4 eb4 g4 bb4"
clip grid = lead .struct("x ~ x x ~ x x ~") .degrade(0.2) .scale(c minor)
  "c4 d4 e4 f4"
clip arp = lead .fast(8) .scale(<a1 d2>/4 minor-pentatonic)
  "<0 2 1 3 2>"
clip turns = lead .fast("<1 2>") .add("<0 <1 -1>>")
  "c2 [eb3,g3]"
```

A progression runs the line on until both come round together: the `arp`
above is 5 bars of degrees against 8 of roots, so it is 40 bars long and
its five-step cycle runs on unbroken through the change of root.

### Parameter methods

`.<param>(<signal>)` on a clip line, after pattern methods, moves a parameter
of the clip's track (its synth or strip) while the clip plays, and puts it
back when it leaves. One per parameter per clip; a pattern method's name wins.
They count with `mod` lines toward 32.

A parameter the track's model never reads is an error at its name, not a
method that does nothing (#673): `.lfospeed` is the DX7's LFO, and on a
Minimoog the error says its vibrato is VCO 3, `vco3coarse`. The model is the
one the track names, or the one picked for its role, which the text then
names. Every model takes its strip's, the arpeggiator's and the voice pool's
parameters (`level`, `pan`, `send1`, `room`, `arpon`, `polyphony`, `glide`).

On a track that plays a Modular setting, the SynthDef's arguments name its
knobs (#674): `.vib(4)` for `|vib = 4|`, in place of `.ctl1(4)`. A knob's name
wins over a parameter of the same name, and so do targets: `lead.vib`.

```song
setting vibra = Modular ModularBasic
  SynthDef(\vibra, { |freq = 220, gate = 1, vib = 4, vibmod = 6|
      var f = freq * (SinOsc.kr(vib) * vibmod).midiratio;
      LFTri.ar(f) * EnvGen.kr(Env.adsr(0.01, 0.1, 0.8, 0.1), gate) * 0.4
  }).add;
track lead synth vibra
clip a = lead .vib(4) .vibmod("<.25 .5 1 2 12>")
  "a3 e3"
```

`.attack`, `.decay`, `.sustain` and `.release` (or `att`, `dec`, `sus`,
`rel`) on a clip are held per note (ADR-0038, #651), without `.onNote`. Each
note takes the value at its own tick and keeps it for its whole life, so a
sequence gives notes their own envelopes:
- On a Mono-voice synth, a sampler, the D-50 (partial 1) or a Modular voice's
  amplifier, a note's voices hold its times, so overlapping notes keep their
  own, and a note let go as its clip stops keeps its release (#688).
- On any other synth, the synth holds the value until the next note.

Written as numbers, a time is 0 to 10 seconds (0 is the shortest) and a
sustain is 0 to 1.

```song
track pad synth Juno106
clip swell = pad .attack("0.1 0") .release("1 0.05")
  "c2 [d#3,g3]"
```

A `~` in a sequence is a step with no value: the parameter has its own value
there, so `.cutoff("~ ~ 800 ~")` locks the cutoff on one step of four and lets
go after it (a per-step lock). Anything combined with a `~` has no value on that
step either. A per-voice signal takes no `~`.

A sequence nests as Strudel's (#675): `[a b]` divides its share of time, `<a b>`
takes one a cycle, and a `<…>` inside moves on only when it is picked.
`.attack("<0.1 [0 0.05]>")` is 0.1 in bar 1, then 0 and 0.05 for the halves of
bar 2; `"[0.1 0]"` is `"0.1 0"`. While the song plays, the number in force
lights in the text, as a note does, while its clip plays and its track is
heard: through `slow`, `fast`, `segment`, `range`, `lag` and arithmetic, not
inside a wave or a list.

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
| `<track>` | its synth's and its strip's; a Modular setting's knobs by name |
| `strip1`–`strip16` | that synth's and strip's |
| `group1`–`group8` | the group's strip parameters |
| `master` | the master parameters |

`Model` and `Out` can't be written. On a track, a parameter its model never
reads is an error at its name, as for a parameter method. A `strip1`–`strip16`
target is not checked: the text doesn't say which synth plays on it, so a
Modular knob's name goes on its track (`lead.vib`), and on a strip is an error
that says so (#688).

## auto

```text
auto <name> = <target>.<Param> <value>… /<bars>
auto <name> = <target>.<Param> ramp <from> <to> /<bars>
auto <name> = <target>.<Param> "<sequence>" /<bars>
```

An automation lane of 1–256 bars: values (at most 64) spread evenly, each
held for its share, or a linear ramp. Placed in scenes like a clip, it
starts at the scene's first bar and loops inside it.

A quoted sequence (#683) is a `mod`'s: one cycle a bar from the lane's start
(a `<…>` longer than the lane is cut at its end), `[ ]` dividing its share and
`< >` one a bar, nesting; a `~` holds the lane's
last value. `auto wob = lead.Cutoff "<300 [800 1200]>" /2` is 300 in bar 1 and
800, 1200 in the halves of bar 2. While the lane plays, its value in force
lights in the text, plain values each for its share.

```song
track kit drums
clip beat = kit
  bd x...x...x...x...
auto sweep = kit.BdDecay ramp 0.25 2 /8
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
term   = <number> | <source> | "<sequence>" | [<n>, …] | ( <signal> ), then .<method>(…)…
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
| `"<300 800 1200>"` / `"0 0.5 1"` | numbers, one a bar / sharing the bar (at most 64); `~` a step with no value; `[ ]` and `< >` nest, `"<300 [800 1200]>"` (#675); the number in force lights while the song plays |
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
The effects return at 1, so `.room(0.4)` alone is heard, as in Strudel (#660).

Strudel's effect names are in its units (#660): `delaytime` (the echo's time
in seconds, P1A), `delayfeedback` (P1B, 0–0.95), `roomsize` (the reverb's
tail in seconds, P2A), `roomlp` `roomdim` (the low-pass in the tail in Hz,
P2B). On a clip they write the master's echo and reverb, shared by the clips
as Strudel's: the last to write wins. `room`, `roomsize` and the rest need P2
to be a Reverb, `delay` and its names P1 an Echo; on another effect they are an
error. A lane or a snapshot takes the knobs (`master.P1A`), not the units.

```song
track kit drums
clip a = kit .delay(0.5) .delaytime(0.375) .delayfeedback(0.4) .room(0.3) .roomsize(4)
  bd x...x...x...x...
```

`.adsr("a:d:s:r")` on a clip is Strudel's envelope in one method (#677). It
splits at the colons into `.attack`, `.decay`, `.sustain` and `.release`, each
a parameter method of its own, held per note (ADR-0038) and counted toward 32:
- **A part** is a number, `[a b]` sharing the bar, or `<a b>` one a bar, as
  in a quoted sequence.
- **Fewer than four parts**, or an empty one (`".1::.5"`), leave the others
  alone.
- **Printing:** it prints back as one method.
- **Not with the long names:** `.adsr` with `.attack` (or another of the four)
  on one clip is an error.

```song
track bass synth Juno106
clip low = bass .adsr("[.1 0]:.2:[1 0]") .room(0.5)
  "c2 [d#3,g3]"
```

A per-voice signal goes on a track whose synth is Mono or Poly (not FM, LA,
drums, samplers, Modular) and a voice parameter: `cutoff resonance vco1level
vco2level vco3level noiselevel ringlevel sublevel wt1pos wt2pos`, and the
envelope times `attack decay sustain release`, which a voice takes as its note
starts; it can't `.lag`.
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
