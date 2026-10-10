// Spike #392 (throwaway): set up one engine per wasm instance, as tools/bench.mjs
// does, for a light load (16 Mono synths) or a heavy one (64 poly voices).
// Runs in Node and in a browser worker: ids come in, nothing is read here.

export const SR = 48_000
const CHORD = [0, 3, 7, 12]
const family = ['Bass', 'MiniLead', 'ProLead', 'Ms20Lead', 'Cs15Brass', 'Sh101Lead', 'CurrieLead']
const polys = ['P5Pad', 'JunoPad', 'JupiterPad', 'MatrixPad', 'PpgSweepPad', 'LaFantasia', 'FmPad']
const NAMES = ['c', 'c#', 'd', 'd#', 'e', 'f', 'f#', 'g', 'g#', 'a', 'a#', 'b']
const noteName = (n) => `${NAMES[n % 12]}${Math.floor(n / 12) - 1}`

export function parseIds(ts) {
  const ids = (name) =>
    Object.fromEntries(
      [...ts.match(new RegExp(`export const ${name} = \\{([^}]*)\\}`))[1].matchAll(/(\w+): (\d+),/g)].map(
        ([, k, v]) => [k, Number(v)],
      ),
    )
  return { Param: ids('Param'), Preset: ids('Preset'), InsertType: ids('InsertType'), ProcType: ids('ProcType') }
}

function song(lowest, chord) {
  const lines = ['tempo 120']
  for (let t = 0; t < 16; t++) lines.push(`track t${t + 1} synth`)
  const spread = (t) => (chord.length > 1 ? 3 * (t % 4) : 3 * t)
  for (let t = 0; t < 16; t++) {
    lines.push(`clip f${t + 1} = t${t + 1} bars 8`)
    lines.push(`  ${chord.map((n) => `${noteName(lowest + spread(t) + n)}@0:384:100`).join(' ')}`)
  }
  return lines.join('\n') + '\n'
}

/** kind: 'light' | 'heavy'; play=false leaves the song stopped (an idle deck). */
export function makeEngine(module, { Param, Preset, InsertType, ProcType }, kind, play = true) {
  const w = new WebAssembly.Instance(module, {}).exports
  w.init(SR)
  for (let g = 0; g < 8; g++) {
    w.set_param(16 + g, Param.Send1, 0.2)
    w.set_param(16 + g, Param.I1Type, InsertType.Eq)
    w.set_param(16 + g, Param.I2Type, InsertType.Comp)
  }
  w.set_param(0, Param.P1Return, 0.5)
  w.set_param(0, Param.P1D, 1)
  w.set_param(0, Param.P2Return, 0.5)
  w.set_param(0, Param.P3Type, ProcType.Chorus)
  w.set_param(0, Param.P3Return, 0.4)
  w.set_param(0, Param.P4Type, ProcType.Flanger)
  w.set_param(0, Param.P4Return, 0.4)
  for (let s = 0; s < 16; s++) {
    if (kind === 'heavy') {
      w.mono_preset(s, Preset[polys[s % polys.length]])
      w.set_param(s, Param.Analog, 1)
      w.set_param(s, Param.Resonance, 0.9)
    } else {
      w.mono_preset(s, Preset[family[s % family.length]])
    }
    w.set_param(s, Param.I1Type, InsertType.Fuzz)
    w.set_param(s, Param.I2Type, InsertType.Eq)
    w.set_param(s, Param.I3Type, InsertType.Comp)
    w.set_param(s, Param.Pan, s / 7.5 - 1)
    w.set_param(s, Param.Out, 1 + (s % 8))
    for (const n of [1, 2, 3, 4]) w.set_param(s, Param[`Send${n}`], 0.3)
  }
  const text = new TextEncoder().encode(kind === 'heavy' ? song(60, CHORD) : song(36, [0]))
  const ptr = w.song_buf(text.length)
  new Uint8Array(w.memory.buffer, ptr, text.length).set(text)
  if (w.song_load() < 0) throw new Error('the spike song did not load')
  for (let t = 0; t < 16; t++) w.song_route(t, t)
  if (play) w.song_play()
  return w
}
