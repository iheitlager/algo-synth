// What the song text doesn't hold (#705): the samples in the store's slots, the
// zones and pads laid out on them, and the user tables and attacks. A bounce's
// worker starts a fresh engine from the song text, so it needs these too; the
// worklet's messages that made them are kept here, each superseded one
// dropped, and sent to the worker again after the song. Knob moves need no
// keeping: the engine folds them into the song text (ADR-0027).

export type Msg = { t: string } & Record<string, unknown>

/** The messages that set what the song text doesn't hold. */
const KEPT = new Set(['sample', 'sampleClear', 'zone', 'zonesClear', 'zoneSets', 'pad', 'padsClear', 'padSets', 'userTable', 'userAttack', 'clear'])

/** Whether `later` makes `earlier` moot, so a fresh engine can do without it. */
function supersedes(later: Msg, earlier: Msg): boolean {
  const same = (...keys: string[]) => keys.every((k) => later[k] === earlier[k])
  switch (later.t) {
    // A slot's new sample replaces its old one; a cleared slot is as a fresh engine's.
    case 'sample':
    case 'sampleClear':
      return (earlier.t === 'sample' || earlier.t === 'sampleClear') && same('slot')
    // A layout in one go, or a clear, starts the synth's zones (pads) over.
    case 'zoneSets':
    case 'zonesClear':
      return ['zone', 'zonesClear', 'zoneSets'].includes(earlier.t) && same('s')
    case 'padSets':
    case 'padsClear':
      return ['pad', 'padsClear', 'padSets'].includes(earlier.t) && same('s')
    case 'zone':
      return earlier.t === 'zone' && same('s', 'zone', 'field')
    case 'pad':
      return earlier.t === 'pad' && same('s', 'pad', 'field')
    case 'userTable':
    case 'userAttack':
      return earlier.t === later.t && same('slot')
    // Start over (#325) clears every zone and pad; the samples and tables stay.
    case 'clear':
      return ['zone', 'zonesClear', 'zoneSets', 'pad', 'padsClear', 'padSets'].includes(earlier.t)
  }
  return false
}

/** Nothing but a clear left behind: a fresh engine is that already. */
const moot = (m: Msg) => ['sampleClear', 'zonesClear', 'padsClear', 'clear'].includes(m.t)

/** `log` with `msg` kept, what it supersedes dropped; other messages leave it as it is. */
export function keep(log: readonly Msg[], msg: Msg): Msg[] {
  if (!KEPT.has(msg.t)) return log as Msg[]
  const left = log.filter((m) => !supersedes(msg, m))
  return moot(msg) ? left : [...left, msg]
}

let log: Msg[] = []

/**
 * Keep `msg` if it sets what the song text doesn't hold. Call before posting
 * it: the buffers it transfers are copied, for the worklet takes the originals.
 */
export function remember(msg: object, rate: number) {
  const m = msg as Msg
  if (!KEPT.has(m.t)) return
  const copy: Msg = { ...m }
  if (m.bytes instanceof ArrayBuffer) copy.bytes = m.bytes.slice(0)
  if (m.values instanceof ArrayBuffer) copy.values = m.values.slice(0)
  // An attack is played at the rate it was made at, whatever the bounce's.
  if (m.t === 'userAttack') copy.rate = rate
  log = keep(log, copy)
}

/** The kept messages, oldest first, each with copies of its buffers to transfer. */
export function resources(): { msgs: Msg[]; transfer: ArrayBuffer[] } {
  const transfer: ArrayBuffer[] = []
  const msgs = log.map((m) => {
    const copy: Msg = { ...m }
    for (const k of ['bytes', 'values'] as const) {
      const b = m[k]
      if (b instanceof ArrayBuffer) {
        copy[k] = b.slice(0)
        transfer.push(copy[k] as ArrayBuffer)
      }
    }
    return copy
  })
  return { msgs, transfer }
}

/** Forget everything kept, for tests. */
export function forgetResources() {
  log = []
}
