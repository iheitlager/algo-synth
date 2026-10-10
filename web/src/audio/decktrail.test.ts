import { describe, expect, it } from 'vitest'
import { ROWS, aheadEntry, at, record, sectionColour, trail, width } from './decktrail'

describe('deck trail', () => {
  it('keep the level of every step, the steps between two reports taking the later', () => {
    const t = trail()
    record(t, 4, 0.5, 0)
    record(t, 7, 0.25, 1)
    expect(at(t, 4)).toEqual({ peak: 0.5, entry: 0 })
    expect([5, 6, 7].map((k) => at(t, k))).toEqual(Array(3).fill({ peak: 0.25, entry: 1 }))
    expect(at(t, 3)).toBeNull()
    expect(at(t, 8)).toBeNull()
  })

  it('hold the highest of several reports on one step', () => {
    const t = trail()
    record(t, 2, 0.3, 0)
    record(t, 2, 0.6, 0)
    record(t, 2, 0.1, 0)
    expect(at(t, 2)?.peak).toBeCloseTo(0.6)
  })

  it('start over when the step goes back, and forget what fell out of its rows', () => {
    const t = trail()
    record(t, 40, 0.5, 2)
    record(t, 3, 0.2, 0)
    expect(at(t, 40)).toBeNull()
    expect(at(t, 3)?.peak).toBeCloseTo(0.2)
    record(t, 3 + ROWS, 0.4, 0)
    expect(at(t, 3)).toBeNull()
    expect(at(t, 4)?.peak).toBeCloseTo(0.4)
    record(t, -1, 1, 0)
    expect(t.last).toBe(3 + ROWS)
  })

  it('name the entry of a bar ahead, −1 outside what the engine sent', () => {
    const a = { from: 5, entries: [1, 1, 2, -1] }
    expect([4, 5, 7, 8, 9].map((b) => aheadEntry(a, b))).toEqual([-1, 1, 2, -1, -1])
    expect(aheadEntry(null, 0)).toBe(-1)
  })

  it('draw a level over 48 dB, and give each section its own colour', () => {
    expect(width(1)).toBe(1)
    expect(width(0)).toBe(0)
    expect(width(10 ** (-24 / 20))).toBeCloseTo(0.5)
    expect(width(10 ** (-60 / 20))).toBe(0)
    expect(sectionColour(0)).not.toBe(sectionColour(1))
  })
})
