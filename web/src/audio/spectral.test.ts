// The Spectral Lab and the main window over a fake BroadcastChannel (a message
// reaches every other port, never the sender), and the worker's flat tracks.
import { afterEach, describe, expect, it, vi } from 'vitest'
import { codeMessage, connectMain, parseTracks, recipeOf, serveLab, NO_EDITS, NO_MORPH, type Port } from './spectral'

class FakeChannel implements Port {
  static all: FakeChannel[] = []
  onmessage: ((e: MessageEvent) => void) | null = null
  closed = false
  constructor() { FakeChannel.all.push(this) }
  postMessage(msg: unknown) {
    for (const c of FakeChannel.all) if (c !== this && !c.closed) c.onmessage?.({ data: structuredClone(msg) } as MessageEvent)
  }
  close() { this.closed = true }
}

afterEach(() => {
  FakeChannel.all = []
  vi.useRealTimers()
})

describe('the lab and the main window', () => {
  it('find each other and a sample sent is loaded there', async () => {
    vi.useFakeTimers()
    const loaded: [string, number][] = []
    serveLab(new FakeChannel(), {
      sample: async (name, bytes) => {
        loaded.push([name, bytes.byteLength])
        return 1234
      },
      table: async () => 0,
      attack: async () => 0,
    })
    const here: boolean[] = []
    const lab = connectMain(new FakeChannel(), (h) => here.push(h))
    expect(here).toEqual([true])
    expect(await lab.send('bell (resynth).wav', new ArrayBuffer(100))).toBe(1234)
    expect(loaded).toEqual([['bell (resynth).wav', 100]])
    // The wait for an answer runs out without taking the main window away.
    vi.advanceTimersByTime(5_000)
    expect(here).toEqual([true])
  })

  it('a table and an attack reach their user slots', async () => {
    const got: string[] = []
    serveLab(new FakeChannel(), {
      sample: async () => 0,
      table: async (slot, values, recipe) => { got.push(`table ${slot} ${values.byteLength} ${recipe}`); return 0 },
      attack: async (slot, root, values, recipe) => { got.push(`attack ${slot} ${root} ${values.byteLength} ${recipe}`); return 0 },
    })
    const lab = connectMain(new FakeChannel(), () => {})
    expect(await lab.sendTable(2, new Float32Array(64 * 256).buffer, 'table 3 = a/b')).toBe(0)
    expect(await lab.sendAttack(3, 45, new Float32Array(1000).buffer)).toBe(0)
    expect(got).toEqual([`table 2 ${64 * 256 * 4} table 3 = a/b`, 'attack 3 45 4000 '])
  })

  it('a lab without a main window hears nothing and a send gives up', async () => {
    vi.useFakeTimers()
    const here: boolean[] = []
    const lab = connectMain(new FakeChannel(), (h) => here.push(h), 500)
    const sent = lab.send('x.wav', new ArrayBuffer(4))
    vi.advanceTimersByTime(500)
    expect(here).toEqual([false])
    expect(await sent).toBe(-100)
  })

  it('a main window that closes says so, and one that opens later says it is there', () => {
    const here: boolean[] = []
    connectMain(new FakeChannel(), (h) => here.push(h))
    const main = serveLab(new FakeChannel(), { sample: async () => 0, table: async () => 0, attack: async () => 0 })
    main.close()
    expect(here).toEqual([true, false])
  })
})

describe('the tracks', () => {
  it('come out of the flat buffer with their frames', () => {
    const flat = new Float32Array([3, 2, 440, 441, 0.5, 0.4, 0, 1, 220, 0.9])
    const t = parseTracks(flat)
    expect(t).toHaveLength(2)
    expect(t[0].start).toBe(3)
    expect(Array.from(t[0].freq)).toEqual([440, 441])
    expect(Array.from(t[0].amp)).toEqual([0.5, Math.fround(0.4)])
    expect(Array.from(t[1].freq)).toEqual([220])
  })

  it('a cut-off buffer keeps the whole tracks', () => {
    expect(parseTracks(new Float32Array([0, 3, 1, 2]))).toEqual([])
    expect(parseTracks(new Float32Array([0, 1, 100, 0.5, 7]))).toHaveLength(1)
  })

  it('error codes read as words', () => {
    expect(codeMessage(-1)).toBe('not a WAV file')
    expect(codeMessage(-12)).toBe('longer than 60 seconds')
    expect(codeMessage(-99)).toBe('error -99')
  })
})

describe('a recipe from the engine', () => {
  it('says its kind, slot, source and transforms', () => {
    const values = [...NO_EDITS, ...NO_MORPH, 2048, 256]
    values[1] = 1.6
    const r = recipeOf({ slot: 16 + 3, text: 'attack 3 = upright-piano-kw/C4vH : inharmonic 1.6, window 2048', values })
    expect(r).toMatchObject({ kind: 1, slot: 3, source: 'upright-piano-kw/C4vH', other: null, window: 2048, hop: 256 })
    expect(r.edits[1]).toBe(1.6)
    expect(recipeOf({ slot: 1, text: 'table 1 = @00ff', values: [...NO_EDITS, ...NO_MORPH, 4096, 256] })).toMatchObject({ kind: 0, slot: 1, source: '@00ff' })
    const m = [...NO_EDITS, 0, 1, 0, 0, 1, 4096, 256]
    expect(recipeOf({ slot: 3, text: 'table 3 = a/saw ~ synth:LaChoir:c4 : mamp 1, sweep 1', values: m })).toMatchObject({ source: 'a/saw', other: 'synth:LaChoir:c4', morph: [0, 1, 0, 0, 1] })
  })
})
