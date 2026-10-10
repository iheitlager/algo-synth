import { describe, expect, it } from 'vitest'
import { arrangeOps, keyAction, landsIn, newRec, recordStep, stepQuantize, typing } from './launch'
import { Quantize } from './params'

const key = (code: string, more: Partial<Parameters<typeof keyAction>[0]> = {}) =>
  keyAction({ code, shiftKey: false, metaKey: false, ctrlKey: false, altKey: false, repeat: false, target: null, ...more })

describe('Launch keys (#489)', () => {
  it('launches scenes with the digits and switches snapshots with z–m, Shift for now', () => {
    expect(key('Digit1')).toEqual({ kind: 'scene', index: 0, now: false })
    expect(key('Digit0', { shiftKey: true })).toEqual({ kind: 'scene', index: 9, now: true })
    expect(key('KeyZ')).toEqual({ kind: 'snapshot', index: 0, now: false })
    expect(key('KeyM', { shiftKey: true })).toEqual({ kind: 'snapshot', index: 6, now: true })
  })

  it('has keys for the moment, the arrangement, cancel and the transport', () => {
    expect(key('BracketLeft')).toEqual({ kind: 'quantize', by: -1 })
    expect(key('BracketRight')).toEqual({ kind: 'quantize', by: 1 })
    expect(key('Backslash')).toEqual({ kind: 'resume' })
    expect(key('Escape')).toEqual({ kind: 'cancel' })
    expect(key('Space')).toEqual({ kind: 'transport' })
  })

  it('leaves the note keys, repeats, shortcuts and typing alone', () => {
    for (const code of ['KeyA', 'KeyW', 'KeyS', 'Semicolon', 'KeyP']) expect(key(code)).toBeNull()
    expect(key('Digit1', { repeat: true })).toBeNull()
    expect(key('Digit1', { metaKey: true })).toBeNull()
    expect(key('Digit1', { ctrlKey: true })).toBeNull()
    const field = { tagName: 'TEXTAREA', isContentEditable: false } as unknown as EventTarget
    expect(typing(field)).toBe(true)
    expect(key('Digit1', { target: field })).toBeNull()
    expect(key('Space', { target: { tagName: 'INPUT' } as unknown as EventTarget })).toBeNull()
    expect(key('Space', { target: { tagName: 'BUTTON' } as unknown as EventTarget })).toEqual({ kind: 'transport' })
  })

  it('steps the moment between bar, end and phrase', () => {
    expect(stepQuantize(Quantize.Bar, 1)).toBe(Quantize.End)
    expect(stepQuantize(Quantize.End, 1)).toBe(Quantize.Phrase)
    expect(stepQuantize(Quantize.Phrase, 1)).toBe(Quantize.Phrase)
    expect(stepQuantize(Quantize.Bar, -1)).toBe(Quantize.Bar)
  })
})

describe('Rec (#489)', () => {
  it('notes each scene each time it starts, and nothing while off', () => {
    const rec = newRec()
    recordStep(rec, 0, 3)
    expect(rec.entries).toEqual([])
    rec.on = true
    for (const [scene, local] of [[0, 5], [0, 12], [0, 2], [0, 9], [2, 0], [2, 8], [-1, 4], [1, 6]]) recordStep(rec, scene, local)
    expect(rec.entries).toEqual([0, 0, 2, 1])
  })

  it('makes the arrangement by removing the old entries from the last, then inserting', () => {
    expect(arrangeOps(2, [1, 0])).toEqual([
      { op: 'remove', at: 1 },
      { op: 'remove', at: 0 },
      { op: 'insert', at: 0, scene: 1 },
      { op: 'insert', at: 1, scene: 0 },
    ])
  })

  it('says how far off a launch lands', () => {
    expect(landsIn(-1)).toBe('')
    expect(landsIn(3)).toBe('1 beat')
    expect(landsIn(11)).toBe('3 beats')
    expect(landsIn(16)).toBe('1 bar')
    expect(landsIn(40)).toBe('3 bars')
  })
})
