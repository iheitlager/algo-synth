// Time 16 Mono voices in V8 (plan.md "Performance budget", #12).
//
// Loads web/public/dsp.wasm in Node, which runs the same V8 as Chrome,
// with the drive, mixer and both send effects on, renders 128-frame blocks at 48 kHz and reports the share of one core.
// Parameter ids come from web/src/audio/params.ts, so nothing is copied here.
// Render capacity in Chrome DevTools' WebAudio panel is still the reference
// (ADR-0002).

import { readFileSync } from 'node:fs'
import { cpus } from 'node:os'

const SR = 48_000
const VOICES = 16
const WARMUP = 200
const BLOCKS = 4_000
const BUDGET = 0.3
// Notes held on each channel; the polyphonic scenarios hold chords (spec 006 Req 15).
const CHORD = [0, 3, 7, 12]

const ts = readFileSync(new URL('../web/src/audio/params.ts', import.meta.url), 'utf8')
const ids = (name) =>
  Object.fromEntries(
    [...ts.match(new RegExp(`export const ${name} = \\{([^}]*)\\}`))[1].matchAll(/(\w+): (\d+),/g)].map(
      ([, k, v]) => [k, Number(v)],
    ),
  )
const Param = ids('Param')
const Preset = ids('Preset')
const Waveform = ids('Waveform')
const InsertType = ids('InsertType')
const ProcType = ids('ProcType')

const bytes = readFileSync(new URL('../web/public/dsp.wasm', import.meta.url))
const module = await WebAssembly.compile(bytes)

// One preset of each model, so the ensemble is the family: ARP 2600, Minimoog,
// Pro-One, MS-20, CS-15, SH-101, Odyssey.
const family = ['Bass', 'MiniLead', 'ProLead', 'Ms20Lead', 'Cs15Brass', 'Sh101Lead', 'CurrieLead']

// Two mod lines per synth, by strip so no track claims a synth: each a sum of
// two LFOs through a lag, the deepest a song is likely to write.
const MODS = Array.from({ length: VOICES }, (_, s) => [
  `mod strip${s + 1}.cutoff = (lfo(${0.3 + s / 10}).exprange(200, 6000) + lfo(5, tri).range(0, 400)).lag(0.02)`,
  `mod strip${s + 1}.resonance = perlin.fast(${1 + s}).range(0.2, 0.9)`,
]).flat().join('\n')

// Per-voice values (ADR-0023): a Juno-106 track on every synth, its cutoff
// following an envelope and its resonance a rate per voice, so each of the
// 64 voices evaluates two signals every block.
const PER_VOICE = Array.from({ length: VOICES }, (_, s) => [
  `track t${s} synth Juno106 JunoPad`,
]).flat().concat(Array.from({ length: VOICES }, (_, s) => [
  `mod t${s}.cutoff = env(adsr).exprange(200, 6000)`,
  `mod t${s}.resonance = lfo([0.5, 1, 2, 3]).range(0.2, 0.9)`,
]).flat()).join('\n')

// A song giving every synth a big SynthDef in a setting (ADR-0024): eight
// oscillators, three ladders, a filter and a comb.
const LARGEST = [
  'setting big = Modular ModularBasic',
  String.raw`  SynthDef(\big, { |freq = 440|`,
  '      var sig = Mix([Saw.ar(freq), Saw.ar(freq), Pulse.ar(freq), LFTri.ar(freq), Saw.ar(freq), Pulse.ar(freq), Saw.ar(freq), LFTri.ar(freq)]);',
  '      CombN.ar(RLPF.ar(MoogFF.ar(MoogFF.ar(MoogFF.ar(sig, 3000), 2000), 1500), 1000), 0.01, 0.003, 0.5)',
  '  }).add;',
  ...Array.from({ length: VOICES }, (_, s) => `track t${s} synth big`),
].join('\n')

// [setup for one synth, lowest note]; voices are 3 semitones apart from
// there. Each song track plays its own synth, all set up the same.
// The SuperCollider hoover of #216 (ADR-0024), as pasted: 40 saws, 20
// delays, a Splay and a FreeVerb2 a voice, on a stereo bus.
const HOOVER = String.raw`SynthDef(\hoover, {
    var snd, freq, bw, delay, decay;
    freq = \freq.kr(440);
    freq = freq * Env([-5, 6, 0], [0.1, 1.7], [\lin, -4]).kr.midiratio;
    bw = 1.035;
    snd = { DelayN.ar(Saw.ar(freq * ExpRand(bw, 1 / bw)) + Saw.ar(freq * 0.5 * ExpRand(bw, 1 / bw)), 0.01, Rand(0, 0.01)) }.dup(20);
    snd = (Splay.ar(snd) * 3).atan;
    snd = snd * Env.asr(0.01, 1.0, 1.0).kr(0, \gate.kr(1));
    snd = FreeVerb2.ar(snd[0], snd[1], 0.3, 0.9);
    snd = snd * Env.asr(0, 1.0, 4, 6).kr(2, \gate.kr(1));
    Out.ar(\out.kr(0), snd * \amp.kr(0.1));
}).add;`

/** Give synth `s` a SynthDef, as the panel's Apply does. */
function setCode(w, s, code) {
  const text = new TextEncoder().encode(code)
  const ptr = w.song_buf(text.length) // may grow the memory: take its buffer after
  new Uint8Array(w.memory.buffer, ptr, text.length).set(text)
  if (w.code_set(s) < 0) throw new Error(`the SynthDef did not build: line ${w.code_error_line()}, column ${w.code_error_col()}`)
}

const scenarios = {
  // Three saws and pink noise, in a string section's range.
  'bowed string': [(w, s) => w.mono_preset(s, Preset.BowedString), 36],
  // Sixteen synths cycling through every model (spec 005 Req 9).
  'all models': [(w, s) => w.mono_preset(s, Preset[family[s % family.length]]), 36],
  // Chord pads on every polyphonic model: sixteen synths, four voices each,
  // which is the whole voice budget (spec 006 Req 15).
  'poly pads': [(w, s) => w.mono_preset(s, Preset[polys[s % polys.length]]), 48, CHORD],
  // The same, with analog drift, resonance and drive at full.
  'poly worst': [(w, s) => {
    w.mono_preset(s, Preset[polys[s % polys.length]])
    w.set_param(s, Param.Analog, 1)
    w.set_param(s, Param.Resonance, 0.9)
    w.set_param(s, Param.Drive, 0.5)
  }, 60, CHORD],
  // The family at its most expensive: ring mod, sub, noise, both filters at
  // full resonance and drive (the MS-20 and CS-15 filters, the high-pass
  // stages), high up.
  'family worst': [(w, s) => {
    w.mono_preset(s, Preset[family[s % family.length]])
    for (const v of [1, 2, 3]) w.set_param(s, Param[`Vco${v}Level`], 1)
    for (const p of ['RingLevel', 'SubLevel', 'Resonance', 'HpResonance', 'Drive']) w.set_param(s, Param[p], 1)
    w.set_param(s, Param.NoiseLevel, 0.5)
    w.set_param(s, Param.HpCutoff, 200)
  }, 72],
  // Every BLEP edge there can be: three pulses, both synced, noise, full
  // resonance and drive, high up where edges come most often.
  'worst case': [(w, s) => {
    w.mono_preset(s, Preset.SyncLead)
    for (const v of [1, 2, 3]) {
      w.set_param(s, Param[`Vco${v}Wave`], Waveform.Pulse)
      w.set_param(s, Param[`Vco${v}Level`], 1)
    }
    w.set_param(s, Param.Vco3Coarse, 24)
    w.set_param(s, Param.Vco3Sync, 1)
    w.set_param(s, Param.PulseWidth, 0.1)
    w.set_param(s, Param.NoiseLevel, 0.5)
    w.set_param(s, Param.Resonance, 1)
    w.set_param(s, Param.Drive, 1)
  }, 72],
  // A big SynthDef (eight oscillators, four filters and a comb), a note on
  // each of the 16 synths.
  'modular max': [(w, s) => w.mono_preset(s, Preset.ModularBasic), 48, [0], LARGEST],
  // The basic Modular voice (a filtered saw) on all 16 synths, and the
  // SuperCollider hoover on four of them: the difference is four hoover voices.
  'modular basic': [(w, s) => w.mono_preset(s, Preset.ModularBasic), 48],
  // The code goes in after the song loads, which picks the tracks' presets.
  'sc hoover x4': [(w, s) => w.mono_preset(s, Preset.ModularBasic), 48, [0], '', (w, s) => {
    w.mono_preset(s, Preset.ModularBasic)
    if (s < 4) setCode(w, s, HOOVER)
  }],
  // The Modular hoover (ADR-0020), a chord on every synth: 64 graph voices of
  // three oscillators, two LFOs and a filter.
  'modular': [(w, s) => w.mono_preset(s, Preset.ModularHoover), 48, CHORD],
  // The family worst case with its cutoff and resonance modulated on every
  // synth by a song of mod lines (ADR-0019, #208), evaluated once a block.
  modulated: [(w, s) => scenarios['family worst'][0](w, s), 72, [0], MODS],
  // A chord on every Juno-106 with two per-voice mod lines each (ADR-0023, #273).
  'per-voice': [(w, s) => w.mono_preset(s, Preset.JunoPad), 48, CHORD, PER_VOICE],
}

// The pad of each polyphonic model, so the ensemble is all of them: Prophet-5,
// Juno-106, Jupiter-8, Matrix-12, PPG Wave, D-50, DX7.
const polys = ['P5Pad', 'JunoPad', 'JupiterPad', 'MatrixPad', 'PpgSweepPad', 'LaFantasia', 'FmPad']

const NAMES = ['c', 'c#', 'd', 'd#', 'e', 'f', 'f#', 'g', 'g#', 'a', 'a#', 'b']
const noteName = (n) => `${NAMES[n % 12]}${Math.floor(n / 12) - 1}`

// One held note per track, eight bars long so it outlasts the run: 16 Mono
// voices, since each track owns one (spec 004 Req 6). With a chord, each
// track holds all of its notes: a polyphonic voice per note. The song is the
// only transport (ADR-0022); track n plays synth n. A scenario's song may
// bring its own tracks; the notes then go to those.
function heldSong(lowest, chord = [0], song = '') {
  const spread = (t) => (chord.length > 1 ? 3 * (t % 4) : 3 * t)
  const own = [...song.matchAll(/^track (\S+)/gm)].map((m) => m[1])
  const tracks = own.length ? own : Array.from({ length: VOICES }, (_, t) => `t${t + 1}`)
  const lines = ['tempo 120', song]
  if (!own.length) for (const t of tracks) lines.push(`track ${t} synth`)
  tracks.forEach((name, t) => {
    lines.push(`clip f${t + 1} = ${name} bars 8`)
    lines.push(`  ${chord.map((n) => `${noteName(lowest + spread(t) + n)}@0:384:100`).join(' ')}`)
  })
  return lines.join('\n')
}

function run(setup, lowest, chord, song, after) {
  const w = new WebAssembly.Instance(module, {}).exports
  w.init(SR)
  // The whole chain: every synth through Fuzz, panned, into both sends and
  // both effects with long feedback and tail.
  for (let g = 0; g < 8; g++) {
    w.set_param(16 + g, Param.Send1, 0.2)
    // Group inserts too: an EQ and a compressor on every group.
    w.set_param(16 + g, Param.I1Type, InsertType.Eq)
    w.set_param(16 + g, Param.I1C, 0.7)
    w.set_param(16 + g, Param.I2Type, InsertType.Comp)
    w.set_param(16 + g, Param.I2B, 0.6)
  }
  w.set_param(0, Param.P1Return, 0.5)
  w.set_param(0, Param.P1D, 1)
  w.set_param(0, Param.P1B, 0.7)
  w.set_param(0, Param.P2Return, 0.5)
  w.set_param(0, Param.P2A, 0.9)
  // P3 and P4 too: a chorus and a flanger.
  w.set_param(0, Param.P3Type, ProcType.Chorus)
  w.set_param(0, Param.P3Return, 0.4)
  w.set_param(0, Param.P4Type, ProcType.Flanger)
  w.set_param(0, Param.P4Return, 0.4)
  // Chain them: P1 into P2 into P3 into P4.
  for (const n of [2, 3, 4]) w.set_param(0, Param[`P${n}In`], 1)
  w.set_param(0, Param.CompThreshold, -30)
  w.set_param(0, Param.CompRatio, 4)
  for (const b of ['Low', 'Mid1', 'Mid2', 'High']) w.set_param(0, Param[`Eq${b}Gain`], 6)
  for (let s = 0; s < w.synth_count(); s++) {
    setup(w, s)
    // Three inserts in series: fuzz, a boosting EQ and a compressor.
    w.set_param(s, Param.I1Type, InsertType.Fuzz)
    w.set_param(s, Param.I1A, 1)
    w.set_param(s, Param.I2Type, InsertType.Eq)
    w.set_param(s, Param.I2A, 0.9)
    w.set_param(s, Param.I2C, 0.8)
    w.set_param(s, Param.I3Type, InsertType.Comp)
    w.set_param(s, Param.I3B, 0.6)
    w.set_param(s, Param.Pan, s / 7.5 - 1)
    // Every pair of synths into a group, the groups into the master.
    w.set_param(s, Param.Out, 1 + (s % 8))
    w.set_param(s, Param.Send1, 0.5)
    w.set_param(s, Param.Send2, 0.5)
    w.set_param(s, Param.Send3, 0.3)
    w.set_param(s, Param.Send4, 0.3)
  }
  // The held notes and the mod lines are one song: one transport.
  const text = new TextEncoder().encode(`${heldSong(lowest, chord, song)}\n`)
  const ptr = w.song_buf(text.length) // may grow the memory: take its buffer after
  new Uint8Array(w.memory.buffer, ptr, text.length).set(text)
  if (w.song_load() < 0) throw new Error('the bench song did not load')
  for (let t = 0; t < VOICES; t++) w.song_route(t, t)
  if (after) for (let s = 0; s < w.synth_count(); s++) after(w, s)
  w.song_play()
  const block = w.block_len()
  for (let i = 0; i < WARMUP; i++) w.process(block)
  const t0 = process.hrtime.bigint()
  for (let i = 0; i < BLOCKS; i++) w.process(block)
  const ns = Number(process.hrtime.bigint() - t0)
  const voices = w.active_voices()
  // The song still plays at the end.
  if (!w.song_playing()) throw new Error('the bench song stopped')
  // Every sample stays within ±1 (untimed: reading the output costs).
  let peak = 0
  for (let i = 0; i < 500; i++) {
    w.process(block)
    for (const x of new Float32Array(w.memory.buffer, w.out_ptr(), 2 * block)) peak = Math.max(peak, Math.abs(x))
  }
  const perBlock = ns / BLOCKS / 1_000
  const realtime = (block / SR) * 1e6
  return { voices, perBlock, load: perBlock / realtime, realtime, peak }
}

console.log(`${cpus()[0]?.model ?? 'unknown CPU'} · Node ${process.version} · V8 ${process.versions.v8}`)
let over = false
for (const [name, [setup, lowest, chord, song, after]] of Object.entries(scenarios)) {
  const r = run(setup, lowest, chord, song, after)
  const pct = (100 * r.load).toFixed(1)
  const ok = r.load <= BUDGET && r.peak <= 1
  over ||= !ok
  console.log(
    `${name.padEnd(13)} ${r.voices} voices  ${r.perBlock.toFixed(1)} µs/block of ${r.realtime.toFixed(0)}  ${pct}% of a core, peak ${r.peak.toFixed(2)}  ${ok ? 'within' : 'OVER'} the ${100 * BUDGET}% budget`,
  )
}
process.exitCode = over ? 1 : 0
