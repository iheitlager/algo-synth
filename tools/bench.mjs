// Time 16 Mono voices in V8 (plan.md "Performance budget", #12).
//
// Loads web/public/dsp.wasm in Node, which runs the same V8 as Chrome,
// renders 128-frame blocks at 48 kHz and reports the share of one core.
// Parameter ids come from web/src/audio/params.ts, so nothing is copied here.
// `chrome://webaudio-internals` is still the reference (ADR-0002).

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
const Source = ids('Source')
const Waveform = ids('Waveform')

const bytes = readFileSync(new URL('../web/public/dsp.wasm', import.meta.url))
const module = await WebAssembly.compile(bytes)

// [setup, lowest note]; voices are 3 semitones apart from there.
const scenarios = {
  // Three saws and pink noise, in a string section's range.
  'bowed string': [(w) => w.mono_preset(Preset.BowedString), 36],
  // Every BLEP edge there can be: three pulses, both synced, noise, full
  // resonance and drive, high up where edges come most often.
  'worst case': [(w) => {
    w.mono_preset(Preset.SyncLead)
    for (const v of [1, 2, 3]) {
      w.set_param(Param[`Vco${v}Wave`], Waveform.Pulse)
      w.set_param(Param[`Vco${v}Level`], 1)
    }
    w.set_param(Param.Vco3Coarse, 24)
    w.set_param(Param.Vco3Sync, 1)
    w.set_param(Param.PulseWidth, 0.1)
    w.set_param(Param.NoiseLevel, 0.5)
    w.set_param(Param.Resonance, 1)
    w.set_param(Param.Drive, 1)
  }, 72],
}

function run(setup, lowest) {
  const w = new WebAssembly.Instance(module, {}).exports
  w.init(SR)
  setup(w)
  for (let i = 0; i < VOICES; i++) w.note_on(Source.Mono, lowest + 3 * i, 0.8)
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
