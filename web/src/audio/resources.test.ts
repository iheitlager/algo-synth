import { afterEach, describe, expect, it } from 'vitest'
import { forgetResources, keep, remember, resources, type Msg } from './resources'
import { wavName } from './bounce'

const run = (msgs: Msg[]) => msgs.reduce<Msg[]>((log, m) => keep(log, m), [])

describe('the resources an export needs (#705)', () => {
  afterEach(forgetResources)

  it('keeps only what the song text does not hold', () => {
    expect(run([{ t: 'param', s: 0, id: 1, v: 2 }, { t: 'on', s: 0, n: 60 }, { t: 'song', bytes: new ArrayBuffer(1) }])).toEqual([])
  })

  it('a slot keeps its last sample, and a cleared slot none', () => {
    const a = { t: 'sample', slot: 3, bytes: new ArrayBuffer(1) }
    const b = { t: 'sample', slot: 3, bytes: new ArrayBuffer(2) }
    const other = { t: 'sample', slot: 4, bytes: new ArrayBuffer(3) }
    expect(run([a, other, b])).toEqual([other, b])
    expect(run([a, other, { t: 'sampleClear', slot: 3 }])).toEqual([other])
  })

  it('a layout in one go starts the synth\'s zones over; a later edit stays after it', () => {
    const edit = { t: 'zone', s: 1, zone: 0, field: 7, v: 0.5 }
    const sets = { t: 'zoneSets', s: 1, sets: [0, 0, 3] }
    const elsewhere = { t: 'zone', s: 2, zone: 0, field: 7, v: 0.1 }
    const again = { t: 'zone', s: 1, zone: 0, field: 7, v: 0.8 }
    expect(run([edit, elsewhere, sets, again])).toEqual([elsewhere, sets, again])
    expect(run([edit, { ...edit, v: 0.9 }])).toEqual([{ ...edit, v: 0.9 }])
    expect(run([sets, { t: 'zonesClear', s: 1 }, elsewhere])).toEqual([elsewhere])
  })

  it('pads likewise, and start over clears every zone and pad but not the samples', () => {
    const sample = { t: 'sample', slot: 0, bytes: new ArrayBuffer(1) }
    const pads = { t: 'padSets', s: 5, sets: [0, 0, 0] }
    const pad = { t: 'pad', s: 5, pad: 1, field: 2, v: 1 }
    expect(run([pads, pad, { t: 'padsClear', s: 5 }])).toEqual([])
    expect(run([sample, pads, { t: 'zoneSets', s: 1, sets: [] }, { t: 'clear' }])).toEqual([sample])
  })

  it('a user table or attack slot keeps its last values', () => {
    const t1 = { t: 'userTable', slot: 0, values: new ArrayBuffer(4) }
    const t2 = { t: 'userTable', slot: 0, values: new ArrayBuffer(8) }
    const a = { t: 'userAttack', slot: 0, values: new ArrayBuffer(4), root: 60 }
    expect(run([t1, a, t2])).toEqual([a, t2])
  })

  it('copies what the worklet takes, and hands out fresh copies to transfer', () => {
    const bytes = new Uint8Array([1, 2, 3]).buffer
    remember({ t: 'sample', slot: 0, bytes }, 48_000)
    remember({ t: 'userAttack', slot: 2, values: new Float32Array([0.5]).buffer, root: 64 }, 44_100)
    // The worklet takes the original: the kept copy is untouched.
    structuredClone(bytes, { transfer: [bytes] })
    const { msgs, transfer } = resources()
    expect(msgs.map((m) => m.t)).toEqual(['sample', 'userAttack'])
    expect([...new Uint8Array(msgs[0]?.bytes as ArrayBuffer)]).toEqual([1, 2, 3])
    // An attack goes with the rate it was made at.
    expect(msgs[1]?.rate).toBe(44_100)
    expect(transfer).toHaveLength(2)
    // A second export gets its own copies.
    expect(resources().transfer[0]).not.toBe(transfer[0])
  })

  it('names the WAV after the song', () => {
    expect(wavName('seed-rave.song')).toBe('seed-rave.wav')
    expect(wavName('Bach.mid')).toBe('Bach.wav')
    expect(wavName('')).toBe('algo-synth.wav')
  })
})
