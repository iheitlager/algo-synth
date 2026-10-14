// Start the engine in V8 as the worklet does (#555): init, a preset on a
// second synth, a second of sound. Native tests can't see what only wasm
// has, such as its 1 MiB-by-default stack: #547's bigger Modular programs
// overflowed it in `init`, and every synth came up a silent ARP 2600.
// Parameter ids come from web/src/audio/params.ts, so nothing is copied here.

import { readFileSync } from 'node:fs'

const SR = 48_000

const ts = readFileSync(new URL('../web/src/audio/params.ts', import.meta.url), 'utf8')
const ids = (name) =>
  Object.fromEntries(
    [...ts.match(new RegExp(`export const ${name} = \\{([^}]*)\\}`))[1].matchAll(/(\w+): (\d+),/g)].map(
      ([, k, v]) => [k, Number(v)],
    ),
  )
const Param = ids('Param')
const Preset = ids('Preset')
const Model = ids('Model')

const bytes = readFileSync(new URL('../web/public/dsp.wasm', import.meta.url))
const w = new WebAssembly.Instance(await WebAssembly.compile(bytes), {}).exports

function fail(msg) {
  console.error(`wasm-check: ${msg}`)
  process.exit(1)
}

try {
  w.init(SR)
} catch (e) {
  fail(`init trapped: ${e.message}`)
}
// What "+ Synth → Minimoog" sends.
w.synth_reset(1)
w.mono_preset(1, Preset.MiniBass)
const model = w.param_value(1, Param.Model)
if (model !== Model.Minimoog) fail(`synth 2 is model ${model} after a Minimoog preset, not ${Model.Minimoog}`)

const block = w.block_len()
let peak = 0
w.note_on(1, 48, 1)
for (let i = 0; i < SR / block; i++) {
  w.process(block)
  const out = new Float32Array(w.memory.buffer, w.out_ptr(), 2 * block)
  for (const x of out) {
    if (!Number.isFinite(x)) fail('the output is not finite')
    peak = Math.max(peak, Math.abs(x))
  }
}
if (peak === 0) fail('a held Minimoog note is silent')
console.log(`wasm-check: the engine starts and plays (peak ${peak.toFixed(3)})`)
