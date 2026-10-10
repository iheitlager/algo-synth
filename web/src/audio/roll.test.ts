import { describe, expect, it } from 'vitest'
import { cellAt, dragLength, isBlack, noteAt, noteBox, noteName, rollRange, stepIn } from './roll'

const note = (start: number, len: number, n: number) => ({ start, len, note: n, accent: false })

describe('the note view geometry (#168)', () => {
  it('names notes as the song text does', () => {
    expect([noteName(60), noteName(61), noteName(33), noteName(127)]).toEqual(['c4', 'c#4', 'a1', 'g9'])
    expect([isBlack(61), isBlack(60)]).toEqual([true, false])
  })

  it('draws the notes range with room, at least an octave', () => {
    expect(rollRange([])).toEqual({ lo: 55, hi: 66 })
    const r = rollRange([note(0, 3, 60), note(3, 3, 64)])
    expect(r.hi - r.lo + 1).toBeGreaterThanOrEqual(12)
    expect(r.lo).toBeLessThanOrEqual(59)
    expect(r.hi).toBeGreaterThanOrEqual(65)
    const wide = rollRange([note(0, 3, 40), note(0, 3, 80)])
    expect(wide).toEqual({ lo: 39, hi: 81 })
    expect(rollRange([note(0, 3, 0)]).lo).toBe(0)
  })

  it('turns a point on the grid into a sixteenth and a pitch', () => {
    // one bar, rows 60 to 71: the top row is the highest pitch
    expect(cellAt(0, 0, 1, 60, 71)).toEqual({ tick: 0, note: 71 })
    expect(cellAt(0.999, 0.999, 1, 60, 71)).toEqual({ tick: 45, note: 60 })
    expect(cellAt(0.5, 0.5, 1, 60, 71)).toEqual({ tick: 24, note: 65 })
    expect(cellAt(0.5, 0.5, 2, 60, 71)).toEqual({ tick: 48, note: 65 })
    expect(cellAt(-1, 2, 1, 60, 71)).toEqual({ tick: 0, note: 60 })
  })

  it('places a note box in percent', () => {
    expect(noteBox(note(12, 12, 71), 1, 60, 71)).toEqual({ left: 25, width: 25, top: 0, height: 100 / 12 })
    expect(noteBox(note(48, 24, 60), 2, 60, 71).left).toBe(50)
  })

  it('finds the note under a click', () => {
    const events = [note(0, 12, 60), note(12, 3, 64)]
    expect(noteAt(events, 6, 60)).toBe(events[0])
    expect(noteAt(events, 12, 60)).toBeUndefined()
    expect(noteAt(events, 12, 64)).toBe(events[1])
    expect(noteAt(events, 6, 61)).toBeUndefined()
  })

  it('snaps a drag to sixteenths and keeps a note at least one', () => {
    expect(dragLength(12, 0.5, 1)).toBe(12)
    expect(dragLength(12, 0.77, 1)).toBe(24)
    expect(dragLength(12, 0.1, 1)).toBe(3)
    expect(dragLength(0, 0.5, 2)).toBe(48)
  })

  it('maps the song step into the clip', () => {
    expect([stepIn(-1, 1), stepIn(5, 1), stepIn(17, 1), stepIn(17, 2)]).toEqual([-1, 5, 1, 17])
  })
})
