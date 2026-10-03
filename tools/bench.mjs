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
const BUDGET = 0.25

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
const DriveMode = ids('DriveMode')

const bytes = readFileSync(new URL('../web/public/dsp.wasm', import.meta.url))
const module = await WebAssembly.compile(bytes)

// One preset of each model, so the ensemble is the family: ARP 2600, Minimoog,
// Pro-One, MS-20, CS-15, SH-101.
const family = ['Bass', 'MiniLead', 'ProLead', 'Ms20Lead', 'Cs15Brass', 'Sh101Lead']

// [setup for one synth, lowest note]; voices are 3 semitones apart from
// there. Each MIDI channel plays its own synth, all set up the same.
const scenarios = {
  // Three saws and pink noise, in a string section's range.
  'bowed string': [(w, s) => w.mono_preset(s, Preset.BowedString), 36],
  // Sixteen synths cycling through the six models (spec 005 Req 9).
  'six models': [(w, s) => w.mono_preset(s, Preset[family[s % family.length]]), 36],
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
}

// A MIDI variable-length quantity.
const vlq = (n) => {
  const out = [n & 0x7f]
  while ((n >>= 7)) out.unshift((n & 0x7f) | 0x80)
  return out
}

// One held note per channel for a minute: 16 Mono voices, since each
// channel owns one (spec 004 Req 6) and live input is monophonic.
function sixteenChannels(lowest) {
  const track = []
  for (let ch = 0; ch < VOICES; ch++) track.push(0, 0x90 | ch, lowest + 3 * ch, 100)
  // The player's song ends at its last note off.
  for (let ch = 0; ch < VOICES; ch++) track.push(...(ch ? [0] : vlq(480 * 120)), 0x80 | ch, lowest + 3 * ch, 0)
  track.push(0, 0xff, 0x2f, 0)
  const len = track.length
  return new Uint8Array([
    ...[0x4d, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 0, 0, 1, 0x01, 0xe0],
    ...[0x4d, 0x54, 0x72, 0x6b, (len >>> 24) & 255, (len >>> 16) & 255, (len >>> 8) & 255, len & 255],
    ...track,
  ])
}

function run(setup, lowest) {
  const w = new WebAssembly.Instance(module, {}).exports
  w.init(SR)
  // The whole chain: every synth through Fuzz, panned, into both sends and
  // both effects with long feedback and tail.
  w.set_param(0, Param.EchoReturn, 0.5)
  w.set_param(0, Param.EchoPingPong, 1)
  w.set_param(0, Param.EchoFeedback, 0.7)
  w.set_param(0, Param.ReverbReturn, 0.5)
  w.set_param(0, Param.ReverbSize, 6)
  for (let s = 0; s < w.synth_count(); s++) {
    setup(w, s)
    w.set_param(s, Param.DriveMode, DriveMode.Fuzz)
    w.set_param(s, Param.DriveAmount, 1)
    w.set_param(s, Param.Pan, s / 7.5 - 1)
    w.set_param(s, Param.EchoSend, 0.5)
    w.set_param(s, Param.ReverbSend, 0.5)
  }
  const file = sixteenChannels(lowest)
  new Uint8Array(w.memory.buffer, w.midi_buf(file.length), file.length).set(file)
  if (w.midi_load() < 0) throw new Error('the bench MIDI file did not load')
  w.play()
  const block = w.block_len()
  for (let i = 0; i < WARMUP; i++) w.process(block)
  const t0 = process.hrtime.bigint()
  for (let i = 0; i < BLOCKS; i++) w.process(block)
  const ns = Number(process.hrtime.bigint() - t0)
  const voices = w.active_voices()
  const perBlock = ns / BLOCKS / 1_000
  const realtime = (block / SR) * 1e6
  return { voices, perBlock, load: perBlock / realtime, realtime }
}

console.log(`${cpus()[0]?.model ?? 'unknown CPU'} · Node ${process.version} · V8 ${process.versions.v8}`)
let over = false
for (const [name, [setup, lowest]] of Object.entries(scenarios)) {
  const r = run(setup, lowest)
  const pct = (100 * r.load).toFixed(1)
  const ok = r.load <= BUDGET
  over ||= !ok
  console.log(
    `${name.padEnd(13)} ${r.voices} voices  ${r.perBlock.toFixed(1)} µs/block of ${r.realtime.toFixed(0)}  ${pct}% of a core  ${ok ? 'within' : 'OVER'} the ${100 * BUDGET}% budget`,
  )
}
process.exitCode = over ? 1 : 0
