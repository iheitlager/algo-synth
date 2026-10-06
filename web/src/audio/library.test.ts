// The preset library (#232) against a small in-memory IndexedDB: open, one
// store, get and put. No fake-indexeddb dependency; this is all library.ts uses.
import { afterEach, describe, expect, it, vi } from 'vitest'
import * as tables from './params'
import { GlobalParam, InsertType, Param, ProcType, StripParam } from './params'
import { capture, libraryText, type PresetRegistry, type UserPreset } from './presets'

const reg: PresetRegistry = {
  params: Param,
  global: GlobalParam,
  strip: StripParam,
  models: (tables as unknown as Record<string, Record<string, number>>).Model,
  maxSynths: 16,
  insertTypes: InsertType,
  procTypes: ProcType,
}

const vals = Array.from({ length: 24 }, () => Object.values(Param).map(() => 0))
const preset = (name: string, s = 0): UserPreset => capture(name, { kind: 'synth', s }, vals, reg)

/** An IndexedDB with one database of one store; `fail` makes every request fail. */
function fakeIndexedDB(stored: Map<string, unknown>, fail = { open: false, write: false }) {
  const later = (f: () => void) => queueMicrotask(f)
  const request = <T>(result: () => T, failed = false) => {
    const req = { result: undefined as T | undefined, error: null as Error | null, onsuccess: null as null | (() => void), onerror: null as null | (() => void) }
    later(() => {
      if (failed) {
        req.error = new Error('denied')
        req.onerror?.()
      } else {
        req.result = result()
        req.onsuccess?.()
      }
    })
    return req
  }
  const store = { get: (k: string) => request(() => stored.get(k)), put: (v: unknown, k: string) => stored.set(k, v) }
  const db = {
    createObjectStore: vi.fn(),
    transaction: () => {
      const tx = { error: null as Error | null, oncomplete: null as null | (() => void), onerror: null as null | (() => void), objectStore: () => store }
      later(() => {
        if (fail.write) {
          tx.error = new Error('quota')
          tx.onerror?.()
        } else tx.oncomplete?.()
      })
      return tx
    },
  }
  return {
    db,
    open: () => {
      const req = { result: db, error: null as Error | null, onupgradeneeded: null as null | (() => void), onsuccess: null as null | (() => void), onerror: null as null | (() => void) }
      later(() => {
        if (fail.open) {
          req.error = new Error('blocked')
          req.onerror?.()
          return
        }
        req.onupgradeneeded?.()
        req.onsuccess?.()
      })
      return req
    },
  }
}

/** A fresh library module, loaded from `stored`. */
async function fresh(stored = new Map<string, unknown>(), fail?: { open: boolean; write: boolean }) {
  vi.resetModules()
  vi.stubGlobal('indexedDB', fakeIndexedDB(stored, fail))
  const lib = await import('./library')
  await lib.loadLibrary(reg)
  return lib
}

const settle = () => new Promise((r) => setTimeout(r))

afterEach(() => vi.unstubAllGlobals())

describe('library (ADR-0014)', () => {
  it('loads what is stored', async () => {
    const stored = new Map<string, unknown>([['presets', libraryText([preset('Pad'), preset('Bass')])]])
    const { library } = await fresh(stored)
    expect(library.presets.map((p) => p.name)).toEqual(['Pad', 'Bass'])
    expect(library.persistent).toBe(true)
    expect(library.notice).toBe('')
  })

  it('starts empty, saying why, when the stored library does not parse', async () => {
    const { library } = await fresh(new Map([['presets', '{nope']]))
    expect(library.presets).toEqual([])
    expect(library.notice).toBe('Presets: the stored library is not a JSON file; starting empty')
  })

  it('lives for the session without IndexedDB, or when it fails', async () => {
    vi.resetModules()
    const lib = await import('./library')
    await lib.loadLibrary(reg)
    expect(lib.library.persistent).toBe(false)
    lib.savePreset(preset('Kept'))
    expect(lib.library.presets).toHaveLength(1)

    const failing = await fresh(new Map(), { open: true, write: false })
    expect(failing.library.persistent).toBe(false)
  })

  it('a failed write makes the library session-only', async () => {
    const lib = await fresh(new Map(), { open: false, write: true })
    lib.savePreset(preset('Lost'))
    await settle()
    expect(lib.library.persistent).toBe(false)
  })

  it('save adds, replaces by id or by kind and name, and persists', async () => {
    const stored = new Map<string, unknown>()
    const lib = await fresh(stored)
    const pad = preset('Pad')
    lib.savePreset(pad)
    lib.savePreset(preset('Lead'))
    // Same kind, model and name: it replaces the first and keeps its id.
    const again = { ...preset('Pad'), params: { ...pad.params, Cutoff: 1 } }
    lib.savePreset(again)
    expect(lib.library.presets.map((p) => p.name)).toEqual(['Pad', 'Lead'])
    expect(lib.library.presets[0]?.id).toBe(pad.id)
    expect(lib.library.presets[0]?.params.Cutoff).toBe(1)
    await settle()
    expect(JSON.parse(stored.get('presets') as string).presets).toHaveLength(2)
  })

  it('rename cleans the name and ignores an empty one or an unknown id', async () => {
    const lib = await fresh()
    const p = preset('Pad')
    lib.savePreset(p)
    lib.renamePreset(p.id, '  Warm   pad ')
    expect(lib.library.presets[0]?.name).toBe('Warm pad')
    lib.renamePreset(p.id, '   ')
    lib.renamePreset('nope', 'X')
    expect(lib.library.presets[0]?.name).toBe('Warm pad')
  })

  it('delete, and presetsOf by kind and model, sorted by name', async () => {
    const lib = await fresh()
    const [b, a, c] = [preset('b'), preset('a'), preset('c')]
    for (const p of [b, a, c]) lib.savePreset(p)
    lib.deletePreset(c.id)
    expect(lib.presetsOf('synth').map((p) => p.name)).toEqual(['a', 'b'])
    expect(lib.presetsOf('synth', { model: a.model })).toHaveLength(2)
    expect(lib.presetsOf('synth', { model: 'NoSuchModel' })).toEqual([])
    expect(lib.presetsOf('insert')).toEqual([])
  })

  it('import merges a library file, or says why it did not', async () => {
    const lib = await fresh()
    lib.savePreset(preset('Pad'))
    await lib.importLibrary(new File(['nope'], 'bad.json'))
    expect(lib.library.notice).toBe('bad.json: not a JSON file; nothing imported')
    await lib.importLibrary(new File([libraryText([preset('Pad'), preset('Lead')])], 'mine.presets.json'))
    expect(lib.library.notice).toBe('imported 2 presets from mine.presets.json')
    // A taken name gets a number.
    expect(lib.library.presets.map((p) => p.name).sort()).toEqual(['Lead', 'Pad', 'Pad 2'])
  })

  it('import does nothing before the library loaded', async () => {
    vi.resetModules()
    const lib = await import('./library')
    await lib.importLibrary(new File([libraryText([preset('Pad')])], 'x.json'))
    expect(lib.library.presets).toEqual([])
  })
})
